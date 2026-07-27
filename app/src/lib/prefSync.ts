/**
 * Miroir des préférences de COMPTE entre le `localStorage` et le nœud
 * (feuille de route §17.4, canal filaire `0x21 SELF_PREF`, SPEC §6.6).
 *
 * ## Pourquoi une couche en plus, et pas une réécriture du store
 *
 * `stores/ui.ts` s'hydrate **synchroniquement** depuis `localStorage` à la
 * construction du store, alors que le nœud est asynchrone et pas encore
 * connecté à cet instant. Faire du nœud la source de vérité imposerait donc un
 * état « préférences en cours de chargement » dans les 1 100 lignes du store et
 * dans tout ce qui les lit — pour une valeur qui, neuf fois sur dix, est déjà la
 * bonne.
 *
 * Le `localStorage` reste donc la source de vérité LOCALE, pour une hydratation
 * instantanée et sans clignotement, et le nœud n'est qu'un **miroir** :
 *
 * - toute écriture de préférence synchronisable part aussi vers le nœud
 *   ([`mirrorSyncedPref`], branché dans l'unique `writeStored` du store) ;
 * - au démarrage, on demande au nœud son état de compte et on n'applique que ce
 *   qui est **plus récent** que ce que cette machine a écrit
 *   ([`hydrateSyncedPrefs`]).
 *
 * ## L'horodatage local, et pourquoi il est indispensable
 *
 * Comparer les VALEURS ne suffirait pas : « différent » ne dit pas « plus
 * récent ». On garde donc, par clé, l'horodatage que le nœud a estampillé au
 * moment où cette machine a poussé la valeur. Une valeur du nœud plus récente
 * que lui vient forcément d'un autre appareil, et gagne.
 */

import { api } from './client';

/**
 * Préférences de compte, par leur clé `localStorage` — les mêmes chaînes que
 * `STORAGE_KEYS` dans `stores/ui.ts`, et les mêmes que la liste blanche de
 * `accord_core::prefs::SYNCED_KEYS` côté nœud.
 *
 * 🔒 Cette liste ne fait pas foi : c'est celle du nœud qui décide, et une clé
 * ajoutée ici sans y être là-bas serait simplement ignorée à la réception. Le
 * doublon est assumé — il évite un aller-retour réseau pour savoir s'il faut
 * faire un aller-retour réseau — mais la motivation des refus, elle, n'est
 * écrite qu'à un seul endroit : `crates/accord-core/src/prefs.rs`.
 */
export const SYNCED_PREF_KEYS = [
  'accord.lang',
  'accord.theme',
  'accord.theme.custom',
  'accord.density',
  'accord.timeFormat',
  'accord.appearance.fontUi',
  'accord.media.emojiSize',
  'accord.media.showPreviews',
  'accord.notifyDms',
  'accord.notifyGroups',
  'accord.notifyOnlyUnfocused',
  'accord.notify.soundEnabled',
  'accord.notify.soundMode',
  'accord.privacy.typingIndicator',
] as const;

export type SyncedPrefKey = (typeof SYNCED_PREF_KEYS)[number];

/** Une préférence de compte telle que `prefs.list` la rend. */
export interface SyncedPref {
  key: string;
  value: string;
  at_ms: number;
}

/**
 * Horodatages, par clé, des valeurs que CETTE machine a poussées au nœud.
 * Un seul enregistrement JSON plutôt qu'une clé par préférence : ce n'est pas
 * une préférence, c'est une pièce comptable interne, et la garder d'un bloc
 * évite d'inventer une seconde convention de nommage dans `localStorage`.
 */
const AT_STORAGE_KEY = 'accord.prefsync.at';

/** Lecture `localStorage` tolérante (stockage indisponible ⇒ `null`). */
function readStored(key: string): string | null {
  try {
    return window.localStorage.getItem(key);
  } catch {
    return null;
  }
}

/** Écriture `localStorage` tolérante. */
function writeStored(key: string, value: string): void {
  try {
    window.localStorage.setItem(key, value);
  } catch {
    // Best effort, comme dans le store : la préférence reste appliquée pour
    // la session en cours.
  }
}

/** Vrai si `key` est une préférence de compte. */
export function isSyncedPrefKey(key: string): key is SyncedPrefKey {
  return (SYNCED_PREF_KEYS as readonly string[]).includes(key);
}

/** Table des horodatages locaux (illisible ou corrompue ⇒ vide). */
function readAtMap(): Record<string, number> {
  const raw = readStored(AT_STORAGE_KEY);
  if (raw === null) return {};
  try {
    const parsed: unknown = JSON.parse(raw);
    if (parsed === null || typeof parsed !== 'object' || Array.isArray(parsed)) return {};
    const out: Record<string, number> = {};
    for (const [key, value] of Object.entries(parsed as Record<string, unknown>)) {
      if (typeof value === 'number' && Number.isFinite(value)) out[key] = value;
    }
    return out;
  } catch {
    return {};
  }
}

