# Audit brief — Accord

> Written **for an external auditor who has never opened this repository**. It
> exists so that scoping a security audit of Accord costs a reading afternoon
> rather than a discovery week.
>
> Every factual claim below carries a `path:line` anchor. Where a claim in the
> project's own documentation is **not** backed by code, this document says so
> instead of repeating it — see §4.1, which is the part worth reading first if
> you read nothing else.
>
> Written 2026-07-28 against commit `9a2c9af` (workspace version `7.1.0`,
> `Cargo.toml:20`). No external audit has ever been performed on this codebase.

---

## 0. What Accord is, in one paragraph

A serverless peer-to-peer end-to-end-encrypted messenger (direct messages,
Discord-style servers with channels and roles, DM groups, voice, screen share,
file transfer), written in Rust with a React/Tauri desktop client. There is no
server and no trusted third party of any kind: identities are Ed25519 key pairs,
peers find each other through a Kademlia DHT, and every byte of application data
travels inside a mutually authenticated AEAD session. The security posture is
documented at three levels of detail — [`SECURITY.md`](../SECURITY.md) (what is
guaranteed, against whom, and what is not),
[`docs/THREAT-MODEL.md`](THREAT-MODEL.md) (trade-offs deliberately accepted) and
[`docs/SPEC.md`](SPEC.md) (the wire protocol, normative).

Size, so you can price the work:

| Crate | Rust lines | Role |
|---|---:|---|
| `accord-crypto` | 4 724 | Handshake, AEAD sessions, sealed boxes, vault, pairing, ML-KEM, device keys |
| `accord-proto` | 8 200 | Every wire structure and every decoder |
| `accord-core` | 24 918 | Group op-log, messaging, offline mailboxes, SQLCipher database, blind search |
| `accord-transport` | 8 463 | UDP/TCP endpoints, fragmentation, rate limiting, relays, hole punching |
| `accord-dht` | 2 008 | Kademlia routing, record store, disjoint-path lookups |
| `accord-node` | 48 798 | Orchestration: everything above wired into a running node |
| `accord-api` | 1 028 | Local JSON-RPC surface (127.0.0.1) |
| `accord-voice` | 3 180 | Opus encode/decode, jitter buffer |
| **Total** | **101 421** | 201 files, 1 381 test functions |

**The security-critical fraction is small and well isolated.** A useful audit of
the cryptographic core (§2 tiers 0 and 1) is roughly 15 000 lines of `crypto` +
`proto` + the group and device modules of `core`/`node` — not the 101 000.

---

## 1. Ground rules that shape what you will read

**Anchor shorthand used throughout this document.** `crypto/handshake.rs:108`
means `crates/accord-crypto/src/handshake.rs:108`. The rule is mechanical: a
leading `crypto`, `proto`, `core`, `node`, `dht`, `transport`, `api`, `voice` or
`macos` expands to `crates/accord-<name>/src/`. Anything else — `docs/…`,
`ci.sh`, `crates/…/tests/…` — is written out in full. This matters because three
different files are called `device.rs` and two are called `groups.rs`.

Four project conventions explain things that would otherwise look odd:

1. **Comments and internal test names are in French; documentation is in
   English.** A `#[test] fn le_mode_hybride_ne_se_lit_jamais_dans_les_capacites_du_pair`
   is not a stray file — that is the house style.
2. **`🔒` in a comment marks a security invariant** deliberately written down at
   the point it is enforced (388 occurrences across `crates/`). **`⚠️` marks a
   known limit or trap** (81 in `crates/`, 47 in the documentation). Grepping
   for those two markers is the fastest way to find where the authors thought
   something was load-bearing.
3. **No panics in production code** is enforced by clippy, not by convention:
   `ci.sh:31` denies `unwrap_used`, `expect_used`, `panic`, `todo`,
   `unimplemented` over `--lib --bins`. The rare proven-infallible exceptions
   carry a justified `#[allow]`, e.g. `crypto/handshake.rs:467`.
4. **`ROADMAP.md` is in the repository** — since 2026-07-28, precisely because
   this brief pointed out that public documents cite it while it was untracked.
   Every `ROADMAP §x` reference now resolves. Sections are numbered as
   `## Partie N`, so "ROADMAP §7" means part 7, not a sub-heading `7`.
   ⚠️ It is a working document, not a specification: where it disagrees with
   `docs/SPEC.md` or the code, it is the one that is wrong.

---

## 2. Where to look first, ranked by consequence

Ranked by *what a break costs*, not by how much code there is. The ordering
disagrees with line counts on purpose: `accord-node` is half the repository and
sits in tier 1.

### Tier 0 — a break here breaks every user at once

**T0.1 — The handshake and the post-quantum hybrid.**
`crypto/handshake.rs` (1 055 lines), `crypto/pq.rs` (350).

This establishes every session in the system. It is also the newest code in the
repository: the ML-KEM-512 hybrid merged in the two commits immediately preceding
this brief and is described under `7.1.0` (`CHANGELOG.md:267`, released
2026-07-27, one day before this was written). It has had the least exposure of
anything in tier 0.

What to attack:

- The two transcript functions, `transcript_1` (`crypto/handshake.rs:108`) and
  `transcript_2` (`crypto/handshake.rs:122`). Everything anti-downgrade rests on
  them.
- The two *additive* absorption helpers, `absorb_capabilities`
  (`crypto/handshake.rs:81`) and `absorb_pq_material` (`crypto/handshake.rs:101`).
  Both are no-ops when the field is absent — that is what keeps a classic
  transcript byte-identical to the pre-hybrid one (`crypto/handshake.rs:1007`),
  and it is exactly the kind of "absence is invisible" construction that hides
  downgrades. The authors' argument is that the *signature* covers presence,
  since a stripped field changes what the receiver absorbs. Verify that argument
  end to end, including for a peer that emits `capabilities: Some(0)` versus
  `None`.
- The distinct domain markers `0x01` / `0x02` (`crypto/handshake.rs:83`,
  `crypto/handshake.rs:102`) — ML-KEM material must never be readable as
  capability bytes.
