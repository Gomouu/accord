# SECURITY — Accord's threat model

> This document describes **honestly** what Accord protects, against whom, and
> what it does **not** protect. It is grounded in the actual code of the
> repository (crates `accord-crypto`, `accord-transport`, `accord-dht`,
> `accord-core`, `accord-node`) and in the `SPEC.md` / `ARCHITECTURE.md`
> contracts.
> Last reviewed: 2026-07-25. **No external audit has been performed.**
> The security trade-offs deliberately accepted for v0 are detailed in
> [docs/THREAT-MODEL.md](docs/THREAT-MODEL.md).

## 1. Principles

- **Zero-trust**: no server, no trusted third party. Every piece of data
  received from the network is authenticated (signature or session key),
  structurally validated (strict decoding, size bounds), and rate-limited.
- **No application data in cleartext on the network**: only the handshake
  packets (`HELLO`/`WELCOME`/`COOKIE`) carry cleartext fields, strictly
  necessary for cryptographic establishment (D-003). Everything else — the DHT
  included — travels inside encrypted sessions.
- **Local defense in depth**: identity sealed by passphrase, database fully
  encrypted, blind search index, secrets `zeroize`d.
- **No over-promising**: Accord is **not** an anonymity tool.

## 2. Primitives and implementations

| Usage | Primitive | Implementation |
|-------|-----------|----------------|
| Identity, signatures | Ed25519 (`NodeId = SHA-256(pubkey)`) | `ed25519-dalek` |
| Key exchange | X25519 (ephemeral-ephemeral; static derived for sealing) | `x25519-dalek` |
| Authenticated encryption | XChaCha20-Poly1305 | `chacha20poly1305` |
| Key derivation | HKDF-SHA-256 | `hkdf`, `sha2` |
| Identity vault | Argon2id (m = 64 MiB, t = 3, p = 4) | `argon2` |
| Anti-Sybil | 16-bit proof of work over `SHA-256(pubkey ‖ nonce)` | `accord-crypto::identity` |
| Device pairing | Symmetric SPAKE2 (Ed25519 group, RFC 9382) + XChaCha20-Poly1305 channel | `spake2` 0.4, `accord-crypto::pairing` |
| Local database | SQLCipher (full-file encryption) | `rusqlite` + embedded SQLCipher (D-013) |
| Search index | HMAC-SHA-256 tokens (blind) | `accord-core::search` (D-011) |
| Recovery phrase | English BIP39, 12 words (128 bits + checksum) | `bip39` |

The cryptographic crates come from the **RustCrypto** and **dalek**
ecosystems, maintained and independently audited (decision D-001 —
`sodiumoxide` rejected because unmaintained). `accord-crypto` is compiled with
`#![forbid(unsafe_code)]`; the no-panic rule is **CI-enforced** (clippy denies
`unwrap`/`expect`/`panic`/`todo`/`unimplemented` in libraries and binaries,
outside tests), and a `cargo-fuzz` harness (8 targets under `fuzz/`) exercises
the wire decoders, DHT records, group state and the backup archive against
arbitrary input; in-memory secrets are wiped via `zeroize`.

## 3. Guarantees offered

### 3.1 Encrypted and authenticated transport

- **Mutually authenticated 1-RTT handshake** (SPEC §2.2): ephemeral-ephemeral
  X25519 DH (⇒ immediate *forward secrecy* per session), both parties sign the
  transcript with their Ed25519 key. Strict validation: version, identity PoW,
  clock (±90 s), anti-replay nonce (5 min cache), signature. Any failure ⇒
  **silent rejection** (no error oracle).
- **XChaCha20-Poly1305 AEAD sessions** (SPEC §2.4): distinct directional keys
  (HKDF of the DH secret), deterministic nonces (direction ‖ epoch ‖ counter),
  authenticated header in AAD, sliding anti-replay window of 1,024 counters per
  (session, epoch, direction).
