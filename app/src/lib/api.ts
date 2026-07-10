/**
 * Enveloppes typées des méthodes de l'API locale (contrat API.md).
 * Les identifiants transitent en hexadécimal ; les corps de messages
 * arrivent déjà décodés en JSON structuré.
 */

import type { RpcClient } from './rpc';

export interface SelfProfile {
  node_id: string;
  pubkey: string;
  friend_code: string;
  /** Pseudo choisi par l'utilisateur (`null` tant qu'aucun n'est défini). */
  name: string | null;
  /** Bio locale (`null` tant qu'aucune n'est définie). */
  bio: string | null;
  /** Racine Merkle de l'avatar (hex 64), ou `null` sans avatar. */
  avatar: string | null;
  /** Racine Merkle de la bannière de profil (hex 64), ou `null` sans bannière. */
  banner: string | null;
}

export type ContactState = 'pending_out' | 'pending_in' | 'friend' | 'blocked';

/** Rich presence of a peer as exposed by `friends.list` / `event.presence`. */
export type PresenceStatus = 'online' | 'idle' | 'dnd' | 'offline';

/** Own presence status (`friends.set_status`): invisible shows as offline. */
export type OwnPresenceStatus = 'online' | 'idle' | 'dnd' | 'invisible';

export interface Contact {
  node_id: string;
  pubkey: string;
  friend_code: string;
  display_name: string;
  /** Bio annoncée par le pair (`null` si inconnue ou effacée). */
  bio: string | null;
  /** Racine Merkle de l'avatar annoncé (hex 64), ou `null` sans avatar. */
  avatar: string | null;
  /** Racine Merkle de la bannière annoncée (hex 64), ou `null` sans bannière. */
  banner: string | null;
  state: ContactState;
  last_seen_ms: number;
  /** Présence best-effort du pair (`friends.list`, D-027) ; absente = inconnue. */
  online?: boolean;
  /** Statut riche annoncé par le pair (`friends.list`) ; absent = inconnu. */
  status?: PresenceStatus;
  /** Texte de statut personnalisé annoncé, ou `null` (absent = inconnu). */
  status_text?: string | null;
  /** Messages du pair reçus après notre `dm.mark_read` ; absent = inconnu. */
  unread?: number;
  /** Mentions non lues dans ce MP (détection locale) ; absent = inconnu. */
  mention_count?: number;
  /** Note privée locale attachée au contact, ou `null` (jamais émise). */
  note?: string | null;
}

/** Référence de pièce jointe (enveloppe des messages et `files.*`). */
export interface FileAttachment {
  merkle_root: string;
  name: string;
  size: number;
  mime: string;
}

export type MsgBody =
  | { type: 'text'; text: string; reply_to: string | null; attachments: number }
  | { type: 'edit'; target: string; text: string }
  | { type: 'delete'; target: string }
  | { type: 'reaction'; target: string; emoji: string; add: boolean }
  | { type: 'meta' }
  | { type: 'unknown' };

/** Réaction emoji : une entrée par paire emoji × auteur (API.md). */
export interface Reaction {
  emoji: string;
  author: string;
}

/** Delivery state of one of our outgoing direct messages (API.md §Direct messaging). */
export type DeliveryState = 'sent' | 'pending' | 'failed';

export interface DmMessage {
  msg_id: string;
  author: string;
  lamport: number;
  sent_ms: number;
  acked: boolean;
  deleted: boolean;
  /** Toujours émis (`false` par défaut) ; optionnel par tolérance. Épinglé localement (`dm.pins`). */
  pinned?: boolean;
  /** Toujours émis ; optionnel par tolérance. État de livraison sortante (`sent` pour l'entrant). */
  delivery?: DeliveryState;
  /** Toujours émis ; `true` si ce message mentionne l'utilisateur local. */
  mentions_me?: boolean;
  body: MsgBody;
  edited: string | null;
  /** Toujours émis par le nœud (`[]` si aucune) ; optionnel par tolérance. */
  reactions?: Reaction[];
  /** Pièces jointes de l'enveloppe (toujours émises, `[]` si aucune). */
  attachments?: FileAttachment[];
}

export interface GroupMessage {
  msg_id: string;
  channel_id: string;
  author: string;
  lamport: number;
  sent_ms: number;
  deleted: boolean;
  /** Toujours émis ; `true` si ce message mentionne l'utilisateur local. */
  mentions_me?: boolean;
  body: MsgBody;
  edited: string | null;
  /** Toujours émis par le nœud (vide pour l'instant côté groupes, D-022). */
  reactions?: Reaction[];
  /** Pièces jointes de l'enveloppe (toujours émises, `[]` si aucune). */
  attachments?: FileAttachment[];
}

/** Référence de conversation d'une entrée de boîte de mentions (`mentions.inbox`). */
export type MentionConversation =
  | { kind: 'dm'; peer: string }
  | { kind: 'group'; group_id: string; channel_id: string | null };

/** Entrée de la boîte de mentions locale (`mentions.inbox`). */
export interface MentionEntry {
  msg_id: string;
  conversation: MentionConversation;
  author: string;
  ts_ms: number;
  lamport: number;
  /** Extrait borné du texte (jamais le corps complet). */
  snippet: string;
  read: boolean;
}

/** Conversation d'un résultat `search.query` (métadonnées côté nœud). */
export type SearchConversation =
  | { type: 'dm'; peer: string }
  | { type: 'group'; group_id: string; channel_id: string };

