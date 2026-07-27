/**
 * Constantes d'apparence et clés de stockage de l'interface : thèmes, échelles
 * de police, densités, saturation, largeurs des panneaux, et les clés
 * localStorage de tous les réglages. Aucune logique, aucun état — extraites de
 * `stores/ui` qui dépassait largement les 800 lignes.
 */

import type { OwnPresenceStatus } from '../lib/api';

/**
 * Identifiants de la galerie de thèmes (Paramètres → Apparence), palettes
 * définies en CSS (`[data-theme='<id>']`, voir global.css). `'dark'` et
 * `'light'` sont les valeurs historiques : une préférence déjà persistée
 * sous l'un de ces deux ids continue de se résoudre sans migration, ce sont
 * simplement deux thèmes de plus dans l'union.
 */
export const THEME_IDS = [
  'dark',
  'light',
  'midnight',
  'storm',
  'forest',
  'sunset',
  'ocean',
  'crimson',
  'boreal',
  'paper',
  'topography',
  'signal',
  'nebula',
  'synthwave',
  'sakura',
  'wisteria',
  'lotus',
  'manga',
  'shojo',
  'abyss',
  'ember',
  'frost',
  'circuit',
  'dream',
  'custom',
] as const;
export type Theme = (typeof THEME_IDS)[number];
export type Density = 'comfortable' | 'compact';

/** Échelles de police proposées, en pourcentage de la taille de base. */
export const FONT_SCALES = [75, 100, 125, 150] as const;
export type FontScale = (typeof FONT_SCALES)[number];

/**
 * Réduction des animations : `system` suit `prefers-reduced-motion`, `on`
 * force la réduction (via `data-motion` à la racine, voir global.css), `off`
 * ne force rien — un système déjà en préférence réduite continue de
 * s'appliquer (la requête média ne peut pas être vaincue depuis le DOM).
 */
export type ReducedMotionPref = 'system' | 'on' | 'off';

/** Taille des émojis personnalisés (`:nom:`) rendus dans le corps des messages. */
export type EmojiSize = 'normal' | 'large';

/** Familles de police d'interface proposées (toutes disponibles nativement,
 * aucune n'est téléchargée — la CSP interdit les hôtes externes). */
export const FONT_UI_CHOICES = ['system', 'rounded', 'serif'] as const;
export type FontUi = (typeof FONT_UI_CHOICES)[number];

/** Pile CSS `font-family` de chaque choix (générique système en repli). */
export const FONT_UI_STACKS: Record<FontUi, string> = {
  system:
    '-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif',
  rounded: 'ui-rounded, "SF Pro Rounded", "Segoe UI", system-ui, sans-serif',
  serif: 'ui-serif, Georgia, "Times New Roman", serif',
};

/** Filtrage du blip sonore par nature de message entrant. */
export type NotifySoundMode = 'all' | 'mentionsOnly' | 'none';

/** Présence appliquée une fois au démarrage ; `null` = ne rien forcer. */
export type StartupPresence = Extract<OwnPresenceStatus, 'online' | 'invisible'> | null;

/** Préférence d'affichage des heures (`auto` suit la locale de l'interface). */
export type TimeFormat = 'auto' | '12h' | '24h';

/** Bornes de la saturation appliquée à toute l'application (%, 100 = neutre). */
export const SATURATION_MIN = 0;
export const SATURATION_MAX = 100;
export const SATURATION_DEFAULT = 100;

/**
 * Largeurs redimensionnables façon Discord (barre latérale de navigation,
 * liste des membres d'un serveur). Bornes en pixels — `ResizeHandle`
 * applique le même clamp côté glissé/clavier, ces constantes restent la
 * source de vérité unique (store et poignée s'y réfèrent toutes deux).
 */
export const SIDEBAR_WIDTH_DEFAULT = 240;
export const SIDEBAR_WIDTH_MIN = 200;
export const SIDEBAR_WIDTH_MAX = 420;

export const MEMBERS_WIDTH_DEFAULT = 240;
export const MEMBERS_WIDTH_MIN = 180;
export const MEMBERS_WIDTH_MAX = 380;

/** Clés localStorage de tous les réglages d'interface. */
export const STORAGE_KEYS = {
  theme: 'accord.theme',
  customTheme: 'accord.theme.custom',
  density: 'accord.density',
  fontScale: 'accord.fontScale',
  lang: 'accord.lang',
  autoLockMinutes: 'accord.autoLockMinutes',
  streamerMode: 'accord.streamerMode',
  linkPreviews: 'accord.privacy.linkPreviews',
  pttEnabled: 'accord.pttEnabled',
  pttKey: 'accord.pttKey',
  notifyDms: 'accord.notifyDms',
  notifyGroups: 'accord.notifyGroups',
  notifyOnlyUnfocused: 'accord.notifyOnlyUnfocused',
  sidebarWidth: 'accord.layout.sidebarWidth',
  membersWidth: 'accord.layout.membersWidth',
  reducedMotion: 'accord.a11y.reducedMotion',
  saturation: 'accord.a11y.saturation',
  showMediaPreviews: 'accord.media.showPreviews',
  emojiSize: 'accord.media.emojiSize',
  fontUi: 'accord.appearance.fontUi',
  videoPreviewMaxMio: 'accord.media.videoPreviewMaxMio',
  notifySoundEnabled: 'accord.notify.soundEnabled',
  notifyNative: 'accord.notify.native',
  notifySoundMode: 'accord.notify.soundMode',
  quietHours: 'accord.notify.quietHours',
  typingIndicatorEnabled: 'accord.privacy.typingIndicator',
  startupPresence: 'accord.privacy.startupPresence',
  timeFormat: 'accord.timeFormat',
  keepInTray: 'accord.system.keepInTray',
  closeToTray: 'accord.system.closeToTray',
  hideMutedChannels: 'accord.channels.hideMuted',
} as const;

/** Touche d'appui-pour-parler par défaut (`KeyboardEvent.code`). */
export const DEFAULT_PTT_KEY = 'Space';