- `derive_keys` (`crypto/handshake.rs:147`): `IKM = X25519 ‖ ML-KEM`, both
  exactly 32 bytes, `ikm_len` switching between 32 and 64. Check that a 32-byte
  IKM is bit-identical to the pre-hybrid derivation.
- **No length prefix is read off the wire for PQ material.** Sizes are fixed
  constants (`proto/limits.rs:62`, `proto/limits.rs:66`) with the rationale
  written out: the handshake precedes session establishment, so a sender-chosen
  length would be an unauthenticated allocation lever. Confirm the decoder
  honours that.
- `peer_capabilities_sans_mode` (`crypto/handshake.rs:193`): `CAP_PQ_HYBRID` is
  stripped unconditionally from the retained peer capabilities, in both roles,
  because the bit means "I can" in a HELLO and "I did" in a WELCOME (SPEC §2.2.2,
  `docs/SPEC.md:183`). The invariant is `peer_capabilities & CAP_PQ_HYBRID == 0`,
  always.
- Identity binding on the initiator side (`crypto/handshake.rs:307`),
  constant-time, before any derivation.
- `CookieJar` (`crypto/handshake.rs:447`): stateless anti-DoS, HMAC over
  `addr ‖ static_pub`, 2-minute rotation. Note the cookie is bound to the source
  *address string*.

**T0.2 — AEAD session state.** `crypto/session.rs` (383 lines). Directional nonce
construction (`crypto/session.rs:40`), the 1 024-counter sliding replay window
(`crypto/session.rs:48`), epoch ratchet and key wiping
(`crypto/session.rs:183`), AAD coverage of the header (`crypto/session.rs:197`,
`crypto/session.rs:254`).

**T0.3 — The group epoch key lifecycle.** `core/group/crypt.rs`,
`core/group/mod.rs:360`–`core/group/mod.rs:424`, `node/node/mod.rs:2490`,
`core/db/groups.rs:214`–`core/db/groups.rs:261`.

**Start here.** Two claimed properties do not appear to hold — see §4.1. Even if
you disagree with the reading there, this is the least-defended path in tier 0.

**T0.4 — Identity, devices, and the account root.**
`docs/MULTI_DEVICE.md` (1 013 lines, the design document — read §2, §3.3, §4.2,
§4.3 and §8 before the code), `proto/device.rs`, `crypto/device.rs`,
`node/device.rs`, `node/node/mod.rs:452`–`node/node/mod.rs:574`.

An identity is an **account** root key that signs a device list; each device has
its own key. Since a design reversal documented at `docs/MULTI_DEVICE.md:537`,
**the account root itself travels to a newly paired device**. The consequences
are stated honestly in `SECURITY.md:393` (item 12) and `SECURITY.md:407`
(item 13): revocation cannot take the root back, and there is no root rotation
because the friend code *is* the root's public key.

Verify: signature coverage including `version` (`crypto/device.rs:183`), the
deliberate check ordering — account identity before signature, to avoid spending
Ed25519 verifications on foreign lists (`crypto/device.rs:150`) — decode bounds
`MAX_DEVICES = 8` / `MAX_REVOKED = 32` (`proto/device.rs:23`,
`proto/device.rs:28`), freshness (`proto/device.rs:151`), and the two-half proof
in `device_list_proves_owner` (`node/node/mod.rs:517`).

⚠️ The clock-skew guard on `issued_ms` lives at the **persistence** chokepoint
(`node/node/mod.rs:700`), not inside `verify_device_list`
(`crypto/device.rs:171`). The in-memory `proven_lists` cache is written at
`node/node/mod.rs:469` → `node/node/mod.rs:562`, before that guard. Worth
checking whether a future-dated list can authorise a device through the in-memory
path even though it can never be persisted.

**T0.5 — Pairing and the account-seed transfer.** `crypto/pairing.rs` (620
lines), `node/pairing.rs`, `node/node/mod.rs` (`send_account_seed`,
`ingest_pairing_seed`).

The project's own summary of what to check is unusually good and already written:
`SECURITY.md:583`–`SECURITY.md:598`. The load-bearing subtlety, stated at
`SECURITY.md:193`: **completing a symmetric SPAKE2 proves nothing** — both sides
derive *a* key even with different codes. The only cryptographic proof of code
knowledge is opening the sealed payload. Confirm every downstream decision rests
on the opening and not on the exchange completing.

### Tier 1 — a break here breaks a group, a conversation, or a node

**T1.1 — The group op-log and its permission whitelist.** `core/group/state.rs`
(5 288 lines — the largest file in the repository and the single densest
concentration of authorisation logic), `core/group/mod.rs:261` (`ingest_op`).

The model: a replicated log of Ed25519-signed operations, folded in a
deterministic total order (`sort_canonical`, `core/group/mod.rs:428`); an
operation not authorised by the state *at its application point* is **ignored**,
not rejected, so every honest peer converges on the same ignore
(`core/group/state.rs:2`, `core/group/state.rs:682`).

Read in this order:

1. `ingest_op` (`core/group/mod.rs:261`): signature, then the content-addressed
   `op_id` invariant (`op_content_id`, `core/group/mod.rs:205`), then body
   decode, then insert. Note the **two grandfathering escapes**:
   `group_is_legacy` (`core/group/mod.rs:248`) and the root-commitment regime
   (`is_committed_create`, `core/group/mod.rs:225`). Legacy groups keep the
   historic `op_id`-collision weakness by design — `docs/THREAT-MODEL.md:239`.
2. `GroupState::apply` (`core/group/state.rs:682`) — the gates in order: CREATE
   only as the very first op (`core/group/state.rs:690`), founder must exist
   (`core/group/state.rs:709`), **author must be a member**
   (`core/group/state.rs:712`), the DM-group whitelist
   (`core/group/state.rs:720` → `refus_en_groupe_mp`,
   `core/group/state.rs:1741`), then the permission closure `has`
   (`core/group/state.rs:739`).
