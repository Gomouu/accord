/**
 * Contacts : liste, ajout par code ami, réponses, retrait, blocage, profils
 * amis et présence riche (statut + texte personnalisé, le sien comme celui
 * des amis).
 */

import { create } from 'zustand';
import { api, rpc } from '../lib/client';
import type { Contact, OwnPresenceStatus, PresenceStatus } from '../lib/api';

/** Charge utile d'`event.profile` : profil annoncé par un ami. */
export interface ProfilAmi {
  pubkey: string;
  name: string;
  bio: string | null;
  avatar: string | null;
  banner: string | null;
  /** Absent : nœud pair ancien, pronoms inconnus (conservés tels quels). */
  pronouns?: string | null;
  /** Absent : nœud pair ancien, accent inconnu (conservé tel quel). */
  accent_color?: number | null;
  /** Absent : nœud pair ancien, couleur de bannière inconnue (conservée telle quelle). */
  banner_color?: number | null;
  /** Absent : nœud pair ancien, décoration inconnue (conservée telle quelle). */
  avatar_decoration?: string | null;
  /** Absent : nœud pair ancien, effet inconnu (conservé tel quel). */
  profile_effect?: string | null;
  profile_frame?: string | null;
}

interface FriendsState {
  contacts: Contact[];
  loaded: boolean;
  /** Statut de présence local (persisté côté nœud, `friends.get_status`). */
  ownStatus: OwnPresenceStatus;
  /** Texte de statut personnalisé local, ou `null`. */
  ownStatusText: string | null;
  load: () => Promise<void>;
  addByCode: (code: string, myDisplayName: string) => Promise<void>;
  respond: (pubkey: string, accept: boolean) => Promise<void>;
  /**
   * Marque la conversation lue jusqu'à `lamport` (dernier message affiché)
   * puis recharge la liste pour faire tomber le compteur `unread`.
   */
  markRead: (pubkey: string, lamport: number) => Promise<void>;
  /** Retire une amitié établie (l'historique MP est conservé). */
  remove: (pubkey: string) => Promise<void>;
  block: (pubkey: string) => Promise<void>;
  unblock: (pubkey: string) => Promise<void>;
  /** Recharge le statut local persisté (`friends.get_status`). */
  loadOwnStatus: () => Promise<void>;
  /**
   * Fixe le statut local (persisté et diffusé aux amis). `custom` : absent =
   * texte inchangé, chaîne vide = effacé, sinon remplacé.
   */
  setOwnStatus: (status: OwnPresenceStatus, custom?: string) => Promise<void>;
  /**
   * Applique un profil annoncé (`event.profile`) au contact correspondant —
   * pseudo, bio, avatar et bannière remplacent les valeurs connues
   * (`null` = effacé). Contact inconnu : ignoré (le nœud n'annonce que des amis).
   */
  applyProfile: (profil: ProfilAmi) => void;
  /**
   * Reflète un changement de présence (`event.presence`) d'un ami. `status`
   * et `statusText` (présence riche) sont ignorés s'ils sont absents —
   * l'appel historique à deux arguments ne garde que `online`.
   */
  applyPresence: (
    pubkey: string,
    online: boolean,
    status?: PresenceStatus,
    statusText?: string | null,
  ) => void;
}

export const useFriends = create<FriendsState>((set, get) => ({
  contacts: [],
  loaded: false,
  ownStatus: 'online',
  ownStatusText: null,

  load: async () => {
    const { contacts } = await api.friendsList();
    set({ contacts, loaded: true });
  },

  addByCode: async (code, myDisplayName) => {
    const { pubkey } = await api.friendsResolve(code.trim());
    await api.friendsRequest(pubkey, myDisplayName);
    await get().load();
  },

  respond: async (pubkey, accept) => {
    await api.friendsRespond(pubkey, accept);
    await get().load();
  },

  markRead: async (pubkey, lamport) => {
    await api.dmMarkRead(pubkey, lamport);
    await get().load();
  },

  remove: async (pubkey) => {
    await api.friendsRemove(pubkey);
    await get().load();
  },

  block: async (pubkey) => {
    await api.friendsBlock(pubkey);
    await get().load();
  },

  unblock: async (pubkey) => {
    await api.friendsUnblock(pubkey);
    await get().load();
  },

  loadOwnStatus: async () => {
    const { status, custom } = await api.friendsGetStatus();
    set({ ownStatus: status, ownStatusText: custom });
  },

  setOwnStatus: async (status, custom) => {
    await api.friendsSetStatus(status, custom);
    set((s) => ({
      ownStatus: status,
      // Absent : texte inchangé ; vide (après nettoyage) : effacé.
      ownStatusText:
        custom === undefined
          ? s.ownStatusText
          : custom.trim() === ''
            ? null
            : custom.trim(),
    }));
  },

  applyProfile: (profil) => {
    set((s) => ({
      contacts: s.contacts.map((c) =>
        c.pubkey === profil.pubkey
          ? {
              ...c,
              display_name: profil.name,
              bio: profil.bio,
              avatar: profil.avatar,
              banner: profil.banner,
              // Pronoms/couleurs/décoration/effet absents (nœud pair ancien) :
              // valeurs connues conservées telles quelles, comme la présence riche.
              ...(profil.pronouns !== undefined ? { pronouns: profil.pronouns } : {}),
              ...('accent_color' in profil ? { accent_color: profil.accent_color } : {}),
              ...('banner_color' in profil ? { banner_color: profil.banner_color } : {}),
              ...('avatar_decoration' in profil
                ? { avatar_decoration: profil.avatar_decoration }
                : {}),
              ...('profile_effect' in profil
                ? { profile_effect: profil.profile_effect }
                : {}),
              ...('profile_frame' in profil
                ? { profile_frame: profil.profile_frame }
                : {}),
            }
          : c,
      ),
    }));
  },

  applyPresence: (pubkey, online, status, statusText) => {
    set((s) => ({
      contacts: s.contacts.map((c) =>
        c.pubkey === pubkey
          ? {
              ...c,
              online,
              // Présence riche absente (nœud ancien ou appel historique) :
              // les champs connus sont conservés tels quels.
              ...(status !== undefined
                ? { status, status_text: statusText ?? null }
                : {}),
            }
          : c,
      ),
    }));
  },
}));

