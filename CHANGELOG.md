# Changelog

All notable changes to Accord. This project follows [semantic versioning](https://semver.org).

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