3. The `has` closure is the whole authorisation model in four lines
   (`core/group/state.rs:736`). In a normal group:
   `perms_of_author & (p | ADMIN) != 0 || fondateur`. In a DM group it is
   replaced by a fixed open set `INVITE | MANAGE_CHANNELS | KICK` — and the
   comment at `core/group/state.rs:731` explains that this is safe *only because*
   the whitelist above already restricted `Kick` to self-departure. That is a
   composition of two guards where either alone is insufficient; it is exactly
   the kind of coupling that a later refactor breaks. The regression it guards
   against is written out at `core/group/state.rs:1754`.
4. `MANAGE_CHANNELS` appears as the gate for ~20 distinct operations. Check the
   ones where authorship is an alternative to the permission
   (`core/group/state.rs:1382`, `core/group/state.rs:1418`,
   `core/group/state.rs:1554`, `core/group/state.rs:1567`) — those are the
   asymmetric ones.

**T1.2 — The wire decoders.** `accord-proto` (8 200 lines).

Strict decoding is the stated contract: any out-of-bounds length, invalid UTF-8
or trailing byte rejects the whole structure (`proto/wire.rs:1`,
`proto/wire.rs:12`). Numeric guard rails are centralised in `proto/limits.rs` and
mirrored in `docs/SPEC.md:910` — and that mirror is **machine-checked** by
`scripts/check-doc-constants.mjs`, wired into the gate at `ci.sh:87`.

The largest decoder is `proto/core_msg.rs` (3 918 lines, ~50 message kinds). It
is fuzzed (§5), and the fuzzing found nothing — read §5's caveats about what that
does and does not mean.

⚠️ One asymmetry worth an hour: the **encoder's** only guard against a length
that does not fit its prefix is a `debug_assert!`, which is compiled out in
release — `proto/wire.rs:100` (`put_vbytes`, then `v.len() as u16`),
`proto/wire.rs:107` (`put_lbytes`), `proto/wire.rs:130` (`put_list`). A caller
that ever exceeds the bound silently emits a truncated length prefix in a release
build. Every present caller looks bounded (voice payloads at 1 200 bytes,
`proto/plaintext.rs:286`; text at `MAX_TEXT_BYTES`, `proto/limits.rs:22`), but
nothing in CI enforces that, and this project has already shipped one
four-release outage caused by a `debug_assert` behaving differently in release
(`CONTRIBUTING.md:229`).

**T1.3 — DHT record validation and lookups.** `dht/store.rs` (validation:
signature, key↔kind consistency, size, expiry, per-publisher quota, ±5 min clock
skew at `dht/store.rs:160`), `dht/lookup.rs:230` (`find_value_bounded`).

Note the disjoint-path selection policy is **stronger than SECURITY.md
describes**: the code takes the record confirmed by the most disjoint paths and
breaks ties by the (bounded) most recent timestamp (`dht/lookup.rs:225`),
whereas `SECURITY.md:117` says only "the most recent valid signed value wins".
Two paths are used for *every* `get`, not only identities (`dht/node.rs:30`).

### Tier 2 — worth a pass, lower blast radius

- **At rest**: vault Argon2id parameters and the absence of an unlock oracle
  (`crypto/vault.rs:51`, `crypto/vault.rs:105`); SQLCipher whole-file encryption;
  the backup archive (`crypto/archive.rs`); the blind search index and its
  acknowledged frequency leak (`core/search.rs`, `SECURITY.md:378`).
- **Offline mailboxes**: signed-then-sealed, recipient-bound, opaque storage keys
  (`core/offline.rs:109`, `core/offline.rs:131`, `core/offline.rs:209`).
- **Local surface**: 127.0.0.1 binding and constant-time token comparison
  (`api/auth.rs:39`); the Tauri IPC/CSP split that keeps secrets off the
  WebSocket (`app/src-tauri/`, `docs/API_CONTRACT.md`).
- **Transport anti-DoS**: `transport/ratelimit.rs`, `transport/endpoint.rs`, and
  the bounded fragment reassembly specified at `docs/SPEC.md:941`.
- **Relay and NAT**: `docs/NAT_TRAVERSAL.md`, `docs/NAT-FIRST-CONTACT.md`, and
  the accepted deterministic-home-relay trade-off at `docs/THREAT-MODEL.md:39`.

---

## 3. Claimed properties, with enforcement site and pinning test

Read as: *the project claims X; X is enforced at Y; a test that fails if Y is
removed is Z.* Where the "pinned by" cell is empty or hedged, that is the
finding.

### Transport

