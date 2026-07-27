# DEV — Developer guide

> How to build, test and contribute to Accord. The reference contracts
> are `SPEC.md` (wire protocol, byte-exact), `ARCHITECTURE.md`
> (layers, including the structural decisions in §7) and `API.md` (UI ↔ node).

## 1. Repository structure

```
accord/
├── crates/               # Rust workspace (the node)
│   ├── accord-proto      # Packet types, strict binary encoding, limits (SPEC §0-§1, §13)
│   ├── accord-crypto     # Identity, PoW, handshake, AEAD sessions, vault, mnemonic, friend codes
│   ├── accord-transport  # UDP/simulated sockets, encrypted-session endpoint, anti-DoS, relay, NAT
│   ├── accord-dht        # 256-bit Kademlia: routing, lookups, signed store, in-memory testnet
│   ├── accord-core       # Application logic: DMs, groups/op-log, friends, offline, files, search
│   ├── accord-voice      # Pure voice DSP (jitter, VAD, adaptive bitrate); Opus/cpal behind `hardware`
│   ├── accord-api        # Local WebSocket JSON-RPC 2.0 server (127.0.0.1 + token)
│   ├── accord-macos      # Native bridge: microphone (TCC) authorisation via AVFoundation.
│   │                     #   The ONLY crate allowed to contain `unsafe`; neutral off macOS
│   └── accord-node       # Assembly: network runtime, maintenance, API service, voice engine,
│                         #   standalone `accord-noded` binary
├── app/                  # React + TypeScript + Tailwind frontend (Vite, Zustand, vitest)
│   └── src-tauri/        # Crate `accord-app`: Tauri 2 host (workspace member)
├── ci.sh                 # Full local CI (Rust + UI)
└── docs/                 # Contracts (SPEC, ARCHITECTURE, API, MULTI_DEVICE, VOICE_CALLS, …)
```

### Crate dependency graph

```
proto ──► crypto ──► transport ──► dht ──► node ──► accord-app (Tauri)
  │          │                              ▲ ▲ ▲
  └──────────┴───────► core ────────────────┘ │ │
             voice (proto seul) ──────────────┘ │
             api (aucun crate accord) ──────────┘
```

Notable points:

- `accord-core` does **not** depend on the network (neither dht nor transport): it
  produces/consumes `accord-proto` types and it is `accord-node` that wires
  everything together (D-019). All application logic is therefore testable without a network.
- `accord-voice` is pure DSP; the hardware (Opus, cpal) lives behind the
  `hardware` feature (D-020).
- `accord-api` is a generic JSON-RPC server: the application service is
  injected by `accord-node`.

### Which document answers which question

They are not interchangeable, and knowing that saves opening three:

| Document | Answers |
|---|---|
| `docs/SPEC.md` | what goes on the wire, byte for byte — the contract |
| `docs/ARCHITECTURE.md` | which layer owns what, and the structural decisions |
| `docs/API.md` | the UI ↔ node JSON-RPC and Tauri IPC contract |
| `docs/API_CONTRACT.md` | the same API as a **public** contract: full surface, stability tiers, security position |
| `docs/REPRODUCIBILITY.md` | what a third party can verify about a published build — and what they cannot |
| `docs/MULTI_DEVICE.md` | the account/device model, and why the naive one breaks |
| `docs/VOICE_CALLS.md` | 1-to-1 calls, capture DSP, voice moderation |
| `docs/COMMUNITY.md` | events, stickers, polls, AutoMod — the D-047 surface |
| `SECURITY.md` | what is guaranteed, against whom, and what is **not** |
| `CONTRIBUTING.md` | the same ground for an outside contributor: setup, gate, principles, how to propose |

A wire change touches `SPEC.md` in the same commit; that is the rule, not an
aspiration (§5).

## 2. Build and test

### All at once: `./ci.sh`

The repository is **never** left in a state where `./ci.sh` fails. It
runs, in sequence:

| # | Step | Note |
|---|------|------|
| 1 | `cargo fmt --all --check` | |
| 2 | `cargo clippy --workspace --all-targets -- -D warnings` | |
| 3 | clippy **anti-panic** on `--lib --bins` | `unwrap_used`, `expect_used`, `panic`, `todo`, `unimplemented`, `debug_assert_with_mut_call` — see §5 |
| 4 | `cargo test --workspace` | e2e over real UDP included, unlike CI |
| 5 | `cargo deny check`, `cargo audit` | **skipped with a warning** if the binary is absent locally; mandatory in CI |
| 6 | UI: `npm ci` (only if `node_modules` is missing) | |
| 7 | UI: `npx tsc --noEmit`, `npm run lint`, `npx prettier --check src` | |
| 8 | UI: `npm test` (vitest), `npm run build` | |
| 9 | `node scripts/check-bundle-budget.mjs` | initial-chunk budget |
| 10 | `npx playwright test` | interface e2e — **inside** the gate, see below |

Step 10 is not an optional extra, and it was added because it had been one:
"mark as read" disappeared from the server menu in the 4.5.0 redesign and the
regression was published, while the test covering it already existed and had been
failing on its own for weeks. A suite outside the gate is a suite that does not
exist.

The GitHub workflow (`.github/workflows/ci.yml`) mirrors this list, and keeping
the mirror exact is a rule — every divergence is a future trap. The known ones,
each deliberate:

- CI runs `cargo test --workspace --lib` only (the real-UDP e2e are flaky on a
  hosted runner with no P2P network), **plus** the transport SimNet e2e in
  **release** profile. That extra step is not redundant: a `debug_assert!` is not
  evaluated in release, which is exactly the 3.0.0 regression, and it would have
  caught it.
- The supply-chain audits are mandatory there, optional here.
- CI pins Node 22 and exports `CMAKE_POLICY_VERSION_MINIMUM=3.5`; `ci.sh` does
  neither (see below on both).

Current status (2026-07-26): **1,274 Rust tests** over 54 binaries and **2,113
vitest tests** over 141 files, zero warnings. Treat these as an order of
magnitude, not a checksum — `cargo test --workspace 2>&1 | grep '^test result'`
is the answer that is always right, and a count in a document is stale the week
after it is written.

### Rust side

```sh
cargo test --workspace                 # all tests
cargo test -p accord-crypto            # a single crate
cargo test -p accord-node --test two_node_e2e   # a specific integration test
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
```

### UI side (`app/`)

```sh
npm ci               # reproducible install
npm run dev          # Vite only (see "browser mode" below)
npm run test         # vitest
npm run test:watch
npm run lint         # eslint
npm run format       # prettier --write
npm run build        # tsc -b && vite build
npm run e2e          # playwright (also run by ci.sh)
```

⚠️ **Node version.** `package.json` declares `engines: ">=20 <25"` and CI runs
**22**; nothing in the repository pins the local version, and npm only *warns*
when it is out of range. So a local Node 25+ installs and runs anyway — and any
"green in CI, red locally" (or the reverse) on the frontend starts by checking
`node --version` against that range, before anything else is suspected.

### Desktop application (Tauri)

```sh
cd app
npm run tauri dev     # UI + Rust host in dev mode
npm run tauri build   # installable bundle
```

The Tauri host (`accord-app`) enables the `hardware` feature: this requires **system
libopus + pkg-config** (macOS: `brew install opus pkgconf`; Debian:
`apt install libopus-dev pkg-config`). Reason: the libopus bundled with
`audiopus_sys` no longer compiles with CMake ≥ 4 (D-020).

⚠️ **This is not only a Tauri concern — it gates `./ci.sh` itself.** `accord-app`
is a workspace member, so `cargo test --workspace` pulls `audiopus_sys` in and a
machine without system libopus fails at step 1 of the gate with a CMake error
about `cmake_minimum_required`, before a single test runs. Two ways out, and CI
uses the second:

```sh
brew install opus pkgconf                    # use the system libopus (preferred)
CMAKE_POLICY_VERSION_MINIMUM=3.5 ./ci.sh     # or build the bundled one anyway
```