- **Re-keying**: at 1,000,000 messages or 24 h of session, a new epoch via HKDF
  ratchet; old keys are wiped (`zeroize`). A long recording of a session
  therefore remains decryptable only in windows, even if one epoch key were to
  leak.
- **Handshake anti-DoS** (SPEC §2.5): under pressure, the responder creates no
  state and demands an HMAC cookie with a rotating secret (2 min) bound to the
  source address — an attacker cannot fill the node's memory with forged
  HELLOs.

### 3.2 Anti-Sybil and DHT

- **Identity PoW**: creating an identity costs a nonce search (16 bits of
  zeros by default), verified by every peer on first encounter and by the DHT —
  mass identity creation has a CPU cost.
- **Signed DHT records** (SPEC §4): each record carries the Ed25519 signature
  of its publisher; nodes validate the signature, key ↔ nature consistency
  (e.g. identity record: the key must be the hash of the friend code), size
  (≤ 8 KiB) and expiration (≤ 7 days). The **end client re-verifies everything
  end to end** (friend code ↔ payload ↔ public key ↔ publisher ↔ signature): a
  lying storage node cannot make a forged identity accepted.
- **Disjoint-path lookups** for sensitive values (identities): two iterations
  whose first hops are disjoint; in case of divergence, the most recent valid
  signed value wins — one must control both paths to eclipse a result.
- **IP diversity**: at most 2 nodes per IPv4 /24 (or IPv6 /48) and per bucket —
  a single-network adversary does not fill the routing table.
- **Per-IP rate limiting** (token bucket: 10 RPC/s, STORE 2/s) and LRS eviction
  of buckets: a newcomer does not replace an old node that responds.

### 3.3 End-to-end contents

- **Direct messages**: composed locally, transmitted only within E2E sessions.
  The author's authenticity comes from the session key (public key of the peer
  authenticated at the handshake). Edits, deletions and reactions are validated
  **author-only at ingestion**.
- **Group messages**: encrypted with a symmetric epoch key
  (XChaCha20-Poly1305, AAD = group ‖ channel ‖ message ‖ epoch), distributed to
  each member via an **X25519 sealed box** to their static key. **Mandatory
  rotation on every kick/ban/departure** (SPEC §6.4): the departed member
  cannot decrypt subsequent messages.
- **Signed group op-log** (SPEC §6.2): each administration operation is signed
  by its author and validated against the permission state at the application
  point, in a deterministic total order — all honest nodes converge and an
  unauthorized op is ignored everywhere.
- **Offline mailboxes** (SPEC §7, D-016/D-017): the deposit is **signed then
  sealed** for the recipient; the storage keys are opaque hashes (pre-image
  unknown to the storage nodes) — a node hosting a deposit can neither read it,
  nor know who is writing to whom, nor replay it to another recipient (the
  internal signature binds the recipient).
- **Relaying** (SPEC §10): relayed blobs are DATA packets of the
  **end-to-end** session between the two peers — the relay sees no content.
- **Voice**: Opus frames travel within the existing encrypted transport
  sessions (full mesh, one E2E session per peer). Incoming room signaling is
  re-validated (group membership, participant cap) and frames are only accepted
  from participants of the active room.

### 3.4 Data at rest (disk theft)

- **Identity vault** (`identity.vault`): Ed25519 seed + PoW nonce, encrypted
  with XChaCha20-Poly1305 under an **Argon2id** key (m = 64 MiB, t = 3, p = 4,
  random salt) derived from the user's passphrase. File created with 0600
  permissions.
- **Local database**: SQLCipher — full-file encryption (data, index, WAL,
  journal) under `db_key = HKDF(seed)` (D-013). Without the passphrase, neither
  the identity nor the database open.
- **Encrypted backups** (`.accordbackup`): a full export is sealed with
  **Argon2id + XChaCha20-Poly1305**, streamed chunk by chunk so the plaintext
  archive never touches the disk; imported/created files are forced to `0600`.
  A backup is worth exactly its passphrase — the same guarantee as the vault.
