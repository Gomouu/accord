# The local API as a public contract

> Complete surface, stability policy, and an honest account of what opening
> this API does to the local attack surface.
>
> [`docs/API.md`](API.md) is the **detailed** reference: shapes, bounds,
> semantics, edge cases. This document is the **contract**: what exists, what
> you may rely on, what may move, and what protects it. Where the two disagree,
> `crates/accord-node/src/service/*.rs` and `crates/accord-api/src/` are the
> authority — every claim below is derived from them, and the divergences found
> while deriving it are listed in §7.

---

## 1. What this API is

The Accord node exposes a **JSON-RPC 2.0 server over WebSocket on
`127.0.0.1`**, authenticated by a session token. The desktop interface is a
client of it and nothing else — it never touches the network directly. That is
why the surface is complete and exercised: everything the application can do,
it does through here.

That also makes alternative clients possible without Accord hosting anything:
bots that run as peers, alternative interfaces, personal scripts, bridges. The
node stays the node.

**It is not a remote API.** There is no listener on any interface other than
loopback, no TLS, no user accounts, no per-client identity. Read §5 before
building anything on it.

---

## 2. Connecting

### Finding the address and token

- **Standalone daemon** (`accord-noded`): writes `<profile>/session.json` at
  startup, containing `{"api": "<ip:port>", "token": "<hex64>"}`. `0600` on
  Unix; see §5.2 for what that does and does not buy you on Windows.
- **Desktop application**: the port and token are handed to the WebView through
  Tauri IPC when the vault is unlocked. There is no file to read.

### The handshake

WebSocket, JSON **text** frames only (binary frames are ignored). One request
per frame — no batching. `id` may be a number, a string, or `null`. Incoming
frames are capped at **16 MiB** (`MAX_WS_MESSAGE`), sized so that
`files.share_bytes` / `files.read` (8 MiB of bytes, ~11 MiB once base64'd
inside a JSON envelope) fit.

The **first** request on every connection must be `auth`:

```json
{ "jsonrpc": "2.0", "id": 0, "method": "auth", "params": { "token": "<hex64>" } }
```

- Success → `{ "result": { "protocole": 1 } }`.
- Failure → `{ "error": { "code": -32001, "message": "jeton invalide" } }`, then
  the server closes the connection.
- Any other method before `auth` is rejected the same way, and the connection
  closes.
- You have **10 seconds** to authenticate; the WebSocket upgrade itself has
  **10 seconds** to complete.
- Re-sending `auth` on an already-authenticated connection is idempotent and
  returns the same result.

### `Origin`, and what it means for a script

The server checks the `Origin` header (defence in depth against DNS rebinding
and WebSocket CSRF). A **missing** `Origin` is allowed — that is the native
WebView's case, and it is also a command-line client's case, so a script works
without doing anything special. `null` (opaque origin) is allowed too. Any
other explicit web origin than the application's own is refused at the
handshake, which is what stops a web page you visit from driving your node.

A runnable client that puts this whole section into practice lives in
[`examples/client-minimal/`](../examples/client-minimal/).

`protocole` is the API protocol version (`API_VERSION`,
`crates/accord-api/src/server.rs`). It is **1** and has always been 1. It is
your only machine-readable handle on compatibility — see §4.

### Error codes

| Code | Meaning |
|------|---------|
| `-32700` | invalid JSON |
| `-32600` | malformed request |
| `-32601` | unknown method |
| `-32602` | invalid parameters |
| `-32000` | node application error |
| `-32001` | token missing or invalid |

The **codes** are part of the contract. The **messages** are not: they are
French, human-facing, and may be reworded at any time. Never branch on a
message string.

### Events

Server-to-client notifications are JSON-RPC notifications (no `id`), broadcast
to **every** authenticated connection:

```json
{ "jsonrpc": "2.0", "method": "event.dm", "params": { "peer": "<hex>", "msg_id": "<hex>" } }
```

If a client falls behind, the node drops the oldest events for it and sends
`event.desynchronise` instead — re-read state through the `*.list` / `*.history`
methods rather than assuming the stream was complete.

---

## 3. The surface

Identifiers (public keys, node ids, message ids, group ids, channel ids,
Merkle roots) travel as **hexadecimal strings**. Group, channel, category, role,
message, event, poll and invite ids are 16 bytes → 32 hex characters; public
keys and content hashes are 32 bytes → 64 hex characters.

### Stability tiers

| Tier | Meaning |
|---|---|
| **F** — frozen | Signature and semantics will not change. |
| **S** — stable | Documented in `docs/API.md` and treated as a contract: additive changes only (§4). |
| **P** — provisional | Implemented and exercised by the interface, but never written down as a contract. Real, usable, and **not yet promised**: shape may still change without a major version. |

A method being **P** is a statement about this project's commitments, not about
the code's quality — most of these are as tested as the rest. Documenting them
in `API.md` is what moves them to **S**.

### 3.1 Session

| Method | Parameters | Result | Tier |
|---|---|---|---|
| `auth` | `{ token }` | `{ protocole: 1 }` | **F** |

### 3.2 Identity and profile

