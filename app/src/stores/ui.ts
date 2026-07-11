/**
 * État d'interface : vue active, modales, toasts, langue et préférences
 * d'apparence (thème, densité, taille de police). Les préférences sont
 * persistées dans localStorage et appliquées immédiatement sur la racine du
 * document (`data-theme`, `data-density`, `font-size`), où les variables CSS
 * du design system les consomment.
 */

import { create } from 'zustand';
import { type Lang } from '../i18n';
import { type OwnPresenceStatus } from '../lib/api';
import {
  loadLastChannelByServer,
  loadLastDm,
  saveLastChannelByServer,
  saveLastDm,
} from '../lib/navPersistence';

export type View =
  | { kind: 'friends' }
  | { kind: 'dm'; peer: string }
  | { kind: 'group'; groupId: string; channelId: string | null };

export type Modal =
  | null
  | { kind: 'createGroup' }
  | { kind: 'createChannel'; groupId: string }
  | { kind: 'invite'; groupId: string }
  | { kind: 'settings' }
  | {
      kind: 'serverSettings';
      groupId: string;
      /** Onglet initial (menu du serveur → « Créer une catégorie ») ; défaut : Profil. */
      initialTab?: 'channels';
    };

export interface Toast {
  id: number;
  kind: 'error' | 'info';
  text: string;
}

/** Rectangle d'ancrage (coordonnées viewport) d'un déclencheur de popover. */
export interface AncrePopover {
  top: number;
  left: number;
  bottom: number;
  right: number;
}

/** Cible d'affichage de la carte de profil (clic sur pseudo/avatar). */
export interface CibleProfil {
  pubkey: string;
  ancre: AncrePopover;
  /** Contexte serveur (rôles colorés) ; `null` en MP ou vue Amis. */
  groupId: string | null;
}

/**
 * Demande de saut vers un message (résultat de recherche, épinglé, citation).
 * `nonce` distingue deux sauts vers le même message pour rejouer l'animation.
 */
export interface JumpRequest {
  view: View;
  msgId: string;
  nonce: number;
}

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

const STORAGE_KEYS = {
  theme: 'accord.theme',
  density: 'accord.density',
  fontScale: 'accord.fontScale',
  lang: 'accord.lang',
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
  notifySoundEnabled: 'accord.notify.soundEnabled',
  notifyNative: 'accord.notify.native',
  notifySoundMode: 'accord.notify.soundMode',
  typingIndicatorEnabled: 'accord.privacy.typingIndicator',
  startupPresence: 'accord.privacy.startupPresence',
  timeFormat: 'accord.timeFormat',
} as const;

/** Touche d'appui-pour-parler par défaut (`KeyboardEvent.code`). */
export const DEFAULT_PTT_KEY = 'Space';

/** Lecture localStorage tolérante (stockage indisponible → null). */
function readStored(key: string): string | null {
  try {
    return window.localStorage.getItem(key);
  } catch {
    return null;
  }
}

/** Écriture localStorage tolérante (préférence non persistée en cas d'échec). */
function writeStored(key: string, value: string): void {
  try {
    window.localStorage.setItem(key, value);
  } catch {
    // Best effort : la préférence reste appliquée pour la session en cours.
  }
}

function isTheme(value: string | null): value is Theme {
  return value !== null && (THEME_IDS as readonly string[]).includes(value);
}

function isDensity(value: string | null): value is Density {
  return value === 'comfortable' || value === 'compact';
}

function isFontScale(value: number): value is FontScale {
  return (FONT_SCALES as readonly number[]).includes(value);
}

function isLang(value: string | null): value is Lang {
  return value === 'fr' || value === 'en';
}

/** Valeurs persistées, validées à la frontière (repli : défauts sûrs). */
function initialTheme(): Theme {
  const stored = readStored(STORAGE_KEYS.theme);
  return isTheme(stored) ? stored : 'dark';
}

function initialDensity(): Density {
  const stored = readStored(STORAGE_KEYS.density);
  return isDensity(stored) ? stored : 'comfortable';
}

function initialFontScale(): FontScale {
  const parsed = Number(readStored(STORAGE_KEYS.fontScale));
  return isFontScale(parsed) ? parsed : 100;
}

