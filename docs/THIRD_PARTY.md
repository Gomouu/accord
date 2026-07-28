# THIRD_PARTY — Direct dependencies and licenses

> Inventory of Accord's **direct** dependencies (Rust workspace + JS
> frontend), grouped by license. Versions resolved from the lockfiles
> (`Cargo.lock`, `app/package-lock.json`) as of 2026-07-08. Source:
> `cargo metadata` and the `package.json` files of the installed packages.
>
> **Summary: all licenses are permissive** (MIT, Apache-2.0,
> BSD-3-Clause, CC0-1.0, Zlib). No copyleft license (GPL/LGPL/AGPL/MPL)
> among the direct dependencies. Accord itself is under the **MIT** license
> (see `LICENSE`).

## 1. Rust — direct workspace dependencies

### MIT OR Apache-2.0 (dual license, your choice)

| Crate | Version | Role |
|-------|---------|------|
| `argon2` | 0.5.3 | KDF of the identity vault (Argon2id) |
| `async-trait` | 0.1.89 | Async traits (transport, DHT, node) |
| `chacha20poly1305` | 0.10.1 | AEAD XChaCha20-Poly1305 (sessions, vault, groups) |
| `futures-util` | 0.3.32 | Async combinators (API server) |
| `hkdf` | 0.12.4 | HKDF key derivation |
| `hmac` | 0.12.1 | Anti-DoS cookies, blind search index |
| `opus`\* | 0.3.1 | libopus binding (feature `hardware`) — "MIT/Apache-2.0" |
| `rand` | 0.8.6 | Randomness (OsRng): nonces, ephemeral keys, jitter |
| `serde` | 1.0.228 | Serialization (JSON-RPC API, Tauri host) |
| `serde_json` | 1.0.150 | JSON (local API) |
| `sha2` | 0.10.9 | SHA-256/512: NodeId, PoW, transcripts, Merkle |
| `tauri` | 2.11.5 | Desktop host (`accord-app`) |
| `tauri-build` | 2.6.3 | Build of the Tauri host |
| `thiserror` | 2.0.18 | Typed errors of all crates |
| `unicode-normalization` | 0.1.25 | Canonical decomposition (NFD) of the AutoMod fold — same rule as the UI's `normalize('NFD')` |
| `zeroize` | 1.9.0 | Wiping of secrets in memory |
| `hex` (dev) | 0.4.3 | Hexadecimal test vectors |
| `tempfile` (dev) | 3.27.0 | Temporary test directories |

### MIT

| Crate | Version | Role |
|-------|---------|------|
| `reed-solomon-erasure` | 6.0.0 | RS 10+4 parity for file transfers (D-014) |
| `rusqlite` | 0.32.1 | Local database — feature `bundled-sqlcipher-vendored-openssl` (D-013) |
| `tokio` | 1.52.3 | Async runtime of the node |
| `tokio-tungstenite` | 0.24.0 | WebSocket of the local API |
| `tracing` | 0.1.44 | Structured logging (never any secrets) |
| `tracing-subscriber` | 0.3.23 | Log output (daemon, Tauri host) |

### BSD-3-Clause

| Crate | Version | Role |
|-------|---------|------|
| `ed25519-dalek` | 2.2.0 | Identity signatures |
| `x25519-dalek` | 2.0.1 | Key exchange (handshake, sealing) |
| `subtle` | 2.6.1 | Constant-time comparisons (API token) |

### Apache-2.0

| Crate | Version | Role |
|-------|---------|------|
| `cpal` | 0.15.3 | Audio capture/playback (feature `hardware`) |

### CC0-1.0 (public domain)

| Crate | Version | Role |
|-------|---------|------|
| `bip39` | 2.2.2 | 12-word recovery phrase |

### Bundled or system native components