| Method | Parameters | Result | Tier |
|---|---|---|---|
| `identity.self` | — | `{ node_id, pubkey, friend_code, name, bio, avatar, banner, pronouns, accent_color, banner_color, avatar_decoration, profile_effect, profile_frame }` | S |
| `profile.get` | — | same fields minus the identity triple | S |
| `profile.set` | `{ name?, bio?, pronouns?, accent_color?, banner_color?, avatar_decoration?, profile_effect?, profile_frame? }` | `{}` | S |
| `profile.set_avatar` | `{ data_b64, mime }` | `{ avatar }` | S |
| `profile.set_banner` | `{ data_b64, mime }` | `{ banner }` | S |

🔒 `pubkey`, `node_id` and `friend_code` are the **account's**, never this
machine's device key. The device key is not exposed by this method at all.

🔒 **The identity lifecycle is deliberately absent from this API.** Creation,
restoration, unlocking, locking, backup export/import and the adoption of an
account root by a newly paired machine go through **Tauri IPC**, not JSON-RPC
(D-023). They predate the node's existence (no port, no token yet) and they
handle secrets — passphrase, 12-word recovery phrase, account seed — that have
no business on a channel readable by anything running on the machine and
reproducible in anyone's debug trace. `docs/API.md` § Identity lists the IPC
commands; `app/src-tauri/src/lib.rs` is the authoritative list.

**A third-party client therefore cannot create or unlock an identity.** It
attaches to a node someone has already unlocked. That is the design, not a gap.

### 3.3 Devices (this account's machines)

| Method | Parameters | Result | Tier |
|---|---|---|---|
| `devices.list` | — | `{ devices: [{ pubkey, name, added_ms, is_current, last_seen_ms, last_seen_route }] }` | S |
| `devices.rename` | `{ name }` | `{ name }` | S |
| `devices.pair_start` | — | `{ code, expires_ms }` | S |
| `devices.pair_submit` | `{ code }` | `{ hello }` | S |
| `devices.pair_status` | — | `{ fingerprint, role, adopted }` | S |
| `devices.pair_confirm` | — | `{}` | S |
| `devices.pair_cancel` | — | `{}` | S |
| `devices.revoke` | `{ pubkey }` | `{}` | S |

🔒 `adopted` is a boolean and **never** the seed. The host picks the account
root up through `Node::pairing_take_adoption`, which does not go through this
API, for the reason in §3.2.

### 3.4 Friends and search

| Method | Parameters | Result | Tier |
|---|---|---|---|
| `friends.list` | — | `{ contacts: [ … ] }` — see `API.md` for the 17 per-contact fields | S |
| `friends.resolve` | `{ friend_code }` | `{ pubkey }` — DHT lookup, verified end to end | S |
| `friends.request` | `{ pubkey, display_name }` | `{ ok: true }` | S |
| `friends.respond` | `{ pubkey, accept }` | `{ ok: true }` | S |
| `friends.remove` | `{ pubkey }` | `{ ok: true }` | S |
| `friends.block` / `friends.unblock` | `{ pubkey }` | `{ ok: true }` | S |
| `friends.set_note` | `{ pubkey, note }` | `{ ok: true }` — local only, never on the wire | S |
| `friends.get_note` | `{ pubkey }` | `{ note }` | S |
| `friends.set_status` | `{ status, custom? }` | `{ ok: true }` | S |
| `friends.get_status` | — | `{ status, custom }` | S |
| `friends.safety_number` | `{ pubkey }` | `{ digits, emoji, verified, key_changed }` | S |
| `friends.set_verified` | `{ pubkey, verified }` | `{ ok: true }` | S |
| `search.query` | `{ query }` | `{ msg_ids, hits }` — blind HMAC index, filter tokens parsed node-side | S |

`friends.resolve` is the one method routed before the synchronous dispatcher
(`service/mod.rs`): it needs the network resolver, and returns an application
error when the node was built without one.

### 3.5 Direct messages

| Method | Parameters | Result | Tier |
|---|---|---|---|
| `dm.send` | `{ pubkey, text, reply_to?, attachments? }` | `{ msg_id }` | S |
| `dm.history` | `{ pubkey, before_lamport?, limit? }` | `{ messages, peer_read_lamport }` | S |
| `dm.history_around` | `{ pubkey, msg_id, limit? }` | `{ messages, found, peer_read_lamport }` | S |
| `dm.edit` | `{ pubkey, msg_id, text }` | `{ ok: true }` | S |
| `dm.delete` | `{ pubkey, msg_id }` | `{ ok: true }` | S |
| `dm.retry` | `{ pubkey, msg_id }` | `{ ok: true }` | S |
| `dm.react` | `{ pubkey, msg_id, emoji, remove? }` | `{ ok: true }` | S |
| `dm.pin` / `dm.unpin` | `{ pubkey, msg_id }` | `{ ok: true }` | S |
| `dm.pins` | `{ pubkey }` | `{ msg_ids }` | S |
| `dm.typing` | `{ pubkey }` | `{ ok: true }` — ephemeral, never queued | S |
| `dm.mark_read` | `{ pubkey, lamport }` | `{ ok: true }` | S |
| `dm.set_read_receipts` | `{ enabled }` | `{ ok: true }` | S |
| `dm.get_read_receipts` | — | `{ enabled }` | S |
| `dm.set_ephemeral` | `{ pubkey, ttl_secs? }` | `{ ok: true }` — local only | S |
| `dm.ephemeral` | `{ pubkey }` | `{ ttl_secs }` | S |
| `dm.schedule` | `{ pubkey, body, fire_at }` | `{ id }` — local deferred send | S |

### 3.6 Groups — documented surface

