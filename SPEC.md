# SPEC — Accord wire protocol, version 1

> **Contract.** Every implementation follows this document down to the byte. Any
> multi-byte integer is **big-endian**. Every string is length-prefixed UTF-8.
> Current protocol version: `0x01`.

## 0. Encoding conventions (accord-proto)

Primitive types used throughout the document:

| Type | Size | Description |
|------|--------|-------------|
| `u8/u16/u32/u64` | 1/2/4/8 | unsigned big-endian integers |
| `bytes<N>` | N | fixed array |
| `vbytes` | 2 + n | `u16` length then bytes (max 65,535) |
| `lbytes` | 4 + n | `u32` length then bytes (max 16 MiB enforced at decode) |
| `str` | 2 + n | `vbytes` containing valid UTF-8 |
| `list<T>` | 2 + Σ | `u16` element count then elements |
| `opt<T>` | 1 (+T) | `u8` 0/1 then T if 1 |

Decoding is **strict**: excess length, invalid UTF-8, out-of-bounds sizes
⇒ `DecodeError`, packet silently rejected on the network side.

### 0.1 Version compatibility

- Every external packet begins with `version: u8`. A v N node accepts v ≤ N packets
  it knows how to decode and **always replies in the requester's version** if ≤ N.
- A packet of version > N is answered (if a session exists) with `ERROR
  code=UNSUPPORTED_VERSION` carrying the max supported version; otherwise ignored.
- Evolution rule: new fields are only added at the **end of a structure** in a new
  version; unknown packet types within a session are ignored (forward-compat); the
  `reserved` fields are written as zero and are not checked. A v N node thus
  coexists with v N+1: they speak to each other in v N.

## 1. Outer envelope (UDP datagram / TCP frame)

Over TCP, every packet is prefixed with a `u32` length (max 1 MiB). Over UDP, one packet
= one datagram (application MTU 1,200 bytes for anything that is not TCP).

```
Offset  Size    Field
0       1       version        (0x01)
1       1       packet_class   0x01=HELLO 0x02=WELCOME 0x03=DATA 0x04=COOKIE
2       ...     body per packet_class
```

Only HELLO/WELCOME/COOKIE have a body that is partially in the clear (needed for
establishment). **Everything else in the protocol lives inside DATA (encrypted).**

## 2. Cryptography

### 2.1 Identity

- Immutable **Ed25519** keypair. `NodeId = SHA-256(pubkey_ed25519)` (32 bytes).
- **Identity PoW**: `pow_nonce: u64` such that `SHA-256(pubkey ‖ pow_nonce_be)` has
  ≥ `POW_BITS` leading zero bits. `POW_BITS = 16` (network-tunable, bounded 8–24).
  Sent in HELLO and verified on first encounter; failure ⇒ silent rejection.
- Derived static X25519 key: `xk = clamp(SHA-512(seed_ed25519)[0..32])` — this uses
  the standard Ed25519-to-X25519 conversion (birational map) provided by dalek.
  Used only for sealing mailboxes (§7).

### 2.2 Handshake (1-RTT, mutually authenticated)

Inspired by Noise-XX condensed into 2 messages with signatures (both parties prove
their identity, secret via ephemeral-ephemeral DH ⇒ immediate forward secrecy).

```
HELLO  (initiator → responder), packet_class=0x01 :
  version:u8=1, class:u8=0x01
  eph_pub_i     : bytes<32>   X25519 ephemeral, initiator
  static_pub_i  : bytes<32>   Ed25519 initiator
  pow_nonce_i   : u64
  timestamp_ms  : u64         UNIX wall-clock ms
  nonce_i       : bytes<16>   anti-replay
  cookie        : vbytes      0 bytes or anti-DoS cookie (§2.5)
  sig_i         : bytes<64>   Ed25519(static_i, transcript_1)

  transcript_1 = SHA-256("accord-hs-v1" ‖ version ‖ eph_pub_i ‖ static_pub_i ‖
                         pow_nonce_i ‖ timestamp_ms ‖ nonce_i)

WELCOME (responder → initiator), packet_class=0x02 :
  version:u8=1, class:u8=0x02
  eph_pub_r     : bytes<32>
  static_pub_r  : bytes<32>
  pow_nonce_r   : u64
  timestamp_ms  : u64
  nonce_r       : bytes<16>
  session_id    : bytes<8>    randomly chosen by the responder
  sig_r         : bytes<64>   Ed25519(static_r, transcript_2)

  transcript_2 = SHA-256("accord-hs-v1" ‖ transcript_1 ‖ eph_pub_r ‖ static_pub_r ‖
                         pow_nonce_r ‖ timestamp_ms_r ‖ nonce_r ‖ session_id)
```

