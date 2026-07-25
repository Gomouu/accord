# Multi-device — design document

**Status**: design of lot 1.A. §4 has been brought back in step with the pairing
code that shipped in lot 1.D, and §5 with the delivery fan-out that shipped in
lot 1.E — where the implementation contradicted the design, the design text was
corrected, not the code. The rest still runs ahead of the code.
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

### 3.2.1 🔴 The switch has to be deployed in two phases

Found while reading the transport, and it changes the order of the work.

Today a peer's transport **static key is its account public key**, and the node
uses it as the identity directly: `is_friend(&static_pub)`, message routing,
profile re-announcement. So "the transport uses the device key" cannot be the
first task:

- our outgoing sessions would present a key no peer recognises — we would become
  a stranger to every one of our friends, on every released version;
- and symmetrically we could not map an incoming device key back to an account,
  because the resolution is a *later* task in the same lot.

Same discipline as the handshake capability field and `RecordKind::Unknown`:
**deploy the ability to read one version before you start to write.**

| Phase | Ships in | What changes on the wire |
|---|---|---|
| 1 — resolve | 6.4 | Publish our device list; resolve and cache a friend's; **accept** a session whose static key is a device in a friend's signed list. We still *present* the account key. Older peers see one new record kind, which they already ignore cleanly (`Unknown(u8)`, 6.3). |
| 2 — switch | 7.0 | **Present** the device key. This is the flag day, and what actually lifts B1. |

💡 **`install_session` needs no change at all.** "At most one direct session per
identity" is already written against `peer_static`. Give the transport
per-device keys and it becomes "per device" for free. B1 was never the eviction
rule — it is the identity fed to it.

#### The list says who, not where — so each entry has to say which

Phase 1 leaves the two halves out of step: a device is *listed* long before it
*presents* its own key. A list alone therefore cannot answer the only question
delivery asks — which key do I write to? Read naively, it would send every
message of the whole fleet to a device key nobody is listening on.

`DeviceEntry::flags` carries `DEVICE_FLAG_TRANSPORT_KEY` for exactly this.
It is set only when that device's transport really presents its own key —
derived from the effective key at startup, never copied from a config toggle,
and re-signed if it ever disagrees. Delivery then reads:

| Fleet state | Targets |
|---|---|
| no fresh list known | the account key |
| list, no entry flagged (all of phase 1) | the account key |
| every entry flagged | one target per device |
| **mixed** | the flagged devices **plus** the account key |

The mixed row is the one that will exist in the wild for weeks, and the one that
disappears if you simplify. Keeping only flagged devices cuts off everyone who
has not updated; always adding the account key files a message forever for a
listener that no longer exists. Unflagged devices collapse into a single target
because they all present the same key — and evict each other from the transport,
which is precisely the blocker this milestone lifts.

A fifth case falls out of the same reading and is worth naming, because the
table's shape invites the opposite answer: a fresh list whose every entry is
revoked yields **the account key**, not the empty set. "No authorised device" is
not the same claim as "unreachable", and an empty target list would silently
drop every message to an account that is merely mid-rotation.

The flag is inside `signable_bytes`, so it cannot be stripped in flight to
redirect a conversation.

#### 🔒 Every device signs the same list, so the merge rule cannot be last-writer-wins

Found while building lot 1.E, and it invalidated an assumption this document
carried without stating it. Every device of an account holds the root key, so
every device signs and publishes *the account's* list, and `version` derives from
a timestamp. Last-writer-wins across a set that several writers hold partial
views of is the classic way to lose data, and it lost devices two ordinary ways:

- a **freshly paired** device has no list in its database, builds one containing
  only itself, and publishes it — erasing every other device of the account, at
  every correspondent;
- a device that was **switched off during an enrolment** republishes the view it
  had before, with the same effect.

Neither produced an error anywhere. The device simply stopped being reachable.

The structure gives the right rule: `devices` and `revoked` only ever **grow**,
so a **union** converges where a replacement does not. Merging in either order
gives the same result, and a merged list is always a superset of both inputs, so
a correspondent adopting it can never lose a device. Revocation keeps priority —
what removes a device is its entry in `revoked`, not its absence from `devices` —
so removal still works, and a merge cannot resurrect a revoked key.

Republication therefore **adopts before it reissues**: fetch the copy that is
online, merge, re-sign, publish. For our own account the monotonic-version rule
is deliberately relaxed when reading that copy — it exists to stop a *third party*
replaying an old list, and this record is signed by our own root. Requiring it
would refuse exactly the case being repaired: a copy published at the version we
already hold.