| Method | Parameters | Result | Tier |
|---|---|---|---|
| `groups.create` | `{ name }` | `{ group_id }` | S |
| `groups.list` | — | `{ groups, unread, mentions, channel_mentions }` | S · see §7 |
| `groups.state` | `{ group_id, channel_id? }` | full state — see `API.md`, plus `read_marks` | S · see §7 |
| `groups.rename` | `{ group_id, name }` | `{ ok: true }` | S |
| `groups.set_icon` | `{ group_id, data_b64, mime }` | `{ icon }` | S |
| `groups.set_topic` | `{ group_id, channel_id, topic }` | `{ ok: true }` | S |
| `groups.channel.add` | `{ group_id, name, kind?, category? }` | `{ channel_id }` | S |
| `groups.channel.edit` | `{ group_id, channel_id, name?, position?, category? }` | `{ ok: true }` | S |
| `groups.channel.perms` | `{ group_id, channel_id, role_id, allow, deny }` | `{ ok: true }` | S |
| `groups.channel.del` | `{ group_id, channel_id }` | `{ ok: true }` | S |
| `groups.category.add` | `{ group_id, name, position? }` | `{ category_id }` | S |
| `groups.category.edit` | `{ group_id, category_id, name?, position? }` | `{ ok: true }` | S |
| `groups.category.del` | `{ group_id, category_id }` | `{ ok: true }` | S |
| `groups.audit` | `{ group_id, before?, limit? }` | `{ entries }` | S |
| `groups.kick` / `groups.ban` / `groups.unban` | `{ group_id, pubkey }` | `{ ok: true }` | S |
| `groups.timeout` | `{ group_id, pubkey, until_ms }` | `{ ok: true }` | S |
| `groups.timeout_clear` | `{ group_id, pubkey }` | `{ ok: true }` | S |
| `groups.set_nickname` | `{ group_id, name, member? }` | `{ ok: true }` | S |
| `groups.leave` | `{ group_id }` | `{ ok: true }` | S |
| `groups.role.add` | `{ group_id, name, color, permissions, position? }` | `{ role_id }` | S |
| `groups.role.edit` | `{ group_id, role_id, name?, color?, position?, permissions? }` | `{ ok: true }` | S |
| `groups.role.del` | `{ group_id, role_id }` | `{ ok: true }` | S |
| `groups.role.assign` / `groups.role.unassign` | `{ group_id, role_id, pubkey }` | `{ ok: true }` | S |
| `groups.pin` / `groups.unpin` | `{ group_id, channel_id, msg_id }` | `{ ok: true }` | S |
| `groups.pins` | `{ group_id, channel_id }` | `{ msg_ids }` | S |
| `groups.history` | `{ group_id, channel_id, before_lamport?, limit? }` | `{ messages }` | S |
| `groups.history_around` | `{ group_id, channel_id, msg_id, limit? }` | `{ messages, found }` | S |
| `groups.send` | `{ group_id, channel_id, text, reply_to?, attachments? }` — **or** `{ …, sticker }` — **or** `{ …, poll: { question, options } }` | `{ msg_id }`, or `{ msg_id, poll_id }` for a poll | S · sticker/poll forms are **P** |
| `groups.edit` | `{ group_id, channel_id, msg_id, text }` | `{ ok: true }` | S |
| `groups.delete` | `{ group_id, channel_id, msg_id }` | `{ ok: true }` | S |
| `groups.react` | `{ group_id, channel_id, msg_id, emoji, add? }` | `{ ok: true }` | S |
| `groups.invite` | `{ group_id, pubkey }` | `{ ok: true, invite_id }` | S · **deprecated**, see §4 |
| `groups.emoji.add` | `{ group_id, name, data_b64, mime }` | `{ merkle_root }` | S |
| `groups.emoji.del` | `{ group_id, name }` | `{ ok: true }` | S |
| `groups.typing` | `{ group_id, channel_id }` | `{ ok: true }` | S |
| `groups.mark_read` | `{ group_id, channel_id, lamport }` | `{ ok: true }` | S |
| `groups.automod.set` | `{ group_id, words }` | `{ ok: true }` | S |
| `groups.automod.get` | `{ group_id }` | `{ words }` | S |
| `groups.set_ephemeral` | `{ group_id, ttl_secs? }` | `{ ok: true }` | S |
| `groups.ephemeral` | `{ group_id }` | `{ ttl_secs }` | S |
| `groups.schedule` | `{ group_id, channel_id, body, fire_at }` | `{ id }` | S |

Every management action emits a **signed op** in the group's replicated op-log.
The caller's permission is checked *before* emission, by replaying the op
against materialised state — the same rule peers apply on ingestion. An
unauthorised action returns an application error `refusé : …`.

⚠️ `groups.audit`'s `ADMIN`/founder gate is a **UX gate, not a confidentiality
boundary**: the op-log is replicated to every member, so any member already
holds this data locally. Do not build a client that relies on the gate to keep
op contents from members.

⚠️ **AutoMod is a display convention between honest clients**, not enforcement.
Nothing is deleted, nothing is blocked at send time, the word list travels in
the clear to every member, and a modified client sees the full text. Treat it
as clutter reduction, never as a safety boundary.

### 3.7 Groups — provisional surface

Implemented, exercised by the interface, absent from `docs/API.md`.