`ci.yml` exports `CMAKE_POLICY_VERSION_MINIMUM=3.5` for the whole job; `ci.sh`
does **not**, so on a CMake ≥ 4 machine the two are not interchangeable without
one of the lines above.

### Standalone daemon `accord-noded`

For multi-node tests without a UI:

```sh
ACCORD_PASSPHRASE='phrase de test' \
ACCORD_PROFILE=/tmp/noeud-a \
ACCORD_P2P_ADDR=0.0.0.0:0 \
cargo run -p accord-node --bin accord-noded
```

Variables: `ACCORD_PROFILE` (default `./accord-profile`), `ACCORD_PASSPHRASE`
(mandatory — never as a CLI argument, visible in `ps`), `ACCORD_API_PORT`
(default ephemeral), `ACCORD_P2P_ADDR` (default `0.0.0.0:0`), `ACCORD_POW_BITS`
(default 16). On startup, the daemon writes `<profile>/session.json` (0600) with
the address and token of the local API.

**UI browser mode**: `npm run dev` without Tauri works by manually writing
a daemon's session into
`localStorage['accord.dev.session'] = '{"port":…,"token":"…"}'`
(see `app/src/lib/bridge.ts`).

### Reading what a running app actually did

A GUI app has no standard output: launched from the Finder or the Start menu,
everything `tracing` produced used to go nowhere, and diagnosis meant asking the
user to describe what they saw. Two surfaces now exist, and knowing they do is
half of any bug investigation.

- **Disk log** — `<app_data>/logs/accord.log`, written on every normal launch
  (`app/src-tauri/src/journal.rs`). The previous run is kept as `accord.log.1`
  rather than being truncated: the restart that follows a crash used to erase the
  trace of that crash. Rotated at 5 MiB, so the footprint is bounded to twice
  that. Startup lines are buffered in memory until the data directory is known
  and then flushed, so a failed boot — exactly the case worth reading — is not
  the one part missing. Level is `info`, overridable with `RUST_LOG`, and
  changeable at runtime from the UI (`journal_niveau` IPC command).
- **Bug report bundle** — the `diagnostics.report` JSON-RPC method
  (`API.md`). It is the only response in the API designed to be sent to someone
  else, and it is redacted for that: no friend public keys, no friend IP
  addresses, external addresses reduced to their port.

🔒 Both are made to be shared, which is a constraint on `tracing::` calls
**everywhere in the repository**, not on the module that writes them: never a
message body, a key, a friend code, or a friend's address. A log nobody dares
send is a log that does not exist.

### macOS: "Operation not permitted" while copying a source file

A build can fail on macOS with `Operation not permitted` at the moment
`tauri-build` **copies** a file it has just read without trouble. The cause is
`com.apple.macl`, an extended attribute the kernel attaches to files opened
through certain sandboxed paths. It survives `chmod`, and `xattr -c` cannot
remove it — macOS reapplies it immediately.

It is neither a repository problem nor a code problem: the same sources build
in CI. Run the diagnostic, which clears every attribute it can and tells you
plainly when the blocking one survives:

```sh
scripts/clean-xattrs.sh
```

If it reports `com.apple.macl` still present, **restart the machine** — that
clears it in practice. Granting the terminal Full Disk Access also works.
Meanwhile the GitHub workflow mirrors `./ci.sh` exactly and remains a valid
gate: no step of the CI depends on a local capability.

## 3. Cargo features

| Feature | Where | Effect |
|---------|----|-------|
| `hardware` | `accord-voice`, re-exported by `accord-node` | Real Opus codec (`opus`) + mic capture/playback (`cpal`). Without it: pure DSP logic only. |

Two configurations to keep green (including clippy):

```sh
cargo build -p accord-voice                      # without hardware
cargo build -p accord-voice --features hardware  # with Opus/cpal (libopus required)
```

Caution: as soon as `accord-app` is part of the build, Cargo feature
resolution enables `hardware` across the entire workspace. That is why the
simulated/hardware choice for voice is made **at runtime**
(`NodeConfig.voice_backend`: `Materiel` or `Simule`), not via a `cfg` (D-025).