| Claim | Enforced at | Pinned by |
|---|---|---|
| Mutual authentication over a full transcript | `crypto/handshake.rs:284`, `crypto/handshake.rs:316` | `crypto/handshake.rs:597` `tampered_hello_rejected`; `crypto/handshake.rs:611` `tampered_welcome_rejected` |
| Forward secrecy per session (ephemeral-ephemeral X25519) | `crypto/handshake.rs:267`, `crypto/handshake.rs:383` | `crypto/handshake.rs:516` `full_handshake_derives_same_keys` |
| A targeted dial cannot be answered by a third party | `crypto/handshake.rs:307` (constant-time) | `crypto/handshake.rs:546` `welcome_from_wrong_identity_rejected` |
| Handshake replay refused (nonce cache, 5 min) | `crypto/handshake.rs:220`, `crypto/handshake.rs:381` | `crypto/handshake.rs:565` `replayed_hello_rejected` |
| Clock skew ±90 s | `crypto/handshake.rs:197`, `proto/limits.rs:37` | `crypto/handshake.rs:578` `stale_timestamp_rejected` |
| Identity PoW verified by the peer | `crypto/handshake.rs:313`, `crypto/handshake.rs:376` | `crypto/handshake.rs:625` `insufficient_pow_rejected` |
| PQ downgrade is not silently possible | transcript absorption, `crypto/handshake.rs:81`, `crypto/handshake.rs:101` | `crypto/handshake.rs:898` `stripping_the_pq_capability_bit_breaks_the_handshake`; `crypto/handshake.rs:917`; `crypto/handshake.rs:934`; `crypto/handshake.rs:969` |
| A classic transcript is byte-identical to the pre-hybrid one | `crypto/handshake.rs:101` (absent ⇒ no-op) | `crypto/handshake.rs:1007` `classic_transcript_is_unchanged_by_this_milestone` |
| Session key depends on **both** secrets | `crypto/handshake.rs:147` | `crypto/handshake.rs:813` `hybrid_handshake_derives_a_key_from_both_secrets` |
| An unsolicited ML-KEM ciphertext is refused | `crypto/handshake.rs:332` | `crypto/handshake.rs:985` `unsolicited_pq_ciphertext_is_refused` |
| Hybrid HELLO/WELCOME stay under the UDP MTU | `proto/limits.rs:62`, `proto/limits.rs:66` | `crypto/handshake.rs:1023` `hybrid_hello_and_welcome_stay_under_the_udp_mtu` |
| ML-KEM conformance to FIPS 203 / ACVP verified **here**, not inherited | `crypto/pq.rs` | `crypto/pq.rs:241`, `crypto/pq.rs:257`; vectors in `crates/accord-crypto/tests/vectors/` |
| In-session replay window (1 024 counters) | `crypto/session.rs:48` | `crypto/session.rs:298` `replay_rejected`; `crypto/session.rs:371` `old_window_replay_rejected` |
| Header integrity via AAD | `crypto/session.rs:197`, `crypto/session.rs:254` | `crypto/session.rs:326` `tampered_header_rejected_via_aad` |
| Directional key separation | `crypto/handshake.rs:160` | `crypto/session.rs:334` `wrong_direction_rejected` |
| Epoch re-keying wipes old keys | `crypto/session.rs:183`, `crypto/session.rs:107`, `crypto/session.rs:130` (`Drop`) | `crypto/session.rs:342` `rekey_epoch_transition` |

### Identity and devices

| Claim | Enforced at | Pinned by |
|---|---|---|
| The device list is signed by the account root over its whole content, version included | `crypto/device.rs:183` | `crypto/device.rs:341` `any_tampering_breaks_the_signature` |
| A list for another account is refused | `crypto/device.rs:177` | `crypto/device.rs:373` `a_list_from_another_account_is_refused` |
| Version monotonicity (no replay of an old list) | `crypto/device.rs:180` | `crypto/device.rs:384` `an_older_or_equal_version_is_ignored` |
| Decode bounds (8 devices, 32 revocations, 8 KiB) | `proto/device.rs:210`, `proto/device.rs:219`, `proto/limits.rs:19` | `proto/device.rs:333` |
| A stale list authorises nobody | `proto/device.rs:151` | `node/device.rs:717`, `node/device.rs:1023` |
| A revoked device stops receiving | `node/device.rs` (routing) | `node/device.rs:837`, `node/device.rs:1161` |
| A future-dated list cannot lock revocation | `node/node/mod.rs:700` | (verify: not applied on the in-memory path, §2 T0.4) |
| A device key is never the account key | `crypto/device.rs` | `crypto/device.rs:290` `a_device_key_is_never_the_account_key` |
| The same recovery phrase yields different device keys | `crypto/device.rs` | `crypto/device.rs:308` `the_same_recovery_phrase_yields_different_devices` |

### Groups

| Claim | Enforced at | Pinned by |
|---|---|---|
| Every op is signed and attributable | `core/group/mod.rs:262` | `core/group/mod.rs:638`, `core/group/mod.rs:695` |
| `op_id` is content-addressed (no silent divergence) | `core/group/mod.rs:205`, `core/group/mod.rs:278` | `core/group/mod.rs:671` |
| A CREATE cannot take over a root-committed group | `core/group/mod.rs:225`, `core/group/mod.rs:273` | `core/group/mod.rs:809`, `core/group/mod.rs:827`, `core/group/mod.rs:901` |
| An unauthorised op is ignored identically everywhere | `core/group/state.rs:682` (`Applied::Ignored`) | `crates/accord-core/tests/proptest_group.rs` (deterministic fold) |
| A DM group cannot gain a second channel, or a member without consent | `core/group/state.rs:1741` | tests in `core/group/state.rs`; `node/service/tests.rs:1079` |
| A pushed op-log cannot force-join you | `node/node/mod.rs:2366` | — (consent gate; confirm coverage) |
| Group message AEAD is bound to (group, channel, message, epoch) | `core/group/crypt.rs:53` | `core/group/crypt.rs:139` |
| **Mandatory epoch rotation on kick/ban/leave** | **nothing in the running node** | **see §4.1, finding 1** |
| **A group key can only be installed by a group member** | **not enforced** | **see §4.1, finding 2** |

### DHT

| Claim | Enforced at | Pinned by |
|---|---|---|
| Records are signed and key↔kind consistent | `dht/store.rs` | `dht/store.rs:335`, `dht/store.rs:344`, `dht/store.rs:376`, `dht/store.rs:396` |
| Expiry ≤ 7 days, size ≤ 8 KiB | `dht/store.rs`, `proto/limits.rs:19`, `proto/limits.rs:90` | `dht/store.rs:354` |
| A record dated in the future is refused | `dht/store.rs:160` | `dht/store.rs:411` `future_timestamp_rejected` |
| Per-publisher quota | `dht/store.rs:174` | `dht/store.rs:434` `publisher_quota_enforced` |
| A device list can only be published at its own account's key | `dht/store.rs` | `dht/store.rs:295` |
| Disjoint-path lookups resist a single compromised path | `dht/lookup.rs:230`, `dht/node.rs:30` | `dht/lookup.rs:381` `find_value_prefers_path_consensus`; `dht/lookup.rs:351` |
| An unknown record kind never kills a whole response | `accord-proto` (`RecordKind::Unknown`) | `dht/store.rs:312`, `dht/store.rs:465` |

### At rest and local surface