| Method | Parameters | Result | Tier |
|---|---|---|---|
| `groups.create_dm` | `{ name, members: [pubkey] }` | `{ group_id }` | P |
| `groups.channel.slowmode` | `{ group_id, channel_id, seconds }` | `{ ok: true }` | P |
| `groups.thread.create` | `{ group_id, parent_channel, root_msg, name }` | `{ thread_id }` | P |
| `groups.thread.archive` | `{ group_id, thread_id, archived? }` — absent `archived` = `true` | `{ ok: true }` | P |
| `groups.entry_check.get` | `{ group_id }` | `{ enabled }` | P |
| `groups.entry_check.set` | `{ group_id, enabled }` | `{ ok: true }` | P |
| `groups.pending.list` | `{ group_id }` | `{ entries: [{ member, invite_id, at_ms }] }` | P |
| `groups.pending.approve` | `{ group_id, pubkey }` | `{ ok: true }` | P |
| `groups.pending.refuse` | `{ group_id, pubkey }` | `{ ok: true }` | P |
| `groups.voice_moderate` | `{ group_id, pubkey, mute?, deafen? }` — absent = `false`, which **lifts** the moderation | `{ ok: true }` | P |
| `groups.purge` | `{ group_id, channel_id, msg_ids: [msg_id] }` | `{ deleted }` | P |
| `groups.invite_create` | `{ group_id, pubkey }` | `{ invite_id }` | P |
| `groups.invite_link_create` | `{ group_id, max_uses?, expires_h? }` — `max_uses: 0` = unlimited; `expires_h: 0` = never | `{ code }` | P |
| `groups.invite_link_redeem` | `{ code }` | `{ ok: true, group_id, group_name }` | P |
| `groups.invite_link_info` | `{ link }` | `{ group_id, invite_id, inviter, group_name, icon, banner, banner_color }` — decode only, no side effect | P |
| `groups.invites_list` | — | `{ invites: [ … ] }` — incoming invitations awaiting a decision | P |
| `groups.invite_accept` | `{ group_id, invite_id }` | `{ ok: true }` | P |
| `groups.invite_decline` | `{ group_id, invite_id }` | `{ ok: true }` | P |
| `groups.sounds.add` | `{ group_id, name, data_b64, mime }` | `{ merkle_root }` | P |
| `groups.sounds.del` | `{ group_id, name }` | `{ ok: true }` | P |
| `groups.soundboard.play` | `{ group_id, channel_id, sound_name }` | `{ ok: true }` | P |
| `groups.stickers.add` | `{ group_id, name, data_b64, mime }` | `{ merkle_root }` | P |
| `groups.stickers.remove` | `{ group_id, name }` | `{ ok: true }` | P |
| `groups.stickers.list` | `{ group_id }` | `{ stickers: [{ name, merkle_root }] }` | P |
| `groups.set_member_avatar` | `{ group_id, data_b64?, mime? }` — `data_b64` absent/`null` clears | `{ avatar }` | P |
| `groups.set_banner` | `{ group_id, data_b64?, mime? }` — same convention | `{ banner }` | P |
| `groups.set_banner_color` | `{ group_id, color }` — `0xRRGGBB` integer, required | `{ ok: true }` | P |
| `groups.events.create` | `{ group_id, title, description?, start_ms?, channel_id? }` | `{ event_id }` | P |
| `groups.events.edit` | `{ group_id, event_id, title, description?, start_ms?, channel_id? }` | `{ ok: true }` | P |
| `groups.events.delete` | `{ group_id, event_id }` | `{ ok: true }` | P |
| `groups.events.rsvp` | `{ group_id, event_id, interested? }` — absent = `true` | `{ ok: true }` | P |
| `groups.polls.vote` | `{ group_id, poll_id, option_index }` | `{ ok: true }` | P |
| `groups.polls.close` | `{ group_id, poll_id }` | `{ ok: true }` | P |
| `groups.polls.delete` | `{ group_id, poll_id }` | `{ ok: true }` | P |

There is no `groups.polls.create`: a poll is posted through `groups.send` with
a `poll` parameter, which returns `{ msg_id, poll_id }`. Likewise a sticker is
sent through `groups.send` with a `sticker` parameter. In both cases `text`,
`reply_to` and `attachments` are ignored.

🔒 `groups.soundboard.play` is the one method with a check outside the
dispatcher (`service/mod.rs`): the caller must be **connected to the targeted
voice channel**, verified against the voice actor. A member who is not in the
channel cannot trigger a sound.

### 3.8 Mentions, files, preferences, privacy

| Method | Parameters | Result | Tier |
|---|---|---|---|
| `mentions.inbox` | `{ before?, limit? }` | `{ entries }` — newest first | S |
| `mentions.mark_read` | `{ msg_ids? }` — absent marks **all** | `{ ok: true, marked }` | S |
| `files.share` | `{ path }` | `{ file: { merkle_root, name, size, mime } }` — **reads an arbitrary local path**, see §5.4 | S |
| `files.share_bytes` | `{ name, mime, data_b64 }` | `{ file }` — bounded to 8 MiB decoded | S |
| `files.read` | `{ merkle_root, hint?, media? }` | `{ data_b64, name, mime, size }` or `{ pending: true }` | S · `media` is **P** |
| `files.status` | `{ merkle_root, hint? }` | `{ known, complete, done, total, name?, size?, mime?, path? }` | S · `path` is **P**, see §7 |
| `files.save` | `{ merkle_root, path }` | `{ ok: true }` — **writes to an arbitrary local path**, see §5.4 | S |
| `prefs.list` | — | `{ prefs: [{ key, value, at_ms }] }` | P |
| `prefs.set` | `{ key, value }` | `{ at_ms }` — key outside the allowlist is an error, not a silent ignore | P |
| `privacy.report` | — | `{ counts, storage, egress }` — read-only, entirely local | S |

