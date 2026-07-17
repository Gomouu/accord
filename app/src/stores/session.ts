/**
 * Cycle de vie de la session : onboarding (création/restauration),
 * déverrouillage, connexion au nœud embarqué, profil local.
 */

import { create } from 'zustand';
import { api, rpc } from '../lib/client';
import type { RpcStatus } from '../lib/rpc';
import type { SelfProfile } from '../lib/api';
import {
  accountCreate,
  accountRestore,
  accountsList,
  accountUnlock,
  createIdentity,
  lockIdentity,
  restoreIdentity,
  sessionClose,
  unlockIdentity,
  type AccountMeta,
  type SessionInfo,
} from '../lib/bridge';
import { clearPendingConversation } from '../lib/notifications';
import { useDms } from './dms';
import { useGroups } from './groups';
import { useFriends } from './friends';
import { useUi } from './ui';

export type { AccountMeta } from '../lib/bridge';

/**
 * Purge les stores account-scoped au verrouillage / changement de compte.
 * Ces stores sont des singletons de module qui SURVIVENT à la transition :
 * sans ce nettoyage, les données du compte précédent (conversations, états de
 * serveurs, contacts) resteraient affichées sous le nouveau compte (fuite
 * inter-comptes). Les PRÉFÉRENCES UI (thème, densité, langue…) sont persistées
 * dans localStorage et ne sont PAS touchées ; seule la vue courante retombe
 * sur « Amis » et toute modale ouverte est fermée. `calls`/`voice`/`typing`
 * se resynchronisent d'eux-mêmes au `ready` du nouveau compte (syncCalls/
 * syncVoice), donc pas besoin de les réinitialiser ici.
 */
function resetAccountScopedStores(): void {
  useDms.setState({
    conversations: {},
    hasMore: {},
    loadingOlder: {},
    pins: {},
    peerRead: {},
  });
  useGroups.setState({
    ids: [],
    states: {},
    messages: {},
    hasMore: {},
    loadingOlder: {},
    pins: {},
    unread: {},
    mentions: {},
    pendingInvites: [],
  });
  useFriends.setState({
    contacts: [],
    loaded: false,
    ownStatus: 'online',
    ownStatusText: null,
  });
  const ui = useUi.getState();
  ui.setView({ kind: 'friends' });
  ui.closeModal();
}

/**
 * `welcome` : sélecteur de comptes (2+ comptes locaux connus, ou atteint
 * volontairement depuis `locked` via le lien « Changer de compte »).
 */
export type Phase =
  'boot' | 'setup' | 'locked' | 'welcome' | 'starting' | 'ready' | 'offline';

/** Bornes du pseudo (contrat profile.set : 2 à 32 caractères). */
export const NAME_MIN = 2;
export const NAME_MAX = 32;

/** Longueur maximale de la bio (contrat profile.set : 2048 caractères). */
export const BIO_MAX = 2048;

/** Longueur maximale des pronoms (contrat profile.set : 40 caractères). */
export const PRONOUNS_MAX = 40;

/** Vrai si le pseudo (une fois épuré) respecte les bornes du contrat. */
export function isValidName(name: string): boolean {
  const trimmed = name.trim();
  return trimmed.length >= NAME_MIN && trimmed.length <= NAME_MAX;
}

/** Nom affichable de l'utilisateur local : pseudo, repli code ami. */
export function selfDisplayName(self: SelfProfile): string {
  if (self.name !== null && self.name.trim() !== '') return self.name;
  return self.friend_code;
}

