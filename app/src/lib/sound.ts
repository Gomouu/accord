/**
 * Sons de soundboard : validation des fichiers audio (contrat `groups.sounds.*`).
 * Contrairement aux émojis/stickers, aucun ré-encodage : un clip audio est
 * transmis tel quel, on se contente d'en vérifier le type MIME et la taille.
 * Le nom suit exactement les mêmes bornes qu'un émoji (`[a-z0-9_]`, 2 à 32).
 */

import { estNomEmojiValide } from './emoji';

/** Taille maximale d'un clip audio, une fois décodé (256 Kio). */
export const SOUND_OCTETS_MAX = 256 * 1024;

/** Types MIME audio acceptés pour un son de soundboard (contrat). */
export const SOUND_MIMES = [
  'audio/ogg',
  'audio/mpeg',
  'audio/mp4',
  'audio/webm',
  'audio/wav',
] as const;

/** Vrai si `mime` est un type audio accepté pour un son de soundboard. */
export function estMimeSonValide(mime: string): boolean {
  return (SOUND_MIMES as readonly string[]).includes(mime);
}

/** Vrai si `taille` (octets décodés) tient sous la limite d'un son. */
export function estTailleSonValide(taille: number): boolean {
  return taille > 0 && taille <= SOUND_OCTETS_MAX;
}

/** Nom de son valide : mêmes bornes qu'un émoji custom (2-32, `[a-z0-9_]`). */
export function estNomSonValide(name: string): boolean {
  return estNomEmojiValide(name);
}
