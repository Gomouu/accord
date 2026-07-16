/**
 * Pont vers l'hôte Tauri : cycle de vie de l'identité et démarrage du nœud
 * embarqué. En mode navigateur (développement UI sans Tauri), une session de
 * secours est lue dans `localStorage` (`accord.dev.session`, écrite à la main
 * à partir du `session.json` d'un démon `accord-noded`).
 */

import { invoke } from '@tauri-apps/api/core';

export type VaultStatus = 'absent' | 'locked';

export interface SessionInfo {
  port: number;
  token: string;
}

export interface CreatedIdentity {
  session: SessionInfo;
  /** Phrase de récupération de 12 mots — affichée une seule fois. */
  recovery_phrase: string;
}

/** Compte local tel qu'exposé au sélecteur de comptes (contrat `AccountMeta`). */
export interface AccountMeta {
  id: string;
  name: string;
  created_ms: number;
  last_used_ms: number;
  is_legacy: boolean;
  pubkey_short: string | null;
}

export interface CreatedAccount {
  session: SessionInfo;
  /** Phrase de récupération de 12 mots — affichée une seule fois. */
  recovery_phrase: string;
  account_id: string;
}

export interface RestoredAccount {
  session: SessionInfo;
  account_id: string;
}

export function isTauri(): boolean {
  return '__TAURI_INTERNALS__' in window;
}

const DEV_SESSION_KEY = 'accord.dev.session';

function devSession(): SessionInfo | null {
  const raw = window.localStorage.getItem(DEV_SESSION_KEY);
  if (raw === null) return null;
  try {
    const parsed: unknown = JSON.parse(raw);
    if (
      typeof parsed === 'object' &&
      parsed !== null &&
      typeof (parsed as SessionInfo).port === 'number' &&
      typeof (parsed as SessionInfo).token === 'string'
    ) {
      return parsed as SessionInfo;
    }
  } catch {
    // Valeur illisible : ignorée.
  }
  return null;
}

export async function vaultStatus(): Promise<VaultStatus> {
  if (!isTauri()) return devSession() ? 'locked' : 'absent';
  return invoke<VaultStatus>('vault_status');
}

export async function createIdentity(passphrase: string): Promise<CreatedIdentity> {
  if (!isTauri()) {
    throw new Error('création indisponible hors Tauri (mode développement)');
  }
  return invoke<CreatedIdentity>('create_identity', { passphrase });
}

export async function restoreIdentity(
  phrase: string,
  passphrase: string,
): Promise<SessionInfo> {
  if (!isTauri()) {
    throw new Error('restauration indisponible hors Tauri (mode développement)');
  }
  return invoke<SessionInfo>('restore_identity', { phrase, passphrase });
}

export async function unlockIdentity(passphrase: string): Promise<SessionInfo> {
  if (!isTauri()) {
    const session = devSession();
    if (session) return session;
    throw new Error('aucune session de développement (accord.dev.session)');
  }
  return invoke<SessionInfo>('unlock', { passphrase });
}

/**
 * Locks the vault without quitting the app: stops the embedded node and wipes
 * its in-memory keys, then returns the fresh vault status so the UI can land
 * on the same screen as a cold start. Outside Tauri (browser development)
 * nothing runs locally, so the status is derived from the dev session alone.
 */
export async function lockIdentity(): Promise<VaultStatus> {
  if (!isTauri()) return devSession() ? 'locked' : 'absent';
  return invoke<VaultStatus>('lock');
}

/** Identifiant du compte de secours en mode navigateur (développement UI). */
const DEV_ACCOUNT_ID = 'dev-session';

/**
 * Liste les comptes locaux connus, du plus récemment utilisé au moins
 * récent — peuple le sélecteur de comptes avant tout déverrouillage. Hors
 * Tauri, dérivée de la session de secours seule (0 ou 1 entrée synthétique) :
 * aucun registre réel n'existe en mode navigateur.
 */
