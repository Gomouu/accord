# Accord local API — JSON-RPC 2.0 over WebSocket

> Contract between the UI and the node. The server listens **only** on
> `127.0.0.1` (ephemeral port by default). The UI reads the address and the token
> from `<profil>/session.json` written by the daemon at startup.

## Transport

- WebSocket, JSON **text** messages only (binary ignored).
- Single request (no batching). `id` numeric, string, or `null`.
- Maximum message size: 1 MiB.

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
| `identity.self` | — | `{ node_id, pubkey, friend_code, name, bio, avatar, banner }` |

`name` is the local nickname (`string`), or `null` if it has never been set
via `profile.set` (see "Profile"); `bio` (`string` or `null`), `avatar` and
`banner` (hex-64 hash or `null`) follow the same rules.

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
| `profile.get` | — | `{ name, bio, avatar, banner }` — nickname (`string`∣`null`), bio (`string`∣`null`), avatar and banner hash (hex 64∣`null`) |
| `profile.set` | `{ name?, bio? }` | `{}` — at least one of the two fields required |
| `profile.set_avatar` | `{ data_b64, mime }` | `{ avatar }` — hex-64 hash of the published blob, or `null` after removal |
| `profile.set_banner` | `{ data_b64, mime }` | `{ banner }` — hex-64 hash of the published blob, or `null` after removal |

`profile.set` validates the nickname (2 to 32 characters once edge whitespace
is trimmed, no control characters) and the bio (at most 2048 characters
after trimming; line breaks and tabs allowed; **empty string = clear**),
stores locally (trimmed forms) then announces the full profile to all
confirmed friends (CORE `PROFILE` message, SPEC §6.5). All or nothing: if one
of the two fields is invalid, neither is written. The profile is also announced
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

### Friends

| Method | Parameters | Result |
|---------|-----------|----------|
| `friends.list` | — | `{ contacts: [{ node_id, pubkey, friend_code, display_name, bio, avatar, banner, state, last_seen_ms, online, status, status_text, unread, mention_count, note }] }` — `bio` `string`∣`null`, `avatar` and `banner` hex-64 hash∣`null` (profile announced by the peer, D-027, D-032); `online` `bool` (kept for backward compatibility) plus `status` ∈ `online`∣`idle`∣`dnd`∣`offline` and `status_text` `string`∣`null` (rich presence, best-effort, see "Presence"); `unread` integer (messages from the peer received after our `dm.mark_read`); `mention_count` integer (unread mentions in this DM, see "Mentions"); `note` `string`∣`null` (private local-only note, see "Private notes") |
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

`state` ∈ `pending_out`, `pending_in`, `friend`, `blocked`. The "add
a friend by code" flow is `friends.resolve` then `friends.request`.

`display_name` is the last nickname announced by the peer (`PROFILE` message,
see "Profile"); failing that, the label given to `friends.request` or the name
carried by their friend request.

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
| `groups.list` | — | `{ groups: [group_id], unread, mentions }` — `unread`: `{ group_id: { channel_id: n } }`, unread per channel (others' messages after the `groups.mark_read` mark); only channels with at least one unread appear. `mentions`: `{ group_id: n }`, unread mentions per group (all channels combined); only groups with at least one appear |
| `groups.state` | `{ group_id, channel_id? }` | full state, see below — with `channel_id`, `my_permissions` becomes the **effective** bitfield in that channel (overrides folded in, `deny` > `allow`) |
| `groups.rename` | `{ group_id, name }` | `{ ok: true }` — 1-100 characters |
| `groups.set_icon` | `{ group_id, data_b64, mime }` | `{ icon }` — image ≤ 512 KiB decoded, published in the file store; `icon` = hex-64 Merkle root |
| `groups.set_topic` | `{ group_id, channel_id, topic }` | `{ ok: true }` — ≤ 2048 bytes |
| `groups.channel.add` | `{ group_id, name, kind?, category? }` | `{ channel_id }` — `kind` ∈ `"text"` (default), `"voice"`, `"announcement"`; `category` = hex id of an existing category |
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

`groups.edit`, `groups.delete` (of one's own message) and `groups.react`
travel as bodies encrypted with the group key, over the same path
as `groups.send`; on ingestion at each member, the action is applied
(author verified) and `event.group_msg` is emitted.

#### Shape of `groups.state`

```json
{
  "group_id": "<hex32>",
  "name": "Guilde",
  "icon": "<hex64>" ,          // Merkle root of the icon, or null
  "founder": "<hex64>",        // public key, or null
  "members": [{ "pubkey": "<hex64>", "roles": ["<role_id>"] }],
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
