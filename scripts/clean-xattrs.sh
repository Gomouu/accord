#!/usr/bin/env bash
# Removes macOS extended attributes from the working tree.
#
# Symptom this fixes: `cargo tauri build` (or any `cargo build` touching
# `app/src-tauri`) fails with "Operation not permitted" while COPYING a source
# file it has already read successfully. macOS attaches `com.apple.macl` to
# files opened through certain sandboxed paths; that attribute survives normal
# permission changes and makes `copy_file` fail even though read and write
# both work.
#
# Usage: scripts/clean-xattrs.sh [path]   (defaults to the repository root)
set -euo pipefail

target="${1:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "clean-xattrs: nothing to do (extended attributes are a macOS concern)"
  exit 0
fi

if ! command -v xattr >/dev/null 2>&1; then
  echo "clean-xattrs: 'xattr' is not available on this system" >&2
  exit 1
fi

echo "clean-xattrs: clearing extended attributes under $target"
# `xattr -c` cannot remove com.apple.macl — the kernel reapplies it. Report
# honestly rather than claiming success: this script clears what it can, and
# tells you when the blocking attribute survives.
find "$target" \
  -path '*/node_modules' -prune -o \
  -path '*/target' -prune -o \
  -path '*/.git' -prune -o \
  -type f -print0 |
  xargs -0 xattr -c 2>/dev/null || true

remaining=$(find "$target" \
  -path '*/node_modules' -prune -o \
  -path '*/target' -prune -o \
  -path '*/.git' -prune -o \
  -type f -print0 |
  xargs -0 xattr 2>/dev/null | grep -c 'com.apple.macl' || true)

if [[ "$remaining" -gt 0 ]]; then
  cat >&2 <<'MSG'

clean-xattrs: com.apple.macl is still present on some files.

This attribute is owned by the kernel and cannot be cleared from userspace —
macOS reapplies it immediately. It is NOT a repository problem and NOT a code
problem: the same sources build fine in CI.

Remedy, in order of preference:
  1. Restart the machine. This clears the attribute in practice.
  2. Grant your terminal Full Disk Access
     (System Settings > Privacy & Security > Full Disk Access), then restart
     the terminal.
  3. Build through CI (the GitHub workflow mirrors ./ci.sh exactly).
MSG
  exit 2
fi

echo "clean-xattrs: done, no blocking attribute left"