#### The same rule is what keeps a list alive at all

`issued_ms` was written once, at construction. `enroll_device`, `revoke_device`
and the flag reconciliation all bumped `version` and left the date alone, so
**a list expired 24 hours after the account first started** and no amount of
republishing renewed it. Two consequences, both silent:

- our own list stopped being fresh, `delivery_targets` fell back to the account
  key, and everything lot 1.E addresses to one's own devices — read marks,
  catch-up — had no sibling left to reach;
- worse at a correspondent: once their cached copy expired they could never
  refresh it, because the republished record carried the *same* version and
  verification requires a strictly greater one. They were stuck on the fallback
  permanently.

So the merge sets `issued_ms` to now and takes `max(timestamp, higher of the two
+ 1)` as the version. Deriving from the clock alone would almost do, but a clock
running behind would produce a version peers reject as stale, and the merge would
be lost without a trace.

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
- **Learning a revocation empties the queue.** The offline queue is indexed by
  transport key and, by design, re-checks nothing when a peer reconnects — that
  is what makes reconnection fast. Anything already queued for a device would
  otherwise have outlived the revocation by the queue's own retention, seven
  days, not `valid_for_s`. It is dropped the moment a newly ingested list
  revokes the key.

⚠️ **What revocation does not reach: a mailbox deposit already written.** It is
sealed to the device's key and sits at a DHT key derived from it. There is no
recall — whoever holds the device can read it until it expires, up to seven
days. Shortening that expiry would degrade offline delivery for everyone to
narrow a window that only matters after a theft, so the bound is documented
instead. Revoking stops what comes next; it does not reach back for what was
already in flight.

🔒 This property must be written in `SECURITY.md` in plain words. It is a
consequence of having no server, and a user deciding whether to revoke needs to
know it is not instantaneous.

---

## 4. Pairing a new device

This is where an attacker would try to slip their device into someone's
account — the one moment where the account grants trust.

**Flow: out-of-band code, mutual fingerprint confirmation.** The channel lives
in `accord-crypto/src/pairing.rs`; the rules around it — single use, expiry,
attempt limit — in `accord-node/src/pairing.rs`.

1. On the **already-authorised** device: "Add a device" shows a short code,
   valid 5 minutes. **8 characters over a 31-symbol alphabet, ≈ 39 bits.** The
   alphabet excludes `0`/`O` and `1`/`I`/`L`: a code is read off one screen and
   typed into another, sometimes read aloud, and two characters that look alike
   turn into a pairing that fails with nobody understanding why. A wrong
   character is rejected, never silently corrected — "0" for "O" is a plausible
   fix and the start of a code you believe you typed.
2. On the **new** device: enter the code. It generates its device key pair.
3. Both derive a channel from the code with a **symmetric PAKE** (`spake2`, see
   §4.1) and send each other one `PairingHello` (0x18). Not a shared secret sent
   in the clear: the code is short and low-entropy, so it must never be usable
   offline by an eavesdropper.
4. Both screens show the **same 6-digit fingerprint** of the channel key, in two
   groups of three, and each user confirms on their own side.
5. Confirming on the **new** device sends its `DeviceEntry` — device public key,
   PoW nonce, name — **sealed under the channel key** (`PairingSealed`, 0x19).
   The authorised device retains it only if the payload opens *and* the proposed
   key carries a valid identity proof of work. Retaining is not enrolling.
6. Confirming on the **authorised** device is what seals the pairing: it adds
   the entry, signs version *n+1*, stores it and publishes it. It refuses when
   no sealed entry has arrived, rather than consuming the offer for nothing.
   **The root key never moves.**

🔒 **Constraints, as implemented**

- **One offer at a time, in memory only.** A code that survived a restart would
  be a code nobody is watching a screen for. Asking for a new one cancels the
  previous.
- **Five minutes.** Expiry and single use are checked *before* the attempt
  counter is touched: a dead offer does not move its counter, so what the screen
  shows cannot be steered from outside.
- **Three attempts, counted whether they complete or not** (§4.2). Past that the
  offer is burnt and even the correct code stops working: you have to walk back
  to the authorised device and open a new one.
- **The offer is consumed by the fingerprint confirmation, not by a completed
  exchange** (§4.2).
- A fingerprint mismatch **cancels** the pairing. There is no "continue anyway":
  confirmation is a separate explicit call, and it refuses when there is no
  channel or no proposed device to seal.