Validation (both directions): version, PoW, |timestamp − now| ≤ 90,000 ms,
`nonce` never seen (5 min cache), signature. Failure ⇒ silent rejection (no oracle).

**Identity binding (initiator).** The handshake is mutually authenticated, but
mutual authentication alone does not guarantee the session is established with
the *intended* identity. When the initiator opens a session toward a specific
peer (CORE delivery: DMs, groups, friends — the expected target Ed25519 identity
`static_pub_r` is known), it **binds** the session to that target: the WELCOME is
accepted only if the received `static_pub_r` is *exactly* the expected identity
(constant-time comparison). A fresh, valid WELCOME signed by a *different* identity
(on-path MITM that spoofs the source address and forges its own reply) is rejected
before any key derivation: the message queue destined for the target is never
sealed under a spoofed session. The binding is applied in defense-in-depth both at
the cryptographic level (WELCOME validation) and the transport level (before
sealing the queue). For an *arbitrary* peer with no expected identity (DHT RPC
toward nodes with no established friendship), no binding is enforced: only standard
mutual authentication applies.

Derivation:

```
shared    = X25519(eph_priv, eph_pub_pair)
prk       = HKDF-Extract(salt=transcript_2, ikm=shared)
k_i2r     = HKDF-Expand(prk, "accord-i2r", 32)   // initiator→responder key
k_r2i     = HKDF-Expand(prk, "accord-r2i", 32)
```

### 2.3 Session state machine

```
Initiator:  IDLE → HELLO_SENT →(valid WELCOME)→ ESTABLISHED
            HELLO_SENT: timeout 2 s, 2 retransmissions, then FAILED
Responder:  IDLE →(valid HELLO)→ ESTABLISHED  (replies WELCOME; stateless before
            cookie validation if under pressure, §2.5)
ESTABLISHED → REKEYING (§2.4) → ESTABLISHED
ESTABLISHED → CLOSED : CLOSE packet received, 120 s inactivity, or fatal error
```

### 2.4 DATA packets and re-keying

```
DATA, packet_class=0x03 :
  version:u8, class:u8=0x03
  session_id : bytes<8>
  epoch      : u8           key generation (re-keying)
  counter    : u64          strictly increasing send counter per direction
  ciphertext : rest of the packet
```

- AEAD **XChaCha20-Poly1305**. Nonce (24) = `direction:u8 ‖ epoch:u8 ‖ zeros<6> ‖
  counter:u64 ‖ zeros<8>` where direction = 0x00 (i→r) or 0x01 (r→i).
  AAD = the first 19 header bytes (version…counter).
- Anti-replay: sliding window of 1,024 counters per (session, epoch, direction).
- **Re-keying**: when `counter` reaches 1,000,000 or the session is 24 h old,
  the sender moves to `epoch+1` with `k' = HKDF-Expand(HKDF-Extract(0, k), "accord-rekey", 32)`
  and sends a `REKEY{new_epoch}` message encrypted under the old key. The receiver
  keeps the last 2 epochs active for 60 s. Old keys are wiped (zeroize).

### 2.5 Handshake anti-DoS (COOKIE)

Under pressure (> 64 HELLO/s globally or > 2/s per IP), the responder creates no
state: it replies `COOKIE (0x04) { cookie: vbytes }` where
`cookie = HMAC-SHA-256(rotating_secret_2min, ip ‖ port ‖ static_pub_i)[0..16]`.
The initiator resends its HELLO with the cookie. Valid HELLO+cookie ⇒ normal processing.