`privacy.report.egress.central_servers` is **0 by construction** and will stay
0. It is exposed as data so an interface can show a verified fact rather than a
slogan.

### 3.9 Voice, calls, video

> `voice.*` room semantics are a **frozen** contract (D-025): signatures and
> notifications implemented to the letter on both sides, no divergence allowed.
> Additive extension is still permitted and has already happened — deafen,
> volumes, DSP, server moderation, priority speaker.

| Method | Parameters | Result | Tier |
|---|---|---|---|
| `voice.join` | `{ group_id, channel_id }` | `{ participants: [pubkey] }` — one active channel at a time; cap of 10 | **F** |
| `voice.leave` | — | `{}` | **F** |
| `voice.mute` | `{ muted }` | `{}` | **F** |
| `voice.deafen` | `{ on }` | `{}` — forces mute; restores the requested mute on undeafen | **F** |
| `voice.set_volume` | `{ peer?, volume }` — `peer` absent = master; `volume` 0..=200 | `{}` | **F** |
| `voice.status` | — | `{ active, master_volume, dsp: { noise_suppression, agc, echo_cancel } }` | **F** · see §7 |
| `voice.devices` | — | `{ inputs, outputs, selected_input, selected_output }` | **F** |
| `voice.set_devices` | `{ input?, output? }` — `null` = default device | `{}` | **F** |
| `voice.mic_test` | `{ enabled }` | `{}` — emits `event.voice_level` at ~10 Hz | **F** |
| `voice.rooms` | — | `{ rooms: [{ group_id, channel_id, participants }] }` | P |
| `voice.set_noise_suppression` | `{ enabled }` | `{}` | P |
| `voice.set_agc` | `{ enabled }` | `{}` | P |
| `voice.set_echo_cancel` | `{ enabled }` | `{}` | P |
| `calls.start` | `{ peer }` | `{ call_id }` | P |
| `calls.accept` | `{ call_id }` | `{ ok: true }` | P |
| `calls.decline` | `{ call_id }` | `{ ok: true }` | P |
| `calls.hangup` | — | `{ ok: true }` | P |
| `calls.status` | — | `{ state, peer, call_id, since_ms }` | P |
| `screen.start` / `screen.stop` | — | `{}` | P |
| `screen.frame` | `{ keyframe, data }` — `data` hex, ≤ 1 MiB decoded | `{}` | P |
| `camera.start` / `camera.stop` | — | `{}` | P |
| `camera.frame` | `{ keyframe, data }` — same bounds | `{}` | P |
| `video.interest` | `{ hidden: [{ peer, streams: ["camera"\|"screen"] }] }` | `{}` | P |

`active` in `voice.status` is `null` or
`{ group_id, channel_id, is_call, muted, deafened, participants: [{ pubkey,
speaking, muted, deafened, volume, server_muted, server_deafened,
priority_speaker }] }`.

`video.interest` declares what the client is **not** displaying. The mask is
negative on purpose: **saying nothing must never turn a stream off.** A client
that never calls it keeps receiving everything; an unknown stream name is
ignored rather than rejecting the whole declaration; a lost datagram leaves a
stream on, never dark. The declaration is soft state — it expires after ~10 s
and must be reaffirmed while anything is still hidden.

`screen.frame` and `camera.frame` take **already-encoded** video bytes: the
node fragments and forwards them, it does not encode. Capture and encoding are
the client's problem (the desktop interface uses WebCodecs in the WebView).

### 3.10 Network and diagnostics

| Method | Parameters | Result | Tier |
|---|---|---|---|
| `network.status` | — | `{ p2p_port, local_addrs, bootstrap, connected_peers, dht_nodes, external_addr, port_mapping, lan_peers, nat_kind }` | S |
| `network.add_peer` | `{ addr }` | up-to-date network status | S |
| `network.remove_peer` | `{ addr }` | up-to-date network status | S |
| `network.peers` | — | `[{ pubkey, live, addr, transport, relay, last_recv_age_ms, rtt_ms, last_delivery_ms }]` | S |
| `diagnostics.counters` | — | `{ punch, relay, mailbox, outbox, reconnect }` — local, since node start | S |
| `diagnostics.selftest` | — | bounded network self-test (seconds) | S |
| `diagnostics.report` | — | redacted bundle, safe to attach to a bug report | S |

All of these need the network subsystem; without it they return the
application error `introuvable : sous-système réseau indisponible`.

🔒 **`diagnostics.report` is the only response in this API designed to leave
the machine.** It is redacted in the node — friends become anonymous ranks with
no public key (a friend's key *is* their friend code; a raw report would hand
over the address book, and two reports could be cross-referenced to prove two
people know each other) and no address (third-party data from someone who was
never asked); the user's own external address keeps its port and loses its host.
**Never rebuild this report client-side from `network.peers`**, which carries
both removed fields.

Nothing here is telemetry. No counter, report or self-test leaves the machine
unless a human sends it.

### 3.11 Planning (all local)

