# Accord local API — JSON-RPC 2.0 over WebSocket

> Contract between the UI and the node. The server listens **only** on
> `127.0.0.1` (ephemeral port by default). The UI reads the address and the token
> from `<profil>/session.json` written by the daemon at startup.
>
> This document is the **detailed** reference: shapes, bounds, semantics, edge
> cases. [`API_CONTRACT.md`](API_CONTRACT.md) is the **public** contract — the
> complete method and event index derived from
> `crates/accord-node/src/service/*.rs`, the stability tiers, the security
> position, and the list of methods and events implemented here but not yet
> described below.

## Transport

- WebSocket, JSON **text** messages only (binary ignored).
- Single request (no batching). `id` numeric, string, or `null`.
- Maximum message size: **16 MiB** (`MAX_WS_MESSAGE`,
  `crates/accord-api/src/server.rs`), frames included. Sized so that
  `files.share_bytes` / `files.read` — 8 MiB of bytes, about 11 MiB once
  base64'd inside a JSON envelope — go through in one frame.
- **Message body: 64 KiB** (`MAX_BODY`, `crates/accord-proto/src/core_msg.rs`).
  This is the wire bound on the *encoded* body of a direct message, a group
  message, a group op and a self-sync item, enforced at decode. It is far below
  the frame limit above, and it is the one a client sending text will hit
  first — attachments travel as references, not inside the body.

## Authentication

Mandatory first request of every connection:

```json
{ "jsonrpc": "2.0", "id": 0, "method": "auth", "params": { "token": "<hex64>" } }
```

- Success → `{ "result": { "protocole": 1 } }`.
- Failure → `{ "error": { "code": -32001, "message": "jeton invalide" } }` then
  closes. Any other method before `auth` is rejected and closes the
  connection (10 s maximum to authenticate).
- Constant-time token comparison; the token is never logged.

## Error codes

| Code | Meaning |
|------|------|
| -32700 | Invalid JSON |
| -32600 | Malformed request |
| -32601 | Unknown method |
| -32602 | Invalid parameters |
| -32000 | Node application error |
| -32001 | Token missing or invalid |

## Methods

Identifiers (keys, node_id, msg_id, group_id, channel_id) travel in
**hexadecimal**. Message bodies are decoded on the node side and delivered as
structured JSON (see "Direct messaging"); they are never logged.

### Identity

| Method | Parameters | Result |
|---------|-----------|----------|
| `identity.self` | — | `{ node_id, pubkey, friend_code, name, bio, avatar, banner, pronouns, accent_color, banner_color, avatar_decoration, profile_effect, profile_frame }` |

`name` is the local nickname (`string`), or `null` if it has never been set
via `profile.set` (see "Profile"); `bio` (`string` or `null`), `avatar` and
`banner` (hex-64 hash or `null`) follow the same rules. The decoration fields
(`pronouns` … `profile_frame`) are `null` when unset; colours are integers
`0xRRGGBB`.

🔒 `pubkey`, `node_id` and `friend_code` are those of the **account**, never of
this machine's device key (`MULTI_DEVICE.md` §2). The device key is not exposed
by this method at all — `devices.list` is where machines are named, and its
`is_current` entry is the only place this machine appears.

`identity.self` is the **only** identity RPC method. The lifecycle
(creation, restoration, unlocking) does **not** go through JSON-RPC: these
operations predate the very existence of the node (no port, no token) and
handle secrets (passphrase, recovery phrase) that have no
business on a network channel, even a local one. They go through **Tauri
IPC** (D-023) — the exact contract between `app/src/lib/bridge.ts` and
`app/src-tauri/src/commandes.rs`:

| IPC command | Arguments | Result |
|--------------|-----------|----------|
| `vault_status` | — | `"absent"` ∣ `"locked"` |
| `create_identity` | `{ passphrase }` | `{ session: { port, token }, recovery_phrase }` |
| `restore_identity` | `{ phrase, passphrase }` | `{ port, token }` |
| `unlock` | `{ passphrase }` | `{ port, token }` |
| `lock` | — | `"absent"` ∣ `"locked"` |

⚠️ This table covers the **identity lifecycle only**, not every registered IPC
command. `app/src-tauri/src/lib.rs` is the authoritative list; the others are the
multi-account surface (`accounts_list`, `account_create`, `account_restore`,
`account_adopt_paired`, `account_unlock`, `session_close`), the encrypted backup
(`backup_export`, `backup_import`), the diagnostic log (`journal_ui`,
`journal_dossier`, `journal_niveau` — see `DEV.md`), the microphone permission
(`micro_autorisation_etat`, `micro_autorisation_demander`,
`ouvrir_reglages_systeme`) and `app_quit`. 🔒 `account_adopt_paired` is where a
newly paired machine takes up the account root; like the lifecycle commands it
handles a seed, which is exactly why it is IPC and not JSON-RPC.

Details of shape and behavior:

- `vault_status`: `"absent"` = no vault on disk (the UI offers
  creation or restoration); `"locked"` = a vault exists (the UI asks for the
  passphrase).
- `create_identity` generates the identity (including the PoW), seals it under
  `passphrase`, starts the node and returns the `{ port, token }` session of
  the WebSocket API **plus** `recovery_phrase`, the 12-word phrase — returned
  **only once**, never stored, to be written down immediately.
- `restore_identity` rebuilds the identity from `phrase` (12 BIP39 words),
  seals it under the new local `passphrase`, then starts the node.
- `unlock` opens the existing vault with `passphrase` then starts the node.
- `lock` is the exact inverse of `unlock`: it stops and drops the running
  node (network, API, encrypted database) — the in-memory secrets are wiped
  on that drop — **without** quitting the app, then returns the fresh vault
  status (normally `"locked"`) so the UI lands on the same screen as a cold
  start. Idempotent: calling it with no node running is a no-op.
- `port`/`token` are to be passed as-is to the WebSocket connection then to the
  `auth` method above. Each startup command replaces (and cleanly stops)
  any previous node.
- The three lifecycle commands are asynchronous: the CPU work
  (16-bit PoW, Argon2id) runs on a blocking thread, the window does not freeze.
- Error: the `invoke` promise rejects with a ready-to-display message **in
  French** (e.g. « identité verrouillée ») — no structured error object.
- Outside Tauri (browser development), `bridge.ts` reads a fallback
  session from `localStorage['accord.dev.session']` written by hand from
  the `session.json` of an `accord-noded` daemon; `create_identity` and
  `restore_identity` are unavailable there.

### Profile

| Method | Parameters | Result |
|---------|-----------|----------|
| `profile.get` | — | `{ name, bio, avatar, banner, pronouns, accent_color, banner_color, avatar_decoration, profile_effect, profile_frame }` — nickname (`string`∣`null`), bio (`string`∣`null`), avatar and banner hash (hex 64∣`null`), pronouns (`string`∣`null`), colours (`0xRRGGBB` integer∣`null`), decoration/effect/frame ids (`string`∣`null`) |
| `profile.set` | `{ name?, bio?, pronouns?, accent_color?, banner_color?, avatar_decoration?, profile_effect?, profile_frame? }` | `{}` — at least one field required; an explicit `null` clears that field |
| `profile.set_avatar` | `{ data_b64, mime }` | `{ avatar }` — hex-64 hash of the published blob, or `null` after removal |
| `profile.set_banner` | `{ data_b64, mime }` | `{ banner }` — hex-64 hash of the published blob, or `null` after removal |

`profile.set` validates the nickname (2 to 32 characters once edge whitespace
is trimmed, no control characters) and the bio (at most 2048 characters
after trimming; line breaks and tabs allowed; **empty string = clear**),
stores locally (trimmed forms) then announces the full profile to all
confirmed friends (CORE `PROFILE` message, SPEC §6.5). The decoration fields
follow the same shape: `pronouns` at most 40 characters, the two colours
`0xRRGGBB` integers rejected outright above 24 bits, and the three ids revalidated
(alphabet and bound) before writing. All or nothing: if any submitted field is
invalid, **none** is written. The profile is also announced
automatically on every friendship establishment (in both directions) and
re-announced periodically by maintenance. As long as no nickname is
set, nothing is announced (the `PROFILE` message requires a nickname).

`profile.set_avatar` receives the image bytes as standard base64
(`data_b64`, `=` padding) with its MIME type — `image/png`, `image/jpeg` or
`image/webp` only — and rejects any content exceeding **512 KiB once
decoded**. The bytes are published in the file store
(`files.*`); only the **hash** (Merkle root, hex 64) is stored in the
profile and announced to friends. `{ "data_b64": null }` removes the avatar (returns
`{ "avatar": null }`). A peer's UI retrieves the bytes via `files.read`
with the received hash.

`profile.set_banner` follows exactly the same mechanism as
`profile.set_avatar` (same MIME types, publication in the file
store, announced hash, `{ "data_b64": null }` removes the banner and returns
`{ "banner": null }`), but the banner is a landscape-format image: its
bound is **1 MiB once decoded** (versus 512 KiB for the avatar). A peer's UI
retrieves the bytes via `files.read` with the received hash.