### 2.6 Storage at rest

- Identity vault `identity.vault`: Ed25519 seed (32) + metadata, encrypted with
  XChaCha20-Poly1305 using key = **Argon2id**(m=64 MiB, t=3, p=4, 16-byte salt) of the
  unlock secret. By default the secret is a random 32-byte key stored in the OS
  keychain (transparent unlock); the user may enable a password (the secret becomes
  the password).
- **Recovery phrase**: 12 BIP39 words (standard 2048-word English dictionary)
  encoding 128 bits + 4 SHA-256 checksum bits. The Ed25519 seed = HKDF-Expand(
  HKDF-Extract("accord-recovery", entropy_128), "identity", 32). Restoring the phrase
  regenerates exactly the identity.
- Local base key: `db_key = HKDF-Expand(HKDF-Extract("accord-db", seed), "sqlite", 32)`.

## 3. Internal RPC layer (inside DATA)

Every DATA plaintext begins with:

```
  channel : u8    0x00=CONTROL 0x01=DHT 0x02=CORE 0x03=VOICE 0x04=FILE 0x05=RELAY
  ...             body per channel
```

CONTROL: `0x00 PING{token:u64}`, `0x01 PONG{token:u64}`, `0x02 CLOSE{reason:u8}`,
`0x03 REKEY{new_epoch:u8}`, `0x04 OBSERVE_ADDR_REQ{}`,
`0x05 OBSERVE_ADDR_RESP{addr: SockAddr}` (SockAddr = `family:u8(4|6) ‖ ip ‖ port:u16`).

## 4. DHT (channel 0x01) — 256-bit Kademlia

Parameters: k=20, α=3, 256 buckets, XOR distance over NodeId.

```
DHT body :
  rpc_id  : bytes<20>   random (160 bits), the reply carries the same
  kind    : u8          0x01 PING, 0x02 PONG, 0x03 FIND_NODE, 0x04 FOUND_NODES,
                        0x05 FIND_VALUE, 0x06 FOUND_VALUE, 0x07 STORE, 0x08 STORE_OK,
                        0x7F ERROR{code:u8}
  body per kind

NodeInfo = { node_id: bytes<32>, static_pub: bytes<32>, pow_nonce: u64,
             addrs: list<SockAddr> (≤ 4) }

FIND_NODE   { target: bytes<32> }
FOUND_NODES { nodes: list<NodeInfo> (≤ k) }
FIND_VALUE  { key: bytes<32> }
FOUND_VALUE { found: u8, value: opt<DhtRecord>, nodes: list<NodeInfo> }
STORE       { record: DhtRecord }
DhtRecord   = { key: bytes<32>, kind: u8, value: lbytes (≤ 8 KiB),
                publisher: bytes<32> (pubkey), timestamp_ms: u64, expiry_s: u32,
                sig: bytes<64> }  // Ed25519(publisher, key‖kind‖value‖timestamp‖expiry)
```

- Record kinds: `0x01 IDENTITY` (friend code → signed identity), `0x02 PRESENCE`
  (signed current addresses), `0x03 MAILBOX_HINT`, `0x04 FILE_PROVIDER`.
- STORE validation: signature, `key` consistent with kind (e.g. IDENTITY:
  key = SHA-256("friendcode-v1" ‖ payload)), size, expiry ≤ 7 days, cost:
  the requester must maintain a session (has already paid handshake + identity PoW).
- RPC timeout 2 s, 2 retransmissions (backoff ×2). LRS eviction: bucket full ⇒ PING
  the oldest; a reply ⇒ the newcomer is rejected, silence ⇒ replacement.
- Refresh: a bucket with no activity for 60 min ⇒ lookup of a random ID in the bucket.
- Iterative lookup: α=3 parallel, crossing **2 disjoint paths** for a sensitive
  FIND_VALUE (IDENTITY): the sets of first hops are disjoint; divergence of the
  results ⇒ we prefer the most recent valid signed value.
