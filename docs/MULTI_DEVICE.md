# Multi-device — design document

**Status**: design, no production code yet. This is lot 1.A of milestone 1.
**Target**: 7.0.
**Rule**: nothing is implemented before this document answers the questions it
raises. That rule exists because two hours of reading `install_session` once
saved weeks of work in a dead end — see §1.

---

## 1. Why the obvious approach is broken

The naive multi-device story is "restore the recovery phrase on the second
machine". It does not merely degrade — it **breaks silently**.

An Accord identity is a single Ed25519 seed (`accord-crypto/src/identity.rs`):
the public key, the `NodeId`, the X25519 key and every signature derive from it.
Two machines restoring the same seed are, on the wire, **the same identity**.

`accord-transport/src/endpoint.rs::install_session` enforces one invariant:

> at most one direct session per identity, deterministic delivery

Every new session evicts the other direct sessions of the same static key. That
eviction is the fix for the reconnection flake (Lot G, cause 4) and must not be
removed. With a shared seed, the two machines therefore **evict each other at
every friend**: only the last one to connect receives anything, and the user
sees messages vanishing with no error anywhere.

So the design constraint is not "make two devices work". It is:

🔒 **Two devices must be two distinct transport identities.** The invariant then
holds untouched, and nothing about the reconnection fix has to be revisited.

---

## 2. The model

Two levels, where there was one:

```
        Account  (root Ed25519 key)
        │  ← this is what your friends know; the friend code points here
        │
        ├── Device A  (its own Ed25519 key)   "Laptop"
        ├── Device B  (its own Ed25519 key)   "Desktop"
        └── Device C  (its own Ed25519 key)   "Phone"
```

| | Account key | Device key |
|---|---|---|
| Derived from | the recovery phrase | generated on the device, never leaves it |
| Used for | signing the device list, nothing else | every transport session, every message |
| Lives | can stay offline most of the time | on one machine only |
| Identifies | the person | one machine of that person |
| Friend code | ✅ points here | ❌ never exposed as an identity |

**What this buys.** Each device is a separate static key, so `install_session`
sees separate identities and no eviction happens. The transport does not change
its invariant; it just stops being applied to the wrong thing.

**What it costs.** Everything that used to say "identity" must now say either
"account" or "device", and getting that wrong in either direction is a bug:
saying *device* where *account* is meant fragments a person into several
contacts; saying *account* where *device* is meant reintroduces the eviction.
§6 lists which is which.

---

## 3. The device list

The device list is the object the whole design rests on. It answers, for anyone
holding an account key: *which devices may act for this account right now?*

### 3.1 Wire structure

New `CoreMsg` variant and new DHT record kind (`RecordKind::DeviceList = 0x05`).
Both carry the same encoded structure:

| Field | Type | Meaning |
|---|---|---|
| `account` | `bytes<32>` | Root public key |
| `version` | `u64` | Monotonic; a lower version is ignored outright |
| `issued_ms` | `u64` | Wall-clock issue time |
| `valid_for_s` | `u32` | Lifetime; past it, holders must refresh before trusting |
| `devices` | `list<DeviceEntry>` | Authorised devices |
| `revoked` | `list<RevokedEntry>` | Explicitly revoked keys |
| `sig` | `bytes<64>` | Root signature over everything above |

`DeviceEntry`:

| Field | Type | Meaning |
|---|---|---|
| `pubkey` | `bytes<32>` | Device Ed25519 public key |
| `pow_nonce` | `u64` | Identity proof-of-work of the device key |
| `name` | `str` (≤ 32 B) | User-visible label ("Laptop") |
| `added_ms` | `u64` | When it was paired |
| `flags` | `u32` | Reserved; unknown bits ignored |

`RevokedEntry`: `pubkey` (`bytes<32>`) + `revoked_ms` (`u64`).

🔒 **Bounds.** `MAX_DEVICES = 8` and `MAX_REVOKED = 32`, enforced at decode.
Without them a hostile list is an allocation bomb: the structure is fetched from
the DHT, i.e. from strangers, before anything about it is trusted.

