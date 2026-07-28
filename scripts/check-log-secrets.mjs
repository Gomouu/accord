#!/usr/bin/env node
/**
 * Vérifie qu'aucun appel `tracing::` n'interpole une valeur dont le NOM annonce
 * un secret.
 *
 * **Pourquoi.** `SECURITY.md` §3.4 et §5 promettent que « les secrets ne sont
 * jamais journalisés » — jeton d'API, phrases de passe, clés, contenus de
 * message — et précisent que ce qui tient cette promesse est « une règle sur
 * les appels `tracing` de tout le dépôt ». Une règle, c'est-à-dire rien
 * d'exécutable : la revue d'audit a constaté qu'aucun lint, aucun test, aucun
 * scanner ne la faisait respecter, sur 201 appels répartis dans 26 fichiers.
 * Le journal est écrit en clair dans le dossier de données, hors de la base
 * chiffrée, et il est conçu pour être envoyé à quelqu'un.
 *
 * ⚠️ **Ce que ce contrôle ne fait PAS**, et il faut le lire avant de s'y fier :
 * il juge des NOMS, pas des valeurs. Un secret passé sous un nom anodin
 * (`valeur`, `x`, `data`) échappe complètement. Il ne remplace donc pas la
 * relecture ; il attrape la faute mécanique — celle qu'on commet à trois heures
 * du matin en ajoutant un champ à un message de debug existant.
 *
 * La liste est volontairement COURTE et sans ambiguïté. `key` seul y manque
 * exprès : `key_epoch`, `keyboard`, `keyword` le rendraient bruyant, et un
 * contrôle bruyant finit ignoré — ce qui est pire que pas de contrôle.
 */

import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join, dirname, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const racine = join(dirname(fileURLToPath(import.meta.url)), '..');
const SOURCES = ['crates', 'app/src-tauri/src'];

/**
 * Fragments de nom qui annoncent un secret sans ambiguïté. Comparés en
 * minuscules sur l'identifiant interpolé.
 */
const INTERDITS = [
  'passphrase',
  'password',
  'secret',
  'seed',
  'token',
  'mnemonic',
  'privkey',
  'private_key',
  'plaintext',
  'sealed',
  'friend_code',
  'msg_body',
  'body_clear',
];

/** Chaque fichier `.rs` sous les racines de source. */
function fichiersRust(dossier) {
  const out = [];
  for (const entree of readdirSync(dossier)) {
    const chemin = join(dossier, entree);
    if (statSync(chemin).isDirectory()) {
      if (entree === 'target' || entree === 'node_modules') continue;
      out.push(...fichiersRust(chemin));
    } else if (entree.endsWith('.rs') && entree !== 'tests.rs') {
      // `tests.rs` : fichier de tests séparé, exclu par son nom puisque la
      // troncature en ligne ne peut pas le couvrir.
      out.push(chemin);
    }
  }
  return out;
}

/**
 * Texte de chaque invocation `tracing::…!(…)`, parenthèses équilibrées.
 *
 * Un simple `indexOf(')')` couperait au premier appel imbriqué
 * (`tracing::debug!("{}", f(x))`), ce qui masquerait la fin de l'appel — donc
 * les arguments les plus susceptibles de porter la fuite.
 */
function invocations(source) {
  const trouvees = [];
  const motif = /tracing::(?:trace|debug|info|warn|error)!\s*\(/g;
  let m;
  while ((m = motif.exec(source)) !== null) {
    let profondeur = 1;
    let i = motif.lastIndex;
    while (i < source.length && profondeur > 0) {
      if (source[i] === '(') profondeur += 1;
      else if (source[i] === ')') profondeur -= 1;
      i += 1;
    }
    const debut = source.slice(0, m.index).split('\n').length;
    trouvees.push({ ligne: debut, texte: source.slice(m.index, i) });
  }
  return trouvees;
}

const fautes = [];
for (const base of SOURCES) {
  for (const chemin of fichiersRust(join(racine, base))) {
    // Les tests ont le droit de manipuler des secrets en clair : ils n'écrivent
    // pas dans le journal d'un utilisateur.
    //
    // ⚠️ La troncature ne vaut que pour un module de tests EN LIGNE. La première
    // version coupait au premier `#[cfg(test)]` rencontré — or il précède
    // souvent `mod tests;`, la déclaration d'un fichier de tests SÉPARÉ, et
    // elle apparaît en tête. Résultat : le fichier était tronqué à sa ligne 112
    // sur 2500, le contrôle ne lisait presque rien et annonçait « vert ».
    // Attrapé en injectant une vraie fuite, qu'il n'a pas vue.
    const source = readFileSync(chemin, 'utf8');
    const enLigne = /^#\[cfg\(test\)\]\s*\n\s*mod tests\s*\{/m.exec(source);
    const code = enLigne === null ? source : source.slice(0, enLigne.index);

    for (const { ligne, texte } of invocations(code)) {
      for (const interdit of INTERDITS) {
        // Cherché sur les identifiants, pas sur le texte littéral : un message
        // qui PARLE d'un jeton (« jeton invalide ») est légitime ; c'en est
        // même le bon usage.
        const sansChaines = texte.replace(/"(?:[^"\\]|\\.)*"/g, '""');
        if (sansChaines.toLowerCase().includes(interdit)) {
          fautes.push(`${relative(racine, chemin)}:${ligne} — interpole « ${interdit} »`);
          break;
        }
      }
    }
  }
}

if (fautes.length > 0) {
  console.error('log-secrets: un appel tracing interpole une valeur nommée comme un secret\n');
  for (const f of fautes) console.error(`  ${f}`);
  console.error(
    "\nLe journal est écrit en clair hors de la base chiffrée, et il est fait pour\n" +
      "être envoyé. Journaliser un compteur ou un état, jamais la valeur.",
  );
  process.exit(1);
}
console.log('log-secrets: aucun appel tracing n’interpole de valeur nommée comme un secret');
