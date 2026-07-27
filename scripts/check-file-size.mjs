/**
 * Fails when a file crosses the 800-line rule, or when a file that already
 * crossed it grows any further.
 *
 * ROADMAP §1.3 lists oversized files as debt D3. Measured on 2026-07-25, then
 * again on 2026-07-27: `api.ts` had gone from 2453 to 2738 lines and `ui.ts`
 * from 1051 to 1229. The entry had been written twice and the files had grown
 * both times — a debt nobody is measuring is a debt that grows, and writing it
 * down a third time would change nothing.
 *
 * So this is a ratchet rather than a wish. The current offenders are listed
 * below with the size they had the day the ratchet was installed; they may
 * shrink, never grow. Any file not on the list must stay under 800 lines.
 *
 * Deliberately not a formatter's job and not a lint: the point is the
 * trajectory, and no linter knows what a file weighed last week.
 *
 * Run from the repository root.
 */

import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';

/** The rule, from ROADMAP §1.3 and the coding-style rules. */
const LIMITE = 800;

/**
 * Files already over the limit the day the ratchet was installed, with their
 * size that day. They are allowed to stay over — they may not get worse.
 *
 * 🔒 **Only ever remove lines from this table, never add them.** Adding an
 * entry converts a rule into a habit. Lowering a number as a file shrinks is
 * the intended direction; a file that drops under 800 leaves the table.
 */
const DETTE = new Map([
  ["crates/accord-core/src/group/state.rs", 5222],
  ["crates/accord-proto/src/core_msg.rs", 3918],
  ["crates/accord-node/src/runtime.rs", 3245],
  ["crates/accord-node/src/voice/engine.rs", 2937],
  ["crates/accord-node/src/node/mod.rs", 2853],
  ["crates/accord-node/src/node/groups.rs", 2812],
  ["app/src/lib/api.ts", 2738],
  ["crates/accord-transport/src/endpoint.rs", 2457],
  ["crates/accord-core/src/group/msg.rs", 2356],
  ["crates/accord-node/src/maintenance.rs", 1524],
  ["app/src/stores/groups.ts", 1479],
  ["crates/accord-core/src/group/invite.rs", 1470],
  ["crates/accord-core/src/db/messages.rs", 1356],
  ["crates/accord-core/src/db/mod.rs", 1352],
  ["crates/accord-node/src/voice/calls.rs", 1324],
  ["crates/accord-core/src/profile.rs", 1271],
  ["app/src/components/MessageInput.tsx", 1262],
  ["crates/accord-node/src/device.rs", 1256],
  ["app/src/stores/ui.ts", 1229],
  ["crates/accord-node/src/node/dm.rs", 1181],
  ["crates/accord-node/src/service/groups.rs", 1129],
  ["app/src/components/Modals.tsx", 1087],
  ["crates/accord-core/src/group/mod.rs", 1009],
  ["app/src/components/Sidebar.tsx", 971],
  ["crates/accord-node/src/backup.rs", 966],
  ["crates/accord-core/src/messaging.rs", 950],
  ["app/src/components/MessageList.tsx", 921],
  ["crates/accord-node/src/lib.rs", 897],
  ["crates/accord-node/src/node/profile.rs", 833],
  ["crates/accord-core/src/friends.rs", 803],
]);

/** Directories worth walking, and the ones never worth walking into. */
const RACINES = ['app/src', 'crates'];
const IGNORES = new Set(['node_modules', 'target', 'dist', '.git']);

/** Source files, recursively, excluding tests. */
function fichiers(racine) {
  const out = [];
  const pile = [racine];
  while (pile.length > 0) {
    const dir = pile.pop();
    let entrees;
    try {
      entrees = readdirSync(dir, { withFileTypes: true });
    } catch {
      continue;
    }
    for (const e of entrees) {
      const chemin = join(dir, e.name);
      if (e.isDirectory()) {
        // `crates/*/tests/` : les tests d'intégration sont des tests, au même
        // titre que `tests.rs` et les `*.test.ts`.
        if (!IGNORES.has(e.name) && e.name !== 'tests') pile.push(chemin);
        continue;
      }
      // Les dictionnaires de traduction sont exemptés : mille cent lignes de
      // chaînes traduites sont une DONNÉE longue, pas une complexité à
      // découper. Les scinder n'aiderait personne à les relire, et le test de
      // parité les couvre déjà en entier.
      if (/^app\/src\/i18n\//.test(relative('.', chemin))) continue;
      // Les tests sont exemptés : un fichier de tests long est une couverture
      // longue, pas une complexité à découper.
      if (/\.(test|spec)\.[jt]sx?$/.test(e.name)) continue;
      if (e.name === 'tests.rs') continue;
      if (/\.(ts|tsx|rs)$/.test(e.name)) out.push(chemin);
    }
  }
  return out;
}

const problemes = [];
for (const racine of RACINES) {
  let existe = true;
  try {
    statSync(racine);
  } catch {
    existe = false;
  }
  if (!existe) continue;

  for (const chemin of fichiers(racine)) {
    const rel = relative('.', chemin);
    // Même convention que `wc -l` : on compte les fins de ligne, pas les
    // éléments d'un `split` (qui en rend un de plus sur un fichier terminé par
    // un saut de ligne — et toute la table serait décalée d'une unité).
    const texte = readFileSync(chemin, 'utf8');
    const lignes = texte.length === 0 ? 0 : texte.replace(/\n$/, '').split('\n').length;
    const plafond = DETTE.get(rel);
    if (plafond === undefined) {
      if (lignes > LIMITE) {
        problemes.push(`${rel}: ${lignes} lignes, au-dessus de ${LIMITE}`);
      }
    } else if (lignes > plafond) {
      problemes.push(
        `${rel}: ${lignes} lignes, contre ${plafond} au moment du cliquet — un fichier de la dette peut maigrir, pas grossir`,
      );
    }
  }
}

if (problemes.length > 0) {
  console.error('file-size: la limite de 800 lignes est franchie\n');
  for (const p of problemes) console.error(`  ${p}`);
  console.error(
    '\nDécouper le fichier plutôt que de relever le plafond : la table de dette de\n' +
      'scripts/check-file-size.mjs ne doit recevoir aucune entrée nouvelle.',
  );
  process.exit(1);
}

console.log(`file-size: aucune infraction (${DETTE.size} fichiers de dette sous surveillance)`);
