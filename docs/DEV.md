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
│   └── accord-node       # Assembly: network runtime, maintenance, API service, voice engine,
│                         #   standalone `accord-noded` binary
├── app/                  # React + TypeScript + Tailwind frontend (Vite, Zustand, vitest)
│   └── src-tauri/        # Crate `accord-app`: Tauri 2 host (workspace member)
├── ci.sh                 # Full local CI (Rust + UI)
└── *.md                  # Contracts and journals (SPEC, ARCHITECTURE, API, DECISIONS, …)
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

## 2. Build and test

### All at once: `./ci.sh`

The repository is **never** left in a state where `./ci.sh` fails. It
runs, in sequence:

1. `cargo fmt --all -- --check`
2. `cargo clippy --workspace --all-targets -- -D warnings`
3. `cargo test --workspace`
4. UI: `npm ci` (if needed), `npm run lint`, `npm run format:check`,
   `npm run build` (tsc + vite), `npm run test` (vitest)

Current status: 279 Rust tests + 115 vitest tests, zero warnings.

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
npm run test         # vitest (115 tests)
npm run test:watch
npm run lint         # eslint
npm run format       # prettier --write
npm run build        # tsc -b && vite build
```

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
  DSP, maintenance decisions). The vast majority of the 279 tests.
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

- **English for the public repository**: documentation, README, release
  notes and commit messages are in English. Code comments are being migrated
  from French to English; new code should be commented in English.
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