| Component | License | How it arrives |
|-----------|---------|----------------|
| SQLCipher | BSD-3-Clause | Compiled and bundled by `rusqlite` (feature `bundled-sqlcipher-vendored-openssl`) |
| OpenSSL (vendored) | Apache-2.0 | Bundled for SQLCipher's crypto via the same feature |
| SQLite | Public domain | Core of SQLCipher |
| libopus | BSD-3-Clause | **System library** required by the feature `hardware` (via `pkg-config`, D-020) — not bundled |

\* The `opus` crate declares its license in the historical form
`MIT/Apache-2.0` (equivalent to `MIT OR Apache-2.0`).

## 2. JavaScript / TypeScript — direct dependencies (`app/package.json`)

### Runtime (bundled into the application)

| Package | Version | License | Role |
|--------|---------|---------|------|
| `react` | 18.3.1 | MIT | UI |
| `react-dom` | 18.3.1 | MIT | DOM rendering |
| `zustand` | 5.0.14 | MIT | State stores (D-009) |
| `@tauri-apps/api` | 2.11.1 | Apache-2.0 OR MIT | IPC bridge to the Tauri host |
| `qrcode` | 1.5.4 | MIT | QR **generation**: friend link, safety number (§17.4) — lazy chunk |
| `jsqr` | 1.4.0 | Apache-2.0 | QR **decoding** from a camera frame: safety-number verification (§17.4) — lazy chunk |

### Development only (never distributed)

| Package | Version | License |
|--------|---------|---------|
| `@tauri-apps/cli` | 2.11.4 | Apache-2.0 OR MIT |
| `@testing-library/jest-dom` | 6.9.1 | MIT |
| `@testing-library/react` | 16.3.2 | MIT |
| `@testing-library/user-event` | 14.6.1 | MIT |
| `@types/react` | 18.3.31 | MIT |
| `@types/react-dom` | 18.3.7 | MIT |
| `@vitejs/plugin-react` | 4.7.0 | MIT |
| `autoprefixer` | 10.5.2 | MIT |
| `eslint` | 9.39.4 | MIT |
| `eslint-plugin-react-hooks` | 5.2.0 | MIT |
| `eslint-plugin-react-refresh` | 0.4.26 | MIT |
| `globals` | 15.15.0 | MIT |
| `jsdom` | 25.0.1 | MIT |
| `postcss` | 8.5.16 | MIT |
| `prettier` | 3.9.4 | MIT |
| `tailwindcss` | 3.4.19 | MIT |
| `typescript` | 5.6.3 | Apache-2.0 |
| `typescript-eslint` | 8.63.0 | MIT |
| `vite` | 5.4.21 | MIT |
| `vitest` | 2.1.9 | MIT |

## 3. Points of attention

- **No non-permissive license** among the direct dependencies. The
  CC0-1.0 of `bip39` is a public domain dedication — even more permissive
  than MIT (no attribution obligation).
- The BSD-3-Clause licenses (`ed25519-dalek`, `x25519-dalek`, `subtle`,
  SQLCipher, libopus) and Apache-2.0 (`cpal`, `typescript`, vendored OpenSSL)
  require the **retention of copyright notices** in binary
  distributions — to be integrated into the "About" screen or a bundled
  license file at the first public distribution.
- `jsqr` is the only runtime JS dependency whose latest release is old
  (1.4.0, 24 April 2021). It was taken knowingly: it has **zero transitive
  dependencies**, no native binding and no WebAssembly, it implements a
  specification frozen since 2000 (ISO/IEC 18004), and it is still downloaded
  ~1.8 M times a week. The full comparison against `zxing-wasm`,
  `@zxing/library`, `qr-scanner` and the native `BarcodeDetector` is written
  where the import lives, in `app/src/components/SafetyQrScanner.tsx`
  (ROADMAP §10.3). Note that `cargo deny` does **not** cover JS packages —
  this inventory is the only written trace on that side.
- The **transitive** tree is not inventoried here. Before any public
  distribution, run `cargo deny check licenses` (Rust) and
  `npx license-checker` (JS) for the full tree, and `cargo audit` /
  `npm audit` for known vulnerabilities.
- This inventory must be updated on every addition of a direct dependency
  (with architecture review when the dependency is structural).
