# Accord Threat Model — v0 accepted trade-offs

> Companion to the root [SECURITY.md](../SECURITY.md), which lists the
> guarantees Accord offers and the attackers considered. This document
> focuses on the **security trade-offs deliberately accepted for v0**,
> surfaced by an internal adversarial review before public distribution.
> For each one: the risk, why it is acceptable in v0, and the hardening
> path. Every claim below is grounded in the current code (file references
> given). Last reviewed: 2026-07-12. No external audit has been performed.

## 1. Trust architecture (recap)

- **Serverless P2P**: there is no server and no trusted third party. Every
  node talks to peers over mutually authenticated, end-to-end encrypted
  sessions: a 1-RTT handshake *inspired by Noise-XX* (condensed into two
  messages, both parties signing the transcript with their Ed25519 identity
  key — SPEC §2.2), then XChaCha20-Poly1305 AEAD with per-direction keys and
  epoch re-keying (SPEC §2.4).
- **Identities are public keys**: an identity is an Ed25519 key pair;
  `NodeId = SHA-256(pubkey)`. Creating an identity requires a 16-bit
  proof of work over `SHA-256(pubkey ‖ nonce)`, verified by every peer and
  by the DHT (anti-Sybil cost, not an anti-Sybil wall).
- **Groups are signed operation logs**: every administrative operation is
  signed by its author and validated against the permission state at its
  application point, in a deterministic total order
  (`crates/accord-core/src/group/`). Group content is encrypted under
  epoch keys rotated on every membership removal.

Everything in this document assumes those mechanisms work as specified;
attacks on them are covered by SECURITY.md §4–5 and the audit checklist.

## 2. Accepted trade-off: deterministic home relays

**What it is.** To allow first contact without any port forwarding, each
node maintains open sessions to its **home relays**: the
`HOME_RELAY_COUNT = 2` relay-flagged nodes whose `NodeId` is closest (XOR
distance) to the node's own identity
(`crates/accord-node/src/node/relay.rs`, `select_home_relays`; sessions
kept alive by `home_relay_tick` in `crates/accord-node/src/maintenance.rs`).
A sender that cannot reach a peer directly tries the deterministic
pair-key relays, then the target's home relays — both computable **from
public keys alone**, which is the point: no out-of-band signaling needed.

**Risk.** The same determinism serves the attacker. Anyone who knows a
victim's public key can compute the victim's rendezvous set. By grinding
fresh identities (one 16-bit PoW each) until a `NodeId` lands closer to
the victim's than the honest relay-flagged nodes, and advertising the
relay flag, an attacker can insert itself as one of the victim's home
relays. From that position it can:

- **observe first-contact metadata**: which identities attempt to reach
  the victim, and when;
- **selectively censor first contact**: silently drop relayed traffic for
  chosen senders.

What it can **not** do:

- **read content** — relayed traffic is opaque DATA packets of the
  end-to-end session between the two peers (SPEC §10); any tampering
  invalidates the AEAD and the packet is dropped;
- **impersonate either party** — the handshake is mutually authenticated
  by transcript signatures; the relay holds neither identity key.

**Why acceptable in v0.**

- Exposure is limited to the **first-contact fallback path**. Once a
  direct or hole-punched session exists, home relays see nothing; offline
  delivery goes through sealed mailbox deposits (SPEC §7) that hide
  sender and recipient from the storage node.
- Censorship is **detectable and recoverable**: a failed first contact is
  visible to the sender, and other paths exist (pair-key relay
  candidates, `RELAY_SELECT_K = 8`, direct dial when reachable).
- Meaningful mitigations (operator diversity requirements, larger
  rendezvous sets) need a relay population that a v0 network does not
  have yet; enforcing them now would break first contact more often than
  it would protect it.

**Hardening path.** Raise `HOME_RELAY_COUNT`; require IP/operator
diversity among a node's home relays (as the DHT already does per bucket:
at most 2 nodes per IPv4 /24); periodically re-select with an
epoch-salted derivation so a squatter's position expires; longer-term,
private contact discovery so the rendezvous set is not computable from
the public key alone.

## 3. Accepted trade-off: content-addressed blobs are bearer capabilities

**What it is.** Avatars, server banners/icons and attachments are
content-addressed blobs identified by their Merkle root. A node serves
`GetManifest` / `GetBlock` to **any authenticated session peer that
presents the root** — the only checks are per-peer rate limits and a
bounded per-peer fetch-intent table (`route_file` in
`crates/accord-node/src/runtime.rs`); there is **no per-peer ACL** in the
file service.

**Risk.** Knowing a root *is* the read capability. If a Merkle root leaks
outside its intended audience, anyone holding it can fetch the blob from
any node that seeds it. There is no revocation: once a blob has
replicated, un-sharing the root does not un-share the content.

**Why acceptable in v0.**

- Roots are 32-byte hashes: **unguessable**; they cannot be enumerated,
  only leaked.
- Roots are distributed inside end-to-end encrypted channels (messages,
  group ops), so the capability travels with the same confidentiality as
  the conversation that references it.
- The blob inherits the sensitivity of the channel that shared it; the
  model is equivalent to attaching the file itself.
- Abuse of the serving path is bounded (per-peer token buckets, bounded
  fetch-intent table).

**Operational rule.** Treat Merkle roots as **secrets scoped to their
audience**: never forward a root to third parties, never publish one in a
public place. Sharing the root is sharing the file.