- Diversity: ≤ 2 nodes per /24 IPv4 (or /48 IPv6) and per bucket.
- Anti-abuse: token bucket per source IP — 10 RPC/s steady, burst 40; STORE: 2/s,
  burst 8. Beyond that: silent rejection.
- Republication of records by their publisher every 60 min; replication to the
  k closest; local expiration at `expiry_s`.

## 5. Friend codes (resolved via DHT)

- `payload` = first 64 bits of `SHA-256(pubkey_ed25519)` (8 bytes, **no
  masking**). The 64 bits of entropy make it infeasible to grind a keypair whose
  code would collide with a victim's (~2^64 attempts; the old 33-bit format was
  grindable in a few hours on a GPU).
- Words: **English BIP39** dictionary (2048 words, 11-bit index) — the 64 bits are
  encoded over **6 words** (base 2048): word1 = bits 63–55 (9 useful bits),
  word2 = bits 54–44, word3 = bits 43–33, word4 = bits 32–22, word5 = bits 21–11,
  word6 = bits 10–0.
- Digits: `checksum = u16(SHA-256("accord-fc" ‖ payload_8bytes)[0..2]) mod 10000`,
  shown as 4 digits. Detects any single-word typo with probability ≥ 99.99%.
- Canonical format: `WORD-WORD-WORD-WORD-WORD-WORD-0042` (uppercase accepted, dashes
  or spaces).
- Resolution: `FIND_VALUE(SHA-256("friendcode-v1" ‖ payload_8bytes))` kind
  IDENTITY ⇒ record containing signed `{pubkey, pow_nonce, display_name, avatar_hash}`.
  The client **verifies** that SHA-256(pubkey) starts with payload. Deep link:
  `p2papp://add/WORD-WORD-WORD-WORD-WORD-WORD-0042`.

## 6. CORE channel (0x02) — messaging, groups, presence

Body: `msg_type: u8` then a structure. Main types:

```
0x01 DIRECT_MSG      { msg_id: bytes<16>, lamport: u64, sent_ms: u64, kind: u8
                       (0=text,1=edit,2=delete,3=reaction,4=file,5=typing,
                        6=read_receipt), body: lbytes }
0x02 MSG_ACK         { msg_id: bytes<16> }
0x03 FRIEND_REQUEST  { display_name: str, message: str, verify_phrase: opt<str> }
0x04 FRIEND_RESPONSE { accepted: u8 }
0x05 GROUP_OP        { group_id: bytes<16>, op: lbytes (signed GroupOp) }
0x06 GROUP_MSG       { group_id: bytes<16>, channel_id: bytes<16>, msg_id: bytes<16>,
                       lamport: u64, sent_ms: u64, key_epoch: u32, kind: u8,
                       body_enc: lbytes }   // encrypted with group key
0x07 GROUP_KEY       { group_id: bytes<16>, key_epoch: u32,
                       sealed_key: bytes<80> }  // key sealed for this member (§6.4)
0x08 PRESENCE        { status: u8 (0=online,1=idle,2=dnd,3=offline), custom: opt<str> }
                     // custom: UTF-8 custom status text, ≤ 256 bytes (decode
                     // bound). Rich status (0-2) and custom text are only
                     // honoured from friends; unknown status values degrade to
                     // offline. "Invisible" is a purely LOCAL mode: the node
                     // announces a bare offline (status=3, no custom text)
                     // while keeping full functionality — it never appears on
                     // the wire. Older nodes sending bare online/offline
                     // (custom absent) interoperate unchanged.
0x09 PROFILE         { display_name: str (≤ 128 bytes), bio: str,
                       avatar_hash: opt<bytes<32>>, banner_hash: opt<bytes<32>> }
0x0A VOICE_SIGNAL    { group_id: bytes<16>, channel_id: bytes<16>, action: u8
                       (0=join,1=leave,2=state), media_kinds: u8, mute: u8 }
                     // media_kinds: bitflags — 0x01 audio (video/screen
                     // reserved); bit 0x80 = sender is self-deafened (deafen
                     // implies mute). Receivers MUST ignore unknown bits:
                     // the byte layout is unchanged, older peers interoperate.
0x0D FRIEND_REMOVE   { }
                     // The sender (authenticated by the encrypted session)
                     // removed the friendship on their side. Best-effort:
                     // never queued offline. On receipt, an ESTABLISHED
                     // friendship is dropped too (DM history kept on both
                     // sides); any other contact state (pending, blocked,
                     // unknown) is left untouched. Distinct from a block:
                     // a new friend request stays possible afterwards.
```

