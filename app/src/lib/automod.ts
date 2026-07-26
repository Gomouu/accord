/**
 * AutoMod côté rendu (modèle serverless) : les clients honnêtes masquent les
 * mots filtrés du groupe à l'affichage — rien n'est supprimé du réseau. La
 * correspondance est insensible à la casse ET aux accents, et par mot entier
 * (frontières Unicode : lettres, chiffres et `_` sont des caractères de mot),
 * donc un mot filtré au milieu d'un autre mot n'est pas masqué.
 *
 * 🔗 JUMEAU de `crates/accord-core/src/automod.rs`, qui applique la MÊME règle
 * pour retrancher les messages masqués du compteur de non-lus. Les deux
 * doivent rester d'accord : toute évolution ici se reporte là-bas, et les deux
 * suites de tests partagent volontairement les mêmes cas (« concert » contre
 * « con », accents précomposés ou décomposés).
 */

/** Caractère de masquage affiché à la place d'un mot filtré. */
const MASK_CHAR = '█';

/** Longueur minimale du masque (ne pas révéler les mots très courts). */
const MASK_MIN = 3;

/** Longueur maximale du masque (ne pas révéler les mots très longs). */
const MASK_MAX = 8;

/**
 * Nombre maximal de mots filtrés par serveur.
 *
 * 🔒 C'est la borne du NŒUD (`MAX_AUTOMOD_WORDS`, `accord-proto`), pas une
 * borne d'interface : le format filaire refuse une liste plus longue, et
 * `groups.automod.set` la rejette avant d'émettre l'op. L'écran de réglage
 * affichait 100 alors que le nœud en acceptait 50 — au-delà du cinquantième
 * mot, « Enregistrer » échouait sur une erreur que rien n'annonçait.
 */
export const MAX_AUTOMOD_WORDS = 50;

/** Longueur maximale d'un mot filtré, en caractères (borne du nœud). */
export const MAX_AUTOMOD_WORD_CHARS = 32;

/**
 * Classe des caractères « de mot » pour les frontières Unicode.
 *
 * `Alphabetic` (et non `L`) parce que c'est exactement ce que rend
 * `char::is_alphabetic` côté Rust : le jumeau `accord-core/src/automod.rs` doit
 * placer les frontières aux mêmes endroits, sinon un message masqué ici
 * resterait compté dans la pastille de non-lus, ou l'inverse.
 */
const WORD_CHAR = '[\\p{Alphabetic}\\p{N}_]';

/**
 * Marques diacritiques combinantes, retirées au repli.
 *
 * Le même mot arrive tantôt précomposé (« é » = U+00E9), tantôt décomposé
 * (« e » + U+0301) selon le clavier, le système ou le copier-coller de
 * l'émetteur — macOS produit couramment la forme décomposée. Sans ce repli,
 * le filtre marchait ou non selon la machine d'en face, ce qui est pire
 * qu'un filtre absent : il donne l'illusion de protéger.
 */
const COMBINING = /[\u0300-\u036f]/g;

/**
 * Texte replié (minuscules, sans diacritiques) et correspondance d'index vers
 * le texte d'origine.
 *
 * Le repli ne conserve PAS les longueurs (« É » décomposé occupe deux unités
 * de code et se replie sur une), donc masquer aux offsets du texte replié
 * découperait le texte d'origine au mauvais endroit. `offsets[i]` donne
 * l'offset d'origine de la i-ème unité de code repliée ; la dernière entrée
 * est une sentinelle qui borne une correspondance allant jusqu'au bout.
 */
interface Folded {
  text: string;
  offsets: number[];
}

/** Replie un texte et construit sa correspondance d'index. */
function fold(input: string): Folded {
  let text = '';
  const offsets: number[] = [];
  let at = 0;
  // Itération par POINT de code (et non par unité de code) : un caractère
  // hors BMP ne doit pas être coupé en deux moitiés de paire de substitution.
  for (const ch of input) {
    const piece = ch.toLowerCase().normalize('NFD').replace(COMBINING, '');
    for (let i = 0; i < piece.length; i++) offsets.push(at);
    text += piece;
    at += ch.length;
  }
  offsets.push(input.length);
  return { text, offsets };
}

/** Échappe les métacaractères d'expression régulière d'un mot filtré. */
function escapeRegExp(word: string): string {
  return word.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

/**
 * Expression d'un mot filtré DÉJÀ replié : occurrence entière (pas de
 * caractère de mot immédiatement avant ni après), Unicode, globale. La casse
 * et les accents sont déjà neutralisés par [`fold`], d'où l'absence du
 * drapeau `i`.
 */
function wordPattern(foldedWord: string): RegExp {
  return new RegExp(`(?<!${WORD_CHAR})${escapeRegExp(foldedWord)}(?!${WORD_CHAR})`, 'gu');
}

/** Mots exploitables d'une liste AutoMod, repliés ; les vides sont écartés. */
function usableWords(words: readonly string[]): string[] {
  return words.map((w) => fold(w.trim()).text).filter((w) => w !== '');
}

/**
 * Occurrence d'un mot filtré : bornes `[from, to[` dans le texte d'ORIGINE
 * (pour découper au bon endroit) et longueur `seen` du mot une fois replié
 * (pour dimensionner le masque sans compter les accents décomposés deux fois).
 */
type Occurrence = { from: number; to: number; seen: number };

/** Occurrences des mots filtrés, triées et sans chevauchement. */
function filteredRanges(text: string, words: readonly string[]): Occurrence[] {
  const useful = usableWords(words);
  if (useful.length === 0) return [];
  const { text: hay, offsets } = fold(text);
  const found: Occurrence[] = [];
  for (const word of useful) {
    for (const m of hay.matchAll(wordPattern(word))) {
      found.push({
        from: offsets[m.index] ?? text.length,
        to: offsets[m.index + m[0].length] ?? text.length,
        seen: [...m[0]].length,
      });
    }
  }
  found.sort((a, b) => a.from - b.from);
  // Deux mots filtrés peuvent se recouvrir (« con » et « concon ») : garder le
  // premier suffit, le second est déjà sous le masque.
  const merged: Occurrence[] = [];
  let last = -1;
  for (const occ of found) {
    if (occ.from < last) continue;
    merged.push(occ);
    last = occ.to;
  }
  return merged;
}

/**
 * Masque chaque occurrence (mot entier, insensible à la casse et aux accents)
 * des mots filtrés par des `█` de longueur bornée ([3, 8]) proche de celle du
 * mot.
 */
export function maskFiltered(text: string, words: readonly string[]): string {
  const ranges = filteredRanges(text, words);
  if (ranges.length === 0) return text;
  let out = '';
  let cursor = 0;
  for (const { from, to, seen } of ranges) {
    out += text.slice(cursor, from);
    out += MASK_CHAR.repeat(Math.min(MASK_MAX, Math.max(MASK_MIN, seen)));
    cursor = to;
  }
  return out + text.slice(cursor);
}

/**
 * Vrai si `text` contient au moins un mot filtré (même règle de
 * correspondance que [`maskFiltered`]) — pour l'avertissement émetteur et
 * pour couper notification et son d'un message masqué.
 */
export function containsFiltered(text: string, words: readonly string[]): boolean {
  const useful = usableWords(words);
  if (useful.length === 0) return false;
  const { text: hay } = fold(text);
  return useful.some((word) => wordPattern(word).test(hay));
}
