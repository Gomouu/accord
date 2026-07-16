# Distributing Accord

This document describes how to produce and organize Accord's deliverables for the
three desktop platforms (macOS, Windows, Linux) **plus the source code**.

Accord is a [Tauri 2](https://tauri.app) application: a React frontend
(`app/`) packaged with a Rust host (`app/src-tauri/` + the `crates/` workspace).
Each platform produces its own native installers.

> **Current version:** `0.1.0` — produces `Accord`, identifier `fr.accord.desktop`.

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
| `macos/` | Universal DMG + `.app` application | **Local on macOS** — `scripts/build-macos.sh`, **or** the CI macOS job |
| `windows/` | NSIS `.exe` installer + MSI `.msi` | **CI** (Windows job) — **or** local on Windows via `scripts/build-windows.ps1` |
| `linux/` | `.deb` package + AppImage | **CI** (Linux job) — **or** local on Linux via `scripts/build-linux.sh` |

---

## Per-platform prerequisites

### Common to all application builds

- **Node 20+** and **npm** (the frontend uses `npm ci` with `app/package-lock.json`)
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
  patchelf
```

`patchelf` is required for AppImage bundling. On Ubuntu older than 22.04,
WebKitGTK 4.1 may be missing — prefer Ubuntu 22.04 or newer.

---

## Exact commands

### 1. Source code (any OS)

```bash
./scripts/preparer-code-source.sh
# -> dist/code-source/accord-source-0.1.0.tar.gz (size shown at the end of the run)
```

### 2. macOS (run on a Mac)

```bash
./scripts/build-macos.sh
# Universal binary by default (Apple Silicon + Intel).
# Artifacts: target/universal-apple-darwin/release/bundle/{dmg,macos}
```

> Building the voice stack compiles the vendored Opus codec via CMake. With
> CMake ≥ 4 (e.g. current Homebrew), `audiopus_sys`'s bundled CMakeLists is
> rejected as too old — export `CMAKE_POLICY_VERSION_MINIMUM=3.5` before any
> `cargo`/`ci.sh` invocation (or install `cmake@3`).
>
> The frontend test suite (vitest 2.x + jsdom) requires a Node LTS in the
> `>=20 <25` range (declared in `app/package.json` `engines`); Node 26+
> breaks the jsdom environment (`window.localStorage` undefined). With
> Homebrew: `brew install node@22` and prefix
> `PATH="/opt/homebrew/opt/node@22/bin:$PATH"`.

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
# Pushing a version tag triggers the CI:
git tag v0.1.0
git push origin v0.1.0
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
│   └── accord-source-0.1.0.tar.gz          # scripts/preparer-code-source.sh
├── macos/
│   ├── Accord_0.1.0_universal.dmg          # drag-and-drop installer
│   └── Accord.app.tar.gz                    # application only (optional)
├── windows/
│   ├── Accord_0.1.0_x64-setup.exe          # NSIS installer
│   └── Accord_0.1.0_x64_en-US.msi          # WiX/MSI installer
└── linux/
    ├── accord_0.1.0_amd64.deb              # Debian/Ubuntu package
    └── accord_0.1.0_amd64.AppImage         # portable binary
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

The detailed network configuration (peer discovery, ports, DHT, etc.) is
out of scope for this distribution document. **See the dedicated network guide**
(defined by a separate effort) for runtime configuration.
