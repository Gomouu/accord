/**
 * État d'interface : vue active, modales, toasts, langue et préférences
 * d'apparence (thème, densité, taille de police). Les préférences sont
 * persistées dans localStorage et appliquées immédiatement sur la racine du
 * document (`data-theme`, `data-density`, `font-size`), où les variables CSS
 * du design system les consomment.
 */

import { create } from 'zustand';
import { detectLang, type Lang } from '../i18n';
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
  | { kind: 'serverSettings'; groupId: string };

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

export type Theme = 'dark' | 'light';
export type Density = 'comfortable' | 'compact';

/** Échelles de police proposées, en pourcentage de la taille de base. */
export const FONT_SCALES = [90, 100, 110, 120] as const;
export type FontScale = (typeof FONT_SCALES)[number];

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
  return value === 'dark' || value === 'light';
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

function initialLang(): Lang {
  const stored = readStored(STORAGE_KEYS.lang);
  return isLang(stored) ? stored : detectLang();
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
}

const TOAST_LIFETIME_MS = 5000;
let nextToastId = 1;

export const useUi = create<UiState>((set) => {
  const theme = initialTheme();
  const density = initialDensity();
  const fontScale = initialFontScale();
  applyTheme(theme);
  applyDensity(density);
  applyFontScale(fontScale);

  return {
    view: { kind: 'friends' },
    modal: null,
    jump: null,
    profile: null,
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
    lastChannelByServer: loadLastChannelByServer(),
    lastDmPeer: loadLastDm(),

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
  };
});

/** Dictionnaire actif (hook de commodité). */
import { dictionaries, type Dict } from '../i18n';

export function useT(): Dict {
  const lang = useUi((s) => s.lang);
  return dictionaries[lang];
}