/**
 * Résultat de `search.query` avec ses métadonnées : de quoi afficher et
 * naviguer (via `dm.history_around` / `groups.history_around`) même vers un
 * message hors des historiques déjà chargés.
 */
export interface SearchQueryHit {
  msg_id: string;
  author: string;
  lamport: number;
  timestamp: number;
  conversation: SearchConversation;
}

/** Genre d'un salon (API.md §Groupes : `kind`, défaut `"text"`). */
export type GroupChannelKind = 'text' | 'voice' | 'announcement';

export interface GroupChannel {
  channel_id: string;
  name: string;
  kind: GroupChannelKind;
  /** Catégorie d'appartenance (hex), ou `null` hors catégorie. */
  category: string | null;
  position: number;
  topic: string;
}

export interface GroupCategory {
  category_id: string;
  name: string;
  position: number;
}

export interface GroupRole {
  role_id: string;
  name: string;
  /** Couleur RGB (`0xRRGGBB`) ; `0` = aucune couleur. */
  color: number;
  position: number;
  /** Bitfield de permissions (voir `PERMISSIONS` dans stores/groups). */
  permissions: number;
}

/** Membre d'un groupe et ses rôles (identifiants de rôles). */
export interface GroupMember {
  pubkey: string;
  roles: string[];
  /**
   * Pseudo par serveur (remplace le pseudo du profil global), ou `null`.
   * Toujours présent dans `groups.state`; optionnel ici pour rester
   * rétrocompatible avec les fixtures antérieures.
   */
  nickname?: string | null;
  /**
   * Échéance murale (ms) de la sourdine active, ou `0` si le membre n'est pas
   * en sourdine. L'UI la compare à l'heure courante (une échéance passée est
   * sans effet). Toujours présent dans `groups.state`.
   */
  timeout_until_ms?: number;
}

export interface GroupInvite {
  invite_id: string;
  max_uses: number;
  uses: number;
  expires_ms: number;
  revoked: boolean;
}

/** Émoji de serveur : nom (`[a-z0-9_]`) et racine Merkle de son image. */
export interface ServerEmoji {
  name: string;
  merkle_root: string;
}

/**
 * Override de permissions d'un rôle sur un salon (`deny` prioritaire sur
 * `allow` ; un bit absent des deux masques est hérité).
 */
export interface ChannelOverride {
  channel_id: string;
  role_id: string;
  allow: number;
  deny: number;
}

/**
 * Entrée du journal d'audit (`groups.audit`) : op signée décodée. `params`
 * porte les champs utiles à la description (name, member, channel_id…).
 */
export interface AuditEntry {
  op_id: string;
  lamport: number;
  wall_ms: number;
  /** Clé publique (hex 64) de l'auteur de l'action. */
  author: string;
  /** Libellé stable de l'op (`create`, `kick`, `add_channel`, …, `unknown`). */
  kind: string;
  params: Record<string, unknown>;
}

export interface GroupStateJson {
  group_id: string;
  name: string;
  /** Racine Merkle de l'icône (hex 64), ou `null` sans icône. */
  icon: string | null;
  founder: string | null;
  members: GroupMember[];
  bans: string[];
  channels: GroupChannel[];
  categories: GroupCategory[];
  roles: GroupRole[];
  invites: GroupInvite[];
  /** Émojis de serveur, ordre stable lexicographique par `name` (peut manquer). */
  emojis?: ServerEmoji[];
  /** Overrides de permissions par salon et rôle (peut manquer). */
  overrides?: ChannelOverride[];
  /** Bitfield global de permissions de l'identité locale. */
  my_permissions: number;
}

/**
 * Résultat de `files.read` : octets en base64 si le blob est complet en
 * local, `{ pending: true }` sinon (le téléchargement vient d'être lancé).
 */
export type FilesReadResult =
  | { pending: true }
  | { data_b64: string; name: string; mime: string; size: number; pending?: undefined };

/**
 * Résultat de `files.status` : progression en blocs de 256 Kio ; `name`,
 * `size` et `mime` ne sont présents que si le manifeste est connu.
 */
export interface FilesStatusResult {
  known: boolean;
  complete: boolean;
  done: number;
  total: number;
  name?: string;
  size?: number;
  mime?: string;
}

/** Participant d'un salon vocal avec son état de parole (voice.status). */
export interface VoiceParticipant {
  pubkey: string;
  speaking: boolean;
  /** Micro coupé, tel que diffusé par le participant (VoiceSignal). */
  muted: boolean;
  /** Sortie coupée (deafen), tel que diffusé par le participant. */
  deafened: boolean;
  /** Volume de sortie local pour ce participant (0-200 %, persisté). */
  volume: number;
}

/** Salon vocal actif tel que rendu par voice.status (`null` si aucun). */
export interface VoiceActive {
  group_id: string;
  channel_id: string;
  muted: boolean;
  /** Sortie locale coupée (deafen force le micro coupé, jamais persisté). */
  deafened: boolean;
  participants: VoiceParticipant[];
}

/**
 * Périphériques audio (voice.devices) : noms cpal, `null` = périphérique par
 * défaut du système. Listes vides et sélections nulles en mode simulé.
 */
export interface VoiceDevices {
  inputs: string[];
  outputs: string[];
  selected_input: string | null;
  selected_output: string | null;
}

/** Sélection de périphériques à appliquer (champ absent = inchangé). */
export interface VoiceDeviceSelection {
  input?: string | null;
  output?: string | null;
}