## 4. Test harness

From fastest to most realistic:

- **Pure unit tests**: logic without I/O (proto codec, crypto, op-log, voice
  DSP, maintenance decisions). The vast majority of the Rust tests, and the only
  level CI runs for the workspace (`--lib`).
- **Deterministic simulated mesh**: `accord-transport` provides a
  `DatagramSocket` abstraction with two implementations — real UDP and an
  **in-memory simulated network** (controlled loss, latency, churn). The full
  protocol runs on both.
- **In-memory DHT testnet**: `accord-dht/src/testnet.rs` spins up dozens
  of Kademlia nodes without a network.
- **Real UDP integration**: `crates/accord-node/tests/` —
  `two_node_e2e.rs` (friendship + DM + groups over two real nodes),
  `maintenance_e2e.rs` (presence published/resolved, outbox drained after
  restart, GroupSync convergence), `voice_e2e.rs` (cross joins,
  simulated frames, WebSocket events, cap of 10).
- **UI**: vitest + Testing Library (`app/src/**/*.test.ts{,x}`) — stores,
  i18n, components, JSON shapes of the API contract.

Test shortcuts worth knowing: `VaultParams::insecure_for_tests()` (lightened
Argon2), reduced PoW (low `pow_bits`), `Simule` voice backend
(`VoiceHandle::inject_pcm` to inject capture). No simulated clock in the e2e
tests: waits are bounded (~20 s max, D-024).

## 5. Project conventions

- **English for the public repository**: documentation, README, release notes
  and commit messages are in English. **Code comments are in French** — that is
  what the code actually does, including everything written since 7.0, and
  claiming an English migration that never happened only produced a third
  convention. Two documents have not caught up either: `docs/COMMUNITY.md` and
  `docs/VOICE_CALLS.md` are still French. Stated rather than quietly ignored.
- **Rust**: no `unwrap()`/`expect()` outside tests (except proven and
  commented invariants); `#![forbid(unsafe_code)]` on sensitive crates;
  typed errors with `thiserror`; `cargo fmt` + `clippy -D warnings`
  mandatory; `tracing` never logging a secret, key, address or
  content.
- **SPEC.md is a byte-exact contract**: any wire-format change goes
  through a SPEC update, with frozen test vectors on the `accord-proto` side.
- **Every structural decision** is tracked (context → options → choice →
  rationale), including the "outstanding debts" (never any silent debt).
- **TypeScript**: strict, `eslint` + `prettier` green; global state via
  per-domain Zustand stores; UI strings via i18n (`app/src/i18n`,
  FR/EN).
- **Security first**: it is the project's #1 priority; in case of conflict
  with performance or simplicity, security wins (see
  `SECURITY.md`).
- Before pushing: `./ci.sh` must pass in full.

### Branch naming

| Prefix | For | Example |
|---|---|---|
| `feat/` | a new user-visible capability | `feat/group-video` |
| `fix/` | a bug fix on existing behaviour | `fix/composer-position` |
| `perf/` | optimisation with no behaviour change | `perf/oplog-fold` |
| `chore/` | tooling, dependencies, housekeeping | `chore/bump-tauri` |
| `docs/` | documentation only | `docs/threat-model` |
| `integrate/` | merging several streams before a release | `integrate/v6-es` |

Housekeeping rules:

- A branch merged into `main` is deleted, locally **and** on the remote. The
  history stays in `main`; a stale branch only creates doubt about what it
  still holds.
- **A parallel stream gets its own `git worktree`**, never a shared checkout.
  Two sessions writing into the same working tree overwrite each other's files
  silently — this has already cost a full re-do of an i18n batch.
- 🔒 Never delete a worktree or a branch holding uncommitted work without
  surfacing it first. `git worktree list` then `git -C <path> status --short`
  takes ten seconds; losing work does not have a fix.
- `dist/` keeps at most the two most recent local builds. Every published
  release is downloadable from GitHub.