/**
 * Horodatage de la dernière valeur poussée par CETTE machine pour `key`.
 * Absent ⇒ `0`, ce qui laisse gagner n'importe quelle valeur du nœud : c'est le
 * bon défaut pour un appareil fraîchement appairé, qui n'a rien décidé et doit
 * tout adopter.
 */
export function prefSyncedAt(key: string): number {
  return readAtMap()[key] ?? 0;
}

/** Mémorise l'horodatage retenu pour `key`. */
export function recordPrefSyncedAt(key: string, atMs: number): void {
  writeStored(AT_STORAGE_KEY, JSON.stringify({ ...readAtMap(), [key]: atMs }));
}

/**
 * Applique une valeur de préférence au store d'interface. Fourni par
 * `stores/ui.ts`, qui détient les validateurs et les setters.
 *
 * L'inversion évite un cycle d'import : ce module ne connaît pas le store, le
 * store ne connaît de ce module que des fonctions.
 */
type PrefApplier = (key: SyncedPrefKey, value: string) => void;

let applier: PrefApplier | null = null;

/** Branche l'applicateur du store (appelé une fois, au chargement de `ui.ts`). */
export function registerPrefApplier(fn: PrefApplier): void {
  applier = fn;
}

/**
 * Suspend le miroir pendant qu'on applique une valeur VENUE du nœud.
 *
 * 🔒 Sans cette garde, appliquer une préférence reçue passerait par les setters
 * du store, donc par `writeStored`, donc la renverrait au nœud — avec un
 * horodatage neuf. Deux appareils se la renverraient indéfiniment, chacun la
 * redatant : une boucle de trafic que rien n'arrête, et qui au passage écrase
 * l'horodatage d'origine.
 */
let miroirSuspendu = false;

/**
 * Pousse une préférence vers le nœud, qui l'annonce aux autres appareils.
 *
 * Best-effort et silencieux en cas d'échec : le réglage a déjà pris sur la
 * machine que l'utilisateur regarde (`localStorage` a été écrit juste avant),
 * et un toast d'erreur signalerait un problème là où l'utilisateur voit son
 * choix appliqué. Hors ligne, l'appel RPC rejette immédiatement et la
 * préférence repartira au prochain changement.
 */
export function mirrorSyncedPref(key: string, value: string): void {
  if (miroirSuspendu || !isSyncedPrefKey(key)) return;
  // ⚠️ L'appel est enveloppé plutôt qu'appelé directement, et le `.catch()`
  // seul ne suffisait pas : il n'attrape que les rejets, pas un lancer
  // SYNCHRONE. Or écrire une préférence passe désormais par le store, donc par
  // ici, donc par le réseau — et tout test qui touche un réglage synchronisé
  // sans avoir mocké `setPref` explosait en `TypeError`, à des kilomètres du
  // sujet qu'il testait. Faire remonter ça jusqu'à l'appelant contredirait de
  // toute façon la promesse écrite ci-dessus : le réglage a déjà pris
  // localement, et ne pas pouvoir tenter le miroir est un échec de miroir
  // comme un autre.
  void Promise.resolve()
    .then(() => api.setPref(key, value))
    .then((atMs) => {
      recordPrefSyncedAt(key, atMs);
    })
    .catch(() => {
      // Sans effet observable : voir le commentaire ci-dessus.
    });
}

/**
 * Applique une préférence venue du compte si elle est plus récente que ce que
 * cette machine a poussé. Rend `true` si elle a été appliquée.
 *
 * Comparaison **strictement** supérieure, comme côté nœud : à horodatage égal
 * il n'y a rien à départager, et garder l'existant rend l'opération idempotente
 * (le même événement reçu deux fois n'a d'effet qu'une fois).
 */
export function applyRemotePref(key: string, value: string, atMs: number): boolean {
  if (!isSyncedPrefKey(key) || applier === null) return false;
  if (atMs <= prefSyncedAt(key)) return false;
  miroirSuspendu = true;
  try {
    applier(key, value);
  } finally {
    miroirSuspendu = false;
  }
  recordPrefSyncedAt(key, atMs);
  return true;
}

/**
 * Au démarrage : demande au nœud l'état de compte des préférences et adopte ce
 * qui est plus récent. Best-effort — un nœud injoignable laisse simplement
 * cette machine sur ses propres valeurs, ce qui est exactement ce qu'elle
 * affichait déjà.
 */
export async function hydrateSyncedPrefs(): Promise<void> {
  let prefs: SyncedPref[];
  try {
    prefs = await api.listPrefs();
  } catch {
    return;
  }
  for (const pref of prefs) {
    applyRemotePref(pref.key, pref.value, pref.at_ms);
  }
}