### 6.1 Direct messages

Body `body` (for kind=0): `{ text: str, reply_to: opt<bytes<16>>,
attachments: list<FileRef> }` with `FileRef = { merkle_root: bytes<32>, name: str,
size: u64, mime: str }`. Edits/deletions reference the original `msg_id`;
tombstones are kept. ACK is mandatory; without an ACK ⇒ offline queue (§7).

**Ephemeral kinds** (typing `5`, read receipt `6`): never ACKed, never queued
offline — an unreachable peer simply misses them.

**Delivery state** is derived locally from the ACK flag and the offline queue
(§7), never carried on the wire: `sent` once ACKed, `failed` when direct
retries are exhausted or the message is unacked, no longer queued and past the
queue expiry, `pending` otherwise. `dm.retry` re-emits the stored `DirectMsg`
(same `msg_id`/`lamport`/`sent_ms`) on the normal path and resets the queue
backoff — it introduces **no new wire message**.

**Pins (direct messages)** are a purely **local view**: unlike group pins
(carried by the `Pin`/`Unpin` group ops in the op-log, §6.2), a DM pin is stored only in the
local `dm_pins` table and is **never** sent to the peer. Deleting a message
also removes its pin. "Jump-to-message" (`dm.history_around` /
`groups.history_around`) is a read-only local query over stored history — no
wire message either.

**Read receipts** (kind=6, body `{ up_to: bytes<16> }`): `up_to` is the
`msg_id` of the most recent message **authored by the receiver** that the
sender has read. Emitted best-effort when the local read mark advances
(throttled: re-marking the same position stays silent) and only towards peers
presumed online. Emission can be disabled locally (privacy setting persisted
in the meta table, default on); incoming receipts are recorded regardless of
that setting. The receiver persists the peer's read position per conversation
and exposes it through the local API (`dm.history` → `peer_read_lamport`,
`event.dm_read`).

**Mentions** are a purely **local, passive** signal — they add **no wire
message and no wire field**. A `DirectMsg`/`GroupMsg` text body already
carries the literal `@…` the sender typed; on ingestion of a stored text
message, the receiver decides whether it is itself mentioned by matching that
text (case-insensitive, word-bounded) against its own nickname, its friend
code, the tokens `@everyone`/`@here` (identical: effective presence is
unknowable in a serverless P2P network, so `@here` is detected exactly like
`@everyone`), and the names of the roles it holds in that group. A match sets
`mentions_me` in history and records **one** entry (deduplicated on `msg_id`)
in the local `mentions` table; a deleted message loses its entry. The inbox and
its per-conversation counts are read through the local API (`mentions.inbox`,
`mentions.mark_read`); nothing is ever transmitted.

**Private contact notes** are a purely **local** free-text annotation attached
to a public key (`contact_notes` table, ≤ 4096 characters). Like DM pins and
mentions, they add **no wire message**: a note never leaves the device.

### 6.2 Group op-log

```
GroupOp = { op_id: bytes<16>, group_id: bytes<16>, lamport: u64, wall_ms: u64,
            author: bytes<32> (pubkey), kind: u8, body: lbytes, sig: bytes<64> }
```