interface SessionState {
  phase: Phase;
  /**
   * Statut fin du lien RPC, distinct de `phase` : `phase` retombe sur
   * `offline` dès que le lien n'est plus `ready`, mais `link` conserve la
   * nuance (reconnexion automatique en cours vs. lien réellement coupé) pour
   * que le bandeau la reflète et propose une reprise manuelle.
   */
  link: RpcStatus;
  self: SelfProfile | null;
  /** Comptes locaux connus (sélecteur de comptes), du plus récent au moins récent. */
  accounts: AccountMeta[];
  /** Phrase de récupération à afficher UNE fois après création, puis effacée. */
  recoveryPhrase: string | null;
  /** Vrai après création/restauration tant qu'aucun pseudo n'est choisi. */
  askName: boolean;
  error: string | null;
  init: () => Promise<void>;
  create: (passphrase: string) => Promise<void>;
  restore: (phrase: string, passphrase: string) => Promise<void>;
  unlock: (passphrase: string) => Promise<void>;
  /**
   * Logs out without quitting: stops the node (in-memory keys wiped host
   * side), closes the RPC link and lands on the unlock screen, exactly like
   * a fresh launch on an existing vault.
   */
  lock: () => Promise<void>;
  /** Force une tentative de reconnexion immédiate (bouton du bandeau hors-ligne). */
  reconnect: () => void;
  /** Rafraîchit `accounts` (best-effort) depuis le registre local. */
  loadAccounts: () => Promise<void>;
  /** Depuis l'écran de déverrouillage à compte unique : bascule vers le sélecteur de comptes. */
  goToWelcome: () => Promise<void>;
  /** Crée un compte **neuf** (jamais sur le profil actif courant) — sélecteur de comptes. */
  createAccount: (passphrase: string) => Promise<void>;
  /** Restaure un compte **neuf** depuis sa phrase de récupération — sélecteur de comptes. */
  restoreAccount: (phrase: string, passphrase: string) => Promise<void>;
  /** Déverrouille un compte existant du registre et bascule dessus. */
  unlockAccount: (accountId: string, passphrase: string) => Promise<void>;
  activateAccount: (accountId: string, passphrase: string) => Promise<void>;
  /**
   * Change de compte sans quitter l'application : ferme la session active
   * (nœud arrêté, secrets en mémoire effacés côté hôte) et ramène l'UI au
   * sélecteur de comptes, avec la liste rafraîchie.
   */
  switchAccount: () => Promise<void>;
  ackRecoveryPhrase: () => void;
  /** Définit le pseudo (profile.set) puis rafraîchit le profil local. */
  setName: (name: string) => Promise<void>;
  /** Définit la bio (profile.set ; chaîne vide = effacer) puis rafraîchit. */
  setBio: (bio: string) => Promise<void>;
  /** Définit les pronoms (profile.set ; chaîne vide = effacer) puis rafraîchit. */
  setPronouns: (pronouns: string) => Promise<void>;
  /** Fixe ou efface la couleur d'accent (profile.set ; `null` = effacer). */
  setAccentColor: (color: number | null) => Promise<void>;
  /** Fixe ou efface la couleur de fond de bannière (profile.set ; `null` = effacer). */
  setBannerColor: (color: number | null) => Promise<void>;
  /** Fixe ou efface la décoration d'avatar (profile.set ; `null` = effacer). */
  setAvatarDecoration: (id: string | null) => Promise<void>;
  /** Fixe ou efface l'effet de profil (profile.set ; `null` = effacer). */
  setProfileEffect: (id: string | null) => Promise<void>;
  setProfileFrame: (id: string | null) => Promise<void>;
  /**
   * Publie l'avatar (profile.set_avatar, PNG/JPEG/WebP en base64) puis
   * rafraîchit le profil local ; `null` retire l'avatar.
   */
  setAvatar: (dataB64: string | null, mime?: string) => Promise<void>;
  /**
   * Publie la bannière de profil (profile.set_banner, image paysage PNG/JPEG/
   * WebP en base64) puis rafraîchit le profil local ; `null` retire la bannière.
   */
  setBanner: (dataB64: string | null, mime?: string) => Promise<void>;
  /** Écarte l'écran « Choisis ton pseudo » sans définir de pseudo. */
  skipNamePrompt: () => void;
}

async function attach(session: SessionInfo): Promise<SelfProfile> {
  await rpc.connect(session.port, session.token);
  return api.identitySelf();
}

/**
 * Écran d'accueil selon le nombre de comptes locaux connus : aucun → création
 * (`setup`), un seul → déverrouillage direct (`locked`, comme aujourd'hui),
 * deux ou plus → sélecteur de comptes (`welcome`).
 */
