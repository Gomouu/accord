# Accord

**Peer-to-peer desktop messaging, end-to-end encrypted, serverless.**

Accord looks like Discord — friends, direct messages, groups with text and
voice channels — but works without any central server: the desktop
applications talk directly to each other, and no one else can read what is
said.

<!-- CAPTURE: overview of the application (main window, a group open with a
     text and voice channel). Insert here:
     ![Accord overview](docs/img/apercu.png) -->

## What Accord promises you

- **Your messages can only be read by their recipients.** Everything that
  travels over the network is end-to-end encrypted: messages, group channels,
  voice, and even the technical signaling.
- **No one to unplug, no one to subpoena.** There is no server hosting your
  conversations: they live with you and with your contacts, encrypted on disk.
- **Your identity fits in 12 words.** When you create your account, Accord
  gives you a 12-word recovery phrase: write it down on paper, it lets you
  recover exactly your identity on another machine. No email, no phone number,
  no "forgot password?".
- **You are found by a code, not a directory.** Your friend code
  (`WORD-WORD-WORD-1234`) is shared hand to hand; there is no public list of
  users.
- **Offline is not lost.** Messages sent to a disconnected friend wait in
  encrypted, unreadable "mailboxes" spread across the network, then reach them
  when they come back (up to 7 days).

To be clear, stated plainly: Accord protects the **content** of your exchanges,
not your **anonymity**. Your contacts and the network see your IP address, as
in most peer-to-peer software. The full detail of the guarantees — and their
limits — is in [SECURITY.md](SECURITY.md).

## Project status

Version 0.2.0 (beta), **for the curious and for contributors**:

- prebuilt installers for macOS, Windows and Linux are published on the
  [Releases](https://github.com/Anthonyvimercati/accord/releases) page — you no
  longer have to compile from source (building from source stays available, see
  below);
- NAT traversal is automatic (hole punching, with a decentralized relay
  fallback for symmetric NAT) — two friends usually connect without opening any
  port;
- no public bootstrap nodes yet — the network forms between machines that you
  connect yourself (ideal for testing between friends or on a local network);
- the protocol and the code have not undergone an external security audit.

## Download & install

Grab the installer for your system from the
[latest release](https://github.com/Anthonyvimercati/accord/releases/latest) —
no compiling required.

| System | File | How to install |
|--------|------|----------------|
| **macOS** (Apple Silicon) | `Accord_*_aarch64.dmg` | Open the DMG, drag Accord into Applications. First launch: **right-click the app → Open** (the app is not code-signed, so a normal double-click is blocked by Gatekeeper). |
| **Windows** | `Accord_*_x64-setup.exe` (or the `.msi`) | Run the installer. SmartScreen may warn about an unknown publisher: click **More info → Run anyway** (the installer is not code-signed). |
| **Linux (Debian/Ubuntu)** | `Accord_*_amd64.deb` | `sudo apt install ./Accord_*_amd64.deb` |
| **Linux (any distro)** | `Accord_*_amd64.AppImage` | `chmod +x Accord_*.AppImage && ./Accord_*.AppImage` |
| **Linux (Fedora/RHEL)** | `Accord-*.x86_64.rpm` | `sudo dnf install ./Accord-*.x86_64.rpm` |

> The installers are **not code-signed or notarized** yet, so macOS and Windows
> show an "unknown developer" warning on first launch — the steps above bypass it
> once. Voice on Windows has not been validated by the team; feedback welcome.

## Build from source

If you prefer to build it yourself (or you are on an Intel Mac, for which no
prebuilt binary is provided yet):

### Common prerequisites

| Tool | Version | For what |
|-------|---------|-----------|
| [Rust](https://rustup.rs) | ≥ 1.85 (stable) | the node and the desktop host |
| [Node.js](https://nodejs.org) + npm | ≥ 20 | the interface (React + Vite) |
| libopus + pkg-config | libopus ≥ 1.3 | the audio codec for voice channels |

### macOS

```sh
xcode-select --install          # Apple build tools
brew install opus pkgconf       # audio codec + pkg-config
```

### Linux (Debian/Ubuntu)

```sh
sudo apt install build-essential curl file pkg-config \
  libopus-dev \
  libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev libssl-dev   # Tauri prerequisites
```

For other distributions, see the
[Tauri 2 prerequisites](https://tauri.app/start/prerequisites/); the only
addition specific to Accord is `libopus` (+ `pkg-config`).

### Windows

Tauri prerequisites: Microsoft C++ Build Tools and WebView2 (preinstalled on
Windows 11). Note: voice requires a system libopus visible to `pkg-config`
(for example via vcpkg) — **the Windows build has not yet been validated by
the team**; feedback is welcome.

### Compile and run

```sh
git clone <repo-url> accord
cd accord/app
npm ci                # interface dependencies
npm run tauri dev     # run in development mode
npm run tauri build   # produce the installable application (e.g. .app/.dmg on macOS)
```

The first build also compiles bundled SQLCipher and OpenSSL: expect several
minutes. The data (encrypted identity, message database) lives in your
system's application data directory, never anywhere else.

## Quick start

### 1. Create your identity

On first launch, Accord guides you through three screens: choose a
**passphrase** (it encrypts everything on your disk — make it long), then
**write down the 12-word recovery phrase** that is displayed. It will **never
be shown again**: without it, a lost identity is unrecoverable; with it, you
restore it on any machine ("Restore an identity" on the welcome screen).

<!-- CAPTURE: onboarding screen showing the 12-word phrase.
     ![Recovery phrase](docs/img/onboarding-phrase.png) -->

### 2. Add a friend by code

Your friend code (three words and four digits, of the type `ACID-MAZE-ROBOT-0042`)
is shown in the Friends tab. Exchange your codes through the channel of your
choice, enter your friend's, and confirm the request on both sides. Accord
cryptographically verifies that the code really matches the person — a
mistyped code is detected immediately.

<!-- CAPTURE: Friends view with the "add by code" field.
     ![Add a friend](docs/img/amis-code.png) -->

### 3. Create a group

Create a group, add text channels, invite your friends. Each group has its own
encryption key, renewed automatically when someone leaves or is removed:
former members cannot read new messages.

### 4. Join the voice channel

Each group has a voice channel: click to join it (up to 10 participants), mute
your mic with one click, the rings indicate who is speaking. The audio goes
directly between participants, encrypted like everything else.

<!-- CAPTURE: voice channel with speaking rings.
     ![Voice channel](docs/img/vocal.png) -->

## How it works (in a nutshell)

Each application embeds a **node**: it encrypts everything locally (identity
sealed by your passphrase, SQLCipher database), finds your friends via a
distributed hash table (Kademlia) where everything is signed, and talks to the
other nodes over encrypted UDP/TCP, with NAT traversal and fallback relays.
The interface, for its part, never touches the network.

To go further:

| Document | Contents |
|----------|---------|
| [SECURITY.md](SECURITY.md) | Threat model: guarantees offered and not offered |
| [ARCHITECTURE.md](ARCHITECTURE.md) | Layer architecture and contracts |
| [SPEC.md](SPEC.md) | Wire protocol, down to the byte |
| [API.md](API.md) | Local UI ↔ node API (JSON-RPC + Tauri IPC) |
| [docs/DEV.md](docs/DEV.md) | Developer guide: build, test, contribute |
| [THIRD_PARTY.md](THIRD_PARTY.md) | Dependencies and licenses |

## Official Repository & Authenticity

This repository (https://github.com/Anthonyvimercati/accord) is the ONLY
official source of Accord. Any copy, fork, mirror, or distribution of the
project found elsewhere is unofficial, unverified, fake, and potentially
dangerous (it may have been modified to compromise security or privacy). Do
not download, compile, or run Accord from any other source. Verify that you
are indeed on the official repository before use.

## Disclaimer

Accord is provided "as is", without any warranty. The creator and the
developers decline all responsibility for what happens on this platform or for
the use made of this software. The end user is solely and fully responsible
for their actions, their behavior, and all consequences related to their use
of the software. By using Accord, you accept full responsibility for your use.

## License

[MIT](LICENSE) — © 2026 the Accord contributors.