Kinds: 0x01 CREATE, 0x02 SET_META, 0x03 ADD_CHANNEL, 0x04 EDIT_CHANNEL,
0x05 DEL_CHANNEL, 0x06 ADD_CATEGORY, 0x07 ADD_MEMBER, 0x08 KICK, 0x09 BAN, 0x0A UNBAN,
0x0B ADD_ROLE, 0x0C EDIT_ROLE, 0x0D DEL_ROLE, 0x0E ASSIGN_ROLE, 0x0F UNASSIGN_ROLE,
0x10 SET_CHANNEL_PERMS, 0x11 PIN, 0x12 UNPIN, 0x13 DELETE_MSG (moderation),
0x14 SET_TOPIC, 0x15 INVITE_CREATE, 0x16 INVITE_REVOKE, 0x17 LEAVE,
0x18 ADD_EMOJI `{ name: str, file: bytes<32> }`, 0x19 DEL_EMOJI `{ name: str }`,
0x1A EDIT_CATEGORY `{ category_id: bytes<16>, name: str, position: u16 }`,
0x1B DEL_CATEGORY `{ category_id: bytes<16> }`,
0x1C SET_CHANNEL_CATEGORY `{ channel_id: bytes<16>, category: opt<bytes<16>> }`.

- Total order: `(lamport, author_node_id)` ascending. Deterministic application;
  an op not authorized by the current state ⇒ ignored (all honest peers converge).
- Permissions (u32 bitfield): VIEW=1, SEND=2, MANAGE_MESSAGES=4, MANAGE_CHANNELS=8,
  INVITE=16, KICK=32, BAN=64, MANAGE_ROLES=128, ADMIN=256 (implies everything),
  MANAGE_EMOJIS=512.
  Resolution: OR of the member's roles, per-channel overrides (allow/deny), deny > allow,
  ADMIN and founder short-circuit. Writing into a channel requires the
  effective VIEW **and** SEND there (a channel hidden from a role cannot be
  written to by its members).
- **Categories** (`MANAGE_CHANNELS`): `EDIT_CATEGORY` renames/repositions;
  `DEL_CATEGORY` removes the category only — its channels survive,
  uncategorized; `SET_CHANNEL_CATEGORY` moves a channel into an existing
  category (`category` absent ⇒ uncategorized). Ops referencing an unknown
  category or channel are ignored at replay.
- **Channel overrides** (`SET_CHANNEL_PERMS`, `MANAGE_ROLES`): per
  `(channel, role)` pair of masks `{ allow, deny }`; `allow = deny = 0`
  clears the entry (full inherit). The op is ignored at replay if the
  channel or the role is unknown.
- **Server emojis**: `ADD_EMOJI` (add or replace) and `DEL_EMOJI` require
  `MANAGE_EMOJIS`. `name`: 2 to 32 `[a-z0-9_]` characters (op ignored
  otherwise); at most 50 emojis per server (an add beyond the bound is
  ignored, replacing an existing name remains allowed). The state materializes
  `name → Merkle root` of the image (published in the file store). A custom
  emoji is written `:name:` in a message's text and `":name:"` as a reaction
  value (no wire impact: these are ordinary strings).
- Synchronization: on (re)connection between members, exchange of `(group_id,
  max_lamport, digest)` then catch-up of the missing ops (anti-entropy).

### 6.4 Group key and rotation

- 32-byte symmetric key per `key_epoch`. Group messages encrypted with
  XChaCha20-Poly1305, random 24-byte nonce prefixed to the ciphertext, AAD =
  group_id ‖ channel_id ‖ msg_id ‖ key_epoch.
- Distribution: for each member, `sealed_key` = X25519 sealing
  (ephemeral → member's static, HKDF, XChaCha20-Poly1305; 32 eph_pub + 24 nonce +
  32+16 box = 80 bytes... exact format: `eph_pub<32> ‖ box<48>`, nonce derived
  HKDF(eph_pub ‖ recipient_pub)).
- **Mandatory rotation** on every KICK/BAN/LEAVE: a member holding
  MANAGE_ROLES/ADMIN (else the founder, else the oldest remaining member —
  deterministic rule) generates epoch+1 and distributes it to all remaining members.

### 6.5 User profile (D-027)