/** État du réseau P2P (voir `network.status`). */
export interface NetworkStatus {
  /** Port UDP local effectivement lié. */
  p2p_port: number;
  /** Adresses `ip:port` joignables (à communiquer à un ami) ; la première est
   * l'adresse publique observée si connue. */
  local_addrs: string[];
  /** Pairs d'amorçage enregistrés (`ip:port`). */
  bootstrap: string[];
  /** Nombre de pairs actuellement connectés. */
  connected_peers: number;
  /** Nombre de nœuds connus dans la table de routage DHT. */
  dht_nodes: number;
  /** Adresse externe (`ip:port` publique) ouverte par le mapping de port
   * automatique, ou `null` si aucun mapping n'est actif. */
  external_addr: string | null;
  /** Méthode de mapping de port active : `'upnp'`, `'natpmp'` ou `'aucun'`. */
  port_mapping: 'upnp' | 'natpmp' | 'aucun';
  /** Nombre de pairs Accord découverts sur le réseau local (mDNS). */
  lan_peers: number;
}

/** Événements poussés par le nœud (API.md §Événements). */
export type AccordEvent =
  | {
      method: 'event.dm';
      params: { peer: string; msg_id: string; attachments: FileAttachment[] };
    }
  | { method: 'event.dm_typing'; params: { peer: string } }
  | { method: 'event.friend_request'; params: { peer: string } }
  | { method: 'event.friend_response'; params: { peer: string; accepted: boolean } }
  | { method: 'event.group_op'; params: { group_id: string } }
  | { method: 'event.group_state'; params: { group_id: string } }
  | {
      method: 'event.group_msg';
      params: {
        group_id: string;
        channel_id: string;
        msg_id: string;
        attachments: FileAttachment[];
      };
    }
  | { method: 'event.group_key'; params: { group_id: string } }
  | {
      method: 'event.group_typing';
      params: { group_id: string; channel_id: string; pubkey: string };
    }
  | {
      method: 'event.voice_joined';
      params: { group_id: string; channel_id: string; pubkey: string };
    }
  | {
      method: 'event.voice_left';
      params: { group_id: string; channel_id: string; pubkey: string };
    }
  | { method: 'event.voice_speaking'; params: { pubkey: string; speaking: boolean } }
  | {
      method: 'event.voice_mute';
      params: { pubkey: string; muted: boolean; deafened: boolean };
    }
  | { method: 'event.voice_level'; params: { level: number; speaking: boolean } }
  | {
      method: 'event.profile';
      params: {
        pubkey: string;
        name: string;
        bio: string | null;
        avatar: string | null;
        banner: string | null;
      };
    }
  | {
      method: 'event.presence';
      params: {
        pubkey: string;
        online: boolean;
        /** Statut riche (absent d'un nœud ancien : ne garder que `online`). */
        status?: PresenceStatus;
        status_text?: string | null;
      };
    }
  | { method: 'event.friend_removed'; params: { peer: string } }
  | { method: 'event.dm_read'; params: { peer: string; lamport: number } }
  | {
      method: 'event.network';
      params: { connected_peers: number; dht_nodes: number };
    }
  | {
      method: 'event.file_progress';
      params: { merkle_root: string; done: number; total: number; complete: boolean };
    }
  | { method: 'event.desynchronise'; params: Record<string, never> };

export class Api {
  constructor(private readonly rpc: RpcClient) {}

  identitySelf(): Promise<SelfProfile> {
    return this.rpc.call('identity.self');
  }

  /** Profil local (`null` pour chaque champ jamais défini). */
  profileGet(): Promise<{
    name: string | null;
    bio: string | null;
    avatar: string | null;
    banner: string | null;
  }> {
    return this.rpc.call('profile.get');
  }

  /**
   * Définit le pseudo (2 à 32 caractères) et/ou la bio (≤ 2048 caractères,
   * chaîne vide = effacer) — au moins un des deux champs requis, tout ou rien.
   */
  profileSet(changes: { name?: string; bio?: string }): Promise<Record<string, never>> {
    return this.rpc.call('profile.set', {
      ...(changes.name !== undefined ? { name: changes.name } : {}),
      ...(changes.bio !== undefined ? { bio: changes.bio } : {}),
    });
  }

  /**
   * Publie l'avatar (image/png|jpeg|webp, ≤ 512 Kio décodés) et rend le hash
   * du blob ; `dataB64: null` retire l'avatar (rend `{ avatar: null }`).
   */
  profileSetAvatar(
    dataB64: string | null,
    mime?: string,
  ): Promise<{ avatar: string | null }> {
    return this.rpc.call('profile.set_avatar', {
      data_b64: dataB64,
      ...(mime !== undefined ? { mime } : {}),
    });
  }

  /**
   * Publie la bannière de profil (image paysage, mêmes formats/limites que
   * l'avatar) et rend le hash du blob ; `dataB64: null` retire la bannière
   * (rend `{ banner: null }`).
   */
  profileSetBanner(
    dataB64: string | null,
    mime?: string,
  ): Promise<{ banner: string | null }> {
    return this.rpc.call('profile.set_banner', {
      data_b64: dataB64,
      ...(mime !== undefined ? { mime } : {}),
    });
  }

  friendsList(): Promise<{ contacts: Contact[] }> {
    return this.rpc.call('friends.list');
  }

  friendsResolve(friendCode: string): Promise<{ pubkey: string }> {
    return this.rpc.call('friends.resolve', { friend_code: friendCode });
  }