**Hardening path.** Access control in the file service (e.g. serve
group-referenced blobs only to authenticated group members), and/or
provider discovery via `FILE_PROVIDER` DHT records as planned in SPEC §9
(not implemented in v0 — today discovery relies on source hints among
session peers), which would allow tighter serving policies per provider.

## 4. Accepted trade-off: server banner/icon is moderator-trusted content

**What it is.** The server banner is set by group op `0x31 SetBanner`
(the icon works the same way), a signed operation in the group log gated
by the `MANAGE_CHANNELS` permission
(`crates/accord-core/src/group/state.rs`). Members' clients resolve the
root and auto-download the blob to render the server header
(`app/src/components/Sidebar.tsx` → `lireFichier`).

**Risk.** A `MANAGE_CHANNELS` holder can point the banner at **any**
Merkle root. The wire protocol accepts manifests up to
`MAX_FILE_SIZE = 2 GiB` (`crates/accord-proto/src/limits.rs`). The
bundled client resizes what it *uploads* (≤ 512 KiB,
`app/src/lib/image.ts`).

Since v0.14.2, peer **profile** avatar/banner auto-fetches (triggered by
a peer's `Profile` message or its reconnection, with no user action) are
capped **at the node level**: `files_fetch_media`
(`crates/accord-node/src/node/files.rs`) stamps a `MEDIA_AUTO_FETCH_MAX =
8 MiB` ceiling on the fetch intent, **persisted** on the `file_fetches`
row so it survives a restart, and `on_file_manifest`
(`crates/accord-node/src/runtime.rs`) refuses a manifest that declares a
larger size and abandons the fetch. The ceiling is written only by a
profile auto-fetch, and an **explicit click-to-download always clears it**
(`media_cap = NULL`, even if a profile announcement inserted the row
first). So a click-to-download attachment carries **no** ceiling (bounded
only by `MAX_FILE_SIZE`), and a profile announcement can neither re-cap
nor cancel a root the user is downloading on purpose — regardless of
message ordering.

Since **v1.0.0** the **server** banner/icon is capped too: the client
renders it through the inline read path (`app/src/lib/files.ts` →
`lireFichier`), which marks the fetch as media, so the node applies
`MEDIA_AUTO_FETCH_MAX` (8 MiB) at manifest arrival and refuses an
oversized blob — the same ceiling already enforced for profile
avatars/banners. A `MANAGE_CHANNELS` holder can no longer make every
member auto-download a huge server image.

**Why the residual is acceptable.**

- `MANAGE_CHANNELS` is a **trust permission**, granted explicitly by
  server admins — the same trust boundary as channel moderation itself.
- The op is **signed and attributable**: the log identifies exactly which
  member set the banner, and an admin can revoke the permission and reset
  it.

**Hardening path.** Validate the declared MIME type of auto-rendered
media, and consider a tighter ceiling for decorations (the bundled client
already produces avatars/icons ≤ 512 KiB).

## 5. Accepted trade-off: DHT presence resolution exposes lookup metadata

**What it is.** Presence records (signed current addresses) and identity
records (friend code → public profile) are public by design — that is
what makes a peer findable by friend code without a server (SPEC §4).
Resolving them means querying DHT nodes chosen by key proximity.

**Risk.** The DHT nodes that store or route a lookup learn **who you are
looking for and when**. Combined over time, a well-placed set of storage
nodes can build a partial graph of who resolves whom. This is inherent to
every open DHT-based discovery system, not specific to Accord.

**Why acceptable in v0.** Serverless discovery requires *someone* to
answer lookups; records are signed and re-verified end-to-end, so storage
nodes can observe queries but cannot forge results (SECURITY.md §3.2).
Message content, group membership and mailbox contents are never exposed
by these lookups.

**Hardening path.** Cache aggressively to reduce query frequency; resolve
through disjoint paths (already done for identity records); longer-term
options (lookup indirection, private information retrieval techniques)
are research-grade and explicitly out of scope for v0.

## 6. Accepted trade-off: op-log reconciliation trusts author-set op ids

**What it is.** Group state is a replicated log of signed operations,
reconciled by anti-entropy: peers exchange a digest over their op ids
(`sync_offer` in `crates/accord-core/src/group/mod.rs`) and pull what
differs. Each op carries a 16-byte `op_id` chosen by its author, and
`insert_group_op` is idempotent on that id.

**Risk.** A **malicious member** can sign two different, individually
valid operations that share one `op_id`. Two honest peers that each folded
a different one of the pair keep divergent state, yet compute the *same*
reconciliation digest (identical id sets) — so anti-entropy believes they
are in sync and never repairs the split.

**Why acceptable in v1.**

- The attacker must already be an **authenticated member** of the server —
  an insider abusing trust, not an outside attacker.
- No confidentiality or integrity of **messages** breaks; the effect is a
  server-settings/membership view that can differ between members.
- Both ops are **signed and attributable** to the same author.

**Hardening path.** Bind `op_id` to a hash of the operation's signed
content and reject ops whose id does not match: two divergent bodies then
have distinct ids, both replicate, and last-writer-wins converges
everywhere. This is a **breaking wire change** (existing random op ids no
longer validate, so groups must be recreated), scheduled for a release
that resets group history rather than shipped silently.

## 7. Out of scope for v0

Unchanged from SECURITY.md §5, which is the authoritative list — notably:
no anonymity (IP addresses visible to peers, DHT nodes and relays),
identity public keys in cleartext during the handshake, no protection
against a compromised OS, DoS resistance is cost-raising rather than
preventive, and no external audit has been performed.
