# Distributing Accord

This document describes how to produce and organize Accord's deliverables for the
three desktop platforms (macOS, Windows, Linux) **plus the source code**.

Accord is a [Tauri 2](https://tauri.app) application: a React frontend
(`app/`) packaged with a Rust host (`app/src-tauri/` + the `crates/` workspace).
Each platform produces its own native installers.

> **Product:** `Accord`, identifier `fr.accord.desktop`. The version is **not**
> restated here — it would rot at every release. It lives in
> `app/src-tauri/tauri.conf.json` (`version`), which must match
> `[workspace.package] version` in the root `Cargo.toml`; every `<version>`
> below stands for that value.

---

## Why three separate builds (and not a single machine)

A desktop installer does not cross-compile reliably from one OS to another,
because packaging depends on native tools and libraries specific to each system:

| Platform | What prevents production from a Mac |
|------------|--------------------------------------------|
| **Windows** | The NSIS (`.exe`) and WiX (`.msi`) installers and the WebView2 runtime are Windows-specific. The WiX toolchain does not exist on macOS. |
| **Linux** | The `.deb`/AppImage bundling relies on native GTK and WebKitGTK, which cannot be cleanly cross-compiled from macOS. |
| **macOS** | Produced only on macOS (signing, DMG, `.app`). |

**Practical consequence.** The current build machine is an Apple Silicon Mac:
it **reliably produces only the macOS target**. For Windows and Linux, we
do not ship a locally built binary — we ship the **reliable means** to
build it:

1. a **GitHub Actions CI workflow** that compiles each platform on its own
   native runner (`.github/workflows/release.yml`);
2. **"one-command" scripts** to run on the target machine
   (`scripts/build-*.sh` / `scripts/build-windows.ps1`).

---

## The four deliverables

| Folder | Contents | How it is produced |
|---------|---------|------------------------|
| `code-source/` | `.tar.gz` archive of the clean source code | **Local** — `scripts/preparer-code-source.sh` (on any OS) |
| `macos/` | DMG + `.app` application | **Local on macOS** — `scripts/build-macos.sh` (universal by default), **or** the CI macOS job (**arm64 only**, see below) |
| `windows/` | NSIS `.exe` installer + MSI `.msi` | **CI** (Windows job) — **or** local on Windows via `scripts/build-windows.ps1` |
| `linux/` | `.deb` package + AppImage | **CI** (Linux job) — **or** local on Linux via `scripts/build-linux.sh` |

---

## Per-platform prerequisites

### Common to all application builds

- **Node in the `>=20 <25` range** and **npm** (the frontend uses `npm ci` with
  `app/package-lock.json`). The range is declared in `app/package.json`
  (`engines`) and CI validates on 22; Node 26+ breaks the jsdom test
  environment, see the macOS section below.
- **Rust stable** via [rustup](https://rustup.rs) (workspace: `rust-version = 1.85`)
- The **Tauri CLI** is provided by `app/`'s `devDependencies` (no global
  installation needed after `npm ci`)

### macOS

- Xcode Command Line Tools: `xcode-select --install`
- Rust targets for the universal binary (added automatically by the script):
  `aarch64-apple-darwin` and `x86_64-apple-darwin`

### Windows

- Rust **MSVC** toolchain (`x86_64-pc-windows-msvc`, default on Windows)
- **Visual Studio C++ Build Tools** ("Desktop development with C++")
- **WebView2 Runtime**: preinstalled on Windows 11 and Windows Server 2022;
  otherwise install it from Microsoft

### Linux (Debian/Ubuntu 22.04+)

System packages required by Tauri 2:

```bash
sudo apt-get update
sudo apt-get install -y \
  libwebkit2gtk-4.1-dev \
  build-essential \
  curl \
  wget \
  file \
  libxdo-dev \
  libssl-dev \
  libayatana-appindicator3-dev \
  librsvg2-dev \
  libopus-dev \
  libasound2-dev \
  pkg-config \
  patchelf \
  cmake
```

`patchelf` is required for AppImage bundling. The last four are not optional
either: the Tauri host builds `accord-node` with the `hardware` feature
(`app/src-tauri/Cargo.toml`), which pulls the real Opus codec — found through
`pkg-config`, or compiled from the vendored source via `cmake` when the system
library is missing — and cpal, which needs ALSA. This is the same list the
release workflow installs. On Ubuntu older than 22.04, WebKitGTK 4.1 may be
missing — prefer Ubuntu 22.04 or newer.

---

## Exact commands

### 1. Source code (any OS)

```bash
./scripts/preparer-code-source.sh
# -> dist/code-source/accord-source-<version>.tar.gz (size shown at the end of
#    the run; the version is read from app/src-tauri/tauri.conf.json)
```

### 2. macOS (run on a Mac)

```bash
./scripts/build-macos.sh
# Universal binary by default (Apple Silicon + Intel); override with
# ACCORD_CIBLE=aarch64-apple-darwin for a single-arch build.
# Artifacts: target/universal-apple-darwin/release/bundle/{dmg,macos}
```

> ⚠️ **The CI macOS job does not produce a universal binary**, unlike this
> script: it builds for the runner's own arch (arm64). A universal binary needs
> libopus for both architectures and Homebrew only ships the native one, so the
> second slice would have to come from the vendored source. Do not expect the
> DMG attached to a release and the one this script produces to be
> interchangeable on an Intel Mac.

> Building the voice stack compiles the vendored Opus codec via CMake. With
> CMake ≥ 4 (e.g. current Homebrew), `audiopus_sys`'s bundled CMakeLists is
> rejected as too old — export `CMAKE_POLICY_VERSION_MINIMUM=3.5` before any
> `cargo`/`ci.sh` invocation (or install `cmake@3`). `build-macos.sh` exports it
> itself: a universal binary has no way around the vendored Opus, since Homebrew
> only ships libopus for the native arch, so the second slice always goes
> through CMake.
>
> The frontend test suite (vitest 2.x + jsdom) requires a Node LTS in the
> `>=20 <25` range (declared in `app/package.json` `engines`); Node 26+
> breaks the jsdom environment (`window.localStorage` undefined). With
> Homebrew: `brew install node@22` and prefix
> `PATH="/opt/homebrew/opt/node@22/bin:$PATH"`.

#### Signature locale stable (macOS)

macOS attaches the microphone permission (TCC) and the firewall's
"accept incoming connections" grant to the app's **code signature**. With
the default ad-hoc signature, every rebuild has a different fingerprint:
the mic prompt comes back after each build, and the firewall asks again at
every launch — fatal for a P2P app that needs incoming connections. Create
a local self-signed identity **once**, and `build-macos.sh` picks it up
automatically (or set `ACCORD_SIGNING_IDENTITY`):

1. Keychain Access → menu **Keychain Access → Certificate Assistant →
   Create a Certificate…**
2. Name: `Accord Dev` — Identity type: *Self-Signed Root* — Certificate
   type: **Code Signing** → Create.
3. Rebuild: the script detects `Accord Dev` and signs with it; mic and
   firewall grants now persist across builds and launches.

### 3. Windows (run on Windows, in PowerShell)

```powershell
powershell -ExecutionPolicy Bypass -File scripts\build-windows.ps1
# Artifacts: target\release\bundle\{nsis,msi}
```

### 4. Linux (run on Linux)

```bash
./scripts/build-linux.sh
# Artifacts: target/release/bundle/{deb,appimage}
```

### 5. Everything via CI (recommended for Windows + Linux)

The `.github/workflows/release.yml` workflow builds all three platforms in
parallel on native runners and attaches the bundles to a **draft GitHub
release**.

```bash
# Pushing a version tag triggers the CI (the tag must match the version in
# app/src-tauri/tauri.conf.json, since the release notes are extracted from the
# matching CHANGELOG.md section):
git tag v<version>
git push origin v<version>
```

Or manual trigger: **Actions** tab → **Release** workflow → *Run
workflow* (preferably choose a tag-type ref).

---

## Distribution folder structure

The orchestrator collects the artifacts into a single `dist/` tree.
The filenames below follow Tauri's naming convention
(`<product>_<version>_<arch>`) and are given for reference:

```text
dist/
├── code-source/
│   └── accord-source-<version>.tar.gz          # scripts/preparer-code-source.sh
├── macos/
│   ├── Accord_<version>_universal.dmg          # local build; CI produces _aarch64
│   └── Accord.app.tar.gz                       # application only (optional)
├── windows/
│   ├── Accord_<version>_x64-setup.exe          # NSIS installer
│   └── Accord_<version>_x64_en-US.msi          # WiX/MSI installer
└── linux/
    ├── accord_<version>_amd64.deb              # Debian/Ubuntu package
    └── accord_<version>_amd64.AppImage         # portable binary
```

Mapping from bundle source folder → distribution folder:

| Build output | Goes into |
|-----------------|---------|
| `target/universal-apple-darwin/release/bundle/dmg/*.dmg` | `dist/macos/` |
| `target/release/bundle/nsis/*.exe` | `dist/windows/` |
| `target/release/bundle/msi/*.msi` | `dist/windows/` |
| `target/release/bundle/deb/*.deb` | `dist/linux/` |
| `target/release/bundle/appimage/*.AppImage` | `dist/linux/` |

---

## Network configuration

The detailed network configuration (peer discovery, ports, DHT, etc.) is out of
scope for this distribution document. It is covered by:

- [`NAT_TRAVERSAL.md`](NAT_TRAVERSAL.md) and
  [`NAT-FIRST-CONTACT.md`](NAT-FIRST-CONTACT.md) — discovery, hole punching and
  what happens when it fails;
- [`BOOTSTRAP.md`](BOOTSTRAP.md) — running a rendezvous node and the
  `ACCORD_BOOTSTRAP` secret baked into release builds (`deploy/bootstrap/`).