- **Blind search index** (D-011): tokens are indexed under
  `HMAC-SHA-256(k_search, token)` — never any cleartext on disk.
- **Recovery phrase**: 12 BIP39 words shown **only once** at creation, never
  stored. It regenerates exactly the identity (and therefore the database key)
  on a new machine.
- **Secrets never logged**: the API token, passphrases, keys and message
  contents appear in no log (`tracing` only logs counters and states).

### 3.5 Device pairing and the device list

An Accord identity is an **account** (root key) that authorises several
**devices**, each with its own key; the signed device list says which
([docs/MULTI_DEVICE.md](docs/MULTI_DEVICE.md)). Pairing is the one moment where
an account grants trust, so it is described here in full — including where it
stops.

- **Out-of-band code, human-confirmed fingerprint.** The already-authorised
  device shows an 8-character code over a 31-symbol alphabet (≈ 39 bits;
  `0/O` and `1/I/L` are excluded so the code survives being read aloud or copied
  between screens), valid 5 minutes. Both sides derive a one-shot channel from
  it with a **symmetric SPAKE2**, both screens then show the same 6-digit
  fingerprint of the channel key, and a human compares them. Nothing is signed
  before that comparison.
- **What the PAKE buys**: an observer who records the exchange cannot derive the
  channel offline, so every guess costs one *online* interaction. Combined with
  the attempt limit below, that is what makes a short code acceptable rather
  than a hole.
- **What actually proves knowledge of the code — and it is not the PAKE.** In
  its symmetric form, SPAKE2 derives a key on both sides even when the two codes
  differ; the keys are simply different. A failure there means the message was
  malformed, never "wrong code". The only cryptographic failure that proves
  anything is the **opening of the sealed payload** under the channel key:
  whoever cannot open it did not have the code. The new device's public key
  therefore travels sealed under that channel, and it is kept only if it also
  carries a valid identity proof of work.
- **The pairing offer is consumed by the fingerprint confirmation, never by a
  completed exchange.** Since completing proves nothing, consuming the offer on
  a completed exchange would let anyone destroy a pairing in progress with one
  well-formed datagram.
- **Three attempts, counted whether they complete or not**, then the offer is
  burnt and even the correct code stops working. Plus: a single offer at a time,
  held **in memory only** (a code does not survive a restart), expiring after 5
  minutes, replaced whenever a new one is requested, and restarting from a fresh
  SPAKE2 state at every attempt. The code and the channel key have a mute
  `Debug` and reach no log.
- **The account root key never travels**, not even encrypted. Pairing grants
  membership in the signed device list; it never grants the ability to issue
  one.
- **The device list is signed by the account root key** over its whole content,
  version included, so an old list cannot be replayed with a bumped version. A
  version lower than or equal to one already held is ignored, bounds are
  enforced at decode (8 devices, 32 revocations, 8 KiB record), and the list
  carries an explicit 24 h lifetime past which a holder must refresh before
  trusting it — a stale list authorises nobody.

**Denial of pairing.** The candidate channel is a single slot and the last
completed exchange wins, so anyone who can reach the node while an offer is open
can change the fingerprint shown on the authorised screen, and three well-formed
messages burn the offer. They cannot pair — the two screens stop agreeing, which
is precisely what the human comparison catches — but they can force the user to
start over. Bounded by the 5-minute window.

### 3.6 Local surface (UI ↔ node)

- The JSON-RPC API listens **only on 127.0.0.1**, requires a session token on
  the first request (constant-time comparison via `subtle`), and closes the
  connection otherwise.
- The identity lifecycle (creation/restoration/unlock) goes through the
  **Tauri IPC**, not the WebSocket: secrets do not transit any network channel,
  even a local one (D-023, see `API.md` § Identity).
