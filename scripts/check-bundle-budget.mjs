/**
 * Fails when the initial JavaScript or CSS chunk exceeds its budget.
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

/**
 * Gzipped ceiling for the entry stylesheet, in bytes.
 *
 * ROADMAP §10.2 has carried a 50 kB CSS budget since the beginning with nothing
 * enforcing it. Measured on 2026-07-27: 33.2 kB, against 34.5 kB when the budget
 * was written — so no drift, and this check is a guard rail rather than a fix.
 *
 * It measures the entry stylesheet ALONE, not the sum of every .css file. Nine
 * of the ten stylesheets in the build are lazy chunks — decorations, animated
 * profiles, the Appearance tab — that a session may never load. Summing them
 * would make the number grow every time a stylesheet is correctly split out,
 * which is to say it would penalise exactly the thing the budget exists to
 * encourage. (The sum is 48.9 kB today; read as a total it looks like a
 * near-breach, and it means nothing.)
 */
const CSS_BUDGET = 50 * 1024;

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

// The entry stylesheet is the one Vite links from index.html; the rest are lazy.
const css = files.find((name) => /^index-.*\.css$/.test(name));
if (css === undefined) {
  console.error('bundle-budget: no index-*.css entry stylesheet in the build output');
  process.exit(1);
}

const cssGzipped = gzipSync(readFileSync(join(assets, css))).length;

if (cssGzipped > CSS_BUDGET) {
  console.error(
    `bundle-budget: entry stylesheet is ${kb(cssGzipped)} gzipped, over the ${kb(CSS_BUDGET)} budget.\n` +
      'Move the offending styles into the lazy chunk of the screen that needs them,\n' +
      'the way decorations and animated profiles already are.',
  );
  process.exit(1);
}

console.log(
  `bundle-budget: entry stylesheet ${kb(cssGzipped)} gzipped (budget ${kb(CSS_BUDGET)})`,
);