  friendsRequest(pubkey: string, displayName: string): Promise<{ ok: true }> {
    return this.rpc.call('friends.request', { pubkey, display_name: displayName });
  }

  friendsRespond(pubkey: string, accept: boolean): Promise<{ ok: true }> {
    return this.rpc.call('friends.respond', { pubkey, accept });
  }

  /**
   * Écrit la note privée locale d'un contact (≤ 4096 caractères, rognée ; une
   * note vide l'efface). Purement locale : jamais émise vers le pair.
   */
  friendsSetNote(pubkey: string, note: string): Promise<{ ok: true }> {
    return this.rpc.call('friends.set_note', { pubkey, note });
  }

  /** Lit la note privée locale d'un contact (`null` si aucune). */
  friendsGetNote(pubkey: string): Promise<{ note: string | null }> {
    return this.rpc.call('friends.get_note', { pubkey });
  }

  friendsBlock(pubkey: string): Promise<{ ok: true }> {
    return this.rpc.call('friends.block', { pubkey });
  }

  friendsUnblock(pubkey: string): Promise<{ ok: true }> {
    return this.rpc.call('friends.unblock', { pubkey });
  }

  /**
   * Retire une amitié établie (distinct du blocage : l'historique MP est
   * conservé et une nouvelle demande d'ami reste possible). Le pair est
   * prévenu best-effort et les deux côtés reçoivent `event.friend_removed`.
   */
  friendsRemove(pubkey: string): Promise<{ ok: true }> {
    return this.rpc.call('friends.remove', { pubkey });
  }

  /**
   * Fixe son statut de présence (persisté, diffusé aux amis). `custom` :
   * absent = texte inchangé, chaîne vide = effacé, sinon remplacé (≤ 256
   * octets UTF-8). `invisible` est annoncé comme hors ligne aux pairs.
   */
  friendsSetStatus(status: OwnPresenceStatus, custom?: string): Promise<{ ok: true }> {
    return this.rpc.call('friends.set_status', {
      status,
      ...(custom !== undefined ? { custom } : {}),
    });
  }

  /** Statut de présence local persisté (défauts : `online`, `custom: null`). */
  friendsGetStatus(): Promise<{ status: OwnPresenceStatus; custom: string | null }> {
    return this.rpc.call('friends.get_status');
  }

  /**
   * Envoie un message direct, éventuellement avec des pièces jointes déjà
   * publiées (`files.share_bytes`) — texte vide admis si au moins une pièce.
   */
  dmSend(
    pubkey: string,
    text: string,
    replyTo?: string,
    attachments?: FileAttachment[],
  ): Promise<{ msg_id: string }> {
    return this.rpc.call('dm.send', {
      pubkey,
      text,
      ...(replyTo !== undefined ? { reply_to: replyTo } : {}),
      ...(attachments !== undefined && attachments.length > 0 ? { attachments } : {}),
    });
  }

  dmHistory(pubkey: string, limit = 50): Promise<{ messages: DmMessage[]; peer_read_lamport: number | null }> {
    return this.rpc.call('dm.history', { pubkey, limit });
  }

  /**
   * Fenêtre d'historique centrée sur `msgId` (jump-to-message) : moitié avant,
   * la cible, moitié après. `found` est `false` avec une fenêtre vide si la
   * cible est inconnue localement.
   */
  dmHistoryAround(
    pubkey: string,
    msgId: string,
    limit = 50,
  ): Promise<{ messages: DmMessage[]; found: boolean; peer_read_lamport: number | null }> {
    return this.rpc.call('dm.history_around', { pubkey, msg_id: msgId, limit });
  }

  /** Épingle un message de la conversation (vue locale, aucune op filaire). */
  dmPin(pubkey: string, msgId: string): Promise<{ ok: true }> {
    return this.rpc.call('dm.pin', { pubkey, msg_id: msgId });
  }

  /** Retire l'épingle d'un message direct. */
  dmUnpin(pubkey: string, msgId: string): Promise<{ ok: true }> {
    return this.rpc.call('dm.unpin', { pubkey, msg_id: msgId });
  }

  /** Identifiants des messages épinglés de la conversation. */
  dmPins(pubkey: string): Promise<{ msg_ids: string[] }> {
    return this.rpc.call('dm.pins', { pubkey });
  }

  /** Relance l'envoi d'un message non acquitté (`delivery` `pending`/`failed`). */
  dmRetry(pubkey: string, msgId: string): Promise<{ ok: true }> {
    return this.rpc.call('dm.retry', { pubkey, msg_id: msgId });
  }

  /** Modifie un de ses propres messages (le nœud refuse sinon). */
  dmEdit(pubkey: string, msgId: string, text: string): Promise<{ ok: true }> {
    return this.rpc.call('dm.edit', { pubkey, msg_id: msgId, text });
  }

  /** Supprime un de ses propres messages (tombstone local immédiat). */
  dmDelete(pubkey: string, msgId: string): Promise<{ ok: true }> {
    return this.rpc.call('dm.delete', { pubkey, msg_id: msgId });
  }

  /** Ajoute (ou retire avec `remove`) une réaction emoji à un message. */
  dmReact(
    pubkey: string,
    msgId: string,
    emoji: string,
    remove = false,
  ): Promise<{ ok: true }> {
    return this.rpc.call('dm.react', {
      pubkey,
      msg_id: msgId,
      emoji,
      ...(remove ? { remove: true } : {}),
    });
  }