- The Tauri window CSP restricts connections to `ws://127.0.0.1:*` and to the
  IPC bridge; no remote content is loadable.

## 4. Attackers considered

### 4.1 Passive network observer

**What it cannot do**: read a message, a group op, a DHT RPC, a voice frame —
everything is encrypted; replay a packet (anti-replay window); retroactively
decrypt a captured session (forward secrecy via ephemeral DH).

**What it sees**: the IP addresses and ports of the peers that communicate, the
sizes and rates of packets (a voice conversation is recognizable by its 20 ms
rhythm), and — importantly — **the identity public keys exchanged in cleartext
in HELLO/WELCOME**: an on-path observer can learn *which identities* establish
a session, not what they say to each other. See §5.

### 4.2 Active network attacker (interception, spoofing)

- **Identity spoofing**: impossible without the Ed25519 private key — the
  handshake is mutually authenticated by signatures over the transcript.
- **Man-in-the-middle**: can cut or delay traffic, not read or modify it (AEAD,
  AAD over the header, strict counters).
- **Replay**: blocked at the handshake (nonce + clock) and in-session (counter
  window).
- **Denial of service**: possible, as on any reachable system (see §5).
  Mitigated by stateless cookies, silent rejections and per-IP token buckets.

### 4.3 Malicious DHT node

- **Lie about a value**: detected — signed records, end-to-end re-verification
  on the client side, disjoint lookup paths.
- **Refuse to store / return `NOT_FOUND`**: mitigated by replication over the
  k = 20 closest nodes and periodic republication by the publisher (maintenance
  loop, D-024).
- **Read the stored data**: nothing to read — identity and presence records are
  public by nature (and signed), mailbox deposits sealed under opaque keys.
- **Eclipse / Sybil**: made more expensive by PoW, per-bucket IP diversity and
  LRS eviction, **not made impossible** for an adversary with many IP addresses
  and CPU (see §5).

### 4.4 Malicious relay

- **Content**: inaccessible — it carries DATA packets of an E2E session between
  the two peers.
- **Metadata**: it learns who talks to whom (among its circuits), when and how
  much. This is inherent to the relay role; the choice of a relay remains
  opportunistic and traffic is capped per circuit.
- **Tampering**: any modification invalidates the AEAD of the end-to-end
  session — tampered packets are dropped silently.

### 4.5 Disk theft (powered-off machine)

- The thief obtains: an Argon2id vault and a SQLCipher database — nothing
  exploitable **without the passphrase**. Robustness then depends entirely on
  the quality of that phrase (Argon2id at 64 MiB makes exhaustive search
  expensive, it does not make it impossible against a weak phrase).
- The search index contains only token HMACs; at worst it leaks **frequencies**
  of hashed tokens (documented trade-off, D-011), never the text.

## 5. Guarantees explicitly NOT offered

Read before recommending Accord to people whose safety depends on anonymity.

1. **No strong anonymity.** Accord does neither onion routing nor mixing: your
   **IP addresses are visible** to your peers, to the DHT nodes you contact and
   to relays. A global observer (or an ISP/State on the path) can map who talks
   to whom, when and at what volume.
2. **Identity public keys transit in cleartext at the handshake.** An on-path
   observer can associate an Accord identity with an IP address.
3. **DHT metadata.** Presence records (signed addresses) and identity records
   (friend code → public profile) are **public by design** — that is what
   allows you to be found by friend code. Anyone who knows your friend code can
   resolve your public key and your presence.
4. **OS compromise.** If the machine is compromised during use (keylogger,
   memory reading, root access), no guarantee holds: the passphrase, the seed
   and the plaintexts are accessible.
5. **Massive targeted denial of service.** An adversary with significant
   network resources can render a node or a portion of the DHT unusable. The
   defenses (cookies, token buckets, PoW) make the attack more expensive, they
   do not prevent it.
6. **DHT eclipse by a wealthy adversary.** 16-bit PoW + IP diversity + disjoint
   paths raise the cost; an adversary controlling many IPs and much CPU can
   still bias lookups.