On the receiving side, a **friend**'s profile is persisted (the nickname replaces their
`display_name` in `friends.list`; bio, avatar hash and banner hash
are kept locally) and triggers `event.profile`; an empty bio, a missing
avatar or banner in the announcement **clear** the known value.
If the received avatar or banner hash matches no local blob, the
node starts downloading it in the background from the sender. Announcements
from non-friends are ignored (anti-abuse).

### Devices

The machines of **this** account (design: `MULTI_DEVICE.md`). Not to be confused
with `friends.*`, which is about other people.

| Method | Parameters | Result |
|---------|-----------|----------|
| `devices.list` | — | `{ devices: [{ pubkey, name, added_ms, is_current, last_seen_ms, last_seen_route }] }` — `pubkey` hex-64, `added_ms` epoch ms (`0` for the device produced by the 7.0 migration), `is_current` `bool`, `last_seen_ms` epoch ms of the last time that device was reached **from this machine** (`null` if never), `last_seen_route` `"direct" \| "relay" \| null` |
| `devices.rename` | `{ name }` | `{ name }` — the trimmed name |
| `devices.pair_start` | — | `{ code, expires_ms }` — opens an offer **on the already-authorised device** |
| `devices.pair_submit` | `{ code }` | `{ hello }` — hex PAKE message; entering the code **on the new device** |
| `devices.pair_status` | — | `{ fingerprint, role, adopted }` |
| `devices.pair_confirm` | — | `{}` — the human confirmed the fingerprint on this side |
| `devices.pair_cancel` | — | `{}` |
| `devices.revoke` | `{ pubkey }` | `{}` |
| `devices.transfer_history` | `{ device }` | `{ conversations, pages }` — pulls the whole history from another device of the account. **Long-running** |

- 🔒 `last_seen_ms` / `last_seen_route` are **local observations**, held in this
  machine's database and in no signed structure. They are deliberately absent
  from `DeviceList`, which the account root signs and publishes in the DHT where
  every friend reads it: a per-device last-seen there would publish each
  machine's activity pattern to the whole address book. `last_seen_route` says
  *by which path* the device was reached, and never an address — a tunnelled
  session only knows the **relay's** address (`SessionView::addr`), so an address
  would be both wrong and needlessly revealing.
- ⚠️ `devices.transfer_history` **does not return until the transfer ends**,
  which can take minutes: one page per round trip, per conversation. Callers must
  follow `event.history_transfer` (`{ done, total, messages, complete }`, the
  same shape as `event.file_progress`) rather than waiting on the reply, and
  treat the reply as a final summary.

  🔒 The target must be in the account's **signed** device list; the check is at
  this boundary rather than in the driver, so a later caller cannot skip it.

  🔴 A result of `pages: 0` is **ambiguous by construction**: the requester
  cannot distinguish "that device has nothing older" from "that device runs a
  version that does not know opcode `0x23` and dropped the request". Interfaces
  must surface both possibilities rather than reporting a clean finish. See
  `docs/MULTI_DEVICE.md` §7.

- `devices.rename` bounds the name at **32 UTF-8 bytes**, which is the wire bound
  (`MAX_DEVICE_NAME`), not 32 characters. Counting characters looks stricter and
  is looser: "é" weighs two bytes, so 32 accented characters would pass here and
  be refused at decode — a setting that looks accepted and never takes.
- **Pairing is symmetric.** SPAKE2 has no server side: one machine displays a
  code (`pair_start`), the other types it (`pair_submit`), and both then sit in
  the same state waiting for the other's PAKE message. `pair_submit` also
  broadcasts on the local network — the new device holds the code and nothing
  else, no session and no address, and the code carries neither; the PAKE fails
  silently for anyone without it, so greeting everyone tells nobody anything.
- `pair_status.fingerprint` is `null` until an exchange has succeeded (the screen
  shows the code and waits rather than inventing a fingerprint). `role` is
  `"authoriser"` ∣ `"joiner"` ∣ `null` — the two sides do not display the same
  screen. `adopted` reports that the account root has arrived.
- 🔒 **`adopted` is a boolean and never the seed.** The host picks the root up
  through `Node::pairing_take_adoption`, which does not go through this API:
  nothing crossing this local JSON channel may contain the account, because it is
  readable by anything running on the machine and it ends up in the traces of
  whoever is debugging. Same rule as the identity lifecycle above (D-023).
- Requesting a new code cancels the previous one, and typing a new one replaces
  the pairing in progress. Someone who clicks again wants to start over, not to
  hold two valid codes.
- `devices.revoke` refuses the device it is called from: that would leave the
  account with no machine able to sign the next list. Revocation is eventually
  consistent — `SECURITY.md` §5, item 13, says what it does and does not buy.
- Event: `event.pairing_adopted` `{}` — this machine has just received the
  account root and is now a full device of the account.

### Friends

| Method | Parameters | Result |
|---------|-----------|----------|
| `friends.list` | — | `{ contacts: [{ node_id, pubkey, friend_code, display_name, bio, avatar, banner, state, last_seen_ms, online, status, status_text, unread, mention_count, note, verified, key_changed }] }` — `bio` `string`∣`null`, `avatar` and `banner` hex-64 hash∣`null` (profile announced by the peer, D-027, D-032); `online` `bool` (kept for backward compatibility) plus `status` ∈ `online`∣`idle`∣`dnd`∣`offline` and `status_text` `string`∣`null` (rich presence, best-effort, see "Presence"); `unread` integer (messages from the peer received after our `dm.mark_read`); `mention_count` integer (unread mentions in this DM, see "Mentions"); `note` `string`∣`null` (private local-only note, see "Private notes"); `verified` and `key_changed` booleans (manual identity verification, see "Safety numbers") |
| `friends.resolve` | `{ friend_code }` | `{ pubkey }` — DHT lookup of the identity record, verified end-to-end |
| `friends.request` | `{ pubkey, display_name }` | `{ ok: true }` |
| `friends.respond` | `{ pubkey, accept }` | `{ ok: true }` |
| `friends.set_note` | `{ pubkey, note }` | `{ ok: true }` — private, **local-only** note attached to a contact (see "Private notes"); `note` ≤ 4096 characters, trimmed; an empty note clears it. Never sent anywhere |
| `friends.get_note` | `{ pubkey }` | `{ note }` — `string`∣`null` |
| `friends.block` | `{ pubkey }` | `{ ok: true }` |
| `friends.unblock` | `{ pubkey }` | `{ ok: true }` |
| `friends.remove` | `{ pubkey }` | `{ ok: true }` — removes an **established** friendship (explicit error otherwise). Distinct from a block: the DM history is kept and a new friend request stays possible. The peer is notified best-effort (`FRIEND_REMOVE`, never queued offline) and drops the friendship on receipt; both sides receive `event.friend_removed` |
| `friends.set_status` | `{ status, custom? }` | `{ ok: true }` — own rich presence: `status` ∈ `online`∣`idle`∣`dnd`∣`invisible`; `custom` string ≤ 256 UTF-8 bytes, no control characters (absent = unchanged, empty after trim = cleared). Persisted (meta table), broadcast to friends immediately then in the periodic announcements. `invisible` is announced as plain offline (no custom text leaks) while the node keeps working normally |
| `friends.get_status` | — | `{ status, custom }` — persisted own presence; defaults to `online` with `custom: null` |
| `friends.safety_number` | `{ pubkey }` | `{ digits, emoji, verified, key_changed }` — anti-MITM safety number of the conversation (see "Safety numbers"): `digits` 60 ASCII digits (display as 12 groups of 5), `emoji` 8 symbols from a fixed table, `verified` bool, `key_changed` bool (`true` when the contact was verified against a different key than the current one) |
| `friends.set_verified` | `{ pubkey, verified }` | `{ ok: true }` — marks/unmarks the contact as manually verified; the pubkey seen NOW is stored with the flag. Emits `event.friend_verified`. Rejected for an unknown contact |

`state` ∈ `pending_out`, `pending_in`, `friend`, `blocked`. The "add
a friend by code" flow is `friends.resolve` then `friends.request`.

`display_name` is the last nickname announced by the peer (`PROFILE` message,
see "Profile"); failing that, the label given to `friends.request` or the name
carried by their friend request.

#### Safety numbers