  /**
   * Signale au pair qu'on est en train d'écrire — indicateur éphémère,
   * jamais persisté (pair hors ligne : silencieusement ignoré par le nœud).
   */
  dmTyping(pubkey: string): Promise<{ ok: true }> {
    return this.rpc.call('dm.typing', { pubkey });
  }

  /**
   * Enregistre la position de lecture de la conversation (lamport du dernier
   * message affiché) — alimente `unread` dans `friends.list`.
   */
  dmMarkRead(pubkey: string, lamport: number): Promise<{ ok: true }> {
    return this.rpc.call('dm.mark_read', { pubkey, lamport });
  }

  /**
   * Active ou coupe l'émission des accusés de lecture (réglage de
   * confidentialité persisté, activé par défaut). Coupé, les accusés
   * entrants restent enregistrés.
   */
  dmSetReadReceipts(enabled: boolean): Promise<{ ok: true }> {
    return this.rpc.call('dm.set_read_receipts', { enabled });
  }

  /** État du réglage d'émission des accusés de lecture. */
  dmGetReadReceipts(): Promise<{ enabled: boolean }> {
    return this.rpc.call('dm.get_read_receipts');
  }

  groupsCreate(name: string): Promise<{ group_id: string }> {
    return this.rpc.call('groups.create', { name });
  }

  /**
   * Liste des groupes, non-lus par salon (`{ group_id: { channel_id: n } }`,
   * seuls les salons ayant au moins un non-lu figurent) et mentions non lues
   * par groupe (`{ group_id: n }`, seuls les groupes en portant) — `unread` et
   * `mentions` optionnels par tolérance.
   */
  groupsList(): Promise<{
    groups: string[];
    unread?: Record<string, Record<string, number>>;
    mentions?: Record<string, number>;
  }> {
    return this.rpc.call('groups.list');
  }

  /**
   * Signale aux membres en ligne qu'on écrit dans le salon — indicateur
   * éphémère, jamais persisté ni mis en file.
   */
  groupsTyping(groupId: string, channelId: string): Promise<{ ok: true }> {
    return this.rpc.call('groups.typing', {
      group_id: groupId,
      channel_id: channelId,
    });
  }

  /**
   * Enregistre la position de lecture du salon (lamport du dernier message
   * affiché) — alimente `unread` dans `groups.list`.
   */
  groupsMarkRead(
    groupId: string,
    channelId: string,
    lamport: number,
  ): Promise<{ ok: true }> {
    return this.rpc.call('groups.mark_read', {
      group_id: groupId,
      channel_id: channelId,
      lamport,
    });
  }

  groupsState(groupId: string): Promise<GroupStateJson> {
    return this.rpc.call('groups.state', { group_id: groupId });
  }

  /**
   * Boîte de mentions locale, la plus récente d'abord. `before` pagine par
   * horloge murale (ms, entrées strictement plus anciennes) ; `limit` borné à
   * [1, 200] (défaut 50).
   */
  mentionsInbox(before?: number, limit?: number): Promise<{ entries: MentionEntry[] }> {
    return this.rpc.call('mentions.inbox', {
      ...(before !== undefined ? { before } : {}),
      ...(limit !== undefined ? { limit } : {}),
    });
  }

  /**
   * Marque des mentions comme lues. `msgIds` absent (ou omis) marque **tout**
   * comme lu ; `marked` = nombre d'entrées effectivement basculées.
   */
  mentionsMarkRead(msgIds?: string[]): Promise<{ ok: true; marked: number }> {
    return this.rpc.call('mentions.mark_read', {
      ...(msgIds !== undefined ? { msg_ids: msgIds } : {}),
    });
  }

  /** Renomme le groupe (1 à 100 caractères, le nœud refuse sinon). */
  groupsRename(groupId: string, name: string): Promise<{ ok: true }> {
    return this.rpc.call('groups.rename', { group_id: groupId, name });
  }

  /**
   * Publie l'icône du groupe (image ≤ 512 Kio décodés) et rend sa racine
   * Merkle ; les octets se relisent ensuite via `files.read`.
   */
  groupsSetIcon(
    groupId: string,
    dataB64: string,
    mime: string,
  ): Promise<{ icon: string }> {
    return this.rpc.call('groups.set_icon', {
      group_id: groupId,
      data_b64: dataB64,
      mime,
    });
  }

  /** Définit le sujet d'un salon (≤ 2048 octets ; chaîne vide = effacer). */
  groupsSetTopic(
    groupId: string,
    channelId: string,
    topic: string,
  ): Promise<{ ok: true }> {
    return this.rpc.call('groups.set_topic', {
      group_id: groupId,
      channel_id: channelId,
      topic,
    });
  }

  groupsChannelAdd(
    groupId: string,
    name: string,
    kind?: GroupChannelKind,
    category?: string,
  ): Promise<{ channel_id: string }> {
    return this.rpc.call('groups.channel.add', {
      group_id: groupId,
      name,
      ...(kind !== undefined ? { kind } : {}),
      ...(category !== undefined ? { category } : {}),
    });
  }