/**
 * Langue au démarrage : celle choisie par l'utilisateur (persistée), sinon
 * anglais par défaut (décision produit — pas de détection système).
 */
function initialLang(): Lang {
  const stored = readStored(STORAGE_KEYS.lang);
  return isLang(stored) ? stored : 'en';
}

/** Booléen persisté (`'true'`/`'false'`), avec repli en valeur par défaut. */
function initialBool(key: string, fallback: boolean): boolean {
  const stored = readStored(key);
  if (stored === 'true') return true;
  if (stored === 'false') return false;
  return fallback;
}

function initialPttKey(): string {
  const stored = readStored(STORAGE_KEYS.pttKey);
  return stored !== null && stored !== '' ? stored : DEFAULT_PTT_KEY;
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

function isReducedMotionPref(value: string | null): value is ReducedMotionPref {
  return value === 'system' || value === 'on' || value === 'off';
}

function initialReducedMotion(): ReducedMotionPref {
  const stored = readStored(STORAGE_KEYS.reducedMotion);
  return isReducedMotionPref(stored) ? stored : 'system';
}

/** Saturation persistée (%), bornée à `[SATURATION_MIN, SATURATION_MAX]`. */
function initialSaturation(): number {
  const stored = readStored(STORAGE_KEYS.saturation);
  if (stored === null) return SATURATION_DEFAULT;
  const parsed = Number(stored);
  return Number.isFinite(parsed)
    ? clamp(parsed, SATURATION_MIN, SATURATION_MAX)
    : SATURATION_DEFAULT;
}

function isEmojiSize(value: string | null): value is EmojiSize {
  return value === 'normal' || value === 'large';
}

function initialEmojiSize(): EmojiSize {
  const stored = readStored(STORAGE_KEYS.emojiSize);
  return isEmojiSize(stored) ? stored : 'normal';
}

function isNotifySoundMode(value: string | null): value is NotifySoundMode {
  return value === 'all' || value === 'mentionsOnly' || value === 'none';
}

function initialNotifySoundMode(): NotifySoundMode {
  const stored = readStored(STORAGE_KEYS.notifySoundMode);
  return isNotifySoundMode(stored) ? stored : 'all';
}

function isStartupPresence(
  value: string | null,
): value is Exclude<StartupPresence, null> {
  return value === 'online' || value === 'invisible';
}

/** Préférence persistée ; `null` (absente ou invalide) = ne rien forcer. */
function initialStartupPresence(): StartupPresence {
  const stored = readStored(STORAGE_KEYS.startupPresence);
  return isStartupPresence(stored) ? stored : null;
}

function isTimeFormat(value: string | null): value is TimeFormat {
  return value === 'auto' || value === '12h' || value === '24h';
}

function initialTimeFormat(): TimeFormat {
  const stored = readStored(STORAGE_KEYS.timeFormat);
  return isTimeFormat(stored) ? stored : 'auto';
}

/**
 * Largeur persistée (px), bornée à `[min, max]` — une valeur absente ou non
 * numérique replie sur `fallback` plutôt que de clamper `0`.
 */
function initialWidth(key: string, fallback: number, min: number, max: number): number {
  const stored = readStored(key);
  if (stored === null) return fallback;
  const parsed = Number(stored);
  return Number.isFinite(parsed) ? clamp(parsed, min, max) : fallback;
}

/* Application immédiate sur la racine du document. */

function applyTheme(theme: Theme): void {
  document.documentElement.dataset.theme = theme;
}

function applyDensity(density: Density): void {
  document.documentElement.dataset.density = density;
}

function applyFontScale(scale: FontScale): void {
  document.documentElement.style.fontSize = `${scale}%`;
}

/**
 * Force la réduction d'animations à la racine (`data-motion="reduce"`),
 * consommé par le bloc `:root[data-motion='reduce']` de global.css (mirroir
 * du bloc `@media (prefers-reduced-motion: reduce)`). `off`/`system` retirent
 * l'attribut : `system` laisse la requête média seule décider, `off` ne peut
 * pas vaincre une préférence système déjà réduite (limitation assumée — voir
 * le commentaire du type `ReducedMotionPref`).
 */
function applyReducedMotion(pref: ReducedMotionPref): void {
  if (pref === 'on') document.documentElement.dataset.motion = 'reduce';
  else delete document.documentElement.dataset.motion;
}

/**
 * Filtre `saturate()` appliqué à toute l'application via une variable CSS
 * consommée à la racine (voir global.css) — un seul filtre au sommet de
 * l'arbre, peu coûteux, plutôt qu'une répétition par composant.
 */
function applySaturation(percent: number): void {
  document.documentElement.style.setProperty('--saturation', `${percent}%`);
}

/**
 * Mémoire de navigation induite par `view` : dernier salon consulté par
 * serveur, dernier pair de conversation privée. Toujours reconstruit un
 * objet limité à ces deux clés (jamais `s` tel quel) : `s` reçu ici est
 * l'état complet du store, et le renvoyer directement écraserait `view`/
 * `jump`/`profile` fraîchement calculés par l'appelant lors du spread. La
 * validation contre l'état courant (salon supprimé, ami retiré) revient à
 * l'appelant qui restaure la vue — ce store se contente d'enregistrer la
 * dernière navigation réussie.
 */
function withNavMemory(
  s: Pick<UiState, 'lastChannelByServer' | 'lastDmPeer'>,
  view: View,
): Pick<UiState, 'lastChannelByServer' | 'lastDmPeer'> {
  if (view.kind === 'group' && view.channelId !== null) {
    if (s.lastChannelByServer[view.groupId] === view.channelId) {
      return { lastChannelByServer: s.lastChannelByServer, lastDmPeer: s.lastDmPeer };
    }
    const lastChannelByServer = {
      ...s.lastChannelByServer,
      [view.groupId]: view.channelId,
    };
    saveLastChannelByServer(lastChannelByServer);
    return { lastChannelByServer, lastDmPeer: s.lastDmPeer };
  }
  if (view.kind === 'dm' && s.lastDmPeer !== view.peer) {
    saveLastDm(view.peer);
    return { lastChannelByServer: s.lastChannelByServer, lastDmPeer: view.peer };
  }
  return { lastChannelByServer: s.lastChannelByServer, lastDmPeer: s.lastDmPeer };
}

interface UiState {
  view: View;
  modal: Modal;
  /** Saut vers un message en attente de traitement par la vue, ou `null`. */
  jump: JumpRequest | null;
  /** Carte de profil ouverte (clic sur un pseudo/avatar), ou `null`. */
  profile: CibleProfil | null;
  /**
   * Mention à insérer dans le composeur actif (menu contextuel « Mentionner »
   * sur un message ou un membre) ; `nonce` rejoue l'insertion même pour un
   * nom identique. Consommée par `MessageInput` puis remise à `null`.
   */
  mentionInsert: { name: string; nonce: number } | null;
  toasts: Toast[];
  lang: Lang;
  theme: Theme;
  density: Density;
  fontScale: FontScale;
  /** Appui-pour-parler : en vocal, micro coupé sauf pendant l'appui. */
  pttEnabled: boolean;
  /** Touche d'appui-pour-parler (`KeyboardEvent.code`). */
  pttKey: string;
  /** Notifications natives pour les messages privés. */
  notifyDms: boolean;
  /** Notifications natives pour les messages de groupe. */
  notifyGroups: boolean;
  /** Ne notifier que lorsque la fenêtre est en arrière-plan. */
  notifyOnlyUnfocused: boolean;
  /** Réduction des animations (voir `ReducedMotionPref`). */
  reducedMotion: ReducedMotionPref;
  /** Saturation globale (%, 100 = neutre) — filtre CSS appliqué à la racine. */
  saturation: number;
  /** Aperçus d'images/médias en ligne ; désactivé = carte fichier seule. */
  showMediaPreviews: boolean;
  /** Taille par défaut des émojis personnalisés dans le corps des messages. */
  emojiSize: EmojiSize;
  /** Blip sonore de notification (message, mention, invitation). */
  notifySoundEnabled: boolean;
  /** Notifications natives du système (plugin Tauri). */
  notifyNative: boolean;
  /** Filtre supplémentaire sur le blip sonore d'un message entrant. */
  notifySoundMode: NotifySoundMode;
  /** Émission de l'indicateur de frappe vers le pair/salon (réception intacte). */
  typingIndicatorEnabled: boolean;
  /** Présence forcée une fois la session prête ; `null` = ne rien forcer. */
  startupPresence: StartupPresence;
  /** Format des heures affichées (horodatages de messages). */
  timeFormat: TimeFormat;
  /**
   * Dernier salon (texte/annonces) consulté par serveur — clé `groupId`.
   * Restauré au reclic sur l'icône du serveur ; l'appelant valide que le
   * salon existe encore avant de s'y fier (voir `ServerRail`).
   */
  lastChannelByServer: Record<string, string>;
  /**
   * Dernier pair de conversation privée ouvert, ou `null`. Restauré au
   * reclic sur l'icône MP/accueil ; l'appelant valide que l'amitié tient
   * toujours avant de s'y fier (voir `ServerRail`).
   */
  lastDmPeer: string | null;
  /** Largeur de la barre latérale (px), bornée à `[SIDEBAR_WIDTH_MIN, SIDEBAR_WIDTH_MAX]`. */
  sidebarWidth: number;
  /** Largeur de la liste des membres (px), bornée à `[MEMBERS_WIDTH_MIN, MEMBERS_WIDTH_MAX]`. */
  membersWidth: number;
  setView: (view: View) => void;
  /** Bascule vers `view` et demande le saut vers `msgId` (recherche, épingle). */
  requestJump: (view: View, msgId: string) => void;
  /** Consomme la demande de saut courante (traitée par la vue cible). */
  clearJump: () => void;
  openModal: (modal: Exclude<Modal, null>) => void;
  closeModal: () => void;
  /** Ouvre la carte de profil d'un pair, ancrée près du clic. */
  openProfile: (pubkey: string, ancre: AncrePopover, groupId?: string | null) => void;
  closeProfile: () => void;
  /** Demande l'insertion de `@name` dans le composeur actif (voir `mentionInsert`). */
  requestMentionInsert: (name: string) => void;
  /** Consomme la demande courante (traitée par `MessageInput`). */
  clearMentionInsert: () => void;
  toast: (kind: Toast['kind'], text: string) => void;
  dismissToast: (id: number) => void;
  setLang: (lang: Lang) => void;
  setTheme: (theme: Theme) => void;
  setDensity: (density: Density) => void;
  setFontScale: (fontScale: FontScale) => void;
  setPttEnabled: (enabled: boolean) => void;
  setPttKey: (key: string) => void;
  setNotifyDms: (enabled: boolean) => void;
  setNotifyGroups: (enabled: boolean) => void;
  setNotifyOnlyUnfocused: (enabled: boolean) => void;
  setReducedMotion: (pref: ReducedMotionPref) => void;
  /** Applique `percent` bornée à `[SATURATION_MIN, SATURATION_MAX]`. */
  setSaturation: (percent: number) => void;
  setShowMediaPreviews: (enabled: boolean) => void;
  setEmojiSize: (size: EmojiSize) => void;
  setNotifySoundEnabled: (enabled: boolean) => void;
  setNotifyNative: (enabled: boolean) => void;
  setNotifySoundMode: (mode: NotifySoundMode) => void;
  setTypingIndicatorEnabled: (enabled: boolean) => void;
  setStartupPresence: (presence: StartupPresence) => void;
  setTimeFormat: (format: TimeFormat) => void;
  /** Applique `width` bornée à `[SIDEBAR_WIDTH_MIN, SIDEBAR_WIDTH_MAX]`. */
  setSidebarWidth: (width: number) => void;
  /** Restaure `SIDEBAR_WIDTH_DEFAULT` (double-clic sur la poignée). */
  resetSidebarWidth: () => void;
  /** Applique `width` bornée à `[MEMBERS_WIDTH_MIN, MEMBERS_WIDTH_MAX]`. */
  setMembersWidth: (width: number) => void;
  /** Restaure `MEMBERS_WIDTH_DEFAULT` (double-clic sur la poignée). */
  resetMembersWidth: () => void;
}

const TOAST_LIFETIME_MS = 5000;
let nextToastId = 1;

export const useUi = create<UiState>((set) => {
  const theme = initialTheme();
  const density = initialDensity();
  const fontScale = initialFontScale();
  const reducedMotion = initialReducedMotion();
  const saturation = initialSaturation();
  applyTheme(theme);
  applyDensity(density);
  applyFontScale(fontScale);
  applyReducedMotion(reducedMotion);
  applySaturation(saturation);

  return {
    view: { kind: 'friends' },
    modal: null,
    jump: null,
    profile: null,
    mentionInsert: null,
    toasts: [],
    lang: initialLang(),
    theme,
    density,
    fontScale,
    pttEnabled: initialBool(STORAGE_KEYS.pttEnabled, false),
    pttKey: initialPttKey(),
    notifyDms: initialBool(STORAGE_KEYS.notifyDms, true),
    notifyGroups: initialBool(STORAGE_KEYS.notifyGroups, true),
    notifyOnlyUnfocused: initialBool(STORAGE_KEYS.notifyOnlyUnfocused, true),
    reducedMotion,
    saturation,
    showMediaPreviews: initialBool(STORAGE_KEYS.showMediaPreviews, true),
    emojiSize: initialEmojiSize(),
    notifySoundEnabled: initialBool(STORAGE_KEYS.notifySoundEnabled, true),
    notifyNative: initialBool(STORAGE_KEYS.notifyNative, true),
    notifySoundMode: initialNotifySoundMode(),
    typingIndicatorEnabled: initialBool(STORAGE_KEYS.typingIndicatorEnabled, true),
    startupPresence: initialStartupPresence(),
    timeFormat: initialTimeFormat(),
    lastChannelByServer: loadLastChannelByServer(),
    lastDmPeer: loadLastDm(),
    sidebarWidth: initialWidth(
      STORAGE_KEYS.sidebarWidth,
      SIDEBAR_WIDTH_DEFAULT,
      SIDEBAR_WIDTH_MIN,
      SIDEBAR_WIDTH_MAX,
    ),
    membersWidth: initialWidth(
      STORAGE_KEYS.membersWidth,
      MEMBERS_WIDTH_DEFAULT,
      MEMBERS_WIDTH_MIN,
      MEMBERS_WIDTH_MAX,
    ),

    setView: (view) =>
      set((s) => ({ view, jump: null, profile: null, ...withNavMemory(s, view) })),
    requestJump: (view, msgId) =>
      set((s) => ({
        view,
        jump: { view, msgId, nonce: (s.jump?.nonce ?? 0) + 1 },
        profile: null,
        ...withNavMemory(s, view),
      })),
    clearJump: () => set({ jump: null }),
    openModal: (modal) => set({ modal }),
    closeModal: () => set({ modal: null }),
    openProfile: (pubkey, ancre, groupId = null) =>
      set({ profile: { pubkey, ancre, groupId } }),
    closeProfile: () => set({ profile: null }),
    requestMentionInsert: (name) =>
      set((s) => ({ mentionInsert: { name, nonce: (s.mentionInsert?.nonce ?? 0) + 1 } })),
    clearMentionInsert: () => set({ mentionInsert: null }),

    toast: (kind, text) => {
      const id = nextToastId++;
      set((s) => ({ toasts: [...s.toasts, { id, kind, text }] }));
      setTimeout(() => {
        set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) }));
      }, TOAST_LIFETIME_MS);
    },
    dismissToast: (id) => set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) })),

    setLang: (lang) => {
      writeStored(STORAGE_KEYS.lang, lang);
      set({ lang });
    },
    setTheme: (nextTheme) => {
      applyTheme(nextTheme);
      writeStored(STORAGE_KEYS.theme, nextTheme);
      set({ theme: nextTheme });
    },
    setDensity: (nextDensity) => {
      applyDensity(nextDensity);
      writeStored(STORAGE_KEYS.density, nextDensity);
      set({ density: nextDensity });
    },
    setFontScale: (nextFontScale) => {
      applyFontScale(nextFontScale);
      writeStored(STORAGE_KEYS.fontScale, String(nextFontScale));
      set({ fontScale: nextFontScale });
    },
    setPttEnabled: (enabled) => {
      writeStored(STORAGE_KEYS.pttEnabled, String(enabled));
      set({ pttEnabled: enabled });
    },
    setPttKey: (key) => {
      if (key === '') return;
      writeStored(STORAGE_KEYS.pttKey, key);
      set({ pttKey: key });
    },
    setNotifyDms: (enabled) => {
      writeStored(STORAGE_KEYS.notifyDms, String(enabled));
      set({ notifyDms: enabled });
    },
    setNotifyGroups: (enabled) => {
      writeStored(STORAGE_KEYS.notifyGroups, String(enabled));
      set({ notifyGroups: enabled });
    },
    setNotifyOnlyUnfocused: (enabled) => {
      writeStored(STORAGE_KEYS.notifyOnlyUnfocused, String(enabled));
      set({ notifyOnlyUnfocused: enabled });
    },
    setReducedMotion: (pref) => {
      applyReducedMotion(pref);
      writeStored(STORAGE_KEYS.reducedMotion, pref);
      set({ reducedMotion: pref });
    },
    setSaturation: (percent) => {
      const clamped = clamp(percent, SATURATION_MIN, SATURATION_MAX);
      applySaturation(clamped);
      writeStored(STORAGE_KEYS.saturation, String(clamped));
      set({ saturation: clamped });
    },
    setShowMediaPreviews: (enabled) => {
      writeStored(STORAGE_KEYS.showMediaPreviews, String(enabled));
      set({ showMediaPreviews: enabled });
    },
    setEmojiSize: (size) => {
      writeStored(STORAGE_KEYS.emojiSize, size);
      set({ emojiSize: size });
    },
    setNotifySoundEnabled: (enabled) => {
      writeStored(STORAGE_KEYS.notifySoundEnabled, String(enabled));
      set({ notifySoundEnabled: enabled });
    },
    setNotifyNative: (enabled) => {
      writeStored(STORAGE_KEYS.notifyNative, String(enabled));
      set({ notifyNative: enabled });
    },
    setNotifySoundMode: (mode) => {
      writeStored(STORAGE_KEYS.notifySoundMode, mode);
      set({ notifySoundMode: mode });
    },
    setTypingIndicatorEnabled: (enabled) => {
      writeStored(STORAGE_KEYS.typingIndicatorEnabled, String(enabled));
      set({ typingIndicatorEnabled: enabled });
    },
    setStartupPresence: (presence) => {
      if (presence === null) writeStored(STORAGE_KEYS.startupPresence, '');
      else writeStored(STORAGE_KEYS.startupPresence, presence);
      set({ startupPresence: presence });
    },
    setTimeFormat: (format) => {
      writeStored(STORAGE_KEYS.timeFormat, format);
      set({ timeFormat: format });
    },

    setSidebarWidth: (width) => {
      const clamped = clamp(width, SIDEBAR_WIDTH_MIN, SIDEBAR_WIDTH_MAX);
      writeStored(STORAGE_KEYS.sidebarWidth, String(clamped));
      set({ sidebarWidth: clamped });
    },
    resetSidebarWidth: () => {
      writeStored(STORAGE_KEYS.sidebarWidth, String(SIDEBAR_WIDTH_DEFAULT));
      set({ sidebarWidth: SIDEBAR_WIDTH_DEFAULT });
    },
    setMembersWidth: (width) => {
      const clamped = clamp(width, MEMBERS_WIDTH_MIN, MEMBERS_WIDTH_MAX);
      writeStored(STORAGE_KEYS.membersWidth, String(clamped));
      set({ membersWidth: clamped });
    },
    resetMembersWidth: () => {
      writeStored(STORAGE_KEYS.membersWidth, String(MEMBERS_WIDTH_DEFAULT));
      set({ membersWidth: MEMBERS_WIDTH_DEFAULT });
    },
  };
});

/** Dictionnaire actif (hook de commodité). */
import { dictionaries, type Dict } from '../i18n';

export function useT(): Dict {
  const lang = useUi((s) => s.lang);
  return dictionaries[lang];
}
