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
 * Back to 140 kB on 2026-07-25, after the structural fix the previous 150 kB
 * ceiling was standing in for. The French dictionary is still the only eager
 * one — the nine others are lazy chunks — but it is now split in two: the core
 * (`app/src/i18n/fr.ts`) and the settings vocabulary (`fr.settings.ts`), which
 * only ever loads when the settings modal opens. That took the entry chunk from
 * 139.8 kB to 134.1 kB, and `SettingsDict` being a type distinct from `Dict`
 * means the shell cannot pull those strings back in without failing to compile.
 *
 * The 6 kB of headroom is deliberate, and it is what the 150 kB ceiling was
 * really buying: the same commit measured 139.7 kB on macOS and 140.0 kB on the
 * CI runner, so a ceiling that sits a few hundred bytes above the measurement
 * fails on the difference between two gzip builds rather than on anything a
 * developer did.
 */
const ENTRY_BUDGET = 140 * 1024;

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