  /**
   * Modifie un salon (champ absent = inchangé). `category`: `null` sort le
   * salon de toute catégorie, un hex 32 le déplace dans cette catégorie.
   */
  groupsChannelEdit(
    groupId: string,
    channelId: string,
    changes: { name?: string; position?: number; category?: string | null },
  ): Promise<{ ok: true }> {
    return this.rpc.call('groups.channel.edit', {
      group_id: groupId,
      channel_id: channelId,
      ...(changes.name !== undefined ? { name: changes.name } : {}),
      ...(changes.position !== undefined ? { position: changes.position } : {}),
      ...(changes.category !== undefined ? { category: changes.category } : {}),
    });
  }

  /**
   * Fixe l'override de permissions d'un rôle sur un salon (`allow`/`deny` :
   * bitfields, `deny` prioritaire) ; `allow = deny = 0` efface l'override.
   */
  groupsChannelPerms(
    groupId: string,
    channelId: string,
    roleId: string,
    allow: number,
    deny: number,
  ): Promise<{ ok: true }> {
    return this.rpc.call('groups.channel.perms', {
      group_id: groupId,
      channel_id: channelId,
      role_id: roleId,
      allow,
      deny,
    });
  }

  /** Renomme et/ou repositionne une catégorie (champ absent = inchangé). */
  groupsCategoryEdit(
    groupId: string,
    categoryId: string,
    changes: { name?: string; position?: number },
  ): Promise<{ ok: true }> {
    return this.rpc.call('groups.category.edit', {
      group_id: groupId,
      category_id: categoryId,
      ...(changes.name !== undefined ? { name: changes.name } : {}),
      ...(changes.position !== undefined ? { position: changes.position } : {}),
    });
  }

  /** Supprime une catégorie ; ses salons deviennent « sans catégorie ». */
  groupsCategoryDel(groupId: string, categoryId: string): Promise<{ ok: true }> {
    return this.rpc.call('groups.category.del', {
      group_id: groupId,
      category_id: categoryId,
    });
  }

  /**
   * Journal d'audit (ADMIN/fondateur) : ops signées décodées, de la plus
   * récente à la plus ancienne. `before` = `op_id` de la plus ancienne
   * entrée déjà chargée (curseur) ; `limit` borné à [1, 100].
   */
  groupsAudit(
    groupId: string,
    before?: string,
    limit?: number,
  ): Promise<{ entries: AuditEntry[] }> {
    return this.rpc.call('groups.audit', {
      group_id: groupId,
      ...(before !== undefined ? { before } : {}),
      ...(limit !== undefined ? { limit } : {}),
    });
  }

  groupsChannelDel(groupId: string, channelId: string): Promise<{ ok: true }> {
    return this.rpc.call('groups.channel.del', {
      group_id: groupId,
      channel_id: channelId,
    });
  }

  groupsCategoryAdd(groupId: string, name: string): Promise<{ category_id: string }> {
    return this.rpc.call('groups.category.add', { group_id: groupId, name });
  }

  /** Expulse un membre (hiérarchie vérifiée par le nœud). */
  groupsKick(groupId: string, pubkey: string): Promise<{ ok: true }> {
    return this.rpc.call('groups.kick', { group_id: groupId, pubkey });
  }

  /** Bannit un membre : il ne peut plus être (ré)admis. */
  groupsBan(groupId: string, pubkey: string): Promise<{ ok: true }> {
    return this.rpc.call('groups.ban', { group_id: groupId, pubkey });
  }

  groupsUnban(groupId: string, pubkey: string): Promise<{ ok: true }> {
    return this.rpc.call('groups.unban', { group_id: groupId, pubkey });
  }

  /**
   * Met un membre en sourdine jusqu'à l'échéance murale `untilMs` (permission
   * `KICK` et hiérarchie de kick vérifiées par le nœud). Le membre reste dans
   * le groupe mais ne peut plus écrire tant que la sourdine est active. Voir
   * `groupsTimeoutClear` pour la lever.
   */
  groupsTimeout(groupId: string, pubkey: string, untilMs: number): Promise<{ ok: true }> {
    return this.rpc.call('groups.timeout', {
      group_id: groupId,
      pubkey,
      until_ms: untilMs,
    });
  }

  /** Lève la sourdine d'un membre. */
  groupsTimeoutClear(groupId: string, pubkey: string): Promise<{ ok: true }> {
    return this.rpc.call('groups.timeout_clear', { group_id: groupId, pubkey });
  }

  /**
   * Fixe (ou efface avec une chaîne vide) le pseudo de serveur d'un membre.
   * `member` absent = soi-même ; un membre peut fixer le sien, un modérateur
   * `MANAGE_ROLES` celui d'un membre de rang inférieur. `name` : 1 à 32
   * caractères après trim, sans caractère de contrôle (vide = efface).
   */
  groupsSetNickname(groupId: string, name: string, member?: string): Promise<{ ok: true }> {
    return this.rpc.call('groups.set_nickname', {
      group_id: groupId,
      name,
      ...(member !== undefined ? { member } : {}),
    });
  }

  /** Quitte le groupe (refusé au fondateur tant qu'il reste des membres). */
  groupsLeave(groupId: string): Promise<{ ok: true }> {
    return this.rpc.call('groups.leave', { group_id: groupId });
  }

  groupsRoleAdd(
    groupId: string,
    name: string,
    color: number,
    permissions: number,
  ): Promise<{ role_id: string }> {
    return this.rpc.call('groups.role.add', {
      group_id: groupId,
      name,
      color,
      permissions,
    });
  }