/**
 * Événements du nœud propres à ce domaine (présence riche, retrait
 * d'amitié). Exporté pour les tests ; câblé au chargement du module — le
 * client RPC est un singleton, aucun désabonnement n'est nécessaire.
 */
export function handleFriendsNodeEvent(method: string, params: unknown): void {
  if (method === 'event.friend_removed') {
    // Retrait local ou distant : la liste seule fait foi.
    void useFriends
      .getState()
      .load()
      .catch(() => {
        // Best effort : la liste sera rechargée au prochain événement.
      });
    return;
  }
  if (method === 'event.presence') {
    const p = params as {
      pubkey: string;
      online: boolean;
      status?: PresenceStatus;
      status_text?: string | null;
    };
    useFriends
      .getState()
      .applyPresence(p.pubkey, p.online, p.status, p.status_text ?? null);
  }
}

// Garde d'environnement : les tests unitaires qui simulent `../lib/client`
// sans `rpc.onEvent` doivent pouvoir importer ce module sans câblage.
try {
  rpc.onEvent(handleFriendsNodeEvent);
} catch {
  // Client simulé (tests) : pas d'événements à câbler.
}

/** Nom affichable d'un pair : contact connu, sinon identifiant court. */
export function displayNameOf(contacts: Contact[], pubkey: string): string {
  const contact = contacts.find((c) => c.pubkey === pubkey);
  if (contact && contact.display_name.trim() !== '') return contact.display_name;
  return pubkey.slice(0, 6);
}

/**
 * Hash d'avatar d'un pair : celui du contact connu, sinon `null` (les
 * avatars des non-amis ne circulent pas — limite connue du protocole).
 */
export function avatarOf(contacts: readonly Contact[], pubkey: string): string | null {
  return contacts.find((c) => c.pubkey === pubkey)?.avatar ?? null;
}

/**
 * Décoration d'avatar d'un pair connu, ou `null`. Le champ reste optionnel
 * pour les profils annoncés par un nœud ancien : absent et explicitement nul
 * ont donc le même rendu (aucune décoration).
 */
export function avatarDecorationOf(
  contacts: readonly Contact[],
  pubkey: string,
): string | null {
  return contacts.find((c) => c.pubkey === pubkey)?.avatar_decoration ?? null;
}

/**
 * Statut de présence affichable d'un contact : statut riche annoncé s'il est
 * connu, sinon la simple joignabilité (`online`) mappée sur online/offline.
 */
export function presenceOf(contact: Contact | undefined): PresenceStatus {
  if (contact?.status !== undefined) return contact.status;
  return contact?.online === true ? 'online' : 'offline';
}

/**
 * Non-lus agrégés de toutes les conversations privées établies : alimente la
 * pastille du bouton Accueil/MP du rail des serveurs. Les demandes en
 * attente et contacts bloqués n'ont pas de fil de discussion à signaler.
 */
export function totalDmUnread(contacts: readonly Contact[]): number {
  return contacts
    .filter((c) => c.state === 'friend')
    .reduce((sum, c) => sum + (c.unread ?? 0), 0);
}

/** Mentions agrégées de toutes les conversations privées établies. */
export function totalDmMentions(contacts: readonly Contact[]): number {
  return contacts
    .filter((c) => c.state === 'friend')
    .reduce((sum, c) => sum + (c.mention_count ?? 0), 0);
}
