/**
 * Notifications natives via le plugin Tauri (import dynamique, repli
 * silencieux hors Tauri) et règle d'éligibilité pure — testable en isolation.
 * Confidentialité : le contenu des messages n'est jamais transmis, seuls des
 * identifiants transitent dans les événements du nœud.
 */

import { isTauri } from './bridge';
import { useUi } from '../stores/ui';

export type NotifyKind = 'dm' | 'group';

/** Réglages de notification (store d'interface, persistés). */
export interface NotifyPrefs {
  dms: boolean;
  groups: boolean;
  onlyWhenUnfocused: boolean;
}

/**
 * Décide si un message entrant mérite une notification native, selon les
 * réglages, le focus de la fenêtre et l'auteur (jamais ses propres messages).
 */
export function isNotificationEligible(options: {
  kind: NotifyKind;
  prefs: NotifyPrefs;
  windowFocused: boolean;
  isOwnMessage: boolean;
  /**
   * Vrai si la conversation/le salon visé est en sourdine (voir
   * `stores/mute.ts#isConversationSilenced`) ; absent = jamais en sourdine
   * (comportement historique, avant l'ajout de la fonctionnalité).
   */
  muted?: boolean;
}): boolean {
  const { kind, prefs, windowFocused, isOwnMessage, muted = false } = options;
  if (isOwnMessage) return false;
  if (muted) return false;
  if (kind === 'dm' && !prefs.dms) return false;
  if (kind === 'group' && !prefs.groups) return false;
  if (prefs.onlyWhenUnfocused && windowFocused) return false;
  return true;
}

/**
 * Éligibilité du son de notification — distincte de `isNotificationEligible`
 * (notification native, régie par les réglages MP/groupes/arrière-plan) : le
 * blip in-app joue toujours sur un message entrant, sauf pour ses propres
 * messages, en mode Ne pas déranger, ou quand la fenêtre a le focus sur
 * exactement cette conversation/salon (la pastille seule suffit alors).
 */
/**
 * Filtrage additionnel choisi par l'utilisateur (Paramètres → Notifications
 * → « Jouer un son pour ») : tous les messages, mentions seulement, ou aucun.
 */
export type NotifySoundMode = 'all' | 'mentionsOnly' | 'none';

/** Réglage « Ne pas déranger programmé » : plage d'heures locales (0-23). */
export interface QuietHours {
  enabled: boolean;
  /** Heure de début (incluse), 0-23. */
  start: number;
  /** Heure de fin (exclue), 0-23. */
  end: number;
}

/**
 * Vrai si `now` tombe dans la plage d'heures calmes active. Gère le passage
 * de minuit (ex. 22 → 8) ; une plage vide (`start === end`) ou désactivée ne
 * s'applique jamais. Fonction pure : l'heure est fournie par l'appelant.
 */
export function isWithinQuietHours(q: QuietHours, now: Date): boolean {
  if (!q.enabled || q.start === q.end) return false;
  const h = now.getHours();
  return q.start < q.end ? h >= q.start && h < q.end : h >= q.start || h < q.end;
}

export interface SoundEligibilityOptions {
  isOwnMessage: boolean;
  /** Vrai si la conversation/le salon visé par le message est celui affiché. */
  isDisplayedConversation: boolean;
  windowFocused: boolean;
  /** Vrai si le statut de présence local est Ne pas déranger. */
  dnd: boolean;
  /** Mode de filtrage courant ; absent = tous les messages (comportement historique). */
  mode?: NotifySoundMode;
  /** Vrai si le message mentionne l'utilisateur ; n'importe que pour `mentionsOnly`. */
  isMention?: boolean;
  /**
   * Vrai si le serveur ou le salon visé est en sourdine (voir
   * `stores/mute.ts#isConversationSilenced`) ; absent = jamais en sourdine
   * (comportement historique, avant l'ajout de la fonctionnalité). MVP
   * volontairement simple : une sourdine coupe le son même pour une mention
   * (pas d'exception « les mentions notifient quand même »).
   */
  muted?: boolean;
}

/** Décide si le blip de notification doit jouer pour un message entrant. */
export function isSoundEligible(options: SoundEligibilityOptions): boolean {
  const {
    isOwnMessage,
    isDisplayedConversation,
    windowFocused,
    dnd,
    mode = 'all',
    isMention = false,
    muted = false,
  } = options;
  if (isOwnMessage) return false;
  if (muted) return false;
  if (dnd) return false;
  if (mode === 'none') return false;
  if (mode === 'mentionsOnly' && !isMention) return false;
  if (isDisplayedConversation && windowFocused) return false;
  return true;
}

