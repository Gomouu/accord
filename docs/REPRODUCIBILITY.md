# Reproducibility and verifiability

> "The code is open" proves nothing if the distributed binary cannot be checked
> against it. This document says exactly what a third party can verify about a
> published Accord build, what they cannot, and why.

**The headline, stated first so nobody has to read to the end for it:**

> Accord builds are **not** reproducible byte for byte, and nothing in this
> document pretends otherwise. What follows is the verification that *is*
> achievable, with the exact commands, and a precise account of what each check
> does and does not prove.

A procedure that admits its limits is worth more than a claim that does not
hold. The concrete blockers are enumerated in §2 — each traceable to a file in
this repository — so that "not reproducible" is a checkable statement rather
than an excuse.

---

## 1. What is published, and by what

`.github/workflows/release.yml` runs on a `v*` tag. It first re-runs the whole
CI gate (`workflow_call` on `ci.yml`) — **no installer is published if
validation fails** — then builds on three native runners: `macos-latest`,
`windows-latest`, `ubuntu-22.04`. Cross-compiling desktop installers from one
OS to another is not viable (see [`DISTRIBUTION.md`](DISTRIBUTION.md) for why),
so there are three builds and three sets of artefacts.

`tauri-action` creates the GitHub release, uploads the bundles, and — because
`app/src-tauri/tauri.conf.json` sets `createUpdaterArtifacts: true` — also
produces **updater artefacts signed with minisign** and a `latest.json`
manifest listing them. The release is created as a **draft** and published by
hand.

Two facts about the published binaries that matter for everything below:

- **They are not signed by Apple or Microsoft.** macOS reports the app as
  "damaged" because it is not notarized; Windows SmartScreen warns because the
  installer is not code-signed. This is stated in the release notes and in the
  README. So the platform's own signature chain gives you nothing here — the
  minisign chain in §3.1 is the only cryptographic authenticity you get.
- **A build-time secret is baked in.** `ACCORD_BOOTSTRAP` (repository secret,
  format `ip:port,ip:port`) is read at compile time through
  `option_env!("ACCORD_BOOTSTRAP")` in `accord-node` and burned into the binary
  as the default rendezvous list. Its value is not in the repository.

---

## 2. Why byte-for-byte reproducibility is not achieved

Each of these is checkable against a file in this repository. None is
hand-waving; all of them would have to be fixed for a bit-identical rebuild to
be possible at all.

| # | Blocker | Where |
|---|---|---|
| 1 | **The Rust compiler version is not pinned.** There is no `rust-toolchain.toml`; the workflow installs `dtolnay/rust-toolchain@stable`, which resolves to whatever `stable` was on the build date. That version is recorded nowhere in the artefacts. | `.github/workflows/release.yml` |
| 2 | **The Node version is not pinned to a patch.** `actions/setup-node` with `node-version: 20` resolves to the latest 20.x of the day. (Note: CI validates on 22, the release job builds on 20 — both inside the `>=20 <25` range `app/package.json` declares.) | same |
| 3 | **The runner images are mutable.** `macos-latest` and `windows-latest` float by design; `ubuntu-22.04` pins a major but the image itself is rebuilt. System libraries (`libopus`, WebKitGTK, MSVC) come from those images. | same |
| 4 | **A build input is secret.** `ACCORD_BOOTSTRAP` is injected from a repository secret. A third party who does not supply the identical string produces a different binary — and cannot know from the repository what the string was. | `.github/workflows/release.yml`, `crates/accord-node/src/lib.rs` |
| 5 | **Absolute paths leak into the binary.** Nothing passes `--remap-path-prefix`, so panic locations and debug info carry the builder's directory layout. | no configuration for it exists |
| 6 | **Timestamps are embedded by the bundlers.** DMG filesystem images, NSIS and WiX installers, `.deb` and AppImage archives all record build times. Nothing sets `SOURCE_DATE_EPOCH`. | Tauri bundling |
| 7 | **The signature cannot be reproduced at all, by design.** The updater artefacts are signed with a private key held by the maintainer. A third party cannot produce a matching `.sig`, and must not want to. | `TAURI_SIGNING_PRIVATE_KEY` |
| 8 | **Windows takes a different build path.** The job pins CMake `3.31.6` from pip and forces the Ninja generator to work around the vendored Opus and the versioned MSVC generator. That path is not the one a Windows contributor gets by default. | `.github/workflows/release.yml` |

