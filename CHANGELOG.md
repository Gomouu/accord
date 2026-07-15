# Changelog

All notable changes to Accord. This project follows [semantic versioning](https://semver.org).

## [1.2.7] — 2026-07-15

### Changed

- All networking now lives in the **"Add friend"** tab (your address, add-by-address,
  peer/DHT counters, automatic connection UPnP/mDNS, firewall panel); the separate
  "Network" settings tab was removed.

### Fixed

- Your **address list now distinguishes public (internet-reachable, incl. your
  global IPv6) from local (same-Wi-Fi) addresses**, so you no longer share a LAN
  address that a friend can't reach. Trying your public IPv6 often works without
  any port forwarding.

## [1.2.6] — 2026-07-14

### Added

- **Default bootstrap nodes** for peer discovery. Friend-code resolution (the
  first step of any invitation, a DHT `FIND_VALUE`) can now fall back to
  always-on rendezvous nodes, so two peers both behind a symmetric NAT can find
  each other without opening a port. Addresses are supplied via
  `ACCORD_BOOTSTRAP` (`ip:port,…`, runtime env or baked at build); with none
  configured behaviour is unchanged. Bootstrap nodes only route ciphertext —
  no central server, no plaintext. See `docs/NAT-FIRST-CONTACT.md` §3ter.

## [1.2.5] — 2026-07-14

### Changed

- Redesigned **profile / account panel** (the user card from the bottom-left
  user menu): Discord-style banner, avatar decoration, presence and custom
  status with quick status management, bio, copyable identifier and actions.
  Responsive, verified in light and dark themes.

## [1.2.4] — 2026-07-14

### Added

- New **immersive themes**: extra palettes with subtle animated scene
  backgrounds, selectable from Settings → Appearance. Motion is
  compositor-only and honours the reduced-motion preference; existing saved
  themes keep working unchanged.

## [1.2.3] — 2026-07-14

### Added

- **Be invited without opening a router port.** First contact behind a
  (symmetric) NAT now works through an authenticated self-announce and a
  relay/hole-punch rendezvous, so a brand-new user behind a home box can be
  invited without ever forwarding a port. Hardened against announce-flooding
  and forged relay-eligibility (per-session control-message rate limit,
  reachability-verified relays, observer-identity vote de-duplication).

### Changed

- Interface polish pass: genuinely responsive conversations, threads, member
  lists and attachments; no overflow across Friends/DMs, modals, onboarding and
  settings; AA contrast, restored native controls, stronger focus rings and
  touch targets; refined active/loading/offline/error/notification states; more
  reliable message scrolling and async search.

## [1.2.2] — 2026-07-14

### Fixed

- **Avatar decorations and profile effects now actually render** (they were
  invisible: SVGs had no explicit size, animations barely moved, and the effect
  sat behind an opaque card). They now show on every avatar surface — profile
  card, user panel, messages, DMs, member lists, calls, invitations, quick
  switcher and server views.

### Changed

- Redesigned personalization: a premium built-in catalogue (6 avatar
  decorations, 4 profile effects, static & secure CSS/SVG) and reworked profile
  cards + personalization picker, tuned for both light and dark themes.

## [1.2.1] — 2026-07-14

### Added

- Profile customization: pick a built-in **avatar decoration** (glow, neon,
  aurora, golden laurel, sakura, pixel crown) and an animated **profile effect**
  (aurora, starfield, petals, particles) from profile settings. Shared with
  peers as a tiny id — no image transfer.
- Discord-style context menus: right-click a member/author, a server icon, or
  the server name for the full set of actions (call, remove/block, mark as read,
  create channel/category/event, hide muted channels…).
- The message composer now mirrors Discord: a single **+** creation menu on the
  left (attach files, create a poll), the input, then the emoji/send cluster.

### Fixed

- Server invitations can be **re-sent**: the "Invited ✓" state was a permanent
  dead-end, so a second invite to the same friend was impossible until reopening
  the modal. It now reverts after a moment.
- Member avatars in **Server settings → Members** now load (were initials-only).

### Changed

- Graceful **close animations** for toasts, the image lightbox and modals
  (they used to vanish instantly).

## [1.2.0] — 2026-07-14

### Added

- Share files of any size: the attach button now uses a native file picker and
  peer-to-peer transfer, lifting the old 8 MiB upload ceiling.
- Downloads show live progress, and large files are saved through a native
  "Save as…" dialog.
- Settings → System has a button to re-request system permissions
  (notifications, microphone) if one was denied by mistake.
- Rich invite embeds: an `accord://invite/…` link posted in a message renders a
  Discord-style card with the server's name, icon and banner and a one-click
  Join button.

### Fixed

- Pinned images, files and voice messages are now visible in the pinned panel
  (previously blank).
- Clicking your own profile or a server's menu a second time now just closes it,
  instead of flickering closed-then-open.
- Self-mentions (`@you`) in a server channel are now highlighted like any other
  mention.
- Joining a server from an invite link is discoverable and reliable.

### Changed

- Smoother motion throughout, including a fade-and-zoom when opening an image
  full-screen.
- Poll creation moved out of the attachment menu into its own composer action.

## [1.1.0] — 2026-07-14

### Added

- Download your recovery phrase as a file from the setup screen, so you keep a
  copy if you ever lose your passphrase.
- See who is already in a voice channel **before** joining — occupants now show
  without having to connect first.
- Messages that mention you are highlighted in the feed, Discord-style.
- Server channels show a per-channel mention badge, not just an unread count.

### Fixed

- DM messages no longer stay stuck on "sending…": the indicator clears as soon
  as the message is delivered, instead of only after leaving and reopening.
- Mention "pings" now clear when you open the channel/DM and sit next to the
  channel name instead of drifting to the far edge.
- Pinned messages in DMs now sync to the other person (server-channel pins
  already replicated).

### Changed

- More life throughout the UI: a pulsing "speaking" halo in voice, an
  active-channel accent, the mention highlight, and button/badge micro-motion.

## [1.0.2] — 2026-07-13

More correctness fixes from the frontend audit.

### Fixed
- Out-of-order network responses can no longer revert a fresh message
  edit/delete/reaction or server-state change: refreshes now apply on a
  latest-wins basis and discard stale responses.
- Starting an outgoing call no longer clobbers an incoming call that arrived
  during the request.

## [1.0.1] — 2026-07-13

Correctness patch from a frontend audit.

### Fixed
- Switching/locking accounts now clears in-memory data (conversations,
  server state, contacts) and returns to Friends — the previous account's
  content can no longer briefly appear under the new one. Saved preferences
  (theme, density, language) are preserved.
- Failed optimistic updates (event RSVP, poll vote, server invite) now roll
  back only the affected item, preserving concurrent changes.
- Reactions no longer duplicate on a rapid double-click.

## [1.0.0] — 2026-07-13

First stable release. Feature-complete, with a peer-to-peer core hardened
through repeated adversarial security audits.

### Fixed — stability
- Remote crash: a crafted message fragment could drive the reassembler out
  of bounds and halt all messaging until restart.
- Full-node freeze: a stuck audio device (e.g. an unanswered mic-permission
  dialog) could freeze the node's networking; the event loop and device
  open are now bounded by timeouts.
- Added an error boundary so a crashing screen no longer blanks the whole app.

### Fixed — security hardening (P2P core)
- Moderator-deleted messages can no longer reappear as live.
- Auto-downloaded media (avatars, banners, **server icons/banners**) is
  size-capped, so a malicious admin can't make members pull a huge blob.
- Unsolicited file blocks are dropped cheaply instead of forcing needless
  verification; duplicate parity no longer re-triggers repair.
- Per-peer rate limits on inbound control messages; several in-memory tables
  are now bounded against abuse.

### Known limitations
- Not code-signed/notarized (see install notes).
- Some protections against a *malicious member of your own server* (an
  insider) remain on the roadmap — see `docs/THREAT-MODEL.md`.
- Not an anonymity tool (peers see each other's IP addresses).

## [0.15.0] — 2026-07-12

Stability release: the first two fixes above (reassembly crash, node freeze)
plus the moderation/DoS hardening, driven by an adversarial audit of the P2P
core. Quick-switcher gains server results.

## [0.14.0] – [0.14.2] — 2026-07-12

- Redesigned soundboard (launchpad-style icon and pad grid).
- Rebuilt voice messages; fixed all app sounds (root cause: a missing
  `media-src` CSP directive blocked every `data:` audio clip).
- Discord-style server invitations, server banners.
- Reliable media replication between peers (durable fetch intents, richer
  reachability fallback).
- First contact **without opening a router port** (deterministic home-relay
  rendezvous + hole punching + relay fallback).
- The red window-close button now quits the app on macOS/Windows.
- Two full accessibility passes (keyboard, focus, ARIA) across the app.
- Published `SECURITY.md` and `docs/THREAT-MODEL.md`.

## [0.2.0] – [0.13.0]

The feature-building phase toward Discord parity: servers with typed
channels, categories, colored roles & permissions, moderation
(kick/ban/timeout), pins, mentions, reactions, custom emojis, forum
channels, custom status, read markers, bulk delete; direct messages and
1-to-1 voice calls; group voice channels with noise suppression; file
sharing and attachments; profiles with avatars & banners; automatic NAT
traversal and encrypted offline mailboxes; multi-account, light/dark
themes, English & French.

[1.0.0]: https://github.com/Gomouu/accord/releases/tag/v1.0.0
[0.15.0]: https://github.com/Gomouu/accord/releases/tag/v0.15.0