  /** Modifie un rôle (champ absent = inchangé). */
  groupsRoleEdit(
    groupId: string,
    roleId: string,
    changes: { name?: string; color?: number; position?: number; permissions?: number },
  ): Promise<{ ok: true }> {
    return this.rpc.call('groups.role.edit', {
      group_id: groupId,
      role_id: roleId,
      ...(changes.name !== undefined ? { name: changes.name } : {}),
      ...(changes.color !== undefined ? { color: changes.color } : {}),
      ...(changes.position !== undefined ? { position: changes.position } : {}),
      ...(changes.permissions !== undefined ? { permissions: changes.permissions } : {}),
    });
  }

  groupsRoleDel(groupId: string, roleId: string): Promise<{ ok: true }> {
    return this.rpc.call('groups.role.del', { group_id: groupId, role_id: roleId });
  }

  groupsRoleAssign(
    groupId: string,
    roleId: string,
    pubkey: string,
  ): Promise<{ ok: true }> {
    return this.rpc.call('groups.role.assign', {
      group_id: groupId,
      role_id: roleId,
      pubkey,
    });
  }

  groupsRoleUnassign(
    groupId: string,
    roleId: string,
    pubkey: string,
  ): Promise<{ ok: true }> {
    return this.rpc.call('groups.role.unassign', {
      group_id: groupId,
      role_id: roleId,
      pubkey,
    });
  }

  /** Épingle un message connu localement (`MANAGE_MESSAGES` requis). */
  groupsPin(groupId: string, channelId: string, msgId: string): Promise<{ ok: true }> {
    return this.rpc.call('groups.pin', {
      group_id: groupId,
      channel_id: channelId,
      msg_id: msgId,
    });
  }

  groupsUnpin(groupId: string, channelId: string, msgId: string): Promise<{ ok: true }> {
    return this.rpc.call('groups.unpin', {
      group_id: groupId,
      channel_id: channelId,
      msg_id: msgId,
    });
  }

  groupsPins(groupId: string, channelId: string): Promise<{ msg_ids: string[] }> {
    return this.rpc.call('groups.pins', {
      group_id: groupId,
      channel_id: channelId,
    });
  }

  /** Modifie un de ses propres messages de salon (le nœud refuse sinon). */
  groupsEdit(
    groupId: string,
    channelId: string,
    msgId: string,
    text: string,
  ): Promise<{ ok: true }> {
    return this.rpc.call('groups.edit', {
      group_id: groupId,
      channel_id: channelId,
      msg_id: msgId,
      text,
    });
  }

  /**
   * Supprime un message de salon : le sien (tombstone diffusée) ou celui
   * d'autrui (op de modération, `MANAGE_MESSAGES` requis).
   */
  groupsDelete(groupId: string, channelId: string, msgId: string): Promise<{ ok: true }> {
    return this.rpc.call('groups.delete', {
      group_id: groupId,
      channel_id: channelId,
      msg_id: msgId,
    });
  }

  /** Ajoute (`add: true`) ou retire une réaction emoji sur un message. */
  groupsReact(
    groupId: string,
    channelId: string,
    msgId: string,
    emoji: string,
    add: boolean,
  ): Promise<{ ok: true }> {
    return this.rpc.call('groups.react', {
      group_id: groupId,
      channel_id: channelId,
      msg_id: msgId,
      emoji,
      add,
    });
  }

  /**
   * Lit un fichier du magasin local par sa racine Merkle (borné à 8 Mio).
   * `hint` : clé publique d'un pair source probable pour le téléchargement.
   */
  filesRead(merkleRoot: string, hint?: string): Promise<FilesReadResult> {
    return this.rpc.call('files.read', {
      merkle_root: merkleRoot,
      ...(hint !== undefined ? { hint } : {}),
    });
  }

  /**
   * Publie des octets fournis par l'UI dans le magasin local (base64
   * standard, borné à 8 Mio décodés) et rend la référence de pièce jointe.
   */
  filesShareBytes(
    name: string,
    mime: string,
    dataB64: string,
  ): Promise<{ file: FileAttachment }> {
    return this.rpc.call('files.share_bytes', { name, mime, data_b64: dataB64 });
  }

  /** État local d'un fichier (manifeste connu, progression en blocs). */
  filesStatus(merkleRoot: string, hint?: string): Promise<FilesStatusResult> {
    return this.rpc.call('files.status', {
      merkle_root: merkleRoot,
      ...(hint !== undefined ? { hint } : {}),
    });
  }

  groupsHistory(
    groupId: string,
    channelId: string,
    limit = 50,
  ): Promise<{ messages: GroupMessage[] }> {
    return this.rpc.call('groups.history', {
      group_id: groupId,
      channel_id: channelId,
      limit,
    });
  }

  /**
   * Fenêtre d'historique d'un salon centrée sur `msgId` (jump-to-message).
   * `found` est `false` avec une fenêtre vide si la cible est inconnue.
   */
  groupsHistoryAround(
    groupId: string,
    channelId: string,
    msgId: string,
    limit = 50,
  ): Promise<{ messages: GroupMessage[]; found: boolean }> {
    return this.rpc.call('groups.history_around', {
      group_id: groupId,
      channel_id: channelId,
      msg_id: msgId,
      limit,
    });
  }

  /**
   * Envoie un message de salon, éventuellement en réponse à `replyTo` (msg_id,
   * hex 32) et avec des pièces jointes déjà publiées (texte vide admis).
   */
  groupsSend(
    groupId: string,
    channelId: string,
    text: string,
    replyTo?: string,
    attachments?: FileAttachment[],
  ): Promise<{ msg_id: string }> {
    return this.rpc.call('groups.send', {
      group_id: groupId,
      channel_id: channelId,
      text,
      ...(replyTo !== undefined ? { reply_to: replyTo } : {}),
      ...(attachments !== undefined && attachments.length > 0 ? { attachments } : {}),
    });
  }

