# Changelog

All notable changes to Accord. This project follows [semantic versioning](https://semver.org).

## [Unreleased]

### Fixed

- **Reconnecting after a restart or lock is now reliable.** When a friend
  restarted, quit, or came back on a new port, messages you sent while they
  were away could silently fail to arrive and sessions could take minutes to
  recover. Messages you send are now durably queued and delivered the moment
  the friend reconnects — no longer dependent on the operating system reusing
  the old port — the reconnection attempt keeps retrying (bounded) instead of
  giving up after a few seconds, a lost handshake reply now recovers on its
  own, and a stale session from the friend's previous run is dropped as soon
  as the fresh one is established. Locking and unlocking also frees the network
  port cleanly instead of leaking it.

### Fixed

- **Server menu is solid again.** The server header dropdown used a
  translucent glass surface meant for large modals; over the dense channel
  list the blur was too weak and the channel names bled through, making the
  menu look unfinished. It is now an opaque panel — clean and readable, with
  the same rim and shadow.

## [4.4.0] — 2026-07-21

### Changed

- **Redesigned server menu.** The server header dropdown is reorganized
  Discord-style — grouped into Invite, management, preferences, Leave and Copy
  ID sections, with permission-gated management actions. Create Channel and
  Create Category now open focused dialogs instead of dropping you into Server
  Settings, and Leave Server uses an in-app confirmation instead of a native
  popup.

### Added

- **Scheduled messages.** Write a direct message now and have it sent at a
  chosen time — a clock button by the DM composer opens a small scheduler, and
  a new Planning settings tab lists, reschedules or cancels pending sends.
  Purely local: nothing leaves the network before the chosen time, then the
  message follows the normal send path (the outbox covers an offline peer).
- **Message reminders.** "Remind me" on any message pins a local reminder
  (in 20 min / 1 h / 3 h / tomorrow / a custom time, with an optional note);
  when it comes due a native notification fires, exactly once. Manage them from
  the Planning tab. Nothing is sent to anyone.
- **Scheduled backup.** Pick a weekly or monthly cadence and an optional folder
  to be reminded to back up your encrypted profile, with the last and next
  backup shown at a glance and a "back up now" button. The reminder is local;
  the backup itself reuses the existing host export (passphrase-protected,
  session-locking).

## [4.3.0] — 2026-07-21

### Changed

- **Themes everywhere.** The 24 built-in themes now reach every surface —
  onboarding, navigation, chat, member list, settings and overlays — instead
  of stopping at the main window. The theme picker shows a real preview of
  each one, and honours reduced-motion.

### Fixed

- **Readable text on every theme.** All 24 themes were audited for WCAG AA
  contrast and corrected where they fell short: body text, glass surfaces,
  pills, focus rings and syntax highlighting now clear the 4.5:1 minimum
  (measured lows: 4.53 / 4.88 / 4.74 / 5.57 / 5.30). An automated test
  enforces it, so a future theme cannot silently regress.

### Under the hood

- Profile personalization consolidated: the decoration registry and its
  stylesheets were merged and de-duplicated (personalization CSS 4,349 →
  3,622 lines). All 63 cosmetic identifiers are preserved — and now locked
  by test, so a saved decoration can never disappear under a refactor.
- Decorations are lazy-loaded in 8 modules: main stylesheet −21.7 %
  (237.8 → 186.3 kB) and main script −5.5 % (729.8 → 690.0 kB).

## [4.2.0] — 2026-07-21

### Added

- **Safety numbers (identity verification).** Open a friend's profile →
  **Verify identity** to display a 60-digit safety number (and an 8-emoji
  quick rendering) derived from both identity keys. Compare it out of band —
  in person or on a call — and mark the contact as **verified**: a shield
  badge appears on their profile. If their key ever changes afterwards, the
  verification is flagged as **broken** and Accord prompts you to re-verify.
  Everything is derived and stored on-device; nothing goes on the wire.
- **Disappearing messages (local).** Each conversation (DM or server) can arm
  an auto-delete timer (1 hour to 90 days): messages older than the chosen
  delay are removed from **this device's** encrypted store, together with
  their attachments references, reactions, pins and search entries. Purely
  local — no network negotiation; the other side keeps its own copy unless
  it arms its own timer.
- **Privacy dashboard.** Settings → **Privacy** now shows, black on white,
  what Accord stores on this device (friends, messages, files, sizes), that
  the database is **encrypted at rest** (SQLCipher), and the only kinds of
  endpoints the app ever talks to — bootstrap peers, DHT nodes, relays, all
  ordinary peers. **Central servers contacted: 0**, by construction.

## [4.1.0] — 2026-07-20

### Added

- **Connection panel.** The network panel (Settings → Add a friend) now shows,
  for each friend, whether your link is **Direct** or through a **Relay** (and
  which one), plus its **latency** — reusing the existing keep-alive, so nothing
  extra goes on the wire. A new **Diagnostics** section reports your **NAT type**
  and runs a bounded **self-test** that verdicts your reachability (direct /
  hole-punching / relay), probing your bootstrap peers and a candidate relay,
  alongside local counters (hole-punching, relay circuits, reconnections,
  mailbox deposits/pickups). Everything is computed **on-device** — nothing is
  sent anywhere.

## [4.0.0] — 2026-07-20

### Added

- **Saved messages.** Right-click any message → **Save message** to keep a
  private, local bookmark. A panel (bookmark icon next to the mentions inbox)
  lists them, jumps back to the original message, and lets you remove one or
  clear all. Everything stays on your device — nothing is sent to the network.
- **Recent searches.** Focusing the empty search field now suggests your last
  queries; click one to run it again, or remove them individually / all at once.
- **Pin conversations.** Right-click a private conversation → **Pin
  conversation** to keep it at the top of the home list.
- **Interface zoom.** `Ctrl/⌘ +`, `-` and `0` scale the whole interface up,
  down, or back to 100 %. Listed in Settings → Keyboard shortcuts.
- **Collapsed server folders** now carry an aggregated unread/mention badge, so
  activity hidden inside a folded stack is still visible.

### Changed

- Unread and mention badges are capped at **99+** everywhere (no more
  stretched pills on very busy conversations).
- **Empty states** (no friends, no search results…) and **loading skeletons**
  (message history) are now designed rather than blank.
- Confirmation toasts (copied, saved, added…) show as green **success** toasts;
  errors are announced assertively to screen readers.
- **New installs follow the operating system language** on first launch instead
  of always defaulting to English.
- Text selection now uses the current theme's accent colour.

### Fixed

- The server-emoji hint no longer shows a misleading `:{name}:` placeholder; it
  now reads `:name:`, the actual shortcode syntax.

### Security

- **Backup import hardening.** Importing a backup used to apply the Unix file
  permissions stored *in the archive* verbatim — so a forged legacy
  (unencrypted) zip handed to you by someone else could drop executable,
  world-writable or setgid files into your profile. Imported files are now
  always written with safe `0600` permissions, never the archive's
  (adversarially tested).
- Production code paths are now **panic-free by construction**: a CI lint
  rejects `unwrap`/`expect`/`panic!` outside tests, and fuzzing was expanded to
  eight targets (handshake, DHT records, group state, file manifests, the
  encrypted backup archive…) with a committed seed corpus.

### Under the hood

- **Network diagnostics.** New local, on-device diagnostics (per-peer link type
  — direct vs relay —, latency reused from the existing keep-alive, connection
  counters, and a bounded reachability self-test), exposed for a future
  in-app connection panel. No new bytes on the wire; fully 3.x-compatible.

## [3.5.0] — 2026-07-20

### Added

- **Backups are now encrypted.** A `.accordbackup` archive used to be a plain
  zip: the identity vault and database inside it stayed sealed, but every
  **media file** you had exchanged (images, videos) sat in the clear — a backup
  dropped on a cloud drive or USB stick exposed them. The whole archive is now
  sealed under your passphrase (Argon2id + XChaCha20-Poly1305, streamed in 4 MiB
  chunks so multi-gigabyte profiles never load into memory at once). Export and
  import prompt for the passphrase; importing an older, unencrypted backup
  (3.4 and earlier) still works by leaving the field blank. A wrong passphrase
  is reported clearly and distinctly from a corrupted or truncated archive.

### Changed

- **Server invitations now live in the DM conversation** (Discord parity):
  inviting a friend drops an invitation card into your private conversation
  with them — server name, inviter, a **Join** button (and Decline). The
  inviter sees the same card ("Invitation sent", then "Joined" once the
  friend is in). The Friends tab no longer shows server invitations (its
  "Invitations" tab is gone) — it is back to real friend requests only.
  Pending invitations from older versions are re-presented as cards in the
  inviter's DM at startup, nothing is lost. Fully wire-compatible with 3.x
  peers: the invitation message on the network is unchanged; only where it
  shows up changes.

### Fixed

- **LAN discovery now survives an mDNS daemon crash.** A malformed mDNS
  announcement from another device on the network can panic the `mdns-sd`
  daemon thread (upstream `assert!(s.len() < 64)` on the packet-write path,
  reported as [keepsimple1/mdns-sd#483]). That thread runs outside our control,
  so its death used to silently and permanently disable LAN peer discovery for
  the rest of the app's lifetime (the 3.4.0 note tracked this separately). The
  discovery task now **supervises** the daemon: it detects the death (the event
  channel disconnects), logs the incident, and recreates the `ServiceDaemon`
  with a capped exponential backoff (1 s → 30 s) instead of losing discovery in
  silence. Remote-friend connectivity was never affected.

[keepsimple1/mdns-sd#483]: https://github.com/keepsimple1/mdns-sd/issues/483

## [3.4.0] — 2026-07-19

### Fixed

- **Critical: messages could no longer be exchanged at all** (regression
  introduced in 3.0.0). The transport installed initiator-side sessions inside
  a `debug_assert!` — whose argument is **not evaluated in release builds** —
  so the shipped app silently discarded every session it initiated. Both peers
  on 3.x could complete handshakes forever without ever being able to talk
  (infinite reconnection churn, every outgoing send failing). Debug-profile
  tests were all green, which is why CI never caught it. If you and your
  friends saw messages stop after updating to 3.x: this was it — update both
  sides to 3.4.0.
- **Update notes are now rendered** (headings, bold, lists, code) in
  Settings → Updates instead of showing raw Markdown markup.

### Added

- **Support log file**: set the `ACCORD_LOG_FILE` environment variable to
  capture the app's journal to a file (a GUI app's stdout is lost), plus
  precise transport diagnostics: every failed send now logs its reason, and
  every established session logs its role, address and tunnel flag.

### CI

- **Release-profile transport tests**: the hermetic SimNet end-to-end suites
  now also run compiled in release mode — code behavior can diverge between
  profiles (`debug_assert!` is compiled out), which is exactly how the 3.0.0
  regression escaped. This step alone would have blocked it (9/11 tests fail
  with the bug present).
- **`clippy::debug_assert_with_mut_call` enforced**: any state-mutating call
  inside a `debug_assert!`/`debug_assert_eq!` is now a hard CI failure.

### Changed

- `mdns-sd` 0.20.1 → 0.20.2 (LAN discovery; note: a malformed mDNS
  announcement from another device can still crash the discovery thread
  upstream — tracked separately, does not affect remote friends).

## [3.3.0] — 2026-07-19

### Added

- **Emoji autocomplete in the composer**: type `:` followed by at least two
  letters (e.g. `:fire`, `:chat`) and a suggestion popup opens — Unicode emojis
  matched by their French/English keywords plus the current server's custom
  emojis (aggregated across your servers in DMs), keyboard navigation
  (arrows, Enter/Tab to insert, Esc to close), mentions keep priority.
- **Personalized quick reactions**: the hover reaction bar now offers your
  most recent emojis first (shared with the emoji picker), topped up with the
  classic defaults.
- **Draft indicator**: conversations and channels where you left an unsent
  message show a small pencil in the sidebar, live-updated as you type and
  restored on restart.

### Fixed

- **Banner/profile sometimes never arriving after a silent restart
  (root-caused, second episode)**: when a peer died abruptly (no UDP goodbye)
  and came back at a new address, its friend kept TWO direct sessions for the
  same identity for up to 2 minutes — and could route every profile announce
  into the dead one (a UDP black hole, no error, no retry before the 30-minute
  periodic re-announce). Identity→session resolution now prefers the session
  with the most recent inbound traffic. Deterministic transport test added;
  the flaky recovery e2e went from ~6/10 to 10/10 in isolation.

### Tests

- **Playwright end-to-end suite** (`app/e2e/`, `npm run e2e`): 15 browser
  tests against the UI showcase — navigation, server menu (keyboard included),
  composer, thread behaviors, plus 6 visual-regression baselines across
  dark/light/wisteria at two window sizes.

## [3.2.0] — 2026-07-19

### Changed

- **Richer server dropdown menu**: "Mark as read" now sits at the top whenever
  the server has unread channels (same sweep as the rail context menu), and a
  new "Edit my server profile" entry opens Server settings → Members directly
  (per-server nickname and avatar).
- **Your panel shows your status, not your friend code**: the bottom-left user
  panel now falls back to your presence (Online, Idle, Do not disturb,
  Invisible) when no custom status is set — the friend code no longer appears
  there (it lives in Settings → My account and the Friends tab). Set a written
  status by clicking your profile and typing in the "Custom status" field.

## [3.1.0] — 2026-07-19

### Changed

- **Server dropdown menu redesigned** (Discord-style): hovering or keyboard-
  focusing a row now fills it with blurple (red for "Leave server") with white
  text and icons, rows are tighter and more polished, the "Hide muted
  channels" checkbox inverts to white on hover, and the French label "Créer la
  catégorie" was fixed to "Créer une catégorie".

## [3.0.0] — 2026-07-19

A reliability, diagnostics and authoring release. Fully backward-compatible on
the wire with 2.x — every new capability is local or negotiated, so a friend
still on 2.3.x keeps working unchanged.

### Added

- **Faster, more reliable reconnection to friends**: Accord now remembers each
  friend's last known direct address (persisted, 14-day freshness) and dials
  those addresses at startup, in addition to the usual DHT bootstrap — so
  sessions come back without waiting for peer discovery, including after the
  Mac wakes from sleep.
- **Per-friend network panel** (Settings → Network → "Connection to your
  friends"): for each friend, whether a session is currently active and the
  last address you reached them at — connectivity diagnostics without
  screenshots.
- **Markdown tables** (GitHub-flavored): header + alignment row, column
  alignment, escaped pipes, rendered in a scrollable frame.
- **Markdown task lists** (`- [ ]` / `- [x]`): checkbox items render without a
  bullet, with a read-only checkbox.
- **Copy conversation as Markdown**: a header button on any DM or channel
  copies the whole loaded thread to the clipboard as a clean Markdown
  transcript — messages grouped by day, deleted messages redacted (never their
  content), attachments listed by name.
- **"Back to latest" button**: when you scroll up in a thread, a floating
  button appears at the bottom-right to jump straight back to the most recent
  message (one click), and disappears once you're at the bottom.
- **Full date on hover**: hovering a message timestamp now shows the complete
  date and time as a tooltip.
- **Copy button on code blocks**: fenced code blocks now show a "Copy" button
  on hover that copies the block's exact source to the clipboard.
- **Voice message playback speed**: a button on the voice-message player cycles
  through 1×, 1.5× and 2× playback speed.
- **Project website + French user guide** (GitHub Pages): a download landing
  page and a step-by-step guide (install, peer-to-peer connection,
  troubleshooting), deployed from `website/`.

### Performance

- **Long conversations scroll smoothly**: the message list now renders only
  the visible tail of the thread to the DOM (80 rows, extending by 80 as you
  scroll up, with scroll anchoring) instead of every loaded message. Jump
  targets and the "new messages" divider are always rendered; switching
  conversations resets the window.

### Internal

- The conversation view was split into focused modules (`chat/DmView`,
  `chat/MemberList`, `chat/panels`, `chat/common`) — every source file is now
  under 800 lines. Pure moves, no behavior change.

- **Custom theme editor** (Settings → Appearance): a 25th "Custom" tile —
  pick your chat background, side-panel and accent colors on a dark or light
  base; the rest of the palette (hovers, inputs, rail, tooltip) is derived
  automatically and applied live. The gallery tile previews your colors.
- **Share a theme by code**: export your custom theme to a compact
  `accord-theme:…` code you can paste to a friend, and import theirs.
- **Interface font choice** (Settings → Appearance): System, Rounded or Serif
  — all native system families, nothing downloaded.
- **Scheduled Do Not Disturb** (Settings → Notifications): silence sounds and
  notifications during a chosen time range (spanning midnight supported).

### Fixed

- **"My friend's banner/profile never arrives" (field bug, root-caused)**:
  when two peers re-established contact after a restart, the direct dial and
  the hole-punch volley crossed — both sides opening a handshake at once
  (*simultaneous open*). The peer that the tie-breaker turns into the
  **responder** had its own outgoing handshake dropped, and **every message it
  had queued on that handshake was silently discarded** — typically the
  profile/banner announce it was about to send. The transport now re-seals and
  delivers that queued backlog under the freshly-established session, so no
  message is lost when handshakes cross. Covered by a new deterministic
  transport test. Two safety nets were added on top: the profile announce is
  replayed on the first inbound message of each session episode, and friend
  addresses are persisted from pending-friendship sessions too (the on-connect
  hook alone missed sessions opened before mutual friendship).

### Tests

- New `reconnexion_e2e` binary: a friend reconnects purely from the persisted
  address cache after a restart (no re-registration, no DHT), and a message
  queued while a friend was offline is delivered once they reconnect.
- e2e determinism: friend-sync test binaries disable mDNS (they register peers
  manually) and widen wait windows to tolerate parallel CPU contention.

## [2.3.4] — 2026-07-18

### Fixed

- Server banner no longer overflows the sidebar's rounded top corners: the
  server header now matches the column radius (with the same responsive
  breakpoints), and the banner image and its scrim inherit it.

## [2.3.3] — 2026-07-18

### Added

- **Play large videos in the conversation**: a new setting (Settings → Text &
  media) raises the inline video-player size limit — 8 MiB (default), 50,
  100 or 500 MiB. Up to 8 MiB videos still load automatically; beyond that
  and up to your chosen limit, the video shows a "Play video" card and the
  download only starts when you click it (auto-download stays capped, so a
  peer can never force a huge transfer). Once downloaded, the video is
  **streamed from disk** (asset protocol) — no giant in-memory payload —
  and plays right in the thread.

## [2.3.2] — 2026-07-18

### Fixed

- **Image zoom no longer breaks**: the fullscreen viewer was rendered inside
  the chat panel, and since Liquid Glass an ancestor `backdrop-filter` turns
  into the containing block for `position: fixed` — the overlay came out
  clipped and misplaced. It now renders at the document root (portal).
- **Floating panels readable again on animated themes**: server menu, server
  settings and other glass surfaces were far too translucent when the
  backdrop blur silently fails (WKWebView + animated scene layers) — content
  behind bled through. Glass panels are now near-opaque, with the blur kept
  as a progressive enhancement.

### Changed

- **Bigger emojis in messages**: unicode emojis are rendered ~45% larger
  than the surrounding text, and custom server emojis go from 22 px to 28 px
  (48 px with the "large" setting).

## [2.3.1] — 2026-07-18

### Added

- **Drag and drop into the chat**: drop images, videos or any files anywhere
  on the conversation — a "Drop to send" overlay appears, and the files are
  attached to the message being written. In the desktop app, dropped files
  go through the unbounded disk path (up to 2 GiB), like the attach button.
  The previous composer-only drop zone silently did nothing in the packaged
  app (the webview intercepts OS file drags) — now handled natively.
- **Built-in video player**: video attachments (up to 8 MiB) play directly
  in the thread — download progress, then a standard player, with a Retry
  button if the sender is unreachable. Larger videos keep the downloadable
  file card.
- **Dedicated Updates tab**: the update section moved out of Settings →
  System into its own Settings → Updates tab.

## [2.3.0] — 2026-07-18

### Fixed

- **Friends' avatars and banners now sync reliably** ("I never see their
  banner even though they set one"): profiles are now exchanged **every time
  two friends connect**, instead of relying on best-effort channels that
  could all fail together in the field — a change announcement missed while
  offline, an offline drop published to an unreachable DHT, or reconnection
  windows shorter than the periodic re-announce. One tiny message per
  session; peers only download media whose hash actually changed.
- **Offline drops no longer vanish into an empty DHT**: a mailbox deposit
  that reached zero replicas was still marked as delivered, silently losing
  the message until its 7-day expiry. Zero-replica deposits are now retried
  (direct sending keeps its own schedule).
- **Failed images can be retried**: an image attachment that could not load
  (sender offline at that moment) showed a permanent "image unavailable"
  card. It now has a Retry button — useful right after the sender comes back
  online. End-to-end coverage added for the three field scenarios (profile
  media while both online, set while the friend is offline, and a lost
  announcement recovered on reconnect).

## [2.2.0] — 2026-07-18

### Added

- **Echo cancellation** (on by default, Settings → Voice to toggle): Accord
  now removes what your speakers just played from your microphone — the fix
  for "I hear myself twice" when the person you're talking to uses speakers.
  Pure-Rust acoustic echo canceller: automatic speaker-to-mic delay tracking
  (up to 500 ms), partitioned frequency-domain adaptive filter (160 ms tail),
  double-talk detection so your own voice is never eaten, and a bounded
  residual suppressor.

### Fixed

- **Voices now mix properly in group channels**: with 3+ people, simultaneous
  speakers were queued one after the other instead of being layered — causing
  robotic interleaving, growing latency and dropped frames. Decoded frames
  are now summed into a single output frame per tick with a soft limiter.
- **Less crackling**: hard clipping (the main "crackle" source) replaced by a
  soft-knee limiter on the auto-gain and the output mix, and audio-output
  starvation now ramps to silence and back (2 ms fades) instead of jumping —
  no more clicks at buffer boundaries.
- Composer bar sat too close to the bottom edge since Liquid Glass — raised
  by 10 px, aligned with the user panel (DM and server channels alike).

## [2.1.2] — 2026-07-18

### Added

- **Animated figurative themes**: 5 illustrated scene themes with original
  artwork and subtle ambient animation — Sakura garden, Wisteria night, Lotus
  pond, Manga ink and Shōjo bloom — bringing the gallery to 24 themes.
- **More profile cosmetics**: 6 new animated decorations, 6 effects and
  5 frames in the nature & manga collection.

## [2.1.1] — 2026-07-18

### Fixed

- **DM images displaying reliably again** (macOS packaged app): the thumbnail
  pipeline introduced with image previews could fail silently in WKWebView
  (WebP canvas encoding) and leave images stuck loading or "unavailable".
  Thumbnails are now non-blocking with a systematic fallback to the proven
  full-resolution path: an invalid thumbnail, a canvas decode that never
  finishes (4 s timeout) or a render failure all fall back to the full-size
  image before ever declaring it unavailable. End-to-end transfer covered by
  two new Rust tests (full UI flow, and relayed transfer behind symmetric
  NAT).

## [2.1.0] — 2026-07-18

### Changed

- **Liquid Glass interface**: floating surfaces — modals, pinned-message and
  thread panels, the soundboard, the avatar cropper, server settings and the
  member drawer — now use a translucent frosted-glass treatment with subtle
  rim light and depth. It reads each theme's own color tokens, so it adapts to
  all 20 themes (light and dark) with no hardcoded colors. The logo and visual
  identity are unchanged.
- Respects system preferences: falls back to opaque surfaces where
  `backdrop-filter` is unsupported, and honors reduced-transparency,
  increased-contrast, forced-colors (Windows high contrast) and
  reduced-motion.

## [2.0.0] — 2026-07-18

### Added

- **Built-in updates**: Accord now updates itself — no more downloading each
  release by hand. The app checks GitHub for new versions at startup and every
  four hours, shows a banner when one is available, and installs it in one
  click (restart included). A new **Updates** section in Settings → System
  shows the installed version and lets you check manually at any time.
- **Signed updates**: every update artifact is signed (minisign) and the app
  verifies the signature against its embedded public key before installing —
  a tampered or corrupted download is rejected.

### Notes

- Installs older than 2.0.0 do not include the updater: this release must be
  installed manually once. From 2.0.0 onward, updates arrive in-app.
- On Linux, in-app updating applies to the AppImage; `.deb`/`.rpm` installs
  still update through the package files on the releases page.

## [1.9.0] — 2026-07-18

### Added

- **Restore from the first screen**: onboarding now opens on a welcome step
  with explicit choices — create an account, restore from a recovery phrase,
  or **import an encrypted `.accordbackup`** — instead of burying restoration
  behind the create form.

### Changed

- **New visual identity**: refreshed Accord logo rolled out everywhere — app
  icons on all three platforms (dock/taskbar/tray), onboarding, README and
  the GitHub screenshots (regenerated, ~8× lighter).
- **Onboarding redesign**: dedicated visual language for the first-launch
  screens (dark hero surface, staged panels, reduced-motion aware) replacing
  the bare centered card.
- Shell polish: aligned spacing across the server rail, sidebar and chat
  header (identity-refresh pass).

## [1.8.0] — 2026-07-18

### Changed

- **Faster message history**: history pages now fetch reactions, attachments and
  mentions in batched `IN (…)` queries (3 queries per page instead of 3 per
  message), with cached prepared statements — large threads and busy channels
  load noticeably quicker.
- **Snappier local database**: SQLCipher tuned for a desktop client —
  `synchronous = NORMAL` under WAL (safe, no corruption), a 16 MiB page cache
  and in-memory temp store — so sending a message no longer waits on a full
  fsync.
- **No more full outbox scan**: a new `outbox(dest, created_ms)` index (schema
  v10, idempotent migration) makes opening a conversation O(matches) instead of
  scanning the whole pending-message table on every reconnect.
- **Lower first-contact latency**: the Kademlia lookup now fans out its α=3
  probe batch concurrently — a round costs the slowest peer, not the sum — so a
  dead peer no longer serializes the others.
- **More resilient voice**: Opus now uses in-band FEC + DTX; a lost packet is
  reconstructed from the next one (forward error correction) instead of being
  masked, improving call quality on lossy links.

## [1.7.0] — 2026-07-18

### Added

- **Right-click, everywhere**: context menus now cover the private-conversation
  list, the Friends rows, the DM header and your own user panel — profile,
  message, call, mark as read, accept/decline a request, copy friend code,
  remove, block, and presence status — mirroring the actions already wired
  elsewhere, Discord-style.
- **Command palette (Ctrl/Cmd+K)**: run actions alongside navigation — open
  settings, create a server, add a friend, change presence status.
- **Unread badge on the dock / taskbar icon**: reflects unread DMs and server
  mentions; clears when nothing is pending.
- **Recovery-phrase safeguard**: the one-time recovery screen now has a Copy
  button and requires retyping a challenge word before confirming — a
  distracted click can no longer discard the only way to recover the account.
- Active empty-Friends state with an "Add a friend" call-to-action on first
  launch.

### Changed

- **Reconnection is legible**: a distinct amber "reconnecting automatically"
  banner with a Retry button (forces an immediate reconnect), separate from the
  red "offline" state.
- **Faster chat rendering**: markdown parsing is memoized and unchanged messages
  keep their identity across refreshes, so a new message no longer re-parses the
  whole thread.
- **File serving off the network loop**: manifests and blocks are served via a
  blocking pool (`spawn_blocking`) instead of stalling the P2P event loop.
- **Smaller initial load**: the bundle is split (React and `qrcode` isolated)
  and the friend QR code is lazy-loaded out of the initial payload.

### Internal

- CI gate runs unit tests only (`--lib`); the networked e2e suite stays in local
  `ci.sh`. Fixed a TCP hole-punch test that hung the Linux runner, and stopped
  `cargo deny` from failing on transitive unmaintained/yanked crates (real
  vulnerabilities still block).

## [1.6.0] — 2026-07-17

### Added

- **Shareable friend link + QR code**: copy `accord://friend/<code>` or show
  a scannable QR from "My friend code"; the add-friend field accepts pasted
  links. First contact is now one click instead of dictating a code.
- **Full encrypted backup**: export the active profile (sealed vault,
  SQLCipher database, files) as a single `.accordbackup` archive from
  Settings → Account, and import it as a new account from the account
  picker. Losing a disk no longer means losing your history.
- **Personalization, third wave**: live real-card preview in Settings
  (layer-for-layer identical to the actual profile card), 6 new animated
  avatar decorations, 6 new profile effects and 4 new frames — all
  compositor-only and reduced-motion aware.
- Explicit P2P delivery states in DMs: a pending message to an offline
  friend now says it was dropped in their encrypted mailbox (with the 7-day
  window), plus a discreet banner at the top of the conversation.
- Real image thumbnails: attachments render a downscaled preview (full
  resolution only in the lightbox) and file caches are bounded (LRU) — a
  photo-heavy channel no longer accumulates hundreds of MB of memory.

### Fixed

- **Delivery reliability overhaul.** The DHT republish loop is now actually
  wired (records — friend codes, mailboxes — previously died with the first
  nodes to go offline); identity records live 7 days instead of 1 hour, so
  a friend code resolves even when its owner's laptop is closed; offline
  mailboxes honor the promised 7-day window (was ~2 days, with a poll that
  missed most of it) and are re-deposited daily; group messages now carry
  application acks — a single lost UDP packet no longer leaves a permanent,
  per-member hole in channel history.
- Group takeover hardening: new groups derive their `group_id` from their
  founding CREATE op — a concurrent rogue CREATE can no longer steal the
  fold, even if it arrives first (THREAT-MODEL §6 closed for new groups).
- A theme scene layer could paint over the sidebar (offset band); fixed
  with `overflow: clip`.

### Changed

- Group state is now cached in memory (invalidated on each new op) instead
  of re-folding the whole op-log from SQLite on every message — the
  per-message cost no longer grows with server history.
- CI: a GitHub validation workflow (fmt, clippy, full Rust + frontend suite,
  cargo-deny/audit) now runs on every push and pull request and gates
  releases; a nightly `cargo-fuzz` job fuzzes the wire decoders.

## [1.5.0] — 2026-07-16

### Added

- **Profile frames are now their own personalization slot**, separate from
  profile effects: pick an animated frame and an animated background
  independently (both id-based, a few bytes over the wire, compositor-only
  and reduced-motion aware). Four frames to start: Lumen Garden, Crystal
  Crown, Celestial Wings, Neon Circuit.

### Fixed

- Your frame now also shows on the profile opened from the **user panel in
  the bottom-left corner** — it previously appeared only on server members'
  cards, because that panel opens a different surface which never rendered
  the frame (and clipped its bleed).

## [1.4.0] — 2026-07-16

### Added

- **Animated profile card frame**: profile popovers and the user panel share
  a decorated, animated card surface (id-based like the rest of the
  personalization catalogue, compositor-only, reduced-motion aware).

### Fixed

- **macOS microphone prompt looping even after accepting.** The bundle was
  only linker-signed (Info.plist not bound, no sealed resources), so TCC
  could not durably attribute the grant. Builds are now fully signed (ad-hoc
  by default, stable identity when available) and the Permissions row reads
  the real system state via AVFoundation — it only offers "Request" when the
  OS can actually show its prompt, so the app can never re-trigger it in a
  loop; when denied it deep-links to the exact system pane.
- A batch of design bugfixes: broken avatar/attachment images fall back
  cleanly, the typing indicator no longer shifts the composer (and shows
  server nicknames, threads included), threads/pinned panels no longer
  overlap, the members list adapts to narrow layouts, the server menu got
  full keyboard support and clearer checkboxes, composer focus and the
  attach menu were polished, jump-to-message honors reduced motion, and
  touch targets were enlarged across the app.
- **Menus: keyboard acts on the focused item, never the hovered one.**
  Resting the pointer over one row while arrow-navigating another could make
  Enter activate the hovered row (e.g. a destructive "Delete message") and
  left the new ArrowRight-to-submenu shortcut dead; both the context menu
  and the server menu now derive every key action from the real focus.

## [1.3.1] — 2026-07-16

### Fixed

- **Repeated permission prompts on macOS.** Root cause: with the default
  ad-hoc signature, macOS ties the microphone grant and the firewall's
  "accept incoming connections" grant (vital for P2P) to a fingerprint that
  changes at every build — so the prompts kept coming back.
  `build-macos.sh` now signs with a stable local identity when available
  (`Accord Dev` certificate or `ACCORD_SIGNING_IDENTITY`; see
  DISTRIBUTION.md), and the Settings → System → Permissions section was
  rebuilt: one row per permission (notifications, microphone, incoming
  connections/firewall) with separate requests — no more two stacked
  system dialogs from a single button — and a direct "Open settings"
  shortcut to the right system pane, the only way back after a denial.

## [1.3.0] — 2026-07-16

### Security

- **Group op-log integrity (audit A): content-addressed op ids.** A group
  operation's `op_id` is now the truncated SHA-256 of its content, and any op
  whose id doesn't match is rejected at ingest. Previously ids were random, so
  a malicious member could sign two different valid ops sharing one id —
  peers folding different ones diverged permanently while anti-entropy saw
  identical digests. Groups created before 1.3.0 are grandfathered: they keep
  working exactly as before (joins, backup restores and sync catch-up intact),
  and keep the historical weakness — recreate a server to get the new
  guarantee. Writers in a group created under 1.3.0+ must be up to date: a
  pre-1.3.0 client's writes there are silently rejected by updated peers and
  keep the logs re-syncing periodically until it upgrades (see
  docs/THREAT-MODEL.md §6). Anti-entropy now re-pulls a group's log from
  zero whenever the local copy lacks its CREATE op, so a lost initial push
  can no longer strand a fresh joiner.

## [1.2.8] — 2026-07-15

### Added

- A much bigger personalization catalogue: **20 themes, 14 avatar decorations and
  12 animated profile effects** — all still id-based (a few bytes over the wire),
  compositor-only and reduced-motion aware.

### Changed

- Unified personal profile card, more secure account switching, removed a
  redundant identifier, slimmer server banner, and a general responsive-polish
  pass across the interface.

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