| Method | Parameters | Result | Tier |
|---|---|---|---|
| `schedule.list` | — | `{ scheduled: [ … ] }` — soonest first | S |
| `schedule.cancel` | `{ id }` | `{ ok: true }` | S |
| `schedule.reschedule` | `{ id, fire_at }` | `{ ok: true }` | S |
| `reminders.add` | `{ scope, scope_id, msg_ref?, note?, fire_at }` | `{ id }` | S |
| `reminders.list` | — | `{ reminders: [ … ] }` | S |
| `reminders.dismiss` | `{ id }` | `{ ok: true }` | S |
| `backup.status` | — | `{ cadence, dir, last_backup_at, next_due_at, due }` | S |
| `backup.schedule` | `{ cadence, dir? }` | `{ ok: true }` | S |
| `backup.record_done` | `{ at? }` | `{ ok: true }` | S |
| `backup.run_now` | — | `{ ok: true }` — re-emits `event.backup_due` | S |

Nothing here crosses the wire at schedule time. A due message is sent by the
maintenance loop through the normal send path; reminders and backup nudges are
local notifications only. `backup.*` schedules and detects due windows — it
does **not** write the archive; that is the host export flow, which stops the
node and re-verifies the passphrase.

### 3.12 Events

Documented in `docs/API.md`, treated as contract (**S**):

`event.dm` · `event.dm_typing` · `event.dm_read` · `event.friend_request` ·
`event.friend_response` · `event.friend_removed` · `event.friend_verified` ·
`event.presence` · `event.profile` · `event.mention` · `event.group_op` ·
`event.group_state` · `event.group_msg` · `event.group_typing` ·
`event.group_key` · `event.voice_joined` · `event.voice_left` ·
`event.voice_speaking` · `event.voice_mute` · `event.voice_level` ·
`event.file_progress` · `event.network` · `event.reminder` ·
`event.backup_due` · `event.pairing_adopted` · `event.desynchronise`

Emitted by the node but not documented (**P**) — payloads below are derived
from the emit sites and from `app/src/lib/api.ts`, which types the same union:

| Event | Payload |
|---|---|
| `event.dm_ack` | `{ peer, msg_id }` — one of our messages was acknowledged |
| `event.dm_pins` | `{ peer }` — the local pin set of a conversation changed |
| `event.dm_self_read` | `{ peer, lamport }` — 🔒 **another device of our own account** read up to `lamport`. Not to be confused with `event.dm_read`, which is the *peer* reading *our* messages. Never emitted on the device that did the reading |
| `event.self_pref` | `{ key, value, at_ms }` — another device of this account changed a synced preference |
| `event.group_invite_pending` | `{ group_id, invite_id, group_name, inviter, expires_ms }` |
| `event.group_event_started` | `{ group_id, event_id, title }` |
| `event.soundboard_play` | soundboard sound triggered in the active voice channel |
| `event.voice_moderate` | `{ group_id, pubkey, server_muted, server_deafened, priority_speaker }` |
| `event.call_outgoing` | `{ peer, call_id }` |
| `event.call_incoming` | `{ peer, call_id }` |
| `event.call_accepted` | `{ peer, call_id }` |
| `event.call_ended` | `{ peer, call_id, reason }` — `reason` ∈ `hangup`, `declined`, `busy`, `timeout`, `missed`, `canceled`, `lost`, `superseded`, `answered_elsewhere` |
| `event.screen_state` | `{ peer, sharing }` |
| `event.screen_frame` | `{ peer, keyframe, data }` — `data` hex |
| `event.camera_state` | `{ peer, on }` |
| `event.camera_frame` | `{ peer, keyframe, data }` |

🔒 On `event.call_ended`: `answered_elsewhere` and `canceled` can both arrive
on a device whose owner *did* take the call, because a call rings on every
device of the account. **Neither proves a missed call.** A client that renders
one as "missed" will lie to its user.

---

## 4. Stability policy

### 4.1 What is frozen

- **The `auth` handshake**: first-request-only, `{ token }` in, `{ protocole }`
  out, connection closed on failure.
- **The JSON-RPC error codes** in §2. Messages are not frozen.
- **The transport shape**: JSON text frames, one request per frame, no batching,
  events as `id`-less notifications.
- **`voice.*` room semantics** (D-025) — join/leave/mute/deafen/volume/status/
  devices/mic-test, and the events that go with them.
- **The absence of secrets on this channel** (§3.2). No passphrase, recovery
  phrase or account seed will ever be added to a JSON-RPC result. This is a
  security invariant, not an API convenience.
- **`diagnostics.report`'s redaction.** Fields removed from it stay removed.

### 4.2 What may move, and how

Everything else follows the same rule as the wire protocol: **always add, never
modify.**

- **New fields may appear in any result.** A client must ignore fields it does
  not know. Every field added so far — `external_addr`, `port_mapping`,
  `lan_peers`, `nat_kind`, `read_marks`, `channel_mentions`, `path` on
  `files.status`, `dsp` on `voice.status` — arrived this way.
- **New methods and new events may appear at any time.** A client must ignore
  `event.*` names it does not know rather than treating them as errors.
- **Existing fields do not change type or meaning.** A field whose meaning must
  change gets a new name; the old one keeps its old meaning until removed under
  §4.3.
- **Optional parameters may be added.** Their absence must always preserve the
  previous behaviour — the rule that makes `video.interest` safe (§3.9) is the
  same rule.
- **Tier P is the honest exception.** Provisional methods and events may change
  shape without any of the above. If you depend on one, pin the Accord version
  and read `CHANGELOG.md` before upgrading.

### 4.3 How a breaking change would be announced

