/**
 * Brouillons de composeur persistés par conversation (comme Discord) : le
 * texte tapé sans être envoyé est conservé par salon/MP dans `localStorage` et
 * restauré au retour, y compris après redémarrage de l'app. Texte seulement —
 * les pièces jointes ne sont jamais persistées (trop lourdes pour localStorage).
 */

import type { TypingTarget } from '../hooks/useTypingEmitter';

/**
 * Longueur maximale d'un brouillon persisté. Au-delà, le brouillon est ignoré
 * (pas d'écriture) pour ne pas gonfler localStorage — un texte aussi long est
 * de toute façon prêt à partir plutôt qu'à traîner en brouillon.
 */
export const MAX_DRAFT_LEN = 4000;

/**
 * Clé de brouillon stable pour une cible de frappe :
 *   - MP     → `draft:dm:{peer}`
 *   - groupe → `draft:grp:{groupId}/{channelId}` (même format que `channelKey`)
 * `null` quand la cible est absente : le composeur n'est alors rattaché à
 * aucune conversation identifiable, donc pas de persistance.
 */
export function draftKey(target: TypingTarget | undefined): string | null {
  if (target === undefined) return null;
  return target.kind === 'dm'
    ? `draft:dm:${target.peer}`
    : `draft:grp:${target.groupId}/${target.channelId}`;
}

/** Lecture tolérante (clé nulle ou stockage indisponible → `null`). */
export function readDraft(key: string | null): string | null {
  if (key === null) return null;
  try {
    return window.localStorage.getItem(key);
  } catch {
    return null;
  }
}

/**
 * Enregistre le brouillon, ou efface la clé quand le texte est vide — pour ne
 * pas accumuler de brouillons morts (l'envoi vide le composeur, ce qui efface
 * donc naturellement le brouillon). Un texte au-delà de `MAX_DRAFT_LEN` est
 * ignoré : aucune écriture, le dernier brouillon valide reste tel quel.
 */
export function writeDraft(key: string | null, text: string): void {
  if (key === null || text.length > MAX_DRAFT_LEN) return;
  try {
    if (text === '') {
      window.localStorage.removeItem(key);
    } else {
      window.localStorage.setItem(key, text);
    }
  } catch {
    // Best effort : le brouillon reste en mémoire pour la session en cours.
  }
}