/** État de l'autorisation système (`unavailable` hors Tauri). */
export type NotificationPermission = 'granted' | 'denied' | 'unavailable';

/** Interroge l'autorisation courante sans rien demander à l'utilisateur. */
export async function queryNotificationPermission(): Promise<NotificationPermission> {
  if (!isTauri()) return 'unavailable';
  try {
    const { isPermissionGranted } = await import('@tauri-apps/plugin-notification');
    return (await isPermissionGranted()) ? 'granted' : 'denied';
  } catch {
    return 'unavailable';
  }
}

/** Demande l'autorisation système (invite native le cas échéant). */
export async function requestNotificationPermission(): Promise<NotificationPermission> {
  if (!isTauri()) return 'unavailable';
  try {
    const plugin = await import('@tauri-apps/plugin-notification');
    if (await plugin.isPermissionGranted()) return 'granted';
    const outcome = await plugin.requestPermission();
    return outcome === 'granted' ? 'granted' : 'denied';
  } catch {
    return 'unavailable';
  }
}

/**
 * Envoie une notification native si l'autorisation est accordée. Best effort :
 * hors Tauri ou sans autorisation, ne fait rien (aucune erreur remontée).
 * Rend `true` si la notification a réellement été envoyée. Respecte le
 * réglage global « Notifications natives » (Paramètres → Notifications),
 * distinct des réglages fins MP/groupes déjà appliqués par l'appelant
 * (`isNotificationEligible`).
 */
export async function sendNativeNotification(
  title: string,
  body: string,
): Promise<boolean> {
  if (!useUi.getState().notifyNative) return false;
  if (!isTauri()) return false;
  try {
    const plugin = await import('@tauri-apps/plugin-notification');
    if (!(await plugin.isPermissionGranted())) return false;
    plugin.sendNotification({ title, body });
    return true;
  } catch {
    // Best effort : une notification manquée ne doit pas casser l'app.
    return false;
  }
}

/**
 * Conversation that triggered a native notification. Structurally compatible
 * with the UI store's `View` union, so `useUi.setView(ref)` routes directly
 * (home rail + DM for `dm`, server rail + channel for `group`).
 */
export type ConversationRef =
  { kind: 'dm'; peer: string } | { kind: 'group'; groupId: string; channelId: string };

/**
 * Notification click → open conversation: per-platform reality check
 * (tauri-plugin-notification 2.3.x, desktop backends inspected).
 *
 * The plugin's `onAction`/`onNotificationReceived` listeners are wired by the
 * iOS/Android implementations only. On desktop the Rust side is
 * fire-and-forget (`notify-rust` on macOS/Linux, `winrt` toasts on Windows):
 * no click, action or dismiss event ever reaches the webview, on any desktop
 * platform. What the OS does on click:
 *   - macOS: activates the app (main window regains focus);
 *   - Windows: activates the app (toast activation);
 *   - Linux: depends on the notification daemon — often dismiss only, the
 *     window may not be focused at all.
 *
 * Fallback implemented here: when a notification is shown while the window is
 * unfocused, the target conversation is remembered for a short while; the
 * next window `focus` event consumes it and navigates there. On macOS and
 * Windows this makes a notification click open the conversation; on Linux it
 * degrades to "first refocus after a notification opens it". If the plugin
 * ever emits desktop click events, replace this registry with `onAction`.
 */
export const PENDING_NAVIGATION_TTL_MS = 120_000;

let pendingConversation: { ref: ConversationRef; at: number } | null = null;

/** Arms navigation-on-next-focus towards the notified conversation. */
export function rememberNotifiedConversation(
  ref: ConversationRef,
  now = Date.now(),
): void {
  pendingConversation = { ref, at: now };
}

/**
 * Consumes (and returns) the pending conversation if the notification is
 * recent enough; `null` otherwise. At most one navigation per notification.
 */
export function takePendingConversation(now = Date.now()): ConversationRef | null {
  const pending = pendingConversation;
  pendingConversation = null;
  if (pending === null) return null;
  if (now - pending.at > PENDING_NAVIGATION_TTL_MS) return null;
  return pending.ref;
}

/** Forgets any pending navigation (logout, tests). */
export function clearPendingConversation(): void {
  pendingConversation = null;
}

/**
 * Total pour la pastille du dock (macOS) / de la barre des tâches : messages
 * privés non lus (toujours « pour soi ») plus mentions de serveur (les salons
 * ne badgent que sur mention, comme la sourdine par défaut). Pur : testable
 * sans la fenêtre Tauri.
 */
export function unreadBadgeTotal(
  dmUnread: number,
  groupMentions: Readonly<Record<string, number>>,
): number {
  return Object.values(groupMentions).reduce((sum, n) => sum + n, dmUnread);
}