A breaking change to a **frozen** or **stable** element requires, all of them:

1. A **major version** of Accord.
2. A `CHANGELOG.md` entry under that version, in a section that names it as
   breaking, written in terms of what a client must do.
3. A **bump of `protocole`** (`API_VERSION`, `crates/accord-api/src/server.rs`),
   which is the only signal a client can act on programmatically. Check it at
   `auth` and refuse to run against a version you do not know.
4. A **deprecation period**: the old form keeps working for at least one minor
   cycle, with the replacement available from the start.

There is precedent for step 4 in the code: `groups.invite` no longer force-joins
anyone. It was kept working and now routes to `groups.invite_create`, which
requires the invitee's explicit consent (D-045). Nothing broke for callers; the
behaviour behind the name was replaced with a safe one. That is the preferred
shape of change here — and it is worth knowing as a client author that this
particular method's *effect* changed under a stable name.

### 4.4 Things that look like a contract and are not

- **Error message strings.** French, human-facing, rewordable. Branch on codes.
- **Ordering of array elements**, unless a method's documentation states it
  (`groups.state.emojis` is lexicographic by name; `schedule.list` and
  `reminders.list` are soonest-first; histories are newest-first).
- **Timing.** Nothing here is a real-time guarantee. Events are best-effort and
  a slow client is told it fell behind (`event.desynchronise`) rather than
  blocking the node.
- **The French identifiers on the wire** — `protocole`, `port_mapping:
  "aucun"`, `event.desynchronise` — are frozen *as they are*. They will not be
  renamed to English: renaming them would break every client for cosmetics,
  which is exactly what §4.2 exists to prevent.

---

## 5. The security position, stated honestly

⚠️ Opening this API to third parties **widens the local attack surface.** This
section says what protects it and what does not. Read the second list.

### 5.1 What protects it today

Verifiable in `crates/accord-api/src/server.rs` and `auth.rs`:

- **Loopback only.** The listener binds `127.0.0.1`. There is no configuration
  that changes this.
- **A 256-bit session token**, generated from the OS CSPRNG, hex-encoded,
  compared in **constant time** (`subtle::ConstantTimeEq`), never logged, with
  a `Debug` implementation that prints `AuthToken(***)`.
- **Authenticate first or be closed.** Any method before `auth` is refused and
  the connection closes. 10 s to authenticate.
- **An `Origin` allowlist** at the WebSocket handshake — defence in depth
  against DNS rebinding and WebSocket CSRF. Accepted: no `Origin` header,
  `null`, `tauri://localhost`, `https://tauri.localhost`, and
  `http(s)://{localhost,127.0.0.1,[::1]}[:port]`. Anything else gets a `403`
  before the upgrade. Lookalikes (`http://localhost.evil.com`,
  `https://tauri.localhost.evil.com`) are refused, and there are tests for
  exactly those.
- **A handshake timeout** (10 s) so a TCP connection that never upgrades cannot
  hold resources — anti-slowloris before the token is ever involved.
- **A cap of 64 simultaneous connections**; beyond it, immediate close.
- **A 16 MiB frame cap.**
- **Shutdown closes established connections.** Locking the vault does not leave
  an authenticated socket holding the node — and therefore the identity and the
  SQLCipher key — alive. There is a test whose failure message says exactly
  that.
- **Secrets are structurally absent** (§3.2, §4.1).
- **The Tauri window CSP** restricts the WebView to `ws://127.0.0.1:*` and the
  IPC bridge; no remote content is loadable into the legitimate client.

### 5.2 What does not protect it

- **The `Origin` check is not authentication.** A request with **no** `Origin`
  header is allowed — that is how the native WebView connects, and it is also
  how every non-browser local process connects. Any script, in any language,
  gets past it by simply not sending the header. The check stops a *web page*
  from reaching your node. It stops nothing else. **The token is the only real
  barrier.**
- **The token is readable by anything running as you.**
  - `accord-noded` writes `session.json` and *then* applies `0600`
    (`crates/accord-node/src/bin/accord-noded.rs`). Between the two there is a
    window where the file carries the process umask.
  - That `chmod` is `#[cfg(unix)]`. **On Windows the session file gets no
    permission restriction at all.**
  - In the desktop app the token lives in the WebView's memory, which any
    debugger attached to the process can read.
- **Authentication is all-or-nothing.** One token, no client identity, no
  scopes, no per-method allowlist. A client that can call `identity.self` can
  also call `dm.send`, `files.save`, `devices.revoke` and `groups.ban`. There is
  no audit trail of which client did what, and no way to revoke one client
  without rotating the token for all of them.
- **Every authenticated client sees everything.** Events are broadcast to all
  connections. A "read-only" client does not exist: connecting at all means
  receiving every incoming message, in real time, in plaintext.
- **There is no rate limiting** on RPC calls, nor on failed authentication
  attempts. The only bounds are the 64-connection cap and the cost of
  reconnecting. Brute-forcing 256 bits is not the concern; a wedged or hostile
  local client hammering the node is.
- **The channel is plaintext `ws://`.** Adequate against a network observer —
  nothing leaves the machine — and useless against a local process privileged
  enough to read loopback traffic.
- **Nothing sandboxes filesystem access.** See §5.4.

### 5.3 What this means

**The current trust boundary is the user account, not the process.**

Any code already running as you can read your messages and send as you — with
or without this API, because it can read the session file, attach to the
process, or read the database key from memory. Documenting the API does not
move that boundary.