7. **Search index leaks.** The local blind index leaks the frequencies of
   HMAC'd tokens to whoever reads the disk (D-011).
8. **Loss of the recovery phrase = loss of the identity.** There is no
   third-party recovery mechanism — it is a design choice.
9. **No external audit.** The protocol and the code have been the subject of
   **no independent security audit**. The cryptography is assembled from proven
   primitives, but the assembly itself has not been audited. Do not use Accord
   where human lives are at stake.
10. **Unevaluated side channels**: clock synchronization, voice traffic
    patterns (VAD modulates transmission — silence is observable), CPU/energy
    consumption.
11. **OS keychain not used.** Contrary to what SPEC §2.6 envisages as an
    optional default, the current implementation always seals the vault under
    the **user passphrase** (no random secret in the system keychain): a weak
    phrase weakens the vault.
12. **Physical presence at an authorised device is enough to pair.** Anyone
    standing in front of your unlocked, already-authorised machine can open a
    pairing offer, read the code off the screen and confirm the fingerprint on
    both sides. The flow is *designed* to require exactly that presence, and it
    cannot tell the account's owner from anyone else who is standing there. No
    cryptography protects against this — physical control of your machines
    does.
13. **Device revocation is eventually consistent.** A peer that has not
    refreshed its copy of your device list keeps treating a revoked device as
    valid until that copy expires — at most 24 hours, usually less, since every
    connection refreshes it. There is no server to push a revocation through; a
    user deciding to revoke must know it is not instantaneous, and the screen
    has to say so. Revoking the device you are currently on is refused: it
    would leave the account with no machine able to sign the next list.
14. **The device list is public.** It is published in the DHT, signed by the
    account key, under a key derived from that same account key — which is what
    lets a contact learn where to reach you without a server. It therefore
    exposes, to anyone who can resolve your friend code, **how many devices your
    account has, the names you gave them ("Work laptop") and when each was
    added**. Name devices accordingly.

## 6. Accepted v0 trade-offs

Documented in full in [docs/THREAT-MODEL.md](docs/THREAT-MODEL.md), with
risk, rationale and hardening path for each. In summary:

1. **Deterministic home relays** (portless first contact): each node keeps
   sessions open to the 2 relay-flagged nodes closest to its own identity,
   a set computable from the public key alone. An attacker willing to grind
   PoW identities can become a victim's home relay and observe **first-contact
   metadata** (who tries to reach whom, when) or censor first contact — it
   can neither read content (E2E) nor impersonate anyone.
2. **Content-addressed blobs are bearer capabilities**: serving a blob
   (avatar, banner, attachment) requires only knowledge of its Merkle root —
   there is no per-peer ACL, only rate limits. **Knowing the root = being
   able to read.** Roots must only be shared with their intended audience
   and never forwarded to third parties.
3. **Server banner/icon is moderator-trusted**: any `MANAGE_CHANNELS`
   holder can set an arbitrary root via a signed group op (`0x31`).
   `MANAGE_CHANNELS` is a trust permission; clients should cap the size of
   auto-downloaded previews (the v0 client does not yet).
4. **DHT presence/identity lookups expose metadata**: storage nodes learn
   who you resolve and when — inherent to open DHT discovery.

## 7. Audit checklist

Recommended entry points for an auditor, from most critical to least critical:

- [ ] **Handshake** (`crates/accord-crypto/src/handshake.rs`): transcripts,
  two-way validation, anti-replay (NonceCache), cookies, silent rejections.
  Frozen test vectors in the crate's tests.
- [ ] **AEAD sessions** (`crates/accord-crypto/src/session.rs`): construction of
  directional nonces, AAD, 1,024 anti-replay window, per-epoch re-keying,
  wiping of old keys.
- [ ] **X25519 sealing** (`crates/accord-crypto/src/sealed.rs`): nonce
  derivation, recipient binding, use for group keys and mailboxes.
