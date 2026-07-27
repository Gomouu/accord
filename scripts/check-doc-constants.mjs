#!/usr/bin/env node
/**
 * Vérifie que les constantes promises aux clients de l'API figurent dans les
 * docs avec leur valeur courante.
 *
 * **Pourquoi.** `docs/API.md` a annoncé « 1 MiB » pour la taille maximale d'un
 * message WebSocket alors que le code dit 16 MiB — et pas par dérive : la
 * valeur n'a jamais changé depuis le premier commit, la doc était fausse dès
 * l'origine. Un auteur de client tiers y lisait une contrainte inexistante.
 *
 * ⚠️ **Portée volontairement étroite.** Ne surveille que les constantes
 * listées ci-dessous, celles dont la valeur est une promesse publique. Élargir
 * à toutes les constantes du code produirait du bruit — la plupart sont
 * internes et n'ont rien à faire dans une doc. Un contrôle bruyant finit
 * ignoré, ce qui est pire que pas de contrôle.
 *
 * Ne vérifie qu'une chose : la valeur courante est CITÉE. Il ne sait pas dire
 * si elle est citée au bon endroit ni si la phrase autour est juste ; c'est un
 * garde-fou contre la dérive silencieuse, pas une relecture.
 */

import { readFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const racine = join(dirname(fileURLToPath(import.meta.url)), '..');

/** Constante → fichiers qui doivent en citer la valeur. */
const SURVEILLEES = [
  {
    nom: 'MAX_WS_MESSAGE',
    source: 'crates/accord-api/src/server.rs',
    docs: ['docs/API.md', 'docs/API_CONTRACT.md'],
  },
  {
    nom: 'MAX_BODY',
    source: 'crates/accord-proto/src/core_msg.rs',
    docs: ['docs/API.md', 'docs/API_CONTRACT.md'],
  },
  {
    nom: 'MAX_LIST',
    source: 'crates/accord-proto/src/limits.rs',
    docs: ['docs/API.md'],
  },
  {
    nom: 'MAX_ATTACHMENTS',
    source: 'crates/accord-proto/src/core_msg.rs',
    docs: ['docs/API.md'],
  },
  {
    nom: 'MAX_CONNECTIONS',
    source: 'crates/accord-api/src/server.rs',
    docs: ['docs/API.md'],
  },
];

/** Valeur d'une constante Rust, expressions arithmétiques simples résolues. */
function valeurDe(source, nom) {
  const texte = readFileSync(join(racine, source), 'utf8');
  const m = new RegExp(`const ${nom}: [a-z0-9]+ = ([^;]+);`).exec(texte);
  if (m === null) return null;
  const expr = m[1].trim().replaceAll('_', '');
  if (!/^[0-9 *+]+$/.test(expr)) return null;
  // Somme de produits : suffisant pour `16 * 1024 * 1024`, et sans `eval`.
  return expr
    .split('+')
    .reduce((t, p) => t + p.split('*').reduce((a, b) => a * Number(b.trim()), 1), 0);
}

/** Écritures sous lesquelles une doc peut légitimement citer `n`. */
function ecritures(n) {
  const formes = [String(n)];
  if (n % 1024 === 0) formes.push(`${n / 1024} KiB`);
  if (n % (1024 * 1024) === 0) formes.push(`${n / (1024 * 1024)} MiB`);
  return formes;
}

const fautes = [];
for (const { nom, source, docs } of SURVEILLEES) {
  const n = valeurDe(source, nom);
  if (n === null) {
    fautes.push(`${nom} : introuvable dans ${source} (renommée ou déplacée ?)`);
    continue;
  }
  const formes = ecritures(n);
  for (const doc of docs) {
    const texte = readFileSync(join(racine, doc), 'utf8');
    if (!formes.some((f) => texte.includes(f))) {
      fautes.push(`${doc} ne cite pas ${nom} = ${formes.at(-1)} (${n})`);
    }
  }
}

if (fautes.length > 0) {
  console.error('doc-constants: une valeur promise aux clients n’est plus à jour\n');
  for (const f of fautes) console.error(`  ${f}`);
  console.error(
    '\nCorriger la doc plutôt que retirer la constante de la liste : ce qui est\n' +
      'écrit dans docs/API*.md est ce sur quoi un client tiers construit.',
  );
  process.exit(1);
}
console.log(`doc-constants: ${SURVEILLEES.length} valeurs publiques à jour dans les docs`);