  groupsInvite(groupId: string, pubkey: string): Promise<{ ok: true }> {
    return this.rpc.call('groups.invite', { group_id: groupId, pubkey });
  }

  /**
   * Ajoute (ou remplace) un émoji de serveur (`MANAGE_EMOJIS`) : nom
   * `[a-z0-9_]` 2-32, image ≤ 256 Kio décodés (png/jpeg/webp/gif). Rend la
   * racine Merkle de l'image publiée.
   */
  groupsEmojiAdd(
    groupId: string,
    name: string,
    dataB64: string,
    mime: string,
  ): Promise<{ merkle_root: string }> {
    return this.rpc.call('groups.emoji.add', {
      group_id: groupId,
      name,
      data_b64: dataB64,
      mime,
    });
  }

  /** Supprime un émoji de serveur par son nom (`MANAGE_EMOJIS`). */
  groupsEmojiDel(groupId: string, name: string): Promise<{ ok: true }> {
    return this.rpc.call('groups.emoji.del', { group_id: groupId, name });
  }

  /**
   * Recherche locale. `query` accepte des mots simples et des filtres
   * `from:` / `in:` / `has:link|image|file` / `before:` / `after:` (voir
   * API.md §Search). `hits` porte les métadonnées par résultat ; `msg_ids`
   * reprend leurs identifiants (plus récents d'abord) par compatibilité.
   */
  searchQuery(query: string): Promise<{ msg_ids: string[]; hits: SearchQueryHit[] }> {
    return this.rpc.call('search.query', { query });
  }

  /**
   * Rejoint un salon vocal (quitte l'ancien implicitement : un seul salon
   * actif à la fois). Le nœud refuse au-delà de 10 participants (full mesh).
   */
  voiceJoin(groupId: string, channelId: string): Promise<{ participants: string[] }> {
    return this.rpc.call('voice.join', { group_id: groupId, channel_id: channelId });
  }

  /** Quitte le salon vocal actif. */
  voiceLeave(): Promise<Record<string, never>> {
    return this.rpc.call('voice.leave');
  }

  /** Coupe ou rétablit la capture micro locale ; on reste dans le salon. */
  voiceMute(muted: boolean): Promise<Record<string, never>> {
    return this.rpc.call('voice.mute', { muted });
  }

  /**
   * Coupe (`true`) ou rétablit (`false`) toute la voix entrante localement.
   * Le deafen force le micro coupé ; le rétablissement restaure l'état de
   * micro demandé auparavant (sémantique Discord). Jamais persisté.
   */
  voiceDeafen(on: boolean): Promise<Record<string, never>> {
    return this.rpc.call('voice.deafen', { on });
  }

  /**
   * Volume de sortie en pourcentage (entier 0-200, 100 = neutre) : volume
   * principal quand `peer` est `null`, sinon volume du participant (clé
   * publique hex). Persisté côté nœud et appliqué à chaud au salon actif.
   */
  voiceSetVolume(peer: string | null, volume: number): Promise<Record<string, never>> {
    return this.rpc.call(
      'voice.set_volume',
      peer === null ? { volume } : { peer, volume },
    );
  }

  /** État vocal courant (`active: null` hors salon), pour resynchronisation. */
  voiceStatus(): Promise<{ active: VoiceActive | null; master_volume: number }> {
    return this.rpc.call('voice.status');
  }

  /** Périphériques audio disponibles et sélection courante. */
  voiceDevices(): Promise<VoiceDevices> {
    return this.rpc.call('voice.devices');
  }

  /**
   * Applique une sélection de périphériques (persistée ; à chaud si un salon
   * vocal est actif). `null` = périphérique par défaut ; nom inconnu = erreur.
   */
  voiceSetDevices(selection: VoiceDeviceSelection): Promise<Record<string, never>> {
    const params: Record<string, unknown> = {};
    if (selection.input !== undefined) params.input = selection.input;
    if (selection.output !== undefined) params.output = selection.output;
    return this.rpc.call('voice.set_devices', params);
  }

  /**
   * Démarre/arrête le test du micro : pendant l'activation, le nœud pousse
   * `event.voice_level` (~10 Hz) depuis la capture réelle. Erreur explicite
   * si le backend matériel n'est pas disponible.
   */
  voiceMicTest(enabled: boolean): Promise<Record<string, never>> {
    return this.rpc.call('voice.mic_test', { enabled });
  }

  /**
   * État du réseau : port P2P local, adresses joignables (à communiquer à un
   * ami), pairs d'amorçage enregistrés, et compteurs de connexions/nœuds DHT.
   */
  networkStatus(): Promise<NetworkStatus> {
    return this.rpc.call('network.status');
  }

  /**
   * Ajoute un pair d'amorçage par son adresse `ip:port` (validation et
   * connexion immédiates) ; rend l'état réseau à jour.
   */
  networkAddPeer(addr: string): Promise<NetworkStatus> {
    return this.rpc.call('network.add_peer', { addr });
  }

  /** Retire un pair d'amorçage ; rend l'état réseau à jour. */
  networkRemovePeer(addr: string): Promise<NetworkStatus> {
    return this.rpc.call('network.remove_peer', { addr });
  }
}
