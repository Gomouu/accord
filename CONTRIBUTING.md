# Contributing to Accord

Accord is a peer-to-peer, end-to-end encrypted desktop messenger with no
central server. This document is what you need to build it, test it, and get a
change merged — and, more importantly, the reasoning behind the rules you will
be asked to follow. Every one of them is here because something broke.

Read [`docs/DEV.md`](docs/DEV.md) alongside this file: it is the working
developer guide (commands, test harness, platform quirks). This document adds
what an outside contributor needs and DEV.md assumes you already know.

---

## 1. The one rule that comes before all the others

**The repository is never left in a state where `./ci.sh` fails.**

Not "before a release". Not "before a merge". Ever. If your change makes the
gate red, the change is not finished — and neither is anything anyone else
builds on top of it, because they will inherit a failure they did not cause and
spend their afternoon proving it was not theirs.

---

## 2. Getting set up

### Prerequisites

| | |
|---|---|
| Rust | stable, ≥ 1.85 (`rust-version` in the workspace `Cargo.toml`) |
| Node | **`>=20 <25`** — see the trap below |
| libopus + pkg-config | `brew install opus pkgconf` (macOS), `apt install libopus-dev pkg-config` (Debian) |
| Linux only | the [Tauri 2 prerequisites](https://tauri.app/start/prerequisites/) — the exact list is in `.github/workflows/ci.yml` |

```sh
git clone https://github.com/Gomouu/accord && cd accord
./ci.sh          # first run compiles everything; several minutes
```

### Two local traps, both documented in `docs/DEV.md`

These are the only two ways a correct checkout fails on a correct machine.
Neither is your mistake, and both cost an hour if you do not know about them.

**1. Node must be in `>=20 <25`.** `app/package.json` declares that range and
CI runs 22. Nothing in the repository pins your local version, and npm only
*warns* when you are outside the range — so Node 25+ installs and runs anyway,
and then fails strangely. On Node 26, 260 of the frontend tests die at
`window.localStorage.clear()`: Node 22+ defines its own global `localStorage`
accessor that returns `undefined` unless `--localstorage-file` is passed, and
it shadows the one jsdom installs. **Any frontend result that differs between
your machine and CI starts with `node --version`**, before anything else is
suspected.

**2. CMake ≥ 4 breaks the vendored Opus.** `app/src-tauri` is a workspace
member, so `cargo test --workspace` pulls in `audiopus_sys`, whose bundled
`CMakeLists.txt` is rejected by CMake ≥ 4. The gate then fails at step 1 with a
`cmake_minimum_required` error, before a single test runs. Two ways out:

```sh
brew install opus pkgconf                    # use the system libopus (preferred)
CMAKE_POLICY_VERSION_MINIMUM=3.5 ./ci.sh     # or build the bundled one anyway
```

`.github/workflows/ci.yml` exports `CMAKE_POLICY_VERSION_MINIMUM=3.5` for the
whole job. **`ci.sh` deliberately does not**, so on a CMake ≥ 4 machine the two
are not interchangeable without one of the lines above.

---

## 3. The gate

`./ci.sh` is the gate. `.github/workflows/ci.yml` is its GitHub mirror, running
the same commands. It ends with `CI OK` and nothing else counts as green.

| # | Step | Note |
|---|------|------|
| 1 | `cargo fmt --all --check` | |
| 2 | `cargo clippy --workspace --all-targets -- -D warnings` | |
| 3 | clippy **anti-panic** on `--lib --bins` | `unwrap_used`, `expect_used`, `panic`, `todo`, `unimplemented`, `debug_assert_with_mut_call` — see §5.2 and §5.3 |
| 4 | `cargo test --workspace` | includes the real-UDP end-to-end tests, which CI skips |
| 5 | `cargo deny check`, `cargo audit` | skipped **with a warning** if the binary is absent locally; mandatory in CI |
| 6 | UI: `npm ci` | only when `node_modules` is missing |
| 7 | UI: `tsc --noEmit`, `eslint`, `prettier --check src` | |
| 8 | UI: `vitest run`, `vite build` | |
| 9 | `node scripts/check-bundle-budget.mjs` | initial-chunk size budget |
| 10 | `npx playwright test` | interface end-to-end — **inside** the gate |

Deliberate differences in the GitHub mirror, each for a reason:

- CI runs `cargo test --workspace --lib` only — the real-UDP end-to-end tests
  are flaky on a hosted runner with no P2P network — **plus** the transport
  SimNet end-to-end suites in **release** profile. That extra step is not
  redundant: a `debug_assert!` is not evaluated in release, which is exactly the
  3.0.0 regression (§5.3), and this step would have caught it.
- The supply-chain audits are mandatory there, optional here.
- CI pins Node 22 and exports `CMAKE_POLICY_VERSION_MINIMUM=3.5`; `ci.sh` does
  neither (§2).

**Step 10 is not an optional extra, and it was added because it had been one.**
"Mark as read" disappeared from the server menu in the 4.5.0 redesign and the
regression was published — while the test covering it already existed and had
been failing on its own for weeks. A suite outside the gate is a suite that
does not exist.

🔒 **Never validate vitest with `tail`.** Read the `Test Files … / Tests …
passed` line. A truncated output has already hidden a failure.

---

## 4. The architecture, for someone seeing it for the first time

Accord is one desktop process containing two things that talk over a local
socket:

```
┌────────────────────────────────────────────────────────────────┐
│  Tauri application (one binary)                                │
│                                                                │
│  ┌───────────────────────┐        ┌─────────────────────────┐  │
│  │  UI (WebView)         │◄──WS──►│  Accord node (Rust)     │  │
│  │  React + TS + Tailwind│ 127.0. │  transport / crypto /   │  │
│  │  JSON-RPC 2.0 client  │  0.1   │  dht / core / voice     │  │
│  └───────────────────────┘        └───────────┬─────────────┘  │
└───────────────────────────────────────────────┼────────────────┘
                                                │ UDP + TCP, encrypted
                                                ▼
                                         Accord P2P network
```

**The UI never touches the network.** It speaks only the local JSON-RPC API on
`127.0.0.1`, authenticated by a session token. That single constraint is what
makes the whole node testable without a UI, and what makes alternative clients
possible at all — see [`docs/API_CONTRACT.md`](docs/API_CONTRACT.md).

### The crates, bottom to top

```
proto ──► crypto ──► transport ──► dht ──► node ──► accord-app (Tauri)
  │          │                              ▲ ▲ ▲
  └──────────┴───────► core ────────────────┘ │ │
             voice (proto only) ──────────────┘ │
             api (no accord crate) ─────────────┘
```

| Crate | Owns |
|---|---|
| `accord-proto` | Wire formats, limits, encode/decode. `limits.rs` is the source of truth for every bound. |
| `accord-crypto` | Identity, signatures, handshake, AEAD sessions, vault, mnemonic, friend codes. |
| `accord-transport` | Encrypted sessions, fragmentation, NAT traversal, relays, anti-DoS. `endpoint.rs` is the most delicate file in the repository. |
| `accord-dht` | 256-bit Kademlia: routing, lookups, signed store, in-memory testnet. |
| `accord-core` | Application state: DMs, groups and the op-log, friends, offline queues, files, search. **Depends on no network crate** — all application logic is testable without one. |
| `accord-voice` | Pure DSP (jitter, VAD, adaptive bitrate). Hardware (Opus, cpal) sits behind the `hardware` feature. |
| `accord-api` | The generic local WebSocket JSON-RPC server. Knows nothing about Accord; the service is injected. |
| `accord-node` | Assembly: network runtime, maintenance, the RPC service (`src/service/`), voice engine, the standalone `accord-noded` binary. |
| `accord-macos` | Native bridge for microphone (TCC) authorisation. **The only crate allowed to contain `unsafe`**; neutral off macOS. |
| `app/` | React + TypeScript frontend; `app/src-tauri/` is the Tauri host, itself a workspace member. |

Two consequences worth internalising before your first change:

- `accord-core` not depending on `dht` or `transport` is a decision, not an
  accident. If you find yourself wanting to import a network type into `core`,
  the design is telling you the logic belongs in `accord-node`.
- `accord-api` not depending on any `accord-*` crate is what keeps the API
  server honest: it cannot be tempted to reach past the service interface.

### Which document answers which question

Knowing this saves opening three.

| Document | Answers |
|---|---|
| [`docs/SPEC.md`](docs/SPEC.md) | what goes on the wire, byte for byte — the contract |
| [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) | which layer owns what, and the structural decisions |
| [`docs/API.md`](docs/API.md) | the UI ↔ node JSON-RPC and Tauri IPC contract, in detail |
| [`docs/API_CONTRACT.md`](docs/API_CONTRACT.md) | the same API as a **public** contract: full surface, stability policy, security position |
| [`docs/DEV.md`](docs/DEV.md) | build, test, run, and the platform quirks |
| [`docs/MULTI_DEVICE.md`](docs/MULTI_DEVICE.md) | the account/device model, and why the naive one breaks |
| [`docs/REPRODUCIBILITY.md`](docs/REPRODUCIBILITY.md) | what you can verify about a published build, and what you cannot |
| [`SECURITY.md`](SECURITY.md) | what is guaranteed, against whom, and what is **not** |

A wire change touches `SPEC.md` in the same commit. That is the rule, not an
aspiration.

---

## 5. The non-negotiable principles

Each of these has an incident behind it. They are stated with the incident,
because the rule without the reason gets negotiated away by the next person who
is in a hurry.

### 5.1 🔒 Wire compatibility

**A client already installed must never stop working because the other side
updated.**

1. **Always add, never modify.** A protocol feature takes a **new variant** (a
   new discriminant), not a field bolted onto an existing one.
2. An unknown kind is **rejected cleanly** at decode — datagram dropped, debug
   trace — never misinterpreted, never a panic.
3. Any break requires an explicit version negotiation and a coexistence period.
4. The wire diff is checked at every delivery: `proto`, `crypto`, `dht`, `frag`,
   `relay` do not move without a conscious decision.

*Precedent.* 6.0 added the camera through new `CameraFrame`/`CameraControl`
variants rather than a flag on `ScreenFrame`. A 5.0 client rejects the unknown
kind cleanly instead of rendering a camera in its screen-share viewer.

The same instinct applies one level up, in the local API: see the additive-only
rule in [`docs/API_CONTRACT.md`](docs/API_CONTRACT.md).

### 5.2 🔒 No panics in production code

`unwrap`, `expect`, `panic!`, `todo!`, `unimplemented!` are **forbidden by
clippy** in libraries **and** binaries. The lint is in the gate (step 3) and in
CI. Inline tests are excluded via `clippy.toml`; integration tests by the
`--lib --bins` scope.

**Never remove it, not even temporarily.** A P2P node that panics is a user
disconnected with no error message and no idea why.

Corollaries you will meet in the code: poisoned mutexes are recovered
(`unwrap_or_else(|e| e.into_inner())`), indices are checked, conversions are
bounded. The rare proven infallibilities carry an `#[allow]` with a comment
explaining *why* it cannot fail.

### 5.3 🔒 No `debug_assert` on a side-effecting path

*Precedent.* A `debug_assert!` whose argument contained the call to
`install_session`. In release the macro disappears — and with it the call.
**Messaging stopped working entirely. Four published versions were affected**
(3.0 through 3.3), with every debug test green the whole time.

The lint `clippy::debug_assert_with_mut_call` is in the gate. **It stays.** So
does the release-profile transport test step in CI, which exists to catch the
same class of bug from the other side: the step that would have caught 3.0.0.

If you are ever tempted to widen an allow around either guard rail, that is the
moment to stop and ask on the pull request instead.

### 5.4 🔒 The gate is the gate

No delivery without `./ci.sh` green — or, if the local machine is blocked (see
the macOS `com.apple.macl` note in `docs/DEV.md`), without the **GitHub CI
green**, which runs the same commands.

"It only touches documentation" is not an exemption. Neither is "the failure is
unrelated": an unrelated failure on `main` is everyone's problem, and finding
out whose is exactly what the gate is for.

### 5.5 🔒 Zero servers

Accord has no central server and will not have one.

The bootstrap nodes are a **rendezvous directory**, not a server: no content, no
stored messages, and the application works without them as soon as peers know
each other.

When a feature seems to require a server — global search, push notifications,
history synchronisation — the right question is: *how do we do it between peers,
or not at all?* A pull request that introduces a hosted component the project
would have to run will not be merged, however convenient it is.

### 5.6 🔒 Verify on screen what shows on screen

*Precedent.* Three blind iterations on the server dropdown menu, three failures.
Setting up a real preview loop found the defect in minutes: the surface was
translucent and the channel list showed through.

For any interface work: **look at the result.** Passing tests is necessary and
not sufficient. Some things — real video capture and rendering, perceived audio
quality, real NAT traversal, system permission dialogs, perceived smoothness —
**cannot** be proven headless at all. Where your change touches one of those,
say so in the pull request, and say what you actually looked at. Never claim to
have verified what you did not see.

### 5.7 🔒 The release ritual, no shortcuts

Releases are cut by the maintainer, but knowing the sequence tells you why some
review comments exist:

1. Bump: `Cargo.toml` (workspace), `Cargo.lock` (the ten `accord-*` crates),
   `app/package.json`, `app/package-lock.json`,
   `app/src-tauri/tauri.conf.json`.
2. `CHANGELOG.md` dated and written **for the user** — what changes for them,
   not a list of commits.
3. Gate green.
4. Fast-forward merge into `main`.
5. Push `main`, wait for CI green.
6. Tag `vX.Y.Z`, push the tag.
7. `release.yml`: four jobs (validate + macOS + Windows + Ubuntu).
8. Check `latest.json`: every platform entry present, **all signed**, URLs
   `https`.
9. `gh release edit vX.Y.Z --draft=false --latest`.

🔒 The signing key (`~/.tauri/accord-updater.key`) is **never** committed,
displayed, or copied anywhere. It is not in the repository and must never enter
it — not in a test fixture, not in a comment, not in a paste in an issue. See
[`docs/REPRODUCIBILITY.md`](docs/REPRODUCIBILITY.md) §6.

### 5.8 Language

- **Code comments are in French.** That is what the code actually does,
  including everything written recently. Claiming an English migration that
  never happened only produced a third convention; the rule is stated rather
  than quietly ignored.
- **Everything published is in English**: commit messages, `CHANGELOG.md`,
  `README.md`, `docs/`, release notes, pull request descriptions.
- Two documents have not caught up: `docs/COMMUNITY.md` and
  `docs/VOICE_CALLS.md` are still in French. Known, not hidden.

If you write only English, write your comments in English rather than
approximate French — a wrong comment is worse than a foreign one. Say so in the
pull request; the maintainer will handle it.

---

## 6. The testing standard actually practised here

Coverage is not the standard. **The standard is that the test bites.**

> After writing a test, **break the code it covers and confirm the test goes
> red.** If it stays green, you have written documentation, not a test — and
> documentation that will be trusted like a test.

This costs thirty seconds and it is not optional. Two entries in
`CHANGELOG.md` under `## [7.1.0]` explain why better than the rule does.

**Example 1 — a whole test dimension that had never been switched on.** The
simulated UDP mesh behind the integration tests has carried per-datagram loss,
variable latency and a per-node kill switch since the day it was written. Every
single caller passed `NetConditions::default()`: zero loss, zero latency.
Everything the project believed about its behaviour on a bad network, *it
believed by deduction.* The knobs were there, the tests looked like network
tests, and not one of them could have failed for a network reason. Three
campaigns now turn them on — one-in-three datagram loss, 5–120 ms of jitter, a
hard cut with no RST and no FIN.

**Example 2 — a test that was vacuous on the first attempt.** Revoking a device
could silently do nothing: a device list dated far in the future stayed "fresh"
for centuries, and the write that should have corrected it reported success even
when the database refused it. The regression tests written for the fix were
verified to fail when the fix is removed — and the first one *did not*. It was
rewritten until it bit. Had nobody checked, 7.1 would have shipped with a green
test guarding nothing.

A third entry in the same release is the mirror image and worth the same
attention: a multi-device test passed only by winning a race, asserting a
conversation held exactly one message when the correct answer is two. It was
green for the wrong reason and its failure message pointed at a bug that does
not exist.

### The levels, fastest to most realistic

- **Unit** — logic without I/O: proto codec, crypto, op-log, voice DSP,
  maintenance decisions. The vast majority of the Rust tests, and the only level
  CI runs for the whole workspace.
- **Deterministic simulated mesh** — `accord-transport` abstracts the socket;
  one implementation is real UDP, the other an in-memory network with
  controlled loss, latency and churn. The full protocol runs on both.
- **In-memory DHT testnet** — `accord-dht/src/testnet.rs`, dozens of Kademlia
  nodes with no network.
- **Real UDP integration** — `crates/accord-node/tests/`: friendship + DM +
  groups across two real nodes, maintenance convergence, voice.
- **UI** — vitest + Testing Library over stores, i18n, components and the JSON
  shapes of the API contract; Playwright over the real interface.

Shortcuts worth knowing: `VaultParams::insecure_for_tests()` (lightened
Argon2), reduced PoW, the `Simule` voice backend
(`VoiceHandle::inject_pcm` to inject capture). There is no simulated clock in
the end-to-end tests — waits are bounded, ~20 s maximum.

### Also true, and easy to forget

- A test that asserts on wall-clock ordering, or on "exactly N items arrived",
  is usually asserting on a race. Assert on what you actually mean.
- A flaky test is a failing test. Quarantining it is a decision to be argued in
  the pull request, not a default.
- If a bug reached a user, the fix comes with a test that fails without it. That
  is how the same bug stops coming back.

---

## 7. Proposing a change

### Before you write code

Open an issue for anything beyond an obvious fix. Say what the user-visible
problem is. A change that cannot be described in terms of what a user gets is
hard to review and harder to keep.

Changes that will not be merged, whatever their quality: anything requiring a
hosted component (§5.5), anything that breaks an installed client without a
negotiated migration (§5.1), anything that removes a guard rail (§5.2, §5.3),
and anything that puts a secret — a passphrase, a recovery phrase, an account
seed — on the local JSON-RPC channel (see `docs/API_CONTRACT.md` §5).

### Branches

| Prefix | For | Example |
|---|---|---|
| `feat/` | a new user-visible capability | `feat/group-video` |
| `fix/` | a bug fix on existing behaviour | `fix/composer-position` |
| `perf/` | optimisation with no behaviour change | `perf/oplog-fold` |
| `chore/` | tooling, dependencies, housekeeping | `chore/bump-tauri` |
| `docs/` | documentation only | `docs/threat-model` |

🔒 **A parallel stream gets its own `git worktree`, never a shared checkout.**
Two sessions writing into the same working tree overwrite each other's files
silently — this has already cost a full re-do of a translation batch.
`git checkout` changes the **shared** tree; uncommitted work travels with it.

```sh
git worktree add ../accord-<topic> -b feat/<topic> origin/main
```

And its corollary: never delete a worktree or a branch without checking it
holds no uncommitted work. `git worktree list` then
`git -C <path> status --short` takes ten seconds; losing work does not have a
fix.

### Commits

```
<type>: <description>

<optional body — why, not what>
```

Types: `feat`, `fix`, `refactor`, `docs`, `test`, `chore`, `perf`, `ci`. In
English (§5.8). The body is for the reasoning that will not be obvious from the
diff in six months.

### Pull requests

State, in the description:

1. **What a user gets** from this change, or what stops going wrong.
2. **What you broke to prove the tests bite** (§6) — name the test and the
   change you made to turn it red.
3. **What you looked at on screen**, for anything touching the interface (§5.6),
   and what you could not verify headless.
4. **Any wire impact**, with the `SPEC.md` update in the same commit (§5.1).

Then: `./ci.sh` green from the branch, ending in `CI OK`. A pull request that
has not been through the gate is not ready for review, and saying so is not a
formality — the reviewer's first act would otherwise be to run it for you.

### Security issues

Do not open a public issue for a vulnerability. `SECURITY.md` describes the
threat model, what is guaranteed and against whom; use the reporting route
stated there.

---

## 8. Some honest limits

- The peer-to-peer core has been through repeated internal adversarial reviews.
  There has been **no external audit**. Treat high-stakes use accordingly.
- Accord protects the **content** of your exchanges, not your **anonymity** —
  peers see your IP, like most P2P software.
- Several things cannot be proven in CI at all (§5.6). Every change touching
  them ends with an on-device verification pass, documented as such. The project
  does not claim to have verified what it has not seen, and neither should your
  pull request.