export async function accountsList(): Promise<AccountMeta[]> {
  if (!isTauri()) {
    const session = devSession();
    if (session === null) return [];
    return [
      {
        id: DEV_ACCOUNT_ID,
        name: 'Session de développement',
        created_ms: 0,
        last_used_ms: 0,
        is_legacy: true,
        pubkey_short: null,
      },
    ];
  }
  return invoke<AccountMeta[]>('accounts_list');
}

/**
 * Crée un compte **neuf** (jamais sur le profil actif courant), démarre son
 * nœud et rend la session ainsi que la phrase de récupération à faire noter.
 */
export async function accountCreate(passphrase: string): Promise<CreatedAccount> {
  if (!isTauri()) {
    throw new Error('création indisponible hors Tauri (mode développement)');
  }
  return invoke<CreatedAccount>('account_create', { passphrase });
}

/**
 * Restaure un compte **neuf** depuis sa phrase de récupération (jamais sur
 * le profil actif courant), le scelle sous la nouvelle phrase de passe
 * locale, puis démarre son nœud.
 */
export async function accountRestore(
  phrase: string,
  passphrase: string,
): Promise<RestoredAccount> {
  if (!isTauri()) {
    throw new Error('restauration indisponible hors Tauri (mode développement)');
  }
  return invoke<RestoredAccount>('account_restore', { phrase, passphrase });
}

/**
 * Déverrouille un compte existant du registre et bascule dessus : arrête
 * l'éventuel nœud actif avant de démarrer celui-ci. Le profil actif n'est
 * changé qu'après succès du déverrouillage.
 */
export async function accountUnlock(
  accountId: string,
  passphrase: string,
): Promise<SessionInfo> {
  if (!isTauri()) {
    const session = devSession();
    if (session) return session;
    throw new Error('aucune session de développement (accord.dev.session)');
  }
  return invoke<SessionInfo>('account_unlock', { accountId, passphrase });
}

/**
 * Ferme la session courante : arrête le nœud actif et ramène l'UI au
 * sélecteur de comptes, sans changer le profil actif ni rien effacer sur
 * disque. Rend le statut de coffre frais, comme `lockIdentity`.
 */
export async function sessionClose(): Promise<VaultStatus> {
  if (!isTauri()) return devSession() ? 'locked' : 'absent';
  return invoke<VaultStatus>('session_close');
}

/**
 * Lancement au démarrage : état géré par l'OS (Registre Windows /
 * LaunchAgent macOS / fichier .desktop Linux) via `tauri-plugin-autostart`,
 * jamais un simple indicateur `localStorage` — c'est pourquoi ces fonctions
 * interrogent/écrivent le système à chaque appel plutôt que de refléter une
 * intention locale. Best effort : hors Tauri, ou si la plateforme ne prend
 * pas en charge le lancement au démarrage, échoue silencieusement plutôt que
 * de casser l'onglet Paramètres → Système.
 */
export async function autostartIsEnabled(): Promise<boolean> {
  if (!isTauri()) return false;
  try {
    const { isEnabled } = await import('@tauri-apps/plugin-autostart');
    return await isEnabled();
  } catch {
    return false;
  }
}

/** Active ou désactive le lancement au démarrage ; best effort (voir ci-dessus). */
export async function autostartSetEnabled(enabled: boolean): Promise<void> {
  if (!isTauri()) return;
  try {
    const plugin = await import('@tauri-apps/plugin-autostart');
    if (enabled) await plugin.enable();
    else await plugin.disable();
  } catch {
    // Plateforme non prise en charge ou erreur OS : la case à cocher se
    // resynchronisera sur l'état réel au prochain appel d'`autostartIsEnabled`.
  }
}

/**
 * Crée ou détruit l'icône de la barre des menus/systray, en direct —
 * aucun redémarrage requis. Appelée une fois au montage de l'application avec
 * la préférence persistée (`stores/ui.ts`), puis à chaque bascule du réglage
 * « Garder Accord dans la barre des menus/systray ».
 */