| Claim | Enforced at | Pinned by |
|---|---|---|
| Vault sealed under Argon2id (64 MiB, t=3, p=4) | `crypto/vault.rs:51`, `crypto/vault.rs:72` | `crypto/vault.rs:164`, `crypto/vault.rs:173` |
| Sealed boxes bind to the recipient | `crypto/sealed.rs:20`, `crypto/sealed.rs:33` | `crypto/sealed.rs:98` `wrong_recipient_fails`; `crypto/sealed.rs:106` |
| Offline deposits cannot be redirected | `core/offline.rs:109` | `core/offline.rs:260` `envelope_cannot_be_redirected`; `core/offline.rs:305` |
| Local API token compared in constant time | `api/auth.rs:39` | (`subtle::ConstantTimeEq`; note slice `ct_eq` still short-circuits on length) |
| `diagnostics.report` is redacted in the node, not the UI | `node/node/diagnostics.rs` | `node/node/diagnostics.rs:482` `le_rapport_ne_porte_ni_cle_ni_adresse_d_ami`; `node/node/diagnostics.rs:551` |
| No `unsafe` in the sensitive crates | `#![forbid(unsafe_code)]` in 8 of 9 crate roots | compiler; the only exception is `accord-macos` (two Objective-C `msg_send!` blocks, `macos/lib.rs:46`, `macos/lib.rs:70`) |
| No `unwrap`/`expect`/`panic!` outside tests | `ci.sh:31` | the gate itself |
| **Secrets never reach a log** | a convention on `tracing` call sites (`SECURITY.md:283`) | **nothing** — see §4.3, item 5 |

---

## 4. What is already known to be weak, unverified, or contradicted

### 4.1 Findings raised while writing this brief — unreviewed by the project

These were found by tracing call graphs, not by exploitation. They are stated
with exact anchors so you can dismiss them in ten minutes if the reading is
wrong. **None of them has been through the project's own review.**

---

**Finding 1 — Group epoch keys are never rotated in the shipped node. The
"mandatory rotation on kick/ban/leave" property is prose only.**

Claimed in three places:

- `SECURITY.md:133` — "**Mandatory rotation on every kick/ban/departure**
  (SPEC §6.4): the departed member cannot decrypt subsequent messages."
- `docs/SPEC.md:632` — normative: "**Mandatory rotation** on every
  KICK/BAN/LEAVE: a member holding MANAGE_ROLES/ADMIN … generates epoch+1 and
  distributes it to all remaining members."
- `docs/THREAT-MODEL.md:33` — "Group content is encrypted under epoch keys
  rotated on every membership removal."

The implementation exists and looks correct: `rotate_key`
(`core/group/mod.rs:368`) picks the next epoch, generates a key, and seals it for
every remaining member; `is_rotation_responsible` (`core/group/mod.rs:360`)
implements the deterministic responsibility rule.

**`rotate_key` has no caller outside its own unit tests.** The only references in
the whole workspace are its definition and two assertions at
`core/group/mod.rs:1009` and `core/group/mod.rs:1012`.

The kick and ban entry points author the operation and do nothing else:
`node/node/groups.rs:1326` (`group_kick`) and `node/node/groups.rs:1331`
(`group_ban`) are one-line wrappers around `group_author(...)`.

Following the key lifecycle confirms it. A group key is written in exactly three
places: at group creation, always epoch 1 (`core/group/mod.rs:160`); inside
`rotate_key`, which never runs (`core/group/mod.rs:384`); and on receipt of a
`CoreMsg::GroupKey` (`accept_sealed_key`, `core/group/mod.rs:416`). Adding a
member re-seals the *existing* key (`seal_current_key_for`,
`core/group/mod.rs:397`, used at `core/group/invite.rs:394` and
`core/group/invite.rs:492`) — it does not create an epoch.

**Consequence if the reading holds**: a kicked or banned member keeps a valid
group key indefinitely. Removal stops them being *sent* messages; it does not
stop them reading any group ciphertext they can still obtain. The named
guarantee — "the departed member cannot decrypt subsequent messages" — does not
hold.

**What to check to confirm or refute**: whether any path reachable from the UI or
the JSON-RPC API reaches `rotate_key`. A repository-wide `grep -rn rotate_key`
returns three lines, all in `accord-core`; `grep -rn rotat crates/accord-api app/src`
returns only CSS transforms.

---

**Finding 2 — Any session peer can install a group key, at any epoch, into any
group you have joined.**

`CoreMsg::GroupKey` is handled at `node/node/mod.rs:2490`. The gate is local
membership only — `db.group_membership(&group_id) != None`
(`node/node/mod.rs:2498`) — i.e. *that I have joined the group*, not that *the
sender is in it*. The sender's key is never compared against the group state.

The comment at `node/node/mod.rs:2502` states the reasoning: "La clé n'est
acceptée que si elle s'ouvre avec notre clé privée ; un tiers ne peut pas nous en
imposer une fausse" — *the key is accepted only if it opens with our private key,
so a third party cannot impose a false one*. **That inference does not follow
from the primitive.** `sealed::seal` (`crypto/sealed.rs:33`) takes only the
recipient's public key and generates its own ephemeral: it is an anonymous sealed
box with no sender authentication of any kind. Opening it proves only that the
sealer knew the recipient's public key — which is the recipient's friend code,
and public by design (`SECURITY.md:364`).

The stored epoch is taken from the wire (`node/node/mod.rs:2492`,
`node/node/mod.rs:2509`) and `latest_group_key` selects
`ORDER BY key_epoch DESC LIMIT 1` (`core/db/groups.rs:245`), while
`put_group_key` is `INSERT OR IGNORE` (`core/db/groups.rs:215`) — so a *new,
higher* epoch is always accepted and immediately becomes the key used to compose
outgoing messages (`core/group/msg.rs:227`).

**Composed with finding 1 this is worse than either alone**: since no legitimate
rotation ever occurs, every group in the wild sits at epoch 1, so *any* epoch > 1
arriving is by construction illegitimate — and it will still be preferred.

**Consequence if the reading holds**: a removed member (who still knows the
`group_id` and can still open a session) pushes a `GroupKey` at a high epoch to
each remaining member; from then on the group composes under a key the attacker
chose. This defeats removal more completely than the missing rotation does.