function phaseForAccountCount(count: number): 'setup' | 'locked' | 'welcome' {
  if (count === 0) return 'setup';
  if (count === 1) return 'locked';
  return 'welcome';
}

export const useSession = create<SessionState>((set) => {
  rpc.onStatus((status) => {
    // `link` suit toujours le statut fin ; `phase` ne bascule ready/offline
    // que depuis ces deux états (jamais depuis boot/setup/starting…).
    set((s) => {
      if (s.phase !== 'ready' && s.phase !== 'offline') return { ...s, link: status };
      return { ...s, link: status, phase: status === 'ready' ? 'ready' : 'offline' };
    });
  });

  return {
    phase: 'boot',
    link: 'idle',
    self: null,
    accounts: [],
    recoveryPhrase: null,
    askName: false,
    error: null,

    init: async () => {
      try {
        const accounts = await accountsList();
        set({ accounts, phase: phaseForAccountCount(accounts.length), error: null });
      } catch (e) {
        set({ phase: 'setup', error: e instanceof Error ? e.message : String(e) });
      }
    },

    create: async (passphrase) => {
      set({ phase: 'starting', error: null });
      try {
        const created = await createIdentity(passphrase);
        const self = await attach(created.session);
        set({
          phase: 'ready',
          self,
          recoveryPhrase: created.recovery_phrase,
          askName: self.name === null,
        });
      } catch (e) {
        set({
          phase: 'setup',
          error: e instanceof Error ? e.message : String(e),
        });
      }
    },

    restore: async (phrase, passphrase) => {
      set({ phase: 'starting', error: null });
      try {
        const session = await restoreIdentity(phrase, passphrase);
        const self = await attach(session);
        set({ phase: 'ready', self, recoveryPhrase: null, askName: self.name === null });
      } catch (e) {
        set({
          phase: 'setup',
          error: e instanceof Error ? e.message : String(e),
        });
      }
    },

    unlock: async (passphrase) => {
      set({ phase: 'starting', error: null });
      try {
        const session = await unlockIdentity(passphrase);
        const self = await attach(session);
        // Pas d'invite au pseudo au déverrouillage : compte déjà établi.
        set({ phase: 'ready', self, recoveryPhrase: null, askName: false });
      } catch (e) {
        set({
          phase: 'locked',
          error: e instanceof Error ? e.message : String(e),
        });
      }
    },

    lock: async () => {
      // Land on the unlock screen first, like a cold start on an existing
      // vault: the RPC 'closed' status below must never bounce the phase
      // through 'offline' (the onStatus guard only touches ready/offline).
      set({
        phase: 'locked',
        self: null,
        recoveryPhrase: null,
        askName: false,
        error: null,
      });
      resetAccountScopedStores();
      clearPendingConversation();
      rpc.close();
      try {
        const status = await lockIdentity();
        // Vault file gone meanwhile: fall back to onboarding, like init().
        if (status === 'absent') set({ phase: 'setup' });
      } catch (e) {
        // Stay on the unlock screen: a later unlock restarts the node and
        // replaces any node that failed to stop.
        set({ error: e instanceof Error ? e.message : String(e) });
      }
    },

    reconnect: () => {
      rpc.retryNow();
    },

    loadAccounts: async () => {
      try {
        const accounts = await accountsList();
        set({ accounts, error: null });
      } catch (e) {
        set({ error: e instanceof Error ? e.message : String(e) });
      }
    },

    goToWelcome: async () => {
      set({ phase: 'welcome', error: null });
      try {
        const accounts = await accountsList();
        set({ accounts });
      } catch {
        // Liste déjà en mémoire depuis `init()` : on continue avec elle.
      }
    },

    createAccount: async (passphrase) => {
      set({ phase: 'starting', error: null });
      try {
        const created = await accountCreate(passphrase);
        const self = await attach(created.session);
        set({
          phase: 'ready',
          self,
          recoveryPhrase: created.recovery_phrase,
          askName: self.name === null,
        });
      } catch (e) {
        set({ phase: 'welcome', error: e instanceof Error ? e.message : String(e) });
      }
    },

    restoreAccount: async (phrase, passphrase) => {
      set({ phase: 'starting', error: null });
      try {
        const restored = await accountRestore(phrase, passphrase);
        const self = await attach(restored.session);
        set({ phase: 'ready', self, recoveryPhrase: null, askName: self.name === null });
      } catch (e) {
        set({ phase: 'welcome', error: e instanceof Error ? e.message : String(e) });
      }
    },

    unlockAccount: async (accountId, passphrase) => {
      set({ phase: 'starting', error: null });
      try {
        const session = await accountUnlock(accountId, passphrase);
        const self = await attach(session);
        // Pas d'invite au pseudo au déverrouillage : compte déjà établi.
        set({ phase: 'ready', self, recoveryPhrase: null, askName: false });
      } catch (e) {
        set({ phase: 'welcome', error: e instanceof Error ? e.message : String(e) });
      }
    },

    activateAccount: async (accountId, passphrase) => {
      set({ error: null });
      let activated = false;
      try {
        const session = await accountUnlock(accountId, passphrase);
        activated = true;
        set({
          phase: 'starting',
          self: null,
          recoveryPhrase: null,
          askName: false,
        });
        resetAccountScopedStores();
        clearPendingConversation();
        rpc.close();
        const self = await attach(session);
        set({ phase: 'ready', self, recoveryPhrase: null, askName: false, error: null });
      } catch (e) {
        const error = e instanceof Error ? e.message : String(e);
        if (activated) {
          rpc.close();
          await sessionClose().catch(() => undefined);
          set({ phase: 'welcome', self: null, error });
        } else set({ error });
        throw e;
      }
    },

    switchAccount: async () => {
      // Land on the welcome screen first, same discipline as `lock()`: the
      // RPC 'closed' status below must never bounce the phase through
      // 'offline' (the onStatus guard only touches ready/offline).
      set({
        phase: 'welcome',
        self: null,
        recoveryPhrase: null,
        askName: false,
        error: null,
      });
      resetAccountScopedStores();
      clearPendingConversation();
      rpc.close();
      try {
        await sessionClose();
      } catch (e) {
        set({ error: e instanceof Error ? e.message : String(e) });
      }
      try {
        const accounts = await accountsList();
        set({ accounts });
      } catch {
        // Best effort : la liste déjà en mémoire reste affichée.
      }
    },

    ackRecoveryPhrase: () => set({ recoveryPhrase: null }),

    setName: async (name) => {
      await api.profileSet({ name: name.trim() });
      // Rafraîchit le profil local pour refléter le pseudo partout.
      const self = await api.identitySelf();
      set({ self, askName: false });
    },

    setBio: async (bio) => {
      await api.profileSet({ bio: bio.trim() });
      const self = await api.identitySelf();
      set({ self });
    },

    setPronouns: async (pronouns) => {
      await api.profileSet({ pronouns: pronouns.trim() });
      const self = await api.identitySelf();
      set({ self });
    },

    setAccentColor: async (color) => {
      await api.profileSet({ accent_color: color });
      const self = await api.identitySelf();
      set({ self });
    },

    setBannerColor: async (color) => {
      await api.profileSet({ banner_color: color });
      const self = await api.identitySelf();
      set({ self });
    },

    setAvatarDecoration: async (id) => {
      await api.profileSet({ avatar_decoration: id });
      const self = await api.identitySelf();
      set({ self });
    },

    setProfileEffect: async (id) => {
      await api.profileSet({ profile_effect: id });
      const self = await api.identitySelf();
      set({ self });
    },

    setProfileFrame: async (id) => {
      await api.profileSet({ profile_frame: id });
      const self = await api.identitySelf();
      set({ self });
    },

    setAvatar: async (dataB64, mime) => {
      await api.profileSetAvatar(dataB64, mime);
      const self = await api.identitySelf();
      set({ self });
    },

    setBanner: async (dataB64, mime) => {
      await api.profileSetBanner(dataB64, mime);
      const self = await api.identitySelf();
      set({ self });
    },

    skipNamePrompt: () => set({ askName: false }),
  };
});
