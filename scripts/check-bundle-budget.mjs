/**
 * Fails when the initial JavaScript chunk exceeds its budget.
 *
 * The budget guards a trajectory, not a number: every feature adds to the
 * entry chunk unless it is deliberately split, and nothing in the build output
 * says so. Without this check the drift is invisible until startup is slow —
 * at which point the cause is spread over dozens of commits.
 *
 * Run after `npm run build`, from the repository root or from `app/`.
 */

import { gzipSync } from 'node:zlib';
import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';

/**
 * Gzipped ceiling for the entry chunk, in bytes.
 *
 * Raised from 140 kB on 2026-07-25, and the reason is written down rather
 * than quietly absorbed. Two things forced it:
 *
 * 1. **The French dictionary is the only eager one.** Every other language is
 *    its own lazily-loaded chunk, so ten languages cost nothing — but every
 *    French string added anywhere lands in the initial download. Multi-device
 *    added a screen's worth of them.
 * 2. **The measurement is not identical across platforms.** The same commit
 *    measured 139.7 kB on macOS and 140.0 kB on the CI runner. A ceiling with
 *    0.3 kB of headroom fails on the difference between two gzip builds, not
 *    on anything a developer did.
 *
 * ⚠️ The real fix is structural and is *not* done: the settings strings —
 *    `settings`, `decorations.labels` — are only ever read inside the settings
 *    modal, which is already a lazy chunk. Splitting them out of the eager
 *    French dictionary would take the initial download back down and make the
 *    ceiling meaningful again. Recorded in REPRISE.md.
 *
 * Raising a ratchet is not free. It is allowed here because the growth is real
 * work, the cause is understood, and the fix is named — not because the number
 * was in the way.
 */
const ENTRY_BUDGET = 150 * 1024;

const assets = join(process.cwd().endsWith('/app') ? '.' : 'app', 'dist', 'assets');

let files;
try {
  files = readdirSync(assets);
} catch {
  console.error(`bundle-budget: ${assets} not found — run "npm run build" first`);
  process.exit(1);
}

// Vite names the entry chunk after the HTML entry point: index-<hash>.js.
const entry = files.find((name) => /^index-.*\.js$/.test(name));
if (entry === undefined) {
  console.error('bundle-budget: no index-*.js entry chunk in the build output');
  process.exit(1);
}

const gzipped = gzipSync(readFileSync(join(assets, entry))).length;
const kb = (bytes) => `${(bytes / 1024).toFixed(1)} kB`;

if (gzipped > ENTRY_BUDGET) {
  console.error(
    `bundle-budget: entry chunk is ${kb(gzipped)} gzipped, over the ${kb(ENTRY_BUDGET)} budget.\n` +
      'Split something out with React.lazy() rather than raising the budget: a screen\n' +
      'most sessions never open should not be in the first load.',
  );
  process.exit(1);
}

console.log(`bundle-budget: entry chunk ${kb(gzipped)} gzipped (budget ${kb(ENTRY_BUDGET)})`);