**What to check to confirm or refute**: whether anything upstream of
`ingest_core_from` (`node/node/mod.rs:1979`) restricts who may send a
`CoreMsg::GroupKey` — there is no global friend gate at the top of that function;
individual arms apply their own (`node/node/mod.rs:2563` does,
`node/node/mod.rs:2490` does not). Also whether an attacker can realistically
learn a `group_id`; it is derived from the CREATE op content
(`core/group/mod.rs:214`) and travels inside E2E sessions, so the realistic
attacker is a current or former member.

---

**Finding 3 — a stranger's group ops are stored and re-replicated.** `ingest_op`
(`core/group/mod.rs:261`) verifies the *author's* signature but never checks that
the author is a member; membership is only checked at fold time
(`core/group/state.rs:712`), which decides state, not storage.
`insert_group_op` (`core/db/groups.rs:59`) has no size or rate bound. Ops
therefore enter the log, are hashed into the anti-entropy digest (`sync_offer`,
`core/group/mod.rs:296`) and propagate to every member through `should_pull`
(`core/group/mod.rs:329`). State converges correctly; storage and sync traffic do
not. Severity depends on whether `CoreMsg::GroupOpMsg` is rate-limited per peer
upstream — worth ten minutes.

**Observation (not a finding)** — a nine-line comment block is duplicated
verbatim at `node/node/mod.rs:472` and `node/node/mod.rs:483`, around the
device-list ingestion gates. Harmless in itself; a signal that the surrounding
branch was edited under pressure and deserves a slow read.

### 4.2 Trade-offs the project accepts knowingly

All four are argued in full — risk, why acceptable, hardening path — in
[`docs/THREAT-MODEL.md`](THREAT-MODEL.md). Do not spend audit time rediscovering
them; do spend it checking the reasoning:

1. **Deterministic home relays** (`docs/THREAT-MODEL.md:39`). Each node keeps
   sessions open to the two relay-flagged nodes closest to its own `NodeId` — a
   set computable from the public key alone. An attacker who grinds PoW
   identities *and is genuinely reachable* can become a victim's home relay and
   observe or censor first-contact metadata. The M1a hardening
   (`verified_relays`, `node/runtime.rs`) defeats the cheap unreachable-flood
   variant only, and the document says so.
2. **Content-addressed blobs are bearer capabilities**
   (`docs/THREAT-MODEL.md:107`). Knowing a Merkle root *is* the read capability;
   there is no per-peer ACL in the file service (`route_file`,
   `node/runtime.rs`), only rate limits. No revocation.
3. **Server banner/icon is moderator-trusted content**
   (`docs/THREAT-MODEL.md:144`). Any `MANAGE_CHANNELS` holder points it at an
   arbitrary root; the residual is bounded by `MEDIA_AUTO_FETCH_MAX = 8 MiB`
   (`node/node/files.rs`).
4. **DHT lookups expose who resolves whom** (`docs/THREAT-MODEL.md:194`).
   Inherent to open DHT discovery.

Plus the twenty-two items of `SECURITY.md:354`–`SECURITY.md:530` ("Guarantees
explicitly NOT offered" — numbered 1–17, then 15–19 a second time, see §4.3
item 1), of which the ones that most often surprise people: no
anonymity; identity public keys in cleartext in HELLO/WELCOME; **authentication
is not post-quantum** — every signature is Ed25519 (`SECURITY.md:444`); the
hybrid covers the *transport session only*, not group epoch keys or mailbox
deposits (`SECURITY.md:459`); `ml-kem` is itself unaudited and unproven
(`SECURITY.md:469`, rationale at `crypto/pq.rs:28`); physical presence at an
unlocked authorised device yields the account root (`SECURITY.md:393`);
revocation is eventually consistent and cannot take the root back
(`SECURITY.md:407`); `session.json` is unprotected outside Unix
(`SECURITY.md:528`).

### 4.3 Documentation defects that will cost you time

1. **`SECURITY.md` §5 numbers items 15, 16 and 17 twice.** The first triple
   (`SECURITY.md:444`, `SECURITY.md:459`, `SECURITY.md:469`) is about
   post-quantum scope; the second (`SECURITY.md:474`, `SECURITY.md:487`,
   `SECURITY.md:498`) is about blocking, device-list dates and silent write
   failures. Every cross-reference into that range is ambiguous — including
   `docs/FUZZING.md:47` ("`SECURITY.md` 16 and 17") and the user-facing
   `CHANGELOG.md:285` ("SECURITY.md §5.15–5.17"). Numbering then continues 18,
   19.
2. **`SECURITY.md:58` says the fuzz harness has 8 targets; there are 9.** The
   audit checklist at `SECURITY.md:578`–`SECURITY.md:582` lists eight by name and
   the one it omits is `device_list` — which `docs/FUZZING.md:45` describes as
   *"the structure at the heart of multi-device identity and revocation"* and the
   target deliberately run first and longest. An auditor working from the
   checklist would skip exactly the target the fuzzing report considers most
   important.
3. **`SECURITY.md:548`–`SECURITY.md:551` contradicts `docs/THREAT-MODEL.md:174`.**
   The summary still says "clients should cap the size of auto-downloaded
   previews (**the v0 client does not yet**)"; the detailed document says the
   server banner/icon has been capped at 8 MiB since v1.0.0. The detailed
   document is the current one.
4. **`SECURITY.md:117` understates the DHT divergence rule** — it says "most
   recent valid signed value wins", the code implements path-consensus with a
   timestamp tie-break (`dht/lookup.rs:225`). Also, disjoint paths are used for
   *every* `get`, not only "sensitive values (identities)" (`dht/node.rs:30`).
5. **"Secrets never logged" (`SECURITY.md:169`, `SECURITY.md:283`) is enforced by
   nothing.** It is a rule on `tracing` call sites across the repository. There
   is no lint, no test, and no scanner: `scripts/` contains only
   `check-bundle-budget.mjs`, `check-doc-constants.mjs` and `check-file-size.mjs`.
   The redaction of `diagnostics.report` *is* tested
   (`node/node/diagnostics.rs:482`); the general property is not. A diagnostic
   log is written on every launch to `<app_data>/logs/accord.log`, outside the
   encrypted database.