- **Every attempt restarts from a fresh SPAKE2 state.** Replaying one state
  across attempts would hand an attacker several observations of the same
  secret.
- The account root key never travels, not even encrypted. Pairing grants
  membership in the signed list; it never grants the ability to sign one.
- Neither the code nor the channel key can reach a log: both types have a mute
  `Debug`, and it is tested.

**Why the PAKE and not a plain code.** With a plain shared secret, anyone who
observes the exchange can derive the channel offline and impersonate the new
device. A PAKE makes each guess cost one *online* interaction, which the attempt
limit then bounds. This is the difference between a short code being fine and
being a hole. It is also why the code could be lengthened to 39 bits for free:
length is not what makes guessing expensive, the online round-trip is.

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
therefore **mandatory** in our flow, and it costs nothing here because the flow
already performs it twice: the fingerprint comparison of §4, by a human on both
screens, and — this one the machine can check — the opening of the sealed
`DeviceEntry` under the channel key. Choosing the maintained implementation of a
frozen specification beats the dormant implementation of a moving one.

Not pinned to `0.5.0-pre.0`: a pre-release has, by definition, an unstable API,
and it raises the MSRV to 1.85. Revisit when it goes stable.

### 4.2 What the symmetric PAKE proves — and what it does not

Written **after** the implementation, because the implementation contradicted an
assumption this document carried without ever stating it.

**A completed exchange proves nothing.** In its symmetric form
(`Spake2::start_symmetric`, the right shape for two devices of one account where
neither is "the server"), SPAKE2 derives a key on **both** sides even when the
two codes differ — the keys are simply different. `finish()` returns an error
only when the peer's message is malformed; it never means "wrong code". There is
no oracle to read here, and none we could offer.

Three consequences, all of them load-bearing in the code:

1. **The offer is consumed at the fingerprint confirmation, never at a completed
   exchange.** Marking it used as soon as an exchange completes would let anyone
   destroy a pairing in progress with a single well-formed datagram, and the
   legitimate user would have to start over without ever learning why.
2. **The one cryptographic failure that proves anything is opening the sealed
   payload** under the channel key: whoever cannot open it did not have the
   code. That is why the new device's `DeviceEntry` travels sealed, and why the
   authorised device retains nothing before that payload opens.
3. **Three attempts, counted whether they complete or not.** A counter that only
   charged for failures would charge for almost nothing, since completing is not
   success. What makes a 39-bit code acceptable is bounding the number of
   *interactions*; the PAKE only makes each interaction necessary.

⚠️ **A stranger can still burn an offer.** Anyone who can reach the node while
one is open may send well-formed `PairingHello` messages, and three of them
exhaust the attempt counter. The counter has to count them — success proves
nothing about the code (§4.2), so refusing to count would make brute force free.

What they can no longer do is change the fingerprint under the user's eyes. The
candidate channel is **frozen** once a sealed payload opens, because opening it
is the one thing that proves knowledge of the code. A later `PairingHello` is
dropped. Without that guard, a stranger could swap the displayed number in the
instant before the user compares it, and have them confirm a pairing that is not
theirs.

So this is a denial of pairing, not a way in, and it is bounded by the
five-minute window and by the user simply asking for a new code.

**Revocation.** `Node::revoke_device` drops the entry from `devices`, records a
`RevokedEntry`, signs version *n+1* and publishes. The record is kept rather
than the mere absence: a peer holding an older list where the device still
appears must be able to see that it was *removed*, not merely fail to find it.
`DeviceList::authorises` treats a key present in both `devices` and `revoked` as
revoked — on an inconsistent list, refusal is the only safe answer.

Revoking the device you are on is refused. It would cut the account off from
itself: no machine left to sign the next list, and no way back without the
recovery phrase. §3.3 describes how the revocation then propagates.

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

🔒 **The fan-out happens at exactly one place**, on the way into the network
layer: an account is resolved into transport keys (§3.2.1) and the message is
handed to the per-key delivery path once per target. Everything upstream — the
op-log, friendships, the application queue — keeps reasoning about *people*;
everything downstream reasons about *machines*. Two translation points would be
one too many: sooner or later something writes to a device under an account's
name, or the reverse. It also means the per-device mailbox is not a second
mechanism — the offline queue is keyed by the same transport key, so it follows
for free, and an account with no switched device resolves to exactly one target
and behaves byte-for-byte as before.