`friends.safety_number` derives a Signal-style **safety number** from the two
Ed25519 identity public keys (ours + the contact's), entirely **locally** —
no network exchange, no wire byte. Both keys are ordered lexicographically
before derivation, so both peers display **exactly the same number**; each key
is reduced by ~5200 rounds of SHA-512 into 30 bytes → 6 groups of 5 digits,
concatenated into 60 digits. Two friends compare the number (or the 8-emoji
rendering) out of band: a match proves no man-in-the-middle substituted a key.

`friends.set_verified` persists the outcome per contact together with the
public key seen at that moment. If that key ever differs from the contact's
current key, `key_changed: true` is reported by `friends.safety_number` and
`friends.list` — the UI should warn that the verification is broken and
prompt to re-verify. Verification state is local-only and survives profile
refreshes.

#### Private notes

`friends.set_note` / `friends.get_note` attach a free-text note to a contact
(keyed by public key). The note is **purely local**: it is stored in the
encrypted local database (`contact_notes` table) and **never** travels on the
wire — no protocol message carries it. It exists for any public key, even one
that is not (yet) a contact. Bound: 4096 characters (trimmed); writing an empty
note deletes it. The current note is also folded into `friends.list` (`note`).

### Direct messaging

| Method | Parameters | Result |
|---------|-----------|----------|
| `dm.send` | `{ pubkey, text, reply_to?, attachments? }` | `{ msg_id }` |
| `dm.history` | `{ pubkey, before_lamport?, limit? }` | `{ messages: [...], peer_read_lamport }` — `peer_read_lamport` integer∣`null`: lamport of the last own message covered by the peer's read receipt (`null` if unknown; see `dm.mark_read`) |
| `dm.history_around` | `{ pubkey, msg_id, limit? }` | `{ messages: [...], found, peer_read_lamport }` — window centered on `msg_id`: up to `limit/2` older messages, the target, then up to `limit/2` newer, newest-first (jump-to-message). `found: false` with an empty `messages` when `msg_id` is unknown locally |
| `dm.pin` | `{ pubkey, msg_id }` | `{ ok: true }` — local pin (no wire op); the message must be known in this conversation |
| `dm.unpin` | `{ pubkey, msg_id }` | `{ ok: true }` |
| `dm.pins` | `{ pubkey }` | `{ msg_ids: [msg_id] }` — pinned messages of the conversation (by id) |
| `dm.edit` | `{ pubkey, msg_id, text }` | `{ ok: true }` — author only, rejected otherwise |
| `dm.delete` | `{ pubkey, msg_id }` | `{ ok: true }` — author only; immediate local tombstone (also unpins) |
| `dm.retry` | `{ pubkey, msg_id }` | `{ ok: true }` — re-attempts one of our unacked messages (`delivery` `pending`/`failed`); resets the offline-queue backoff. Rejected if the message is unknown, not ours, deleted, or already delivered |
| `dm.react` | `{ pubkey, msg_id, emoji, remove? }` | `{ ok: true }` — `remove: true` removes the reaction |
| `dm.typing` | `{ pubkey }` | `{ ok: true }` — **ephemeral** typing indicator: emitted only if the peer is presumed online, never persisted or queued (unreachable peer ⇒ silently ignored). When received, it triggers `event.dm_typing` |
| `dm.mark_read` | `{ pubkey, lamport }` | `{ ok: true }` — records our local read position in the conversation (for `unread` in `friends.list`). When the mark **advances**, best-effort emission of a read receipt to the peer (**ephemeral** like `dm.typing`: online peers only, never queued offline, silent if the privacy setting is off). When received, the peer's read position is persisted and `event.dm_read` is pushed |
| `dm.set_read_receipts` | `{ enabled }` | `{ ok: true }` — privacy setting (persisted, default on): when off, no read receipt is ever emitted; **incoming** receipts are still recorded |
| `dm.get_read_receipts` | — | `{ enabled }` |
| `dm.set_ephemeral` | `{ pubkey, ttl_secs? }` | `{ ok: true }` — **local** disappearing-message timer for this conversation (see "Disappearing messages"); `ttl_secs` integer in [60, 31 536 000], `null` or absent disables |
| `dm.ephemeral` | `{ pubkey }` | `{ ttl_secs }` — integer∣`null` (`null` = disabled) |
| `dm.schedule` | `{ pubkey, body, fire_at }` | `{ id }` — schedules a **local** deferred send (see "Planning"); `fire_at` wall-clock ms |

`limit` bounded to [1, 200] (default 50). `messages` sorted from most recent to
oldest: `{ msg_id, author, lamport, sent_ms, acked, deleted, pinned, delivery,
mentions_me, body, edited, reactions, attachments }`. `body` is decoded on the node side into structured JSON:
`{ type: "text", text, reply_to, attachments }` ∣ `{ type: "edit"|"delete"|"reaction", ... }`
∣ `{ type: "meta" }` ∣ `{ type: "unknown" }`. Shape details:

- `reply_to` is **always emitted** in a `text` body, nullable (`null` if
  the message is not a reply).
- In the **body**, `attachments` is a **counter** (number of
  attachments); the detailed list lives at the **envelope** level (see
  "Attachments" below).
- A deleted message keeps its envelope (`msg_id`, `author`, …) with
  `deleted: true` and a body rendered as `{ type: "unknown" }` (the body is
  erased locally, it is never retransmitted); its attachments are
  erased too.
- `edited` is the last edited text (string) or `null`; the `body` keeps
  the original text.
- `reactions` is always present: `[{ emoji, author }]` (one entry per
  emoji × author pair), `[]` if none.
- `pinned` is a boolean: `true` when the message is pinned in this
  conversation (see `dm.pin`/`dm.pins`). DM pins are a **local view** — no
  wire op, stored in a local `dm_pins` table, never synchronized to the peer.
- `delivery` is the delivery state of one of **our** outgoing messages:
  `"sent"` once the peer acks it, `"pending"` while in flight or being retried,
  `"failed"` when direct retries are exhausted (or the message is unacked, no
  longer queued, and older than the 7-day offline-queue expiry). Incoming
  messages (`author` = the peer) always report `"sent"`. `failed` is a UI hint,
  not terminal: the offline queue keeps retrying until expiry, and `dm.retry`
  forces an immediate re-attempt.
- `mentions_me` is a boolean: `true` when this message mentions the local user.
  Detection is **local and passive** at ingestion (the wire carries no mention
  metadata; see "Mentions"). Present on both `dm.history` and `groups.history`
  messages.

`dm.edit`, `dm.delete` and `dm.react` apply the action locally then
emit it to the peer over the same path as `dm.send` (direct send or
offline queue). On ingestion at the peer, the action triggers
`event.dm`. Group messages (`groups.history`) follow the same schema,
plus `channel_id`, without `acked`, `pinned` or `delivery` (group pins live in
the op-log; see `groups.pins`).

#### Disappearing messages (local)

`dm.set_ephemeral` / `groups.set_ephemeral` arm a per-conversation timer:
messages older than `ttl_secs` are periodically deleted **from this device's
encrypted store only** (first purge shortly after startup, then every few
minutes; bounded work per pass). Deletion is complete: the message row plus
its attachments references, reactions, local pins, mention-inbox entries and
search-index tokens.

This is **purely local** — no control message, no wire negotiation, zero
wire byte: the peer's device keeps its own copy unless it arms its own
timer (a bilaterally negotiated variant would be a future wire extension,
out of scope here). The setting itself is persisted locally
(`conversation_ephemeral` table) and never leaves the device.

#### Attachments

`dm.send` and `groups.send` accept `attachments`: a list (10 at most)
of references to files **already published** in the local store
(`files.*` domain), each of the form:

```json
{ "merkle_root": "<hex64>", "name": "photo.png", "size": 2048, "mime": "image/png" }
```

Bounds: `name` 1-256 bytes, `mime` 1-256 bytes, `size` from 1 byte to 2 GiB.
A message may have only attachments (`text` empty). On retrieval
(`dm.history`, `groups.history`, `event.dm`, `event.group_msg`), the envelope
carries `attachments: [{ merkle_root, name, size, mime }]` (always present,
`[]` if none); the recipient retrieves the bytes from peers via the
`files.*` domain with `merkle_root`.

### Groups

Every management action emits a **signed op** in the group's replicated
op-log. The caller's permissions are checked **before emission** by
replaying the op on the materialized state (same rules as on ingestion at the
peers): an unauthorized action returns an application error "denied: …".
After each applied op (local or remote), the node emits
`event.group_state { group_id }` — the UI then reloads `groups.state`.

| Method | Parameters | Result |
|---------|-----------|----------|
| `groups.create` | `{ name }` | `{ group_id }` |
| `groups.list` | — | `{ groups: [group_id], unread, mentions }` — `unread`: `{ group_id: { channel_id: n } }`, unread per channel (others' messages after the `groups.mark_read` mark), **AutoMod-masked messages deducted** (see "AutoMod"); only channels with at least one unread appear. `mentions`: `{ group_id: n }`, unread mentions per group (all channels combined); only groups with at least one appear |
| `groups.state` | `{ group_id, channel_id? }` | full state, see below — with `channel_id`, `my_permissions` becomes the **effective** bitfield in that channel (overrides folded in, `deny` > `allow`). `members` is the **whole** list, always; see `groups.members` to read it in slices |
| `groups.members` | `{ group_id, offset?, limit? }` | `{ members: [...], total }` — one bounded slice of the same member list `groups.state` returns, in the same order and with the same per-member object. `offset` (default 0) counts members from the start of that order; `limit` bounded to [1, **200**] (default 50), out-of-range values are clamped rather than refused. `total` is the size of the **whole** list, not of the page, so a caller knows when to stop. An `offset` at or past `total` is an empty page and the right `total` — an end of list, not an error. No permission gate: `groups.state` already exposes the same data to any member |
| `groups.rename` | `{ group_id, name }` | `{ ok: true }` — 1-100 characters |
| `groups.set_icon` | `{ group_id, data_b64, mime }` | `{ icon }` — image ≤ 512 KiB decoded, published in the file store; `icon` = hex-64 Merkle root |
| `groups.set_topic` | `{ group_id, channel_id, topic }` | `{ ok: true }` — ≤ 2048 bytes |
| `groups.channel.add` | `{ group_id, name, kind?, category? }` | `{ channel_id }` — `kind` ∈ `"text"` (default), `"voice"`, `"announcement"`; `category` = hex id of an existing category. An `announcement` channel is **read-only**: only members with the effective `MANAGE_CHANNELS` there may post (everyone else reads), enforced at compose and ingest |
| `groups.channel.edit` | `{ group_id, channel_id, name?, position?, category? }` | `{ ok: true }` — absent field = unchanged; `category`: `null` moves the channel out of any category, hex id of an existing category moves it there (`SetChannelCategory` op, `MANAGE_CHANNELS`) |
| `groups.channel.perms` | `{ group_id, channel_id, role_id, allow, deny }` | `{ ok: true }` — per-channel role override (`SetChannelPerms` op, `MANAGE_ROLES`): `allow`/`deny` permission bitfields, `deny` wins; overlapping or unknown bits = explicit error; `allow = deny = 0` clears the override (full inherit) |
| `groups.channel.del` | `{ group_id, channel_id }` | `{ ok: true }` |
| `groups.category.add` | `{ group_id, name, position? }` | `{ category_id }` |
| `groups.category.edit` | `{ group_id, category_id, name?, position? }` | `{ ok: true }` — absent field = unchanged (`EditCategory` op, `MANAGE_CHANNELS`) |
| `groups.category.del` | `{ group_id, category_id }` | `{ ok: true }` — deletes the category **only**: its channels remain, uncategorized (`DelCategory` op, `MANAGE_CHANNELS`) |
| `groups.audit` | `{ group_id, before?, limit? }` | `{ entries: [{ op_id, lamport, wall_ms, author, kind, params }] }` — read-only audit log (the signed op-log decoded), newest first. The `ADMIN`/founder gate here is a **UX gate, not a confidentiality boundary**: the op-log is replicated to every member for CRDT state folding (`GroupSyncPull` + real-time `GroupOpMsg`), so any member already holds this data locally. Do not rely on the gate to hide op contents from members. `limit` bounded to [1, 100] (default 50); `before` = `op_id` of the oldest entry already loaded (cursor, unknown = explicit error). `author` = hex-64 public key of the actor; `kind` = stable label (`create`, `add_channel`, `kick`, …, `unknown` for an undecodable body); `params` = the human-relevant fields of the op (`name`, `member`, `channel_id`, `role_id`, …), never the raw wire |
| `groups.kick` | `{ group_id, pubkey }` | `{ ok: true }` — hierarchy: you cannot kick a member of higher or equal role; the founder is untouchable |
| `groups.ban` | `{ group_id, pubkey }` | `{ ok: true }` — same rules; a banned member can no longer be (re)admitted |
| `groups.unban` | `{ group_id, pubkey }` | `{ ok: true }` |
| `groups.timeout` | `{ group_id, pubkey, until_ms }` | `{ ok: true }` — temporary mute (`KICK` permission + kick hierarchy: the founder is untouchable, you cannot time out a member of higher or equal role). The member stays in the group but cannot send messages while `until_ms` (wall ms deadline) is in the future; enforced at compose **and** ingest. `until_ms = 0` lifts it (same as `groups.timeout_clear`). Surfaced as `timeout_until_ms` per member in `groups.state` |
| `groups.timeout_clear` | `{ group_id, pubkey }` | `{ ok: true }` — lifts a member's timeout |
| `groups.set_nickname` | `{ group_id, name, member? }` | `{ ok: true }` — per-server display name. `member` absent = self; a member may set their own, a `MANAGE_ROLES` moderator may set/clear anyone strictly below them (founder untouchable). `name` trimmed to 1-32 characters without control characters; empty clears. Surfaced as `nickname` per member in `groups.state` |
| `groups.leave` | `{ group_id }` | `{ ok: true }` — refused to the founder as long as other members remain |
| `groups.role.add` | `{ group_id, name, color, permissions, position? }` | `{ role_id }` — `color` RGB (`0xRRGGBB`), `permissions` bitfield (see table) |
| `groups.role.edit` | `{ group_id, role_id, name?, color?, position?, permissions? }` | `{ ok: true }` — absent field = unchanged; you cannot modify a role of higher or equal position than your own |
| `groups.role.del` | `{ group_id, role_id }` | `{ ok: true }` — removed from all members and overrides |
| `groups.role.assign` | `{ group_id, role_id, pubkey }` | `{ ok: true }` |
| `groups.role.unassign` | `{ group_id, role_id, pubkey }` | `{ ok: true }` |
| `groups.pin` | `{ group_id, channel_id, msg_id }` | `{ ok: true }` — `MANAGE_MESSAGES` permission; the message must be known locally |
| `groups.unpin` | `{ group_id, channel_id, msg_id }` | `{ ok: true }` |
| `groups.pins` | `{ group_id, channel_id }` | `{ msg_ids: [msg_id] }` |
| `groups.history` | `{ group_id, channel_id, before_lamport?, limit? }` | `{ messages: [...] }` — same schema as `dm.history`, plus `channel_id`, without `acked` |
| `groups.history_around` | `{ group_id, channel_id, msg_id, limit? }` | `{ messages: [...], found }` — window centered on `msg_id` (jump-to-message), same message schema as `groups.history`; `found: false` with empty `messages` when `msg_id` is unknown in this channel |
| `groups.send` | `{ group_id, channel_id, text, reply_to?, attachments? }` | `{ msg_id }` — encrypted with the group key, broadcast to members; `reply_to` (hex 32) quotes a message and is returned in `groups.history` (`text` body, same shape as DMs) |
| `groups.edit` | `{ group_id, channel_id, msg_id, text }` | `{ ok: true }` — author only |
| `groups.delete` | `{ group_id, channel_id, msg_id }` | `{ ok: true }` — our message: tombstone broadcast to members; someone else's message: signed moderation op (`MANAGE_MESSAGES` required) |
| `groups.react` | `{ group_id, channel_id, msg_id, emoji, add? }` | `{ ok: true }` — `add` (default `true`); `false` removes the reaction |
| `groups.invite` | `{ group_id, pubkey }` | `{ ok: true }` — `AddMember` op + op-log replay + sealed key sent to the invitee |
| `groups.emoji.add` | `{ group_id, name, data_b64, mime }` | `{ merkle_root }` — `MANAGE_EMOJIS` permission; `name` 2-32 characters `[a-z0-9_]` (replacement allowed on an existing name); image ≤ 256 KiB decoded, `mime` ∈ `image/png`, `image/jpeg`, `image/webp`, `image/gif`; published in the file store then `AddEmoji` op; at most 50 emojis per server |
| `groups.emoji.del` | `{ group_id, name }` | `{ ok: true }` — `MANAGE_EMOJIS` permission |
| `groups.typing` | `{ group_id, channel_id }` | `{ ok: true }` — **ephemeral** typing indicator, broadcast only to members presumed online (never persisted or queued); when received, it triggers `event.group_typing` |
| `groups.mark_read` | `{ group_id, channel_id, lamport }` | `{ ok: true }` — records our local read position in the channel (for `unread` in `groups.list`) |
| `groups.automod.set` | `{ group_id, words }` | `{ ok: true }` — replaces the server's filtered-word list wholesale (`SetAutoModWords` op, `MANAGE_CHANNELS`). At most **50** words, each 1-32 characters, lowercased by the node, no control or spoofing characters; one malformed word rejects the whole call (never a partial replacement). See "AutoMod" below |
| `groups.automod.get` | `{ group_id }` | `{ words: [...] }` — the current list (also surfaced as `automod_words` in `groups.state`) |
| `groups.set_ephemeral` | `{ group_id, ttl_secs? }` | `{ ok: true }` — **local** disappearing-message timer for the whole group (every channel); same contract as `dm.set_ephemeral` (see "Disappearing messages") |
| `groups.ephemeral` | `{ group_id }` | `{ ttl_secs }` — integer∣`null` |
| `groups.schedule` | `{ group_id, channel_id, body, fire_at }` | `{ id }` — schedules a **local** deferred send in a channel (see "Planning") |

`groups.edit`, `groups.delete` (of one's own message) and `groups.react`
travel as bodies encrypted with the group key, over the same path
as `groups.send`; on ingestion at each member, the action is applied
(author verified) and `event.group_msg` is emitted.

#### AutoMod

A server's filtered-word list (`groups.automod.set`, `MANAGE_CHANNELS`) lives
in the replicated signed op-log and is surfaced as `automod_words` in
`groups.state`. It is a **display convention between honest clients**, not
enforcement: nothing is deleted, nothing is blocked at send time, the list
itself travels in the clear to every member, and a modified client always sees
the full text. Treat it as clutter reduction, never as a safety boundary.

What a matching message does on a compliant client:

- its occurrences are replaced by `█` at render time (`app/src/lib/automod.ts`);
- it raises **no** native notification and plays **no** sound;
- it is **deducted from the `unread` counters** of `groups.list`.

The last point is why the node also matches: a message whose word is masked
but whose red badge still lights up points straight at what the filter was
meant to hide. The node applies the same rule as the UI
(`crates/accord-core/src/automod.rs`), and re-evaluates it on every count —
removing a word makes the affected messages count again, without reindexing.

Matching is **case- and accent-insensitive** and bounded to **whole words**
(Unicode `Alphabetic`, `N` and `_` are word characters): a filter on `con`
does not mask `concert`. Both precomposed (`é`) and decomposed (`e` + U+0301)
forms match, so the outcome does not depend on the sender's keyboard. Only
message text (and its latest edit) is matched: polls, stickers and attachment
names are not masked at render time, so they are not deducted either.

Per-channel unread deduction scans at most 500 unread messages per channel;
beyond that the surplus counts as unmasked (the badge can then be too high,
never too low).

#### Shape of `groups.state`

```json
{
  "group_id": "<hex32>",
  "name": "Guilde",
  "icon": "<hex64>" ,          // Merkle root of the icon, or null
  "founder": "<hex64>",        // public key, or null
  "members": [{ "pubkey": "<hex64>", "roles": ["<role_id>"],
                "nickname": "Capitaine"∣null,   // per-server display name
                "avatar": "<hex64>"∣null,        // per-server avatar, Merkle root
                "timeout_until_ms": 0,           // active mute deadline (wall ms), 0 = none
                "voice_muted": false,            // server-side voice moderation
                "voice_deafened": false }],
  "bans": ["<hex64>"],
  "channels": [{ "channel_id": "<hex32>", "name": "général", "kind": "text",
                 "category": "<hex32>"∣null, "position": 0, "topic": "" }],
  "categories": [{ "category_id": "<hex32>", "name": "Vocaux", "position": 0 }],
  "roles": [{ "role_id": "<hex32>", "name": "Modo", "color": 16711680,
              "position": 5, "permissions": 96 }],
  "invites": [{ "invite_id": "<hex32>", "max_uses": 0, "uses": 0,
                "expires_ms": 0, "revoked": false }],
  "emojis": [{ "name": "parrot", "merkle_root": "<hex64>" }],  // server emojis
  "overrides": [{ "channel_id": "<hex32>", "role_id": "<hex32>",
                  "allow": 0, "deny": 2 }],   // per-channel role overrides
  "my_permissions": 1023       // effective bitfield of the local identity
}
```

`members`: stable order (ascending by `pubkey`), the same order `groups.members`
pages through.

#### Reading members in pages (`groups.members`)

`groups.state` returns the **entire** member list on every call, and it always
will — nothing about this method changes for a client that ignores
`groups.members`. That is also its cost: at 500 members the reply is
**115.8 KiB of JSON**, and the node emits `event.group_state` after every op it
applies, so a client that reloads the state on that event re-reads the whole
list each time (`docs/PERFORMANCE.md` §3.1 and §3.2).

`groups.members` returns one bounded slice of that same list:

```json
// groups.members { "group_id": "<hex32>", "offset": 100, "limit": 50 }
{
  "members": [ /* same objects as groups.state.members, same order */ ],
  "total": 500                 // size of the whole list, not of the page
}
```

- **The member objects are the same objects** — the node builds both through one
  function, so a client that already decodes `groups.state.members` needs no
  second decoder.
- **The order is the same** and stable (ascending by `pubkey`), so concatenating
  the pages of a group whose membership did not change in the meantime yields
  exactly `groups.state.members`.
- `limit` is bounded to **[1, 200]** (default 50) and out-of-range values are
  clamped, never refused. `limit: 0` yields one member, not zero.
- An `offset` at or past `total` is an empty page with a correct `total`, not an
  error: a client paging while someone leaves the server must not get a failure
  for it.

**When to prefer it.** Use `groups.members` for anything that displays members —
a member sidebar, a mention picker, a moderation list — on servers that may grow
past a few dozen people. Keep `groups.state` for everything else it carries
(channels, roles, categories, invites, permissions); it remains the way to read
a server's structure.

⚠️ **What paging does not buy.** It bounds the *reply*, not the node's work: the
group state is folded in full either way (that fold is memoised per database
handle and invalidated by every incoming op — `docs/PERFORMANCE.md` §3.4). The
saving is serialised bytes and client-side work, which is what was measured to
be unbounded here; it is not a fix for the fold.

`emojis`: stable order (lexicographic by `name`). A custom emoji is written
`:name:` in a message's text and `":name:"` as a reaction value
(`groups.react` / `dm.react`); rendering (loading the image via the
`files.*` domain with `merkle_root`) is the UI's responsibility — no wire
impact, they are ordinary strings.

#### Permission bits (`permissions`, `my_permissions`)

| Name | Value | Meaning |
|-----|--------|------|
| `VIEW` | `0x1` | view the channel |
| `SEND` | `0x2` | send messages |
| `MANAGE_MESSAGES` | `0x4` | delete/pin messages |
| `MANAGE_CHANNELS` | `0x8` | manage channels, categories, metadata, topics |
| `INVITE` | `0x10` | invite members |
| `KICK` | `0x20` | kick |
| `BAN` | `0x40` | ban/reinstate |
| `MANAGE_ROLES` | `0x80` | manage roles and channel overrides |
| `ADMIN` | `0x100` | implies all permissions |
| `MANAGE_EMOJIS` | `0x200` | add/remove server emojis |

Every member implicitly has `VIEW | SEND` (removable by channel override,
`deny` takes priority over `allow`). The founder has all permissions.
Without `channel_id`, `my_permissions` is the **global** bitfield of the
local identity; with `channel_id` (see `groups.state`), the per-channel
overrides are folded in (`deny` > `allow`; `ADMIN` and the founder
short-circuit). Sending into a channel requires the effective `VIEW | SEND`
there — a role denied `VIEW` on a channel cannot write to it either.

### Search

| Method | Parameters | Result |
|---------|-----------|----------|
| `search.query` | `{ query }` | `{ msg_ids: [msg_id], hits: [...] }` |

Blind local search (HMAC word index); plain words are an intersection of all
words. The query string also accepts **filter tokens**, parsed and resolved
node-side, applied to the candidate messages before returning:

| Token | Meaning |
|-------|---------|
| `from:<name-or-code>` | author is a contact whose display name (fragment, case-insensitive) or friend code matches; `from:me` (or `from:moi`) is our own identity |
| `in:<name>` | conversation is a contact DM (by name), or a group channel (by channel name, or all channels of a group whose name matches) |
| `has:link` | the message text contains a URL (`http://` / `https://`) |
| `has:image` | at least one `image/*` attachment |
| `has:file` | at least one attachment (any kind) |
| `before:<date>` | `sent_ms` strictly before the resolved instant |
| `after:<date>` | `sent_ms` at or after the resolved instant |

`<date>` is an ISO `YYYY-MM-DD` (midnight UTC), the keyword `today`/`yesterday`,
or a relative offset `Nd` / `Nh` / `Nm` / `Nw` counted back from now. Multiple
`from:`/`in:` operands widen (OR within a kind); different filter kinds narrow
(AND). A filter that resolves to nothing (unknown contact/conversation, or an
unreadable date) — the date filter is simply skipped; an unresolved
`from:`/`in:` matches no message. Unknown `has:` values and empty operands are
ignored. Plain-word search keeps working unchanged.

Each entry of `hits` carries per-hit metadata (recent first, capped at 200):

```json
{
  "msg_id": "<hex32>",
  "author": "<hex64>",
  "lamport": 42,
  "timestamp": 1710000000000,
  "conversation": { "type": "dm", "peer": "<hex64>" }
}
```

`conversation` is `{ "type": "dm", "peer" }` or `{ "type": "group", "group_id",
"channel_id" }` — enough to render a result and jump to it via
`dm.history_around` / `groups.history_around`. `msg_ids` mirrors the `hits`
ids in the same order (backward compatibility). With filters but no plain word,
candidates are drawn from the most recent messages (bounded).

### Mentions

Mention awareness is **local and passive**. A group or direct message carries
**no** mention metadata on the wire — the text simply contains the literal
`@…` typed by the sender. On ingestion, the node decides whether the **local**
user is targeted by matching the message text (case-insensitive, word-bounded)
against:

- the local **nickname** (`profile.set`), if set;
- the local **friend code**;
- the special tokens **`@everyone`** and **`@here`** (treated identically:
  effective presence is not knowable server-side in a P2P network, so `@here`
  is detected exactly like `@everyone`);
- the names of the **roles** the local user holds in that group (group
  messages only).

A matching message sets `mentions_me: true` in history (`dm.history`,
`groups.history`) and creates **one** entry (deduplicated per message) in a
local **mention inbox**. Detection is purely a social/UX signal: spamming
`@everyone` is a social problem, not a permission one — no permission is
required to be detected. Nothing here is transmitted; the inbox lives only in
the local database.

| Method | Parameters | Result |
|---------|-----------|----------|
| `mentions.inbox` | `{ before?, limit? }` | `{ entries: [{ msg_id, conversation, author, ts_ms, lamport, snippet, read }] }` — newest first; `before` paginates by wall-clock ms (entries strictly older), `limit` bounded to [1, 200] (default 50) |
| `mentions.mark_read` | `{ msg_ids? }` | `{ ok: true, marked }` — marks the given messages read; **absent `msg_ids` marks all** as read. `marked` = number of entries actually flipped to read |

`conversation` is `{ "kind": "dm", "peer" }` or `{ "kind": "group",
"group_id", "channel_id" }` — enough to render and jump to the message via
`dm.history_around` / `groups.history_around`. `snippet` is a bounded excerpt
of the message text (never the full body). A message that is deleted (locally
or by moderation) loses its inbox entry. Per-conversation unread mention
counts are exposed in `friends.list` (`mention_count`) and `groups.list`
(`mentions`).

### Files

| Method | Parameters | Result |
|---------|-----------|----------|
| `files.share` | `{ path }` | `{ file: { merkle_root, name, size, mime } }` — copy into the store, signed manifest |
| `files.share_bytes` | `{ name, mime, data_b64 }` | `{ file }` — publication of bytes provided by the UI (standard base64, bounded to 8 MiB decoded; beyond that, `files.share` with a path) |
| `files.read` | `{ merkle_root, hint? }` | `{ data_b64, name, mime, size }` if complete locally; `{ pending: true }` otherwise (download triggered) |
| `files.status` | `{ merkle_root, hint? }` | `{ known, complete, done, total, name?, size?, mime? }` |
| `files.save` | `{ merkle_root, path }` | `{ ok: true }` — copy of the complete blob to `path` |

- `merkle_root`: Merkle root of the file in hexadecimal (64 characters),
  the content identifier across the whole network (that of the `attachments`).
- `files.share` bounds the size to 2 GiB and guesses `mime` from the extension;
  republishing the same content is idempotent (same root).
- `files.read` is bounded to **8 MiB**: beyond that, an outright refusal with a
  clear error — use `files.save`. If the file is not (yet) complete
  locally, the read returns `{ pending: true }` and triggers the download:
  the UI follows `event.file_progress` then calls `files.read` again.
- `hint` (optional): public key (hex) of a probable source peer —
  typically the sender of the message carrying the attachment. The other
  sources are the connected peers that hold the content.
- `done`/`total` count the 256 KiB blocks; `name`, `size` and `mime` are
  present only if the manifest is known (`known: true`).
- Resumption: progress is persisted (bitmap); an interrupted
  download resumes on restart without re-downloading the held blocks.
  Without durable progress, the transfer is abandoned cleanly (last
  `event.file_progress` with `complete: false`).

### Voice channels

> **Frozen** contract (D-025): signatures and notifications implemented to the
> letter on both sides, no divergence allowed.

| Method | Parameters | Result |
|---------|-----------|----------|
| `voice.join` | `{ group_id, channel_id }` | `{ participants: [pubkey] }` — joins the channel; a single active channel at a time (`join` leaves the previous one implicitly) |
| `voice.leave` | — | `{}` |
| `voice.mute` | `{ muted }` | `{}` — mutes/unmutes the local capture, you stay in the channel; while deafened the mute stays forced and the requested state is restored on undeafen |
| `voice.deafen` | `{ on }` | `{}` — stops (`true`) or restores (`false`) decoding/playing **all** incoming voice locally; deafen forces mute, undeafen restores the previously requested mute state (Discord semantics); session-scoped (never persisted); idempotent, no effect outside a channel |
| `voice.set_volume` | `{ peer?, volume }` | `{}` — output volume in percent (integer 0..=200, 100 = unity, > 100 = boost with saturation); `peer` absent = **master** output volume, otherwise the hex public key of a participant; persisted (per peer public key) and applied live as a linear gain on the decoded PCM; out-of-range volume or malformed `peer` = explicit error |
| `voice.status` | — | `{ active: null ∣ { group_id, channel_id, muted, deafened, participants: [{ pubkey, speaking, muted, deafened, volume }] }, master_volume }` — participant `muted`/`deafened` reflect the state broadcast in their `VoiceSignal`; `volume` is the local persisted per-peer volume; `master_volume` is returned even without an active channel |
| `voice.devices` | — | `{ inputs: [string], outputs: [string], selected_input: string∣null, selected_output: string∣null }` — `cpal` names; `null` = default device (D-029) |
| `voice.set_devices` | `{ input?: string∣null, output?: string∣null }` | `{}` — absent field = unchanged, `null` = default device; persisted; applied on the fly if a channel is active; unknown name = explicit error |
| `voice.mic_test` | `{ enabled }` | `{}` — while enabled, `event.voice_level` at ~10 Hz from the real capture; explicit error if the audio hardware is unavailable |

Details of shape and behavior:

- **UI convention**: each group has **one** default voice channel, identified
  by `channel_id == group_id`. The node treats `channel_id` as an opaque
  key (no channel existence check).
- `participants` (in `voice.join` as well as in `voice.status`) includes
  **oneself**.
- **Cap of 10 participants** (full mesh): `voice.join` beyond that returns an
  explicit application error ("voice channel full").
- `speaking` is derived from the local VAD for oneself and from frame
  activity for peers, with hysteresis (~400 ms): the indicator does not flicker.
- `voice.leave`, `voice.mute` and `voice.deafen` are idempotent; `voice.mute` and
  `voice.deafen` outside a channel have no effect.
- **Deafen semantics** (Discord-like): `voice.deafen { on: true }` forces
  `muted: true` and stops decoding/playback of all incoming voice (jitter
  buffers are drained, no stale audio bursts on undeafen). While deafened,
  `voice.mute` only records the requested state; `voice.deafen { on: false }`
  restores it. The deafen state is broadcast to the channel through bit `0x80`
  of `media_kinds` in `VOICE_SIGNAL` (older peers ignore the bit: the wire
  stays backward compatible) and is session-scoped: joining a channel always
  starts unmuted and undeafened.
- **Volumes**: master and per-peer volumes are linear gains applied to the
  decoded PCM before mixing (saturating at the `i16` bounds). They are
  persisted node-side (`meta` table: master, and one entry per peer public
  key) and survive restarts; mute/deafen states are not.
- You must be a member of the group to join its channel; signaling
  from non-members is ignored.
- A silent participant remains detected as alive by its quality pings;
  without traffic for 10 s, it is deemed to have left (`event.voice_left`).
- **Without audio hardware** (simulated mode, `hardware` feature absent):
  `voice.devices` returns empty lists and `null` selections;
  `voice.set_devices` accepts and **persists** the choice (applied when the
  hardware returns); `voice.mic_test { enabled: true }` returns the explicit
  error "audio hardware unavailable".
- The **mic test** opens the chosen capture and emits
  `event.voice_level { level, speaking }` at ~10 Hz (`level`: normalized RMS
  peak 0..1 since the last emission; `speaking`: VAD with
  hysteresis). It stops on its own when disabled
  (`{ enabled: false }`, always idempotent), on `voice.join` of a channel
  (the channel takes over the capture), and on the closing of the last
  API connection. Enabling it during an active voice channel is refused
  (explicit error).
- Device names are the exact `cpal` names (opaque keys, neither
  trimmed nor case-folded); 1 to 256 characters, no control characters.

### Network

Real networking (B2): stable P2P port, bootstrap peers and status, so
that two friends can find each other without a central server. So that this works
**without manual configuration** in the maximum number of cases, the node additionally
attempts, at startup: an **automatic port mapping** (UPnP-IGD then NAT-PMP/PCP as a
fallback) to be reachable from outside without forwarding the port by hand, and
a **peer discovery on the local network** (mDNS) so that two friends on
the same Wi-Fi connect without configuring anything. As a last resort, manual
bootstrapping remains possible: one communicates their `ip:port` address to the other, who
adds it as a bootstrap peer.

| Method | Parameters | Result |
|---------|-----------|----------|
| `network.status` | — | `{ p2p_port, local_addrs: [string], bootstrap: [string], connected_peers, dht_nodes, external_addr: string\|null, port_mapping: "upnp"\|"natpmp"\|"aucun", lan_peers, nat_kind: "unknown"\|"cone"\|"symmetric" }` |
| `network.add_peer` | `{ addr }` | up-to-date network status — validates `addr` (`ip:port`), persists it, connects immediately (handshake) and seeds the DHT |
| `network.remove_peer` | `{ addr }` | up-to-date network status — removes the persisted bootstrap peer |
| `network.peers` | — | `[{ pubkey, live, addr: string\|null, transport: "direct"\|"relay"\|"none", relay: string\|null, last_recv_age_ms: number\|null, rtt_ms: number\|null, last_delivery_ms: number\|null }]` — one entry per friend (see Diagnostics below) |
| `diagnostics.counters` | — | `{ punch: { requested, received, ok, fail }, relay: { open_ok, open_fail }, mailbox: { deposits, pickups }, outbox: { enqueued, flushed }, reconnect: { attempts, ok } }` — local counters since node start |
| `diagnostics.selftest` | — | `{ p2p_port, nat_kind, port_mapping, external_addr: string\|null, observed_consensus: string\|null, dht_nodes, connected_peers, relay_eligible: bool, bootstrap: [{ addr, ok }], relay_probe: { addr, ok }\|null, reachability: "direct"\|"punch"\|"relay"\|"unknown" }` — bounded network self-test (a few seconds at most) |
| `diagnostics.report` | — | `{ version, platform, counters, selftest, links: [{ peer, live, transport, relay: string\|null, last_recv_age_ms, rtt_ms, capabilities }] }` — redacted diagnostic bundle, safe to attach to a bug report |

- **Stable P2P port**: by default `48016/udp`. If it is occupied, the range
  `48017`…`48026` is tried, then an ephemeral port as a last resort. The
  actually bound port is **persisted** (meta `network.port`) and reused
  on subsequent launches; the API may impose an explicit port at startup.
- `p2p_port`: the actually bound UDP port.
- `local_addrs`: `ip:port` addresses to communicate to a friend (loopback excluded).
  The **public address observed** by a peer appears first when it is
  known; followed by the IPs of the detected outbound interfaces.
- `bootstrap`: configured bootstrap peers (persisted, meta
  `network.bootstrap`). Adding one connects and seeds the DHT routing
  table; maintenance reconnects them periodically with a per-peer backoff.
  Bounded count (64).
- `addr`: routable `ip:port`; unspecified address (`0.0.0.0`) and zero port
  refused. Loopback is tolerated (local bootstrapping, tests).
- `connected_peers`: peers whose session has been learned; `dht_nodes`:
  nodes in the DHT routing table.
- `external_addr`: external address (public IP : port) opened by the automatic
  port mapping and reachable from the Internet, or `null` if no mapping
  is active. This is the address to communicate to a remote friend when it is
  present. **Additive** field.
- `port_mapping`: active mapping method — `"upnp"` (UPnP-IGD), `"natpmp"`
  (NAT-PMP/PCP) or `"aucun"` (failure, disabled, or loopback listening). **Additive**
  field. The UI may display "port opened automatically ✓" when the
  value is `upnp`/`natpmp` (then show `external_addr`), and "to be opened
  manually" when it is `aucun`.
- `lan_peers`: number of Accord peers discovered on the local network (mDNS),
  automatically added as reachable. **Additive** field.
- `nat_kind`: local NAT type inferred by cross-checking address observations
  from several peers (SPEC §11.1): `"cone"` (peers report the same public
  address — direct hole punching is viable), `"symmetric"` (observations
  diverge — direct punching cannot pass, a relay is required), or `"unknown"`
  (too few observations yet). **Additive** field.
- **Automatic port mapping**: attempted at startup as a background task, non-
  blocking and bounded by short timeouts. UPnP-IGD first (gateway
  discovery via SSDP), NAT-PMP/PCP as a fallback (default gateway). The lease
  is renewed periodically and released best-effort at shutdown. On failure
  (no router, without UPnP/NAT-PMP, hostile, timeout exceeded): clean
  degradation, no panic — the node continues without mapping.
- **Local network discovery (mDNS)**: the node announces the service
  `_accord._udp.local.` (carrying its public key and its P2P port) and discovers
  the other Accord nodes on the same LAN, automatically added as reachable
  peers (like a bootstrap peer). Can be disabled; no effect on loopback.
- **Emission**: `event.network` is refreshed (full status, shape of
  `network.status`) on every change of the counters, the mapping, or the LAN
  peers.
- Once bootstrapping is done, the normal flow works: `friends.resolve`
  (friend code → verified DHT identity record) then `friends.request`.

#### Diagnostics (per-peer link, local counters, self-test)

Everything below is **local only** — no counter or report ever leaves the
machine (no telemetry). All fields are **additive**; future fields will be
additive too.

- `network.peers` — one entry per friend, for the UI connection card:
  - `transport`: `"direct"` (UDP or punched TCP link), `"relay"` (end-to-end
    session tunneled through a relay circuit, SPEC §11.3) or `"none"` (no
    established session right now). When both a dead and a fresh direct
    session coexist (silent peer restart), the freshest one is reported.
  - `relay`: the relay's `ip:port` hosting the tunnel when `transport` is
    `"relay"`, `null` otherwise.
  - `last_recv_age_ms`: age of the last **inbound** traffic on the current
    session, `null` without a session.
  - `rtt_ms`: last round-trip measured on the transport keep-alive PING/PONG
    (measured locally, no new wire bytes; `null` until a first cycle
    completes — first keep-alive fires 25 s into an idle session).
  - `last_delivery_ms`: epoch ms of the last **successful** message delivery
    to that peer (any channel: direct, relay, outbox flush), `null` if none
    since node start.
- `diagnostics.counters` — monotonic counters since node start:
  - `punch`: coordinated hole-punching (SPEC §11.2) — `requested` (outgoing
    requests sent), `received` (inbound accepted), `ok`/`fail` (salvo ended
    with/without a direct session).
  - `relay`: fallback circuits (SPEC §11.3) — `open_ok` (circuit opened and
    tunneled handshake launched), `open_fail` (all candidates exhausted).
  - `mailbox`: DHT offline mailboxes — `deposits` (deposits actually
    replicated), `pickups` (messages retrieved and ingested).
  - `outbox`: persistent offline queue — `enqueued` (unreachable recipient),
    `flushed` (messages that left on a link).
  - `reconnect`: bootstrap reconnection — `attempts` (backoff due), `ok`.
- `diagnostics.selftest` — triggerable, bounded self-test (a few seconds at
  most; backend data only, the UI renders it):
  - snapshot fields (`nat_kind`, `port_mapping`, `external_addr`,
    `observed_consensus`, `dht_nodes`, `connected_peers`, `relay_eligible`);
  - `bootstrap`: probes the effective bootstrap peers (up to 8) — idempotent
    `connect` then a short wait for the session; `ok` means a session is in
    place before the deadline;
  - `relay_probe`: same probe against the closest announced relay (own home
    relay derivation), `null` when no relay candidate is known;
  - `reachability` verdict: `"direct"` (port mapping active or public port
    confirmed by observation consensus), `"punch"` (cone NAT: direct
    hole punching viable), `"relay"` (symmetric NAT: relay required),
    `"unknown"` (too few observations).

- `diagnostics.report` — everything above in one object, **redacted so that it
  can be shared**. This is the only response in the API designed to be sent to
  someone else, and it is built for that:
  - each friend becomes an anonymous entry: a rank (`peer: 1, 2, …`), the link
    state, transport, RTT and staleness. **No `pubkey`** — a friend's public key
    is their friend code, and a report carrying it hands over the user's address
    book, and lets two reports be cross-referenced to prove that two people know
    each other. **No `addr`** — that is the friend's IP address, third-party data
    from someone who was never asked;
  - `external_addr` and `observed_consensus` keep their port and lose their host
    (`masqué:41234`). The port is what diagnoses a NAT; the host is the user's
    home IP address;
  - bootstrap and relay addresses are kept as-is: that is public infrastructure,
    entered by the user, and without it a relay problem cannot be diagnosed.

  Redaction happens in the node (`diagnostics::bug_report`) and is tested there
  and at the JSON boundary. Never rebuild this report client-side from
  `network.peers`, which carries both removed fields.

> **NAT limit.** Direct P2P requires that at least **one of the two peers** has
> its UDP port reachable from outside. The **automatic mapping**
> (UPnP-IGD / NAT-PMP-PCP) obtains it without intervention on most consumer
> routers: on success, `external_addr` is filled in and the remote friend can
> reach it directly. When the mapping fails (UPnP/NAT-PMP absent or
> disabled on the router, carrier double NAT, CGNAT), `port_mapping` is
> `"aucun"`: one of the two friends must then have a public IP, **manually open/
> forward** the port `48016/udp`, or go through the same local network
> (mDNS discovery). Address candidates (observed address) and relays
> help, but **with no guarantee**. Releasing the mapping at shutdown is
> best-effort (the process may terminate before the request completes; the
> lease expires on its own on the router side).
>
> **Not testable without a router.** The real mapping (UPnP/NAT-PMP) and multicast
> mDNS discovery depend on a real router and LAN: they are not
> covered by automated tests (which listen on loopback, a case where these
> mechanisms are deliberately ignored). Only the parseable logic (method,
> addresses, state transitions, degradation on failure) and the shape of the status
> are unit-tested.

### Privacy

| Method | Parameters | Result |
|---------|-----------|----------|
| `privacy.report` | — | read-only report, see below |

`privacy.report` aggregates, **read-only and entirely locally**, what this
device stores and what kinds of endpoints the node talks to — the concrete
proof behind "0 central server, 100 % local & encrypted":

```json
{
  "counts": { "friends": 0, "dm_messages": 0, "groups": 0,
              "group_messages": 0, "files": 0, "pins": 0 },
  "storage": { "db_bytes": 1234, "file_bytes": 0, "db_encrypted_at_rest": true },
  "egress": { "available": true, "bootstrap_peers": 1, "dht_nodes": 12,
              "connected_peers": 3, "relay_circuits": 0, "central_servers": 0 }
}
```

- `counts`: rows kept in the local database (friends, messages, joined
  groups, stored files, DM pins).
- `storage`: `db_bytes` is the size of the SQLCipher database file on disk
  (`null` when unknown); `file_bytes` the declared size of stored files;
  `db_encrypted_at_rest` is always `true` (SQLCipher) — stated as data so the
  UI shows a verified fact, not a slogan.
- `egress`: the only endpoint kinds the node ever contacts, all of them
  ordinary peers — bootstrap peers (first-contact seeding), DHT nodes
  (Kademlia routing table), connected peers, relay circuits (E2E-encrypted
  fallback links). `central_servers` is **0 by construction** and will stay 0.
  `available: false` (all counts zero) when the network runtime is not up.

### Planning (local)

Three purely-local features share one primitive — tasks persisted in the
database and driven by the existing maintenance loop. **Nothing crosses the
wire**: a scheduled message that fires reuses the normal send path (and the
outbox for offline peers); a reminder and a backup nudge are local
notifications only.

| Method | Parameters | Result |
|---------|-----------|----------|
| `schedule.list` | — | `{ scheduled: [{ id, scope, scope_id, channel_id, fire_at, created_at, preview }] }` — soonest first; `scope` ∈ `dm`∣`group`, `channel_id` hex∣`null` |
| `schedule.cancel` | `{ id }` | `{ ok: true }` — removes a scheduled message; rejected if unknown |
| `schedule.reschedule` | `{ id, fire_at }` | `{ ok: true }` — moves the firing time; rejected if unknown |
| `reminders.add` | `{ scope, scope_id, msg_ref?, note?, fire_at }` | `{ id }` — pins a local reminder; `scope` ∈ `dm`∣`group`, `scope_id` peer pubkey (32) or group id (16), `msg_ref` referenced message id∣`null`, `note` ≤ 500 chars |
| `reminders.list` | — | `{ reminders: [{ id, scope, scope_id, msg_ref, note, fire_at, fired, created_at }] }` — soonest first; `fired` bool |
| `reminders.dismiss` | `{ id }` | `{ ok: true }` — removes a reminder; rejected if unknown |
| `backup.status` | — | `{ cadence, dir, last_backup_at, next_due_at, due }` — `cadence` ∈ `off`∣`weekly`∣`monthly`, `dir` string∣`null`, times ms∣`null`, `due` bool |
| `backup.schedule` | `{ cadence, dir? }` | `{ ok: true }` — sets the cadence and optional destination folder (empty/absent `dir` = reminder only) |
| `backup.record_done` | `{ at? }` | `{ ok: true }` — records a completed backup (advances `last_backup_at`; defaults to now) |
| `backup.run_now` | — | `{ ok: true }` — re-emits `event.backup_due` so the UI runs the export path immediately |

`dm.schedule` / `groups.schedule` create the scheduled messages listed here. A
due message is sent by the maintenance loop and its row dropped — `fire_at` is
wall-clock ms, bounded to at most one year ahead.

Reminders and the backup nudge fire as local notifications (`event.reminder`,
`event.backup_due`). A reminder fires exactly once (a `fired_at` stamp guards
against re-firing). The backup archive itself is **not** written by the node:
`backup.*` only schedules and detects due windows; the sealed `.accordbackup`
is produced by the host export flow (`backup_export`), which stops the node and
re-verifies the passphrase, so a running node never touches it.

## Events (server → client notifications)

Pushed to all authenticated clients, without `id`:

```json
{ "jsonrpc": "2.0", "method": "event.dm", "params": { "peer": "<hex>", "msg_id": "<hex>" } }
```

| Event | Payload |
|-----------|--------------|
| `event.dm` | `{ peer, msg_id, attachments }` — direct message received (or edited/deleted/reacted by the peer); `attachments`: detailed list, `[]` outside of a new message |
| `event.dm_typing` | `{ peer }` — the peer is typing (ephemeral; bounded to one event every 2 s per peer) |
| `event.friend_request` | `{ peer }` — friend request received |
| `event.friend_response` | `{ peer, accepted }` — response to our request |
| `event.presence` | `{ pubkey, online, status, status_text }` — a **friend**'s presence changed: `online` `bool` (kept for backward compatibility, `status != "offline"`), `status` ∈ `online`∣`idle`∣`dnd`∣`offline`, `status_text` `string`∣`null`; see "Presence" |
| `event.friend_removed` | `{ peer }` — a friendship was removed (by us via `friends.remove`, or by the peer via a `FRIEND_REMOVE` wire message): refresh `friends.list`; the DM history is kept |
| `event.friend_verified` | `{ peer, verified }` — the manual verification flag of a contact changed locally (`friends.set_verified`): refresh the shield badge |
| `event.reminder` | `{ id, scope, scope_id, msg_ref, note, fire_at }` — a local reminder came due (see "Planning"): show a notification. Fires exactly once |
| `event.backup_due` | `{ auto, dir }` — a scheduled backup is due (see "Planning"): `auto` `bool` (a destination folder is configured), `dir` string∣`null`. The UI runs the host export flow |
| `event.dm_read` | `{ peer, lamport }` — the peer's read receipt advanced: they have read our messages of the conversation up to `lamport` (same value as `peer_read_lamport` in `dm.history`) |
| `event.profile` | `{ pubkey, name, bio, avatar, banner }` — a **friend**'s profile updated (`bio` `string`∣`null`, `avatar` and `banner` hex-64 hash∣`null`; nickname reflected in `friends.list`) |
| `event.group_op` | `{ group_id }` — replicated group op |
| `event.group_state` | `{ group_id }` — the group state has changed (op applied, local or remote): reload `groups.state` |
| `event.group_msg` | `{ group_id, channel_id, msg_id, attachments }` — channel message received (or edited/deleted/reacted); `attachments`: detailed list, `[]` outside of a new message |
| `event.mention` | `{ msg_id, peer }` (DM) ∣ `{ msg_id, group_id, channel_id }` (group) — a newly received message mentions the local user (see "Mentions"); a fresh inbox entry was created. Fires only on a new detection, not on replay |
| `event.group_typing` | `{ group_id, channel_id, pubkey }` — a member is typing in a channel (ephemeral; bounded to one event every 2 s per peer) |
| `event.group_key` | `{ group_id }` — group key received (messages become decryptable) |
| `event.voice_joined` | `{ group_id, channel_id, pubkey }` — a participant (including oneself) entered a voice channel |
| `event.voice_left` | `{ group_id, channel_id, pubkey }` — a participant left a voice channel (departure or liveness expired) |
| `event.voice_speaking` | `{ pubkey, speaking }` — the "speaking" indicator of a participant in the active channel has changed |
| `event.voice_mute` | `{ pubkey, muted, deafened }` — the mute/deafen state of a participant in the active channel has changed (including oneself; peers' states come from their `VoiceSignal` broadcasts) |
| `event.voice_level` | `{ level, speaking }` — mic level during the test (`voice.mic_test`): normalized RMS peak 0..1 and VAD, at ~10 Hz |
| `event.file_progress` | `{ merkle_root, done, total, complete }` — progress of a download (steps of about 5% then final state; `complete: false` in the last event = abandon) |
| `event.network` | `{ connected_peers, dht_nodes }` — the network counters have changed (emitted sparingly, never in bursts) |
| `event.pairing_adopted` | `{}` — this machine has just received the account root and is now a full device of the account (see "Devices") |
| `event.desynchronise` | `{}` — the client has fallen behind; re-synchronize via the `*.list`/`*.history` methods |

## Presence

Friends' presence is **best-effort**, not persisted and tracked only in
memory:

- A friend is marked **online** as soon as a message is received from them (first
  message of a session) or on an online presence announcement, and **offline**
  on a clean-shutdown presence announcement.
- The node broadcasts a presence announcement to its friends at startup (online) and
  at clean shutdown (offline); these announcements are **never queued
  offline** (an unreachable friend loses them with no effect).
- The absence of news does **not** prove that a friend is offline: without
  an explicit shutdown announcement, a friend remains presumed online. `friends.list`
  exposes the current state (`online`) and `event.presence { pubkey, online }`
  signals changes. `last_seen_ms` (already present) timestamps the last contact.
- **Rich presence**: a friend's explicit announcement carries a status
  (`online`, `idle`, `dnd`) and an optional custom text (≤ 256 UTF-8 bytes),
  exposed as `status` / `status_text` in `friends.list` and `event.presence`.
  A reachable friend without an explicit announcement is plain `online`; an
  offline announcement (or none at all) clears the rich status. Non-friends
  only update plain reachability (anti-abuse).
- **Own status** (`friends.set_status`): persisted across restarts and
  broadcast on change, at startup and in the periodic announcements.
  `invisible` is local-only: friends see a regular offline announcement
  (never the custom text) while the node keeps working normally.