6. ~~**`ROADMAP.md` is cited but untracked**~~ — **fixed 2026-07-28**: it is
   versioned, and every `ROADMAP §x` reference resolves (see §1, item 4).

---

## 5. What the project has already done to check itself

Read this section as *coverage you do not need to repeat* — and, just as
importantly, as *coverage that does not mean what its headline number suggests*.
The project is unusually candid about this itself; `docs/FUZZING.md` is worth
reading in full before you decide what to re-do.

### 5.1 Fuzzing — nine targets, 746 M executions, zero crashes

`docs/FUZZING.md`. Targets in `fuzz/fuzz_targets/`: `proto_decode`, `core_msg`,
`group_op_body`, `group_state`, `handshake_decode`, `dht_record`, `device_list`,
`file_manifest`, `backup_archive` — one per decoding surface that takes bytes
from a stranger, with the standing rule that a new decoding surface gets a new
target.

The 2026-07-27 campaign (Apple M1 Pro, `cargo +nightly fuzz`), summing the two
tables at `docs/FUZZING.md:38` and `docs/FUZZING.md:75`:

| Target | Duration | Executions | Crashes |
|---|---:|---:|---:|
| `group_op_body` | 601 s | 136 741 712 | 0 |
| `dht_record` | 601 s | 126 857 208 | 0 |
| `core_msg` | 601 s | 126 750 559 | 0 |
| `file_manifest` | 601 s | 120 399 425 | 0 |
| `proto_decode` | 601 s | 112 857 325 | 0 |
| `device_list` | 301 s | 70 391 994 | 0 |
| `handshake_decode` | 241 s | 47 127 339 | 0 |
| `group_state` | 241 s | 4 944 821 | 0 |
| `backup_archive` | 601 s | 21 926 | 0 |
| **Total** | **~73 min** | **746 092 309** | **0** |

**Four caveats, three of them the project's own** (`docs/FUZZING.md:51`,
`docs/FUZZING.md:85`):

- **Executions are not coverage, and the column is not comparable across rows.**
  `group_state` replays an op-log per input; `backup_archive` runs argon2 and a
  zip decryption on *every* input, hence ~36 exec/s against ~200 000/s for the
  pure decoders. Its four-orders-of-magnitude-lower row is a cost difference, not
  a coverage gap.
- **Fuzzing finds crashes, not wrong answers.** Every defect the project's own
  adversarial reviews found — a refused write reported as success, an unbounded
  timestamp — is a decoder returning cleanly and a *caller* drawing the wrong
  conclusion. No fuzzer would have found either. The same is true of §4.1's
  findings.
- **73 minutes is not "a multi-day campaign"**, which is what the project's own
  milestone asked for; `docs/FUZZING.md:20` says so plainly. A GitHub job is cut
  at six hours.
- Two CI campaigns exist: `.github/workflows/fuzz.yml` nightly (4 min/target,
  regression guard) and `.github/workflows/fuzz-campagne.yml` weekly
  (45 min/target, 9 parallel jobs, keeps the enriched corpus as an artefact).
  Only `backup_archive`'s corpus is committed, and the reason is stated: for the
  fast decoders re-deriving an equivalent corpus costs seconds, so storing it
  buys nothing.