🔒 **The signature covers the version.** Otherwise an attacker replays an old
list with a bumped version number and resurrects a revoked device.

### 3.2 Publication and lookup

- Published under `RecordKind::DeviceList`, at DHT key
  `SHA-256("device-list-v1" ‖ account_pubkey)`.
- The record's publisher **is** the account key, so `Store::validate` already
  authenticates it with no new code — the existing signature check applies.
- Also pushed directly to friends on session establishment, so the common case
  never waits on the DHT.
- Cached locally per contact with its `valid_for_s`; refreshed on connection and
  when stale.

### 3.3 Revocation is eventually consistent, and that is not a bug

A friend who is offline when you revoke a device keeps accepting that device
until they fetch the newer list. There is no server to push the revocation
through, and pretending otherwise would be dishonest.

What bounds the damage:

- **Monotonic version + refresh on every connection.** The window is "until this
  friend next comes online", not "forever".
- **Explicit lifetime.** Past `valid_for_s`, a holder must refresh before
  trusting the list. A revoked device cannot ride an indefinitely stale list.
- **The revoked device cannot suppress the update.** The new list travels
  through the DHT and through every other friend; blocking one path does not
  block the others.

🔒 This property must be written in `SECURITY.md` in plain words. It is a
consequence of having no server, and a user deciding whether to revoke needs to
know it is not instantaneous.

---

## 4. Pairing a new device

This is where an attacker would try to slip their device into someone's
account — the one moment where the account grants trust.

**Flow: out-of-band code, mutual fingerprint confirmation.**

1. On the **already-authorised** device: "Add a device" shows a short code
   (and a QR of the same value), valid 5 minutes, single use.
2. On the **new** device: enter or scan the code. It generates its key pair.
3. Both derive a channel from the code with a **PAKE** (`spake2`, see §4.1).
   Not a shared secret sent in the clear: the code is short and low-entropy, so
   it must never be usable offline by an eavesdropper.
4. Both screens show the **same short fingerprint** of the established channel.
   The user confirms on **both** sides.
5. The authorised device adds the new key, signs version *n+1*, publishes it.
6. The new device receives the list. **The root key never moves.**

🔒 **Constraints**

- The code is single-use and expires after 5 minutes.
- A fingerprint mismatch **cancels** the pairing. There is no "continue anyway".
- The account root key never travels, not even encrypted.
- Pairing attempts are rate-limited: a short code is brute-forceable if you may
  keep guessing.

**Why the PAKE and not a plain code.** With a plain shared secret, anyone who
observes the exchange can derive the channel offline and impersonate the new
device. A PAKE makes each guess cost one *online* interaction, which the rate
limit then bounds. This is the difference between a 6-digit code being fine and
being a hole.

**Why the fingerprint confirmation on top.** The PAKE authenticates the channel
to whoever knows the code. If the code leaks — shoulder-surfed, screenshot in a
chat — the fingerprint step still requires the attacker to be *in front of the
authorised device* to confirm. It converts a code leak from a compromise into a
failed attempt.

### 4.1 Which PAKE — decision

**`spake2` (RustCrypto), pinned to the stable 0.4.0.** Balanced PAKE, both sides
know the same short code; that is the shape this flow needs. The augmented
family (OPAQUE) models a client proving a password to a server, which is not
what two devices of one account are doing.

The candidates, and why the others lost:

| Crate | Licence | Latest | Recent downloads |
|---|---|---|---|
| `spake2` (RustCrypto) | MIT OR Apache-2.0 | 0.5.0-pre.0, Jan 2026 | 250 000 |
| `pake-cpace` (jedisct1) | ISC | 0.1.7, Dec 2023 | 884 |
| `cpace` (hdevalence) | BSD-3-Clause | 0.1.0, May 2020 | 237 |

All three licences already pass `deny.toml`, so the licence is not what decides
it. Three things do:

1. **It adds one crate, not a subtree.** Every dependency of `spake2` 0.4.0 —
   `curve25519-dalek`, `sha2`, `hkdf`, `hmac`, `subtle`, `rand_core`,
   `getrandom` — is already in the workspace at the *same* version. For a
   dependency on the trust path, an audit surface that does not grow is worth
   more than a marginally better protocol.
2. **It is alive.** 250 000 recent downloads against 884, a release this year
   against one in 2023 and one in 2020. An unmaintained cryptographic
   dependency is a liability: a vulnerability in it would have no upstream fix,
   and we would be forking a PAKE.
3. **Its specification is frozen.** SPAKE2 is RFC 9382. CPace is still
   `draft-irtf-cfrg-cpace`, so its wire format can still move — and a pairing
   protocol has to stay compatible across versions of Accord.

⚠️ **The honest counter-argument.** CPace, not SPAKE2, is what the CFRG PAKE
selection chose for the balanced case, and it is the better protocol on paper:
SPAKE2 depends on fixed group elements *M* and *N*, and its base form provides
no forward secrecy without an explicit key-confirmation step. That step is
therefore **mandatory** in our flow — which costs nothing here, because the
fingerprint confirmation of §4 already *is* a key confirmation, performed by a
human on both screens. Choosing the maintained implementation of a frozen
specification beats the dormant implementation of a moving one.

Not pinned to `0.5.0-pre.0`: a pre-release has, by definition, an unstable API,
and it raises the MSRV to 1.85. Revisit when it goes stable.

---

## 5. Message delivery

Today a message goes to an identity, which is one session. Tomorrow it goes to
an account, which is N devices.

- The sender resolves the recipient's device list (cached, refreshed).
- The message is sealed **once per device** — one session each.
- The offline mailbox becomes **per device**. With a shared mailbox, the first
  device to collect would deprive the others; the DHT key therefore derives from
  the *device* key, not the account key.
- Read receipts become ambiguous. **Decision: read = read on at least one
  device.** It is the only convention that does not require devices to agree
  with each other, and it matches what a sender means by the word.

⚠️ **Cost.** N devices means N times the traffic for a direct message.
Negligible for text; **unacceptable for voice and video** — three devices would
triple an already-heavy media stream for no benefit, since a person can only
watch one screen.

**Decision: real-time media stays single-device.** A call rings on every device;
the moment it is answered on one, the others stop ringing and receive no media.
This is also what users expect from every other platform.

---

## 6. What belongs to the account, and what belongs to the device

Getting this table wrong in either direction is a bug (see §2).

| Concern | Level | Why |
|---|---|---|
| Friendships | **Account** | You befriend a person, not a machine |
| Server membership, op-log authorship | **Account** | Otherwise one person appears N times in a member list |
| Profile (name, avatar, decorations) | **Account** | It is who you are |
| Friend code | **Account** | It is the account's public handle |
| Transport sessions | **Device** | This is the whole point — see §1 |
| Mailbox | **Device** | A shared one lets the first collector starve the others |
| Settings (audio device, volume, theme) | **Device** | An output volume has no shared meaning |
| Active call | **Device** | One screen, one microphone |

**Decision on settings: everything per device in 7.0.** Syncing content
preferences (language, theme) would be pleasant but requires a synchronisation
channel that does not exist yet; it is deferred rather than half-built.

---

## 7. History synchronisation

The most expensive part, and the one most easily reduced.

**Not doing**: full automatic history sync between devices. That needs a
complete reconciliation protocol, extra storage, and conflict resolution — a
milestone of its own, not a sub-task.

**Doing**, in delivery order, each useful on its own:

1. **New messages reach every connected device.** Free once §5 is done. A device
   that is switched on receives everything.
2. **Catch-up on reconnect.** A returning device asks *its own other devices*
   what it missed since its last timestamp. Direct, encrypted, device-to-device;
   no third party involved.
3. **History transfer at pairing** (optional, on request). The new device may
   ask the original for the full history. Long, explicit, with a progress bar.

Stopping after step 1 still leaves a working product. That is the point of the
ordering.

---

## 8. Threat analysis