`0x09 PROFILE` carries the sender's public profile; only `display_name`
(the nickname) is used today — `bio`, `avatar_hash` and `banner_hash`
are reserved and constitute the message's versioning path.

- **Bounds**: `display_name` ≤ 128 UTF-8 bytes at decode (strict rejection,
  anti-abuse); 2 to 32 characters after trim, without control characters, at
  ingestion.
- **Emission**: on every change of the local nickname, to all confirmed
  friends; and on every friendship establishment, in both directions
  (the accepter sends it with their response, the requester in reply to
  the received acceptance — including auto-accepted crossed requests).
- **Reception**: the sender is authenticated by the encrypted session; the
  message is silently ignored if it is not a **friend** (anti-abuse).
  Otherwise the validated nickname replaces the contact's display name (the one
  rendered by `friends.list`) and the API emits `event.profile { pubkey, name }`.
- **Delivery**: queued offline like the stateful messages (§7) so that
  disconnected friends converge.

## 7. Offline: queues and mailboxes

- Persistent local queue per recipient; backoff 5 s ×2 → cap 15 min;
  expiration 7 days.
- **Mailbox**: `mailbox_key(dest, day, sender, frag) = SHA-256("accord-mb"
  ‖ dest_node_id ‖ day_unix:u64be ‖ sender_node_id ‖ frag_no:u32be)` — one key per
  sender and per fragment, since the DHT keeps a single record per key (D-016).
  The message is sealed for the recipient (same sealing as §6.4, the sender
  signs on the inside), stored via STORE (kind MAILBOX_HINT, ≤ 8 KiB per fragment)
  on the k closest to the key. On connection, the recipient queries the keys
  (contact × {day, previous day} × ascending frag until absent), decrypts, ACKs to
  the sender, then the records expire. Storage nodes can neither read nor correlate
  the sender (opaque keys, unknown preimage).

## 8. Voice (channel 0x03)

```
  vkind : u8   0x01 = AUDIO_FRAME, 0x02 = VOICE_PING
  AUDIO_FRAME { room: bytes<16>, media_type: u8 (0x01=opus-audio),
                seq: u16, ts_ms: u32, payload: vbytes (20 ms Opus frame) }
```

- Sent full mesh to each participant via their UDP session. Loss measured by gaps
  in `seq` (5 s window) and returned in VOICE_PING{loss_pct: u8, rtt_ms: u16};
  the encoder adapts the bitrate: ≥ 10% ⇒ 16k; ≥ 5% ⇒ 24k; ≥ 2% ⇒ 32k; otherwise
  up to 64k in steps.
- VAD: gate at −50 dBFS with 200 ms hysteresis; optional push-to-talk on the UI side.

## 9. Files (channel 0x04)

- Split into **256 KiB** blocks; `block_hash = SHA-256(block)`; binary Merkle
  tree (leaves = block hashes, node = SHA-256(left ‖ right), last odd node
  duplicated); root = file identifier.
- **Reed-Solomon 10+4** per group of 10 data blocks ⇒ 4 parity blocks
  (GF(2^8)). Manifest: `{ merkle_root, size, block_count, rs_groups, name, mime,
  leaf_hashes }`, signed by the sender, ≤ 8 KiB otherwise fragmented.
- Availability announcement: STORE kind FILE_PROVIDER on key=merkle_root.
- FILE channel RPC: `0x01 GET_MANIFEST{root}`, `0x02 MANIFEST{...}`,
  `0x03 GET_BLOCK{root, index:u32}`, `0x04 BLOCK{root, index, data:lbytes}`,
  `0x05 HAVE{root, bitmap:lbytes}`.
- Multi-source download: window of 8 blocks in flight per source, per-block hash
  verification before write, resume via persisted bitmap.
- Target replication r=3 (re-replication < 2), configurable offered quota (default 2 GB),
  LRU eviction.

## 10. Relay (channel 0x05)

