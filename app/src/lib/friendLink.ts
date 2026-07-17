/**
 * Liens d'ami partageables. Un lien a la forme `accord://friend/<code>` —
 * même schéma que les liens d'invitation serveur (`accord://invite/`, voir
 * `invite.ts`), le code ami restant une chaîne opaque résolue par le nœud
 * (`friends.resolve` tolère casse et séparateurs). Le deep link historique
 * côté Rust (`accord-crypto::friendcode::deep_link`, `p2papp://add/<code>`)
 * est accepté en lecture pour rester interopérable.
 */

/** Préfixe canonique des liens d'ami partageables (`accord://friend/<code>`). */
export const FRIEND_LINK_PREFIX = 'accord://friend/';

/** Variante sans schéma acceptée au collage (`friend/<code>`). */
const SCHEMELESS_PREFIX = 'friend/';

/** Deep link hérité du cœur Rust (`accord-crypto::friendcode::deep_link`). */
const LEGACY_LINK_PREFIX = 'p2papp://add/';

/**
 * Forme d'un code ami plausible : groupes de lettres (accents inclus) ou de
 * chiffres, séparés par des tirets ou des espaces simples. Volontairement
 * laxiste — la validation forte (dictionnaire, somme de contrôle) reste
 * l'affaire du nœud ; on écarte seulement ce qui ne peut pas être un code
 * (URL étrangère, texte avec ponctuation…).
 */
const CODE_RE = /^[\p{L}\p{N}]+(?:[- ][\p{L}\p{N}]+)*$/u;

/** Lien partageable du code ami donné (le code est simplement borné/trim). */
export function buildFriendLink(code: string): string {
  return `${FRIEND_LINK_PREFIX}${code.trim()}`;
}

/**
 * Extrait le code ami d'une saisie libre : lien complet (`accord://friend/…`),
 * lien sans schéma (`friend/…`), deep link Rust (`p2papp://add/…`) ou code
 * brut. Rend le code normalisé (bornes et barre finale retirées, espaces
 * internes réduits) ou `null` si la saisie ne peut pas être un code ami.
 */
export function parseFriendLink(input: string): string | null {
  const trimmed = input.trim();
  if (trimmed === '') return null;

  // Préfixes comparés en minuscules (schéma insensible à la casse), mais le
  // code lui-même garde sa casse d'origine — il est opaque côté UI.
  const lower = trimmed.toLowerCase();
  let candidate = trimmed;
  if (lower.startsWith(FRIEND_LINK_PREFIX)) {
    candidate = trimmed.slice(FRIEND_LINK_PREFIX.length);
  } else if (lower.startsWith(SCHEMELESS_PREFIX)) {
    candidate = trimmed.slice(SCHEMELESS_PREFIX.length);
  } else if (lower.startsWith(LEGACY_LINK_PREFIX)) {
    candidate = trimmed.slice(LEGACY_LINK_PREFIX.length);
  } else if (lower.includes('://') || lower.startsWith('//')) {
    // Autre lien (accord://invite/…, https://…) : ce n'est pas un code ami.
    return null;
  }

  // Barre oblique finale (copie depuis un navigateur) et espaces superflus.
  const normalized = candidate.replace(/\/+$/, '').trim().replace(/\s+/g, ' ');
  return CODE_RE.test(normalized) ? normalized : null;
}