| Attacker | What they try | What stops them |
|---|---|---|
| Network attacker (on-path) | Rewrite a device list in flight | Root signature over the whole structure, version included |
| | Replay an old list to resurrect a revoked device | Monotonic `version`; a lower one is ignored |
| | Strip the pairing fingerprint step | Both sides require explicit confirmation; no bypass path exists |
| | Downgrade a 7.0 peer to single-device behaviour | Capability bit `CAP_DEVICE_KEYS`, authenticated in the handshake transcript (shipped in 6.2) |
| Revoked device | Keep receiving messages | Refused by any peer holding a list ≥ the revoking version |
| | Keep the old list alive by refusing to relay | It cannot: the list travels through the DHT and every other friend |
| | Publish a forged list removing its own revocation | It has no root key; the signature fails |
| Malicious friend | Send a device list for someone else's account | The DHT key binds to the account; the record's publisher must be the account key |
| | Claim a person has a device they do not | Same — requires the root signature |
| Stranger on the DHT | Store a huge device list to exhaust memory | `MAX_DEVICES = 8`, `MAX_REVOKED = 32`, `MAX_DHT_VALUE = 8 KiB`, all enforced at decode |
| | Brute-force a pairing code | PAKE makes each guess an online interaction; rate limit bounds them; the code expires in 5 min |
| Someone who steals a device | Read that device's history | Out of scope — the local database is already encrypted at rest by the vault. Revoking the device stops *future* delivery, not past storage |

**What this design does not protect against**, stated plainly:

- A compromised authorised device can pair another device. It holds the ability
  to sign as the account (via the pairing flow). Revocation is the remedy, after
  the fact.
- Losing the recovery phrase *and* all devices means losing the account. There
  is no recovery authority, by construction.

---

## 9. Migration from a 6.2 account

Must be automatic, lossless, and invisible.

On first start of 7.0 on an existing profile:

1. The existing seed becomes the **account root key**. The friend code, the
   profile and every friendship keep pointing at the same public key — nothing
   changes for the user's contacts.
2. ⚠️ **A brand-new device key is generated.** If the existing seed became both
   root *and* device, we would have gained nothing: two restored machines would
   share the device key and evict each other exactly as before. This is the step
   the migration must not get wrong.
3. A version-1 device list is signed with the root key, containing that single
   device, and published.
4. The local tables `account` and `devices` are created by a numbered migration
   step (the mechanism landed in 6.2 as T0.2; `MIGRATIONS` is currently empty,
   and this is its first entry).

**Backwards compatibility.** A 6.2 peer does not know about device lists and
keeps talking to the account key directly. A 7.0 node must therefore keep
accepting sessions on the account key for peers that do not advertise
`CAP_DEVICE_KEYS` — a compatibility path to be removed only once the fleet has
moved.

---

## 10. Open questions, decided

The roadmap left four open. Deciding them here is the point of this document.

1. **Settings sync** → per device in 7.0, no sync. (§6)
2. **Read receipts** → read on at least one device. (§5)
3. **Loss of all devices** → the recovery phrase regenerates the root key, which
   can then issue a version-*n+1* list revoking every previous device and
   containing only the new one. ⚠️ **To verify during 1.B**: the version counter
   must survive in the phrase-derived state, or a fresh restore would emit
   version 1 and be ignored by peers holding a higher version. **Proposed fix**:
   derive the starting version from `issued_ms` rather than a stored counter, so
   a restored root always outranks anything older.
4. **Real-time media across devices** → single device per call. (§5)

---

## 11. Definition of done for lot 1.A

- [x] Account/device model, with the reason the naive approach fails.
- [x] Exact wire structures, with their bounds and the reason for each.
- [x] Pairing flow, with the rationale for the PAKE and the fingerprint.
- [x] Delivery and catch-up flows.
- [x] Written threat analysis, including what is *not* covered.
- [x] Migration plan for an existing account, with the trap called out.
- [x] The four open questions decided.

**Remaining before code**: pick the concrete PAKE implementation (SPAKE2 vs
CPace), check its licence against `deny.toml`, and confirm the crate is
maintained. That is the first task of lot 1.B, not a design question.
