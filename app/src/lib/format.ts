/** Aides d'affichage : horodatages, initiales, couleurs d'avatar. */

import type { Lang } from '../i18n';

/**
 * Préférence de format d'heure (Paramètres → Langue et heure) : `auto` suit
 * la convention de la locale (fr-FR → 24 h, en-US → 12 h par défaut), les
 * deux autres valeurs forcent `Intl`'s `hour12` indépendamment de la langue.
 */
export type HourFormat = 'auto' | '12h' | '24h';

/** Horodatage court d'un message (heure si aujourd'hui, sinon date). */
export function formatTimestamp(
  ms: number,
  lang: Lang,
  now = Date.now(),
  hourFormat: HourFormat = 'auto',
): string {
  const locale = lang === 'fr' ? 'fr-FR' : 'en-US';
  const date = new Date(ms);
  const today = new Date(now);
  const sameDay =
    date.getFullYear() === today.getFullYear() &&
    date.getMonth() === today.getMonth() &&
    date.getDate() === today.getDate();
  if (sameDay) {
    const options: Intl.DateTimeFormatOptions = { hour: '2-digit', minute: '2-digit' };
    if (hourFormat !== 'auto') options.hour12 = hourFormat === '12h';
    return date.toLocaleTimeString(locale, options);
  }
  return date.toLocaleDateString(locale, {
    day: '2-digit',
    month: '2-digit',
    year: 'numeric',
  });
}

/** Séparateur de jour dans un fil de messages. */
export function formatDay(ms: number, lang: Lang): string {
  const locale = lang === 'fr' ? 'fr-FR' : 'en-US';
  return new Date(ms).toLocaleDateString(locale, {
    weekday: 'long',
    day: 'numeric',
    month: 'long',
    year: 'numeric',
  });
}

/** Initiales d'un nom affiché (1 à 2 caractères). */
export function initials(name: string): string {
  const words = name.trim().split(/\s+/).filter(Boolean);
  if (words.length === 0) return '?';
  const first = [...(words[0] ?? '')][0] ?? '?';
  const second = words.length > 1 ? ([...(words[1] ?? '')][0] ?? '') : '';
  return (first + second).toUpperCase();
}

const AVATAR_COLORS = [
  '#5865f2',
  '#23a55a',
  '#f0b232',
  '#f23f43',
  '#eb459e',
  '#3ba55c',
  '#faa61a',
] as const;

/** Couleur d'avatar stable dérivée d'un identifiant hexadécimal. */
export function avatarColor(id: string): string {
  let hash = 0;
  for (let i = 0; i < id.length; i++) {
    hash = (hash * 31 + id.charCodeAt(i)) >>> 0;
  }
  return AVATAR_COLORS[hash % AVATAR_COLORS.length] ?? AVATAR_COLORS[0];
}

/** Libellé court d'une clé publique inconnue (jamais la clé entière). */
export function shortId(hex: string): string {
  return hex.slice(0, 6);
}

const UNITES_TAILLE: Record<Lang, readonly string[]> = {
  fr: ['o', 'Ko', 'Mo', 'Go'],
  en: ['B', 'KB', 'MB', 'GB'],
};

/** Taille de fichier lisible (base 1024, une décimale au plus). */
export function tailleLisible(octets: number, lang: Lang): string {
  const unites = UNITES_TAILLE[lang];
  let valeur = Math.max(0, octets);
  let rang = 0;
  while (valeur >= 1024 && rang < unites.length - 1) {
    valeur /= 1024;
    rang += 1;
  }
  const arrondi = rang === 0 ? String(valeur) : String(Math.round(valeur * 10) / 10);
  const texte = lang === 'fr' ? arrondi.replace('.', ',') : arrondi;
  return `${texte} ${unites[rang] ?? ''}`;
}

/**
 * Horodatage compact pour la gouttière des messages groupés (40 px) : même
 * heure que [`formatTimestamp`], mais le méridien 12 h est collé et en
 * minuscules (« 12:05am ») pour tenir sans retour à la ligne. Sans effet en
 * format 24 h ou pour les dates (jours précédents).
 */
export function formatTimestampCompact(
  ms: number,
  lang: Lang,
  now = Date.now(),
  hourFormat: HourFormat = 'auto',
): string {
  return formatTimestamp(ms, lang, now, hourFormat).replace(
    /\s*([AP]M)$/i,
    (_, meridiem: string) => meridiem.toLowerCase(),
  );
}