- [ ] **Vault** (`crates/accord-crypto/src/vault.rs`): Argon2id parameters,
  `ACCVLT01` format, absence of an oracle at unlock.
- [ ] **Mnemonic** (`crates/accord-crypto/src/mnemonic.rs`): HKDF derivation of
  the seed, BIP39 checksum, non-storage of the phrase.
- [ ] **Strict decoding** (`crates/accord-proto`): size bounds (SPEC §13),
  rejection of excess, absence of panic on arbitrary input — now covered by a
  `cargo-fuzz` harness (`fuzz/fuzz_targets/`: `proto_decode`, `core_msg`,
  `handshake_decode`, `dht_record`, `group_op_body`, `group_state`,
  `file_manifest`, `backup_archive`).
- [ ] **Device pairing** (`crates/accord-crypto/src/pairing.rs`,
  `crates/accord-node/src/pairing.rs`): what a completed symmetric SPAKE2
  exchange does and does not prove, the sealed payload as the only
  machine-checkable key confirmation, single use tied to the fingerprint
  confirmation rather than to the exchange, attempt counter, expiry, fresh state
  per attempt, absence of the code and the channel key from `Debug` and logs.
- [ ] **Device list** (`crates/accord-proto/src/device.rs`,
  `crates/accord-crypto/src/device.rs`, `crates/accord-node/src/device.rs`):
  root signature coverage including the version, version monotonicity, decode
  bounds, freshness window, and a key present in both `devices` and `revoked`
  counting as revoked.
- [ ] **DHT record validation** (`crates/accord-dht/src/store.rs`): signature,
  key/nature consistency, expiration, quotas.
- [ ] **Disjoint paths** (`crates/accord-dht/src/lookup.rs`): real disjunction
  of the first hops, arbitration of divergences.
- [ ] **Transport anti-DoS** (`crates/accord-transport/src/ratelimit.rs`,
  `endpoint.rs`): token buckets, cookies under pressure, memory bounds.
- [ ] **Group op-log** (`crates/accord-core/src/group/`): total order,
  permission control at the application point, key rotation on exclusion.
- [ ] **Offline deposits** (`crates/accord-core/src/offline.rs`): signed-then-
  sealed, recipient binding, self-describing fragmentation.
- [ ] **Local API** (`crates/accord-api`): 127.0.0.1 binding, constant-time
  token comparison, absence of secret logging.
- [ ] **Tauri host** (`app/src-tauri/`): IPC commands (secrets never touch the
  WebSocket), CSP, clean node shutdown.
- [ ] **Blind search** (`crates/accord-core/src/search.rs`): quantify the
  frequency leak (D-011).
- [ ] Verify the absence of `unsafe` (`forbid(unsafe_code)` on the sensitive
  crates) and of `unwrap()`/`expect()`/`panic!` outside tests — **enforced in
  CI** (`ci.sh`: clippy deny-list), together with `cargo audit` / `cargo deny`
  on the dependency tree.

## 8. Reporting a vulnerability

Please do **not** open a public issue for an exploitable flaw.

- **How to report**: use GitHub's *private vulnerability reporting* on this
  repository (Security → Report a vulnerability) if enabled, or contact the
  repository maintainers privately.
- **What to include**: the attack scenario (which attacker from §4, which
  guarantee from §3 is broken), affected version/commit, and if possible a
  test or minimal reproduction.
- **What to expect**: acknowledgement as soon as possible; security fixes
  take priority over any other task (project priority rule #1). Please allow
  time for a fix before any public disclosure. There is no bug bounty.

### Supported versions

**Only the latest release** receives security fixes (current line: 4.x,
delivered through the in-app updater). Older releases are not patched — upgrade
to the current release before reporting.

| Version | Supported |
|---------|-----------|
| latest release (4.x) | ✅ |
| older releases | ❌ |