Blockers 1–3 and 5–6 are the ordinary ones and are solvable with known
techniques (§6). Blocker 4 is specific to this project. Blocker 7 is not a
blocker to fix — it is what a signature is for.

**Consequence, said plainly:** if you rebuild from a tag and the installer's
SHA-256 differs from the published one, **you have learned nothing.** It will
differ. A hash comparison on the final installer is not a meaningful test here,
and presenting it as one would be dishonest.

---

## 3. What you *can* verify

Four checks, in decreasing order of strength.

### 3.1 That an artefact came from the release key (strongest)

This is real cryptographic authenticity, checkable offline, by anyone, today.
The updater public key is committed in
`app/src-tauri/tauri.conf.json` → `plugins.updater.pubkey`, and the same key is
compiled into every installed copy of Accord — which is how the built-in
updater authenticates an update before installing it.

The `pubkey` field is base64 of a two-line minisign public-key file. Decoded, it
reads:

```
untrusted comment: minisign public key: 8E442580FFC01989
RWSJGcD/gCVEjqMbrdAeSw5HwMq/q5eoFWySTDcSTAKjuxW//cP1VEgj
```

Verify an artefact (needs `minisign`, `jq`, `curl`):

```sh
# 1. The public key, straight out of the checked-out source — not out of the
#    release. Taking the key from the thing you are verifying proves nothing.
jq -r '.plugins.updater.pubkey' app/src-tauri/tauri.conf.json \
  | base64 -d > accord-updater.pub

# 2. The manifest of the release you are checking.
curl -sL https://github.com/Gomouu/accord/releases/latest/download/latest.json \
  -o latest.json

# 3. What it covers. Do not assume the platform names — read them.
jq -r '.version, (.platforms | keys[])' latest.json

# 4. For one platform key, e.g. the one printed above:
PLAT=darwin-aarch64
curl -sL "$(jq -r --arg p "$PLAT" '.platforms[$p].url' latest.json)" -o artefact
jq -r --arg p "$PLAT" '.platforms[$p].signature' latest.json | base64 -d > artefact.minisig

# 5. The check.
minisign -Vm artefact -p accord-updater.pub
```

⚠️ **Read what this covers.** `latest.json` lists the **updater** artefacts.
If the file you downloaded by hand from the Releases page is not at one of the
URLs in `latest.json`, **this signature does not cover it** — step 3 is how you
find out, and it is not a formality.

What a successful verification proves: the bytes were signed by the holder of
the release key and have not been altered since. What it does **not** prove:
that those bytes were built from any particular source. A signature is a
statement about the signer, not about the compiler.

Also check, as step 8 of the release ritual does: every platform entry in
`latest.json` has a `signature`, and every `url` is `https`.

### 3.2 That the download you have is the one that was published

Compare against the hashes published with the release (§4):

```sh
shasum -a 256 Accord_7.1.0_aarch64.dmg      # macOS
sha256sum accord_7.1.0_amd64.deb            # Linux
certutil -hashfile Accord_7.1.0_x64-setup.exe SHA256   # Windows (cmd)
```

Proves integrity of the transfer and nothing more. If §3.1 succeeded for the
same file, this adds nothing; it exists for the artefacts §3.1 does not cover.

### 3.3 That the release was built by the published workflow, from a known commit

The repository is public, so the Actions run that produced a release is public
too. For a release `vX.Y.Z`:

```sh
gh run list --repo Gomouu/accord --workflow release.yml
gh run view <run-id> --repo Gomouu/accord --log
```

The run records the commit SHA it checked out, the workflow file at that
commit, every command executed, and the tool versions the setup actions
resolved. Cross-check that SHA against the tag:

```sh
git fetch --tags
git rev-list -n 1 vX.Y.Z
```

This is provenance by public log, not by cryptographic attestation: it is only
as trustworthy as GitHub. It is still the difference between "someone says this
came from that commit" and "here is the log".

### 3.4 An independent rebuild, and what to compare

Worth doing — but only if you know in advance what a difference means.

```sh
git clone https://github.com/Gomouu/accord && cd accord
git checkout vX.Y.Z
./ci.sh                # the gate the release itself had to pass
cd app && npm ci && npm run tauri build
```

Comparable, and meaningful if it differs:

- **The gate result.** A tag whose `./ci.sh` is red is a real finding, and it
  needs no reproducibility at all to report.
- **The declared version** in `Cargo.toml`, `app/package.json` and
  `app/src-tauri/tauri.conf.json` against the tag and against the app's own
  About screen.