- Every publicly reachable node advertises `relay=true` in its NodeInfo.addrs meta.
- `0x01 RELAY_OPEN{target_node: bytes<32>}` → the relay opens a circuit if it has a
  session with the target; `0x02 RELAY_ACCEPT{circuit:u32}`, `0x03 RELAY_DATA{circuit,
  blob:lbytes}`, `0x04 RELAY_CLOSE{circuit}`. The blobs are DATA packets of the
  **end-to-end** session between the two peers (the relay sees nothing).
- Per-circuit cap: 1 MB/s (configurable), round-robin fairness, max 64 circuits.

## 11. NAT traversal

1. Candidates: local (interfaces), public (OBSERVE_ADDR ×3 nodes — 2/3 agreement
   required; disagreement ⇒ symmetric NAT likely ⇒ direct relay), relays.
2. Candidate exchange via DHT (PRESENCE record) or via a common peer / mailbox
   for coordinated hole punching: both peers simultaneously send UDP HELLOs toward
   the other's candidates (5 attempts, 200 ms interval).
3. Try order: local direct → public direct → UDP hole punch → TCP hole punch
   (simultaneous open, best effort) → relay. The first session established wins, the
   other attempts stop.
4. Keep-alive 25 s (CONTROL PING) on active UDP links. Network change detected
   (failure of 3 keep-alives or interface change) ⇒ full renegotiation.
5. Dual-stack IPv4/IPv6, IPv6 preferred at equal latency (±10 ms).

## 12. Protocol error codes

| Code | Name | Context |
|------|-----|----------|
| 0x01 | UNSUPPORTED_VERSION | version > max supported |
| 0x02 | RATE_LIMITED | token bucket empty (often silent) |
| 0x03 | STORAGE_FULL | STORE refused |
| 0x04 | INVALID_SIGNATURE | invalid record/op |
| 0x05 | NOT_FOUND | FIND_VALUE/GET_BLOCK with no result |
| 0x06 | PERMISSION_DENIED | group op refused |
| 0x07 | MALFORMED | decoding impossible (generally silent) |

## 13. Numeric limits (decode guardrails)

Application UDP MTU 1,200 B; max TCP packet 1 MiB; DHT record ≤ 8 KiB; text
message ≤ 8,000 characters; attachment ≤ 2 GiB; 10 voice participants; Merkle
depth ≤ 24; list<> ≤ 4,096 elements unless a stricter constraint applies.

### 13.1 Session fragmentation (encrypted transport)

An application plaintext larger than the payload of a DATA packet (256 KiB file
blocks, DMs up to 32,000 B) does not fit in a datagram under the 1,200 B MTU. The
transport fragments it **transparently inside the encrypted session**: each
fragment is sealed separately (its own nonce derived from the counter, like any
DATA packet), sent in its own datagram, and reassembled after decryption at the
peer. This is a split **internal to the session, after encryption/decryption**:
the DATA envelope (SPEC §1) is unchanged and both ends share the same version (no
wire compatibility to manage).

**Session framing.** Each sealed plaintext begins with a kind byte:

| Kind | Format | Overhead |
|-------|--------|---------|
| `0x00` single frame | `[0x00][application payload]` | 1 B |
| `0x01` fragment | `[0x01][id: u32 BE][total: u16 BE][index: u16 BE][slice]` | 9 B |

A message is sent as a single frame as soon as it fits in a datagram; otherwise it
is fragmented. `id` identifies the message within the session (wrapping counter),
`total` the number of fragments, `index` the position (0-based). Budget per
datagram: `1200 − 19 (DATA header) − 16 (AEAD tag) = 1165 B` of sealable
plaintext, i.e. ≤ 1,164 B of payload in a single frame and ≤ 1,156 B of slice per
fragment.

**Bounded reassembly (anti-DoS), per session.** Reassembly memory capped at
2 MiB and ≤ 8 simultaneous messages; a partial reassembly is abandoned after
30 s (timeout) or on an inconsistent fragment (diverging total, out-of-bounds
index, oversized slice); a reassembled message exceeding 1 MiB (aligned with the
max TCP frame) is rejected. Losing a single fragment loses the entire message —
there is **no retransmission at the transport level** (UDP); the upper layers
(messaging outbox, file-transfer windows) resend.