What it *does* change is how many programs end up holding a live token, and how
casually. A bot with a token is a bot with your account. Treat handing one out
the way you would treat handing over your unlocked laptop.

### 5.4 Filesystem reach

Three methods reach outside Accord's own storage, with the caller's full user
rights and no path restriction:

- `files.share { path }` — reads **any file the user can read** and publishes
  it into the content store.
- `files.save { merkle_root, path }` — `std::fs::copy` to **any path the user
  can write**, overwriting what is there.
- `files.status` returns `path`, the on-disk location of a complete blob.

For the desktop application this is correct: the user picked the file in a
native dialog. For a third-party client it is arbitrary local read and write
behind one shared token. There is no allowlist, no confirmation prompt, and no
record of it.

### 5.5 What third-party support would actually require

Authentication here was designed for a **single** client — the interface
shipped in the same binary. Supporting more, safely, needs at least:

- **per-client tokens** with a human-readable name, so one can be revoked
  without breaking the others;
- **scopes** — at minimum read/write, realistically per-domain — so a bot that
  posts in one channel cannot revoke a device;
- **a user-visible grant and revoke surface**, because a permission nobody can
  see is a permission nobody can withdraw;
- **an audit line per client**, so "who deleted that" has an answer;
- **a bound on filesystem reach** for anything not the first-party client.

**None of that exists today.** Until it does, treat every client you connect as
fully trusted, and do not ship a public bot on this API expecting the node to
contain it.

---

## 6. A minimal third-party client

Illustrative. It is **not** run by the gate, and it is not a supported library —
the authoritative frame sequence is the test module at the bottom of
`crates/accord-api/src/server.rs`, which exercises the real server.

Start a node you can attach to:

```sh
ACCORD_PASSPHRASE='a test passphrase' \
ACCORD_PROFILE=/tmp/accord-demo \
ACCORD_P2P_ADDR=0.0.0.0:0 \
cargo run -p accord-node --bin accord-noded
```

Then (`pip install websockets`):

```python
import asyncio, json, pathlib, websockets

PROFILE = pathlib.Path("/tmp/accord-demo")

async def main() -> None:
    session = json.loads((PROFILE / "session.json").read_text())
    async with websockets.connect(f"ws://{session['api']}") as ws:
        # 1. Authenticate — mandatory first frame, or the server hangs up.
        await ws.send(json.dumps({"jsonrpc": "2.0", "id": 0,
                                  "method": "auth",
                                  "params": {"token": session["token"]}}))
        hello = json.loads(await ws.recv())
        if "error" in hello:
            raise SystemExit(hello["error"]["message"])
        if hello["result"]["protocole"] != 1:
            raise SystemExit("unknown API protocol version")

        # 2. One request.
        await ws.send(json.dumps({"jsonrpc": "2.0", "id": 1,
                                  "method": "identity.self", "params": {}}))

        # 3. Responses and events share the socket: a frame with no "id" is an
        #    event. Unknown event names must be ignored, never treated as
        #    errors (§4.2).
        async for raw in ws:
            msg = json.loads(raw)
            if "id" in msg:
                print("response", msg.get("result", msg.get("error")))
            elif msg["method"] == "event.desynchronise":
                print("fell behind — re-read state via *.list / *.history")
            else:
                print("event", msg["method"], msg["params"])

asyncio.run(main())
```

Four things this small example already gets right, and that a real client must
also get right:

1. `auth` is the first frame, and the connection is useless without it.
2. `protocole` is checked rather than assumed (§4.3).
3. Responses and events are distinguished by the presence of `id`, not by
   ordering.
4. Unknown events are ignored instead of raising.

---

## 7. Known divergences between this API and `docs/API.md`

Found while deriving this document from `crates/accord-node/src/service/*.rs`.
Recorded rather than quietly fixed, because a reference that hides its own
staleness is worse than one that admits it. The **code** is right in every case.

- **52 methods are implemented and absent from `API.md`**: the whole of `calls.*`,
  `screen.*`, `camera.*`, `video.*`, `prefs.*`, `voice.rooms`, the three
  `voice.set_*` DSP toggles, and 34 `groups.*` methods (threads, slowmode,
  invitations by link, entry checks, pending members, purge, stickers,
  soundboard, server events, polls, member avatar, banner, voice moderation).
  They are listed in §3.7 and §3.9.
- **16 events are emitted and absent from `API.md`**: listed in §3.12.
- `groups.list` also returns **`channel_mentions`**.
- `groups.state` also returns **`read_marks`** (`{ channel_id: lamport }`).
- `files.status` also returns **`path`** when the blob is complete.
- `files.read` accepts a **`media`** flag, which caps the auto-download at
  8 MiB — the guard that stops a malicious `MANAGE_CHANNELS` from making
  everyone auto-fetch a 2 GiB blob disguised as a server icon.
- `voice.status` returns **`dsp`**, and its `active` object carries
  **`is_call`**; participants carry **`server_muted`**, **`server_deafened`**
  and **`priority_speaker`**. `API.md`'s row predates all of these. Each is
  additive, so no client broke — which is the rule working, and also why the
  drift went unnoticed.
- `event.pairing_adopted` is documented and emitted, and **no code under
  `app/src/` references it**. The interface appears to follow the pairing
  through `devices.pair_status.adopted` instead. Stated as an observation, not
  a bug: a third-party client may still want the event.