- **The updater public key** compiled into your build against the one in the
  published `latest.json` chain (§3.1). A mismatch there is serious.
- **The CSP** in `tauri.conf.json` against what the shipped app enforces.
- **Behaviour.** Run both builds side by side against the same peer. This is
  weak evidence and it is not nothing.

Not comparable, and proving nothing when it differs: **the SHA-256 of any
installer, bundle, executable or archive.** See §2.

---

## 4. Publishing artefact hashes

Not automated today. Until it is, this is the manual step, to be run after the
draft release exists and before publishing it (between steps 8 and 9 of the
release ritual):

```sh
# From an empty directory.
gh release download vX.Y.Z --repo Gomouu/accord --dir .
shasum -a 256 * > SHA256SUMS.txt     # sha256sum on Linux
cat SHA256SUMS.txt                   # read it; a truncated file is worse than none
gh release upload vX.Y.Z SHA256SUMS.txt --repo Gomouu/accord
```

Rules, so the file means something:

- **Generate it from the downloaded release assets**, never from the local
  build tree. The point is to describe what users will actually fetch.
- **Include every asset**, `latest.json` and the `.sig` files included.
- Publish it as a release asset, not in the release notes body, so it can be
  fetched and diffed rather than copy-pasted.
- 🔒 **Do not sign `SHA256SUMS.txt` with the updater key.** That key exists for
  the updater and is used by an automated workflow; widening its use widens the
  blast radius of a compromise (§5). A hash list is an integrity aid, not an
  authenticity claim — §3.1 is where authenticity lives.

Automating this inside `release.yml` is the obvious improvement and is
deliberately not done in this document's change: it modifies the release path,
which deserves its own review.

---

## 5. 🔒 The signing key

The updater signing key lives at `~/.tauri/accord-updater.key` on the
maintainer's machine and is mirrored into the repository secret
`TAURI_SIGNING_PRIVATE_KEY`, which `release.yml` passes to `tauri-action`.
`TAURI_SIGNING_PRIVATE_KEY_PASSWORD` is empty — **the key file is the whole
secret.**

**Never** read it, print it, copy it, paste it into an issue or a chat, commit
it, add it to a test fixture, or include it in a diagnostic bundle. Nothing in
this repository needs its contents, and no contribution ever will: the public
half is already committed in `tauri.conf.json`, and that is the only half
anybody verifying anything needs.

If it ever leaks, the consequences are worse than usual, and it is worth knowing
why before you get anywhere near it. The public key is **compiled into every
installed copy** of Accord. Rotating it means installed apps can no longer
verify new releases — the built-in updater stops working for everyone, and every
existing user has to download and install a new build by hand, through the same
unsigned channel that the warning in §1 describes. There is no in-band way to
distribute a new key to installations that only trust the old one. A leak is not
"rotate and move on"; it is a migration imposed on every user.

If you believe it has been exposed: do not open a public issue. Use the route in
[`SECURITY.md`](../SECURITY.md).

---

## 6. What it would take to make this real

Roughly in order of cost, and stated as work that has not been done rather than
as a promise:

1. **Pin the toolchain.** A committed `rust-toolchain.toml` and an exact Node
   version in the workflow. Cheap, and it removes blockers 1 and 2 outright.
2. **Record the build inputs in the release.** A small `BUILD-INFO.txt` asset
   per platform: `rustc -V`, `node -v`, `npm -v`, the runner image label, the
   commit SHA, and whether `ACCORD_BOOTSTRAP` was set. This alone turns "you
   cannot reproduce it" into "here is what you would need" — and it is the
   single highest-value item on this list, because every other step is
   worthless while the inputs are unknown.
3. **Remap paths and freeze timestamps.** `--remap-path-prefix` in the release
   profile, `SOURCE_DATE_EPOCH` from the tag date. Removes blockers 5 and 6 for
   the compiled objects.
4. **Make the bootstrap list a public build input.** The rendezvous addresses
   are public infrastructure — they are visible in the app's Network settings
   and in every packet that reaches them. Keeping them in a repository secret
   buys no confidentiality and costs reproducibility. Publishing the value used
   for a given release removes blocker 4.
5. **Pin the runner images** to immutable labels where the platform allows it.
6. **Then, and only then, attempt a bit-identical rebuild** — and publish the
   result honestly, including the parts that still differ.

Until at least items 1 and 2 exist, §3.1 and §3.3 are the whole story, and this
document says so rather than implying more.