export async function traySetEnabled(enabled: boolean): Promise<void> {
  if (!isTauri()) return;
  try {
    await invoke('tray_set_enabled', { enabled });
  } catch {
    // Best effort : une icône de tray manquante ne doit jamais bloquer l'UI.
  }
}

/**
 * Intercepte la demande de fermeture de la fenêtre principale (croix/Cmd+W) :
 * si `shouldHideOnClose()` répond vrai *au moment de la fermeture*, la
 * fenêtre se masque au lieu de se fermer (« fermer réduit dans la barre des
 * menus ») — d'où un callback plutôt qu'un booléen figé, pour toujours lire
 * la préférence courante sans avoir à réenregistrer l'écouteur à chaque
 * bascule. « Quitter » depuis le menu de la tray (ou Cmd+Q) ne passe pas par
 * cet événement : ces chemins quittent réellement l'application (voir
 * `src-tauri/src/tray.rs`, `app.exit(0)`).
 */
export function registerCloseInterception(shouldHideOnClose: () => boolean): void {
  if (!isTauri()) return;
  void (async () => {
    try {
      const { getCurrentWindow } = await import('@tauri-apps/api/window');
      const fenetre = getCurrentWindow();
      await fenetre.onCloseRequested((event) => {
        if (shouldHideOnClose()) {
          // « Fermer réduit dans la barre des menus » : on masque au lieu de
          // fermer (quitter reste accessible via le menu de la tray).
          event.preventDefault();
          void fenetre.hide();
          return;
        }
        // Sinon : quitter RÉELLEMENT l'application. On ne laisse pas le
        // comportement par défaut de la plateforme s'appliquer car sur macOS
        // fermer la fenêtre ne quitte pas l'app (le process et le nœud
        // restent vivants) — d'où une sortie explicite via l'hôte, cohérente
        // sur macOS comme sur Windows/Linux.
        event.preventDefault();
        void invoke('app_quit');
      });
    } catch {
      // Best effort : indisponible hors Tauri ou erreur de plateforme —
      // la fermeture reprend alors son comportement par défaut.
    }
  })();
}

/** Sections des réglages système ouvrables depuis l'onglet Autorisations. */
export type SystemSettingsSection = 'microphone' | 'notifications' | 'firewall';

/**
 * Ouvre le panneau des réglages système d'une autorisation (micro,
 * notifications, pare-feu). Après un refus, l'OS ne ré-affiche jamais son
 * invite : ce raccourci est le seul recours. Rejette hors Tauri ou sur une
 * plateforme sans panneau connu (Linux) — l'appelant affiche alors l'astuce
 * textuelle.
 */
export async function openSystemSettings(section: SystemSettingsSection): Promise<void> {
  if (!isTauri()) throw new Error('hors Tauri');
  await invoke('ouvrir_reglages_systeme', { section });
}

/** État de l'autorisation micro système (aligné sur AVAuthorizationStatus). */
export type MicPermissionState =
  'granted' | 'denied' | 'undetermined' | 'restricted' | 'unsupported';

/**
 * État RÉEL de l'autorisation micro, sans jamais déclencher l'invite.
 * `unsupported` hors Tauri ou hors macOS — l'UI n'affiche alors pas d'état.
 */
export async function micPermissionState(): Promise<MicPermissionState> {
  if (!isTauri()) return 'unsupported';
  try {
    return await invoke<MicPermissionState>('micro_autorisation_etat');
  } catch {
    return 'unsupported';
  }
}

/**
 * Déclenche l'invite micro système (utile à l'état « indéterminé » seulement)
 * et rend l'issue. Sans invite possible, l'OS répond immédiatement.
 */
export async function micPermissionRequest(): Promise<boolean> {
  if (!isTauri()) throw new Error('hors Tauri');
  return invoke<boolean>('micro_autorisation_demander');
}