**What is not fuzzed**: anything above the decoders. There is no target for the
session state machine, the group *authorisation* logic (as opposed to
`group_state`'s fold), the pairing state machine, or the DHT routing table.

### 5.2 The gate — `./ci.sh`, and its two divergences from GitHub CI

`ci.sh` is the single gate; `.github/workflows/ci.yml` mirrors it. The rule at
`CONTRIBUTING.md:16` is that the repository is *never* left in a state where
`./ci.sh` fails.

Steps: `cargo fmt --check` (`ci.sh:20`), `cargo clippy --workspace --all-targets
-D warnings` (`ci.sh:23`), **the anti-panic clippy pass** (`ci.sh:31`),
`cargo test --workspace` (`ci.sh:37`), `cargo deny check` + `cargo audit`
(`ci.sh:42`, optional locally, mandatory in CI), then the frontend: tsc, eslint,
prettier, vitest, production build, bundle budget, an 800-line file-size ratchet
(`ci.sh:81`), the doc-constants checker (`ci.sh:87`), and Playwright end-to-end
**inside** the gate (`ci.sh:93`).

Two divergences that matter to you:

- **GitHub CI runs `cargo test --workspace --lib` only**
  (`.github/workflows/ci.yml:119`), against `cargo test --workspace` locally
  (`ci.sh:37`). Everything under `crates/*/tests/` therefore **does not run in
  CI** — including all three property-test suites
  (`crates/accord-proto/tests/proptest_codecs.rs`,
  `crates/accord-core/tests/proptest_group.rs`,
  `crates/accord-crypto/tests/proptest_friendcode.rs`) and every end-to-end suite
  (19 files under `crates/accord-node/tests/`, 7 under
  `crates/accord-transport/tests/`). They run only on a maintainer's machine.
  The reason given is real-UDP flakiness on hosted runners; the consequence is
  that a large part of the test estate is not gate-enforced. The inline unit
  tests — including the ML-KEM NIST vector checks at `crypto/pq.rs:241` and
  `crypto/pq.rs:257` — *are* covered by `--lib`.
- CI additionally runs the transport suites in **release** profile
  (`.github/workflows/ci.yml:127`), specifically because a `debug_assert!`
  argument is not evaluated in release. That step exists because of a real
  four-release outage (see 5.4).

### 5.3 Property tests

Three suites, all deterministic-fold or codec round-trip oriented:
`crates/accord-proto/tests/proptest_codecs.rs` (with a checked-in
`proptest-regressions` file), `crates/accord-core/tests/proptest_group.rs` (the
group CRDT folds to the same state regardless of arrival order),
`crates/accord-crypto/tests/proptest_friendcode.rs`. See the CI caveat above.

### 5.4 Adversarial reviews and the incidents behind the guard rails

The project has run repeated **internal** adversarial reviews and — unusually —
writes up what each one found, in the user-facing `CHANGELOG.md` marked `🔴`. The
ones that tell you most about the codebase's failure modes:

- **A device list dated in the future locked revocation permanently, silently**
  (`SECURITY.md:487`). Not an attack — a dead CMOS battery produces the same
  list. Found by the milestone-1 adversarial review.
- **`revoke_device` returned success having persisted nothing**
  (`SECURITY.md:498`): `cache_device_list` refused the write and said so through
  a boolean the caller discarded. "A silent failure on a security control is
  worse than no control."
- **A `debug_assert!` whose argument contained the call to `install_session`**:
  in release the macro disappears and with it the call. Messaging stopped working
  entirely across four published versions, 3.0 to 3.3, with every debug test
  green (`CONTRIBUTING.md:229`). Two guard rails exist solely because of it —
  the `clippy::debug_assert_with_mut_call` lint and the release-profile transport
  test step.
- **A whole test dimension that had never been switched on**
  (`CONTRIBUTING.md:332`): the simulated mesh has carried packet loss, jitter and
  a kill switch since it was written, and every caller passed
  `NetConditions::default()` — zero loss, zero latency. Everything the project
  believed about bad-network behaviour, it believed by deduction.
- **The testing standard is "the test bites"** (`CONTRIBUTING.md:323`): after
  writing a test, break the code it covers and confirm it goes red. One of the
  7.1 regression tests did not, and was rewritten until it did.

### 5.5 Supply chain

`cargo deny check` and `cargo audit` are mandatory in CI
(`.github/workflows/ci.yml:133`, `.github/workflows/ci.yml:137`); policy in
`deny.toml`. Third-party inventory and the JS-side gap (`cargo deny` does not
cover npm) are documented at `docs/THIRD_PARTY.md:136`. Reproducible-build notes
are at `docs/REPRODUCIBILITY.md`.

---

## 6. Building and running it

Do not duplicate the setup instructions here — they are maintained and this
document is not. Read, in order:

- [`CONTRIBUTING.md`](../CONTRIBUTING.md) §2 (prerequisites and the two local
  traps) and §3 (the gate, step by step).
- [`docs/DEV.md`](DEV.md) §1 (repository structure and crate dependency graph),
  §2 (build and test), §4 (test harness and its shortcuts).

The three-line version:

```sh
git clone https://github.com/Gomouu/accord && cd accord
brew install opus pkgconf            # or: apt install libopus-dev pkg-config
CMAKE_POLICY_VERSION_MINIMUM=3.5 ./ci.sh
```

Two environment traps, both real and both costing an hour if unknown
(`CONTRIBUTING.md:41`):

- **Node must be in `>=20 <25`.** CI pins 22. On Node 26, 260 frontend tests die
  at `window.localStorage.clear()`. Any frontend result differing from CI starts
  with `node --version`.
- **CMake ≥ 4 breaks the vendored Opus.** `app/src-tauri` is a workspace member,
  so `cargo test --workspace` pulls in `audiopus_sys`. Either install the system
  libopus or export `CMAKE_POLICY_VERSION_MINIMUM=3.5`.

To run a fuzz target yourself (nightly toolchain required, `docs/FUZZING.md:118`):

```sh
CMAKE_POLICY_VERSION_MINIMUM=3.5 \
  cargo +nightly fuzz run device_list fuzz/seeds/device_list -- -max_total_time=300
```

Test shortcuts you will meet and should not mistake for weakened production
paths: `VaultParams::insecure_for_tests()` (`crypto/vault.rs:42`), reduced PoW
bits (`verify_device_list_with_pow_bits`, `crypto/device.rs:171` — note the
comment explaining why the parameter exists at all: without it the tests had to
*reimplement* verification, so removing a check would have failed nothing), and
the `Simule` voice backend.

---

## 7. Scoping notes

**A cost-effective engagement, in priority order:**

1. Tier 0 (§2) — handshake and hybrid KEM, session AEAD, the group key
   lifecycle, the device/account model, pairing and the seed transfer. This is
   where a finding changes what users should be told.
2. §4.1 — confirm or refute the three findings above before anything else; they
   are cheap to settle and one of them, if it holds, invalidates a headline
   guarantee.
3. Tier 1 — group op-log authorisation, the decoders, DHT validation.
4. Tier 2 only if budget remains.

**What is probably not worth your money**: re-fuzzing the decoders (746 M
executions, zero crashes, corpus available); re-checking for `unsafe` or panics
(compiler- and lint-enforced, `ci.sh:31`); re-deriving the accepted trade-offs of
§4.2, which are already written up with their hardening paths.

**What the project explicitly cannot verify itself** and would most value an
outside opinion on (`CONTRIBUTING.md:266`): real NAT traversal behaviour, voice
and video capture paths, and anything requiring a network the project does not
have. Those are also the least security-critical, so weigh accordingly.

**Reporting**: `SECURITY.md:625` — private vulnerability reporting on GitHub, or
the maintainers privately. Not a public issue. There is no bug bounty. Security
fixes take priority over every other task, and only the latest release is
patched.

---

## 8. One-page checklist

The project maintains its own, at `SECURITY.md:555`–`SECURITY.md:623`, ordered
most to least critical and written by the people who wrote the code. It is good;
use it, with three corrections from §4.3: it omits the `device_list` fuzz target,
its §5 cross-references are ambiguous because of duplicate numbering, and its
banner-capping summary is out of date.

Add to it, from this brief:

- [ ] Does anything call `rotate_key`? (§4.1, finding 1)
- [ ] Who may send a `CoreMsg::GroupKey`, and at what epoch? (§4.1, finding 2)
- [ ] Is `CoreMsg::GroupOpMsg` bounded per peer? (§4.1, finding 3)
- [ ] Can a future-dated device list authorise through the in-memory
      `proven_lists` path? (§2, T0.4)
- [ ] Can any caller reach `Writer::put_vbytes` / `put_lbytes` / `put_list` with a
      length exceeding the prefix, in a release build? (§2, T1.2)
- [ ] Does any `tracing` call site anywhere emit a key, a friend code, a message
      body or a friend's address? (§4.3, item 5)