⚠️ **The sender's own device key matters too, and this is the part that reads as
a detail and is not.** The mailbox DHT key mixes recipient, day *and sender*
(SPEC §7). If two machines of one account deposited under the account key, they
would compute the same key when writing to the same person on the same day, and
the second STORE would erase the first one's deposit — messages lost with no
error anywhere, the same failure mode as §1 and for the same reason. Deposits
are therefore made under the depositing machine's key, and the recipient polls
one mailbox per device of each contact.

⚠️ **Cost.** N devices means N times the traffic for a direct message.
Negligible for text; **unacceptable for voice and video** — three devices would
triple an already-heavy media stream for no benefit, since a person can only
watch one screen. Note the split: call *signalling* is CORE traffic and does fan
out, which is what makes every device ring; only the media stream does not.

**Decision: real-time media stays single-device.** A call rings on every device;
the moment it is answered on one, the others stop ringing and receive no media.
This is also what users expect from every other platform.

### 5.1 How the other devices actually stop ringing

Shipped in lot 1.E. The wire contract and the constants are in
`VOICE_CALLS.md` §1.5; what belongs here is the shape of the guarantee, because
it is the first place in this design where a multi-device behaviour had to be
made correct **without** relying on a message arriving.

The tempting design is one message: the caller tells the callee's account "taken"
and the losing devices stop. That message exists (`CallTaken`, 0x1A, sent to the
account so it reaches every device including the winner, which ignores it), but
it is a **latency shortcut and nothing more**. It travels over UDP, it is never
queued offline, and it can be lost in full.

Correctness comes from a second, independent property that depends on no received
message at all: the caller resends its offer while ringing and **stops the instant
it honours an answer**, so a device that is still ringing and has stopped hearing
offers concludes on its own. The ordering is deliberate. Had correctness been hung
on `CallTaken`, losing it would leave the other devices ringing for the full 45 s
and then reporting a **missed call** for a call that was in fact taken — a false
notification, which is strictly worse than a ring that stops a few seconds late.
A device cut off from the caller has exactly one thing left to reason from, the
silence itself; that is what the guarantee is built on, and `CallTaken` only makes
the common case fast.

⚠️ **The race this does not close: two devices answering within one round trip.**
The caller honours the first answer and ignores the second, but it cannot tell
*which* device lost. Inbound traffic is translated device → account at the router's
edge — the mirror of the outbound resolution in §3.2.1, and for the same reason:
one translation point, not two — so by the time the voice engine sees the two
answers they carry the same account key. The loser sits in an active call receiving
no media and only exits ~10 s later through audio-loss detection.

That exit used to be actively harmful: losing audio emitted a best-effort hangup,
which correlated on the caller exactly like the winner's would and tore down the
call that was actually established. Losing audio is now **silent**. The message
only ever helped when it was useless — audio is lost precisely when the peer has
gone, so the hangup was landing nowhere — and hurt in the one case where it
arrived. What remains of the race is therefore ten seconds of a second device
showing an active call it cannot hear, which is a display fault rather than a
lost call.

Closing it fully means carrying the **transport key** up to the voice engine
rather than collapsing it to an account at the boundary. That is not done, and it
is not a small change: the roster, the per-peer rate limiting and the call state
machine all key on the account today. It would also fix the jitter buffers, which
currently merge two devices' frames into one sequence.

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

1. **New messages reach every connected device.** Free once §5 is done — landed
   in lot 1.E. A device that is switched on receives everything.
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
| | Burn a pairing offer with well-formed hellos | Nothing does — three attempts, counted whether they complete or not (§4.2). It is a denial of pairing, not a way in |
| Someone who steals a device | Read that device's history | Out of scope — the local database is already encrypted at rest by the vault. Revoking the device stops *future* delivery, not past storage |

**What this design does not protect against**, stated plainly:

- **Someone physically in front of an unlocked authorised device can pair their
  own.** They open the offer, read the code off the screen and confirm the
  fingerprint on both sides. The flow is *built* to require that presence; it
  cannot tell the account's owner from anyone else who is standing there. No
  cryptography addresses this.
- A compromised authorised device can pair another device. It holds the ability
  to sign as the account (via the pairing flow). Revocation would be the remedy,
  after the fact — once it exists (§4.2).
- **The device list is public.** It is published in the DHT, signed and readable
  by anyone who can derive its key from the account public key, which is what
  lets a contact find you. It exposes how many devices an account has, their
  user-chosen names and when each was added.
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

**Settled since**: the PAKE choice (SPAKE2 vs CPace), its licence against
`deny.toml` and the state of the crate — §4.1. §4.2 records what only writing
the code could reveal.
