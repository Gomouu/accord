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
  /** Pronoms libres (`null` tant qu'aucun n'est défini). */
  pronouns: string | null;
  /** Couleur d'accent du profil (entier 0xRRGGBB), ou `null` sans accent. */
  accent_color: number | null;
  /**
   * Couleur de fond de la bannière (entier 0xRRGGBB), utilisée tant qu'aucune
   * image de bannière n'est définie ; `null` sans couleur.
   */
  banner_color: number | null;
  /**
   * Id de décoration d'avatar (cadre/anneau décoratif), clé d'un catalogue
   * intégré côté client (`[a-z0-9_-]`, ≤ 24) ; `null` sans décoration.
   */
  avatar_decoration: string | null;
  /**
   * Id d'effet de profil (fond animé de la carte de profil), clé du même
   * catalogue ; `null` sans effet.
   */
  profile_effect: string | null;
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
  /** Pronoms annoncés par le pair (`null` si inconnus ou effacés). */
  pronouns?: string | null;
  /** Couleur d'accent annoncée par le pair (entier 0xRRGGBB), ou `null` sans accent. */
  accent_color?: number | null;
  /**
   * Couleur de fond de bannière annoncée par le pair (entier 0xRRGGBB),
   * utilisée tant qu'aucune image de bannière n'est définie ; `null` sans
   * couleur.
   */
  banner_color?: number | null;
  /** Id de décoration d'avatar annoncé par le pair, ou `null` (absent = inconnu). */
  avatar_decoration?: string | null;
  /** Id d'effet de profil annoncé par le pair, ou `null` (absent = inconnu). */
  profile_effect?: string | null;
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
  /**
   * Ma dernière position lue (lamport) dans ce MP (`friends.list`) : sert à
   * tracer le séparateur « nouveaux messages » à l'ouverture. `0` si jamais
   * lu ; absent = nœud plus ancien (aucun séparateur).
   */
  read_lamport?: number;
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
  | { type: 'sticker'; name: string; merkle_root: string }
  | { type: 'poll'; poll_id: string; question: string; options: string[] }
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
  { type: 'dm'; peer: string } | { type: 'group'; group_id: string; channel_id: string };

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
export type GroupChannelKind = 'text' | 'voice' | 'announcement' | 'forum';

export interface GroupChannel {
  channel_id: string;
  name: string;
  kind: GroupChannelKind;
  /** Catégorie d'appartenance (hex), ou `null` hors catégorie. */
  category: string | null;
  position: number;
  topic: string;
  /**
   * Mode lent du salon (`groups.channel.slowmode`) : délai minimal en
   * secondes entre deux messages d'un même auteur, `0` = désactivé.
   * Optionnel par tolérance (nœud plus ancien).
   */
  slowmode_secs?: number;
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
  /**
   * Modération vocale serveur (`groups.voice_moderate`, op 0x1F) : micro/sortie
   * forcés coupés dans tous les salons vocaux du groupe. Toujours présents
   * dans `groups.state` ; optionnels par tolérance (nœud plus ancien).
   */
  voice_muted?: boolean;
  voice_deafened?: boolean;
  /**
   * Avatar de serveur self-service (`groups.set_member_avatar`), racine
   * Merkle (hex 64) ou `null` sans avatar de serveur. Prime sur l'avatar de
   * profil global quand présent. Optionnel par tolérance (nœud plus ancien).
   */
  avatar?: string | null;
}

export interface GroupInvite {
  invite_id: string;
  max_uses: number;
  uses: number;
  expires_ms: number;
  revoked: boolean;
}

/**
 * Invitation entrante en attente (consentement explicite, D-045 : plus de
 * force-join) — `groups.invites_list` et `event.group_invite_pending`.
 * `received_ms` n'est porté que par `groups.invites_list` (absent de
 * l'événement temps réel, qui vient tout juste d'arriver).
 */
export interface PendingInvite {
  group_id: string;
  invite_id: string;
  group_name: string;
  inviter: string;
  expires_ms: number;
  received_ms?: number;
}

/** Émoji de serveur : nom (`[a-z0-9_]`) et racine Merkle de son image. */
export interface ServerEmoji {
  name: string;
  merkle_root: string;
}

/** Sticker de serveur : même forme qu'un émoji (nom + racine Merkle de l'image). */
export interface ServerSticker {
  name: string;
  merkle_root: string;
}

/** Son de soundboard : nom (`[a-z0-9_]`) et racine Merkle du clip audio. */
export interface ServerSound {
  name: string;
  merkle_root: string;
}

/**
 * Événement planifié d'un groupe (`groups.events.*`) tel qu'exposé dans
 * `groups.state.events`. `channel_id` référence un salon vocal existant, ou
 * `null` (aucun salon associé, ou salon vocal supprimé depuis — l'événement
 * survit). `rsvped` reflète l'appelant local (clé publique locale dans
 * l'ensemble des RSVP).
 */
export interface GroupEvent {
  event_id: string;
  title: string;
  description: string;
  start_ms: number;
  channel_id: string | null;
  author: string;
  rsvp_count: number;
  rsvped: boolean;
}

/**
 * Dépouillement d'un sondage de salon (`groups.state.polls`, D-048) — la
 * question et les options n'y voyagent jamais : elles vivent dans le message
 * (`MsgBody::Poll`, kind 7, `groups.history`). `counts` est toujours large de
 * `MAX_POLL_OPTIONS` (10), quel que soit le nombre réel d'options du
 * sondage ; les cases au-delà du nombre réel restent à `0` de la part d'un
 * pair honnête, mais un pair malveillant peut voter sur un `option_index`
 * hors bornes réelles (accepté structurellement au repli) — une UI honnête
 * clampe donc `counts` au nombre réel d'options avant affichage (voir
 * `stores/groups.ts#pollResults`).
 */
export interface GroupPoll {
  poll_id: string;
  author: string;
  closed: boolean;
  counts: number[];
  total_votes: number;
  /** Option votée par l'appelant local, ou `null` s'il n'a pas voté. */
  my_vote: number | null;
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

/**
 * Fil de discussion d'un salon (`groups.state.threads`) : se comporte comme un
 * salon dérivé dont `thread_id` sert de `channel_id` aux mêmes RPC d'historique
 * et d'envoi. `parent_channel` est le salon d'origine, `root_msg` le message
 * depuis lequel le fil a été ouvert (affiché en tête du fil, badge sur la
 * racine). Un fil `archived` reste consultable mais est rangé à part.
 */
export interface GroupThread {
  thread_id: string;
  parent_channel: string;
  root_msg: string;
  name: string;
  archived: boolean;
}

export interface GroupStateJson {
  group_id: string;
  name: string;
  /** Racine Merkle de l'icône (hex 64), ou `null` sans icône. */
  icon: string | null;
  /**
   * Racine Merkle de la bannière du serveur (hex 64), ou `null` sans
   * bannière. Champ additif de `SetMeta` ; optionnel par tolérance (nœud
   * plus ancien).
   */
  banner?: string | null;
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
  /**
   * Couleur de fond de la bannière de serveur (entier 0xRRGGBB), ou `null`
   * sans couleur. Champ additif de `SetMeta` ; optionnel par tolérance (nœud
   * plus ancien).
   */
  banner_color?: number | null;
  /** Stickers de serveur, même forme que `groups.stickers.list` (peut manquer). */
  stickers?: ServerSticker[];
  /** Sons de soundboard, même forme que `emojis` (peut manquer, nœud plus ancien). */
  sounds?: ServerSound[];
  /** Événements planifiés du groupe (peut manquer, nœud plus ancien). */
  events?: GroupEvent[];
  /**
   * Sondages du groupe, dépouillement uniquement (peut manquer, nœud plus
   * ancien) — question/options vivent dans le message (`groups.history`).
   */
  polls?: GroupPoll[];
  /**
   * Mots filtrés par l'AutoMod (`groups.automod.set`) : appliqués au rendu
   * par les clients honnêtes (masquage, jamais de suppression réseau).
   * Optionnel par tolérance (nœud plus ancien).
   */
  automod_words?: string[];
  /** Fils de discussion ouverts dans les salons (peut manquer, nœud plus ancien). */
  threads?: GroupThread[];
  /**
   * Ma dernière position lue (lamport) par salon (`channel_id → lamport`) :
   * sert à tracer le séparateur « nouveaux messages » à l'ouverture. `0` (ou
   * salon absent) = jamais lu ; champ entier absent = nœud plus ancien.
   */
  read_marks?: Record<string, number>;
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
  /**
   * Micro forcé coupé par un modérateur de groupe (`groups.voice_moderate`) ;
   * toujours `false` dans une session d'appel 1-à-1. Toujours émis ;
   * optionnel par tolérance (nœud plus ancien).
   */
  server_muted?: boolean;
  /** Sortie forcée coupée par un modérateur ; mêmes remarques que `server_muted`. */
  server_deafened?: boolean;
  /** Porteur de la permission `PRIORITY_SPEAKER` en train de parler. */
  priority_speaker?: boolean;
}

/** Salon vocal actif tel que rendu par voice.status (`null` si aucun). */
export interface VoiceActive {
  group_id: string;
  channel_id: string;
  muted: boolean;
  /** Sortie locale coupée (deafen force le micro coupé, jamais persisté). */
  deafened: boolean;
  participants: VoiceParticipant[];
  /**
   * Distingue une session d'appel 1-à-1 (`group_id` sentinelle, 32 zéros) d'un
   * salon de groupe. Toujours émis ; optionnel par tolérance (nœud plus ancien).
   */
  is_call?: boolean;
}

/** Réglages DSP de capture (voice.status, champ additif `dsp`). */
export interface VoiceDsp {
  noise_suppression: boolean;
  agc: boolean;
}

/**
 * Présence d'un salon vocal connu (rejoint ou non) rendue par voice.rooms :
 * permet d'afficher les occupants d'un salon avant de le rejoindre.
 */
export interface VoiceRoomPresence {
  group_id: string;
  channel_id: string;
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

/** Phase de l'appel 1-à-1 courant (`calls.status`, voir VOICE_CALLS.md §1.3). */
export type CallState = 'idle' | 'outgoing_ringing' | 'incoming_ringing' | 'active';

/** État de l'appel 1-à-1 courant (`calls.status`). */
export interface CallStatus {
  state: CallState;
  peer: string | null;
  call_id: string | null;
  /**
   * Début de la phase courante sur l'horloge interne du moteur (ms depuis son
   * démarrage) — sert de repère pour une durée relative, jamais un temps mural
   * (voir VOICE_CALLS.md §1.1). `null` au repos.
   */
  since_ms: number | null;
}

/** Motif stable de fin d'appel (`event.call_ended.reason`, voir VOICE_CALLS.md §1.2). */
export type CallEndedReason =
  | 'hangup'
  | 'declined'
  | 'busy'
  | 'timeout'
  | 'missed'
  | 'canceled'
  | 'lost'
  | 'superseded';

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
      method: 'event.group_invite_pending';
      params: {
        group_id: string;
        invite_id: string;
        group_name: string;
        inviter: string;
        expires_ms: number;
      };
    }
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
      method: 'event.voice_moderate';
      params: {
        group_id: string;
        pubkey: string;
        server_muted: boolean;
        server_deafened: boolean;
        priority_speaker: boolean;
      };
    }
  | { method: 'event.call_outgoing'; params: { peer: string; call_id: string } }
  | { method: 'event.call_incoming'; params: { peer: string; call_id: string } }
  | { method: 'event.call_accepted'; params: { peer: string; call_id: string } }
  | {
      method: 'event.call_ended';
      params: { peer: string; call_id: string; reason: CallEndedReason };
    }
  | {
      method: 'event.profile';
      params: {
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
  | { method: 'event.dm_ack'; params: { peer: string; msg_id: string } }
  | { method: 'event.dm_pins'; params: { peer: string } }
  | {
      method: 'event.network';
      params: { connected_peers: number; dht_nodes: number };
    }
  | {
      method: 'event.file_progress';
      params: { merkle_root: string; done: number; total: number; complete: boolean };
    }
  | { method: 'event.desynchronise'; params: Record<string, never> }
  | {
      method: 'event.group_event_started';
      params: { group_id: string; event_id: string; title: string };
    }
  | {
      method: 'event.soundboard_play';
      params: {
        group_id: string;
        channel_id: string;
        /** Racine Merkle (hex) du clip à jouer. */
        sound: string;
        /** Clé publique (hex) de l'émetteur, indice de source pour le fetch. */
        from: string;
      };
    };

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
    pronouns: string | null;
    accent_color: number | null;
    banner_color: number | null;
    avatar_decoration: string | null;
    profile_effect: string | null;
  }> {
    return this.rpc.call('profile.get');
  }

  /**
   * Définit le pseudo (2 à 32 caractères), la bio (≤ 2048 caractères, chaîne
   * vide = effacer) et/ou les pronoms (≤ 40 caractères, chaîne vide =
   * effacer) — au moins un champ requis, tout ou rien.
   *
   * `accent_color`/`banner_color`/`avatar_decoration`/`profile_effect` sont
   * tri-états : absent du sous-objet = inchangé, `null` = effacé, valeur =
   * fixé. Utiliser `'key' in changes` (et non `!== undefined`, que
   * `exactOptionalPropertyTypes` interdit d'ailleurs pour ces champs) pour
   * distinguer absent de `null`.
   */
  profileSet(changes: {
    name?: string;
    bio?: string;
    pronouns?: string;
    accent_color?: number | null;
    banner_color?: number | null;
    avatar_decoration?: string | null;
    profile_effect?: string | null;
  }): Promise<Record<string, never>> {
    return this.rpc.call('profile.set', {
      ...(changes.name !== undefined ? { name: changes.name } : {}),
      ...(changes.bio !== undefined ? { bio: changes.bio } : {}),
      ...(changes.pronouns !== undefined ? { pronouns: changes.pronouns } : {}),
      ...('accent_color' in changes ? { accent_color: changes.accent_color } : {}),
      ...('banner_color' in changes ? { banner_color: changes.banner_color } : {}),
      ...('avatar_decoration' in changes
        ? { avatar_decoration: changes.avatar_decoration }
        : {}),
      ...('profile_effect' in changes ? { profile_effect: changes.profile_effect } : {}),
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

  dmHistory(
    pubkey: string,
    limit = 50,
  ): Promise<{ messages: DmMessage[]; peer_read_lamport: number | null }> {
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
  ): Promise<{
    messages: DmMessage[];
    found: boolean;
    peer_read_lamport: number | null;
  }> {
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
   * seuls les salons ayant au moins un non-lu figurent), mentions non lues par
   * groupe (`{ group_id: n }`, seuls les groupes en portant) et mentions non
   * lues par salon (`{ group_id: { channel_id: n } }`, miroir de `unread`) —
   * `unread`, `mentions` et `channel_mentions` optionnels par tolérance.
   */
  groupsList(): Promise<{
    groups: string[];
    unread?: Record<string, Record<string, number>>;
    mentions?: Record<string, number>;
    channel_mentions?: Record<string, Record<string, number>>;
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

  /**
   * Publie la bannière du serveur (image ≤ 512 Kio décodés) et rend sa racine
   * Merkle ; `dataB64: null` retire la bannière (rend `{ banner: null }`).
   * Permission MANAGE_CHANNELS, comme l'icône.
   */
  groupsSetBanner(
    groupId: string,
    mime: string | null,
    dataB64: string | null,
  ): Promise<{ banner: string | null }> {
    return this.rpc.call('groups.set_banner', {
      group_id: groupId,
      ...(mime !== null ? { mime } : {}),
      ...(dataB64 !== null ? { data_b64: dataB64 } : {}),
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
   * Fixe le mode lent d'un salon : délai minimal en secondes entre deux
   * messages d'un même auteur (`0` = désactivé). Le nœud refuse déjà les
   * envois prématurés ; l'UI reflète la même règle (voir `slowmode_exempt`).
   */
  groupsChannelSlowmode(
    groupId: string,
    channelId: string,
    seconds: number,
  ): Promise<{ ok: true }> {
    return this.rpc.call('groups.channel.slowmode', {
      group_id: groupId,
      channel_id: channelId,
      seconds,
    });
  }

  /**
   * Remplace la liste des mots filtrés par l'AutoMod du groupe (masquage au
   * rendu côté clients honnêtes — rien n'est supprimé du réseau).
   */
  groupsAutomodSet(groupId: string, words: string[]): Promise<{ ok: true }> {
    return this.rpc.call('groups.automod.set', { group_id: groupId, words });
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

  /**
   * Ouvre un fil de discussion sur `rootMsg` dans `parentChannel`. Le fil se
   * comporte ensuite comme un salon : ses messages voyagent par les mêmes RPC
   * d'historique et d'envoi, avec `thread_id` en guise de `channel_id`.
   */
  groupsThreadCreate(
    groupId: string,
    parentChannel: string,
    rootMsg: string,
    name: string,
  ): Promise<{ thread_id: string }> {
    return this.rpc.call('groups.thread.create', {
      group_id: groupId,
      parent_channel: parentChannel,
      root_msg: rootMsg,
      name,
    });
  }

  /** Archive (`archived: true`) ou désarchive un fil de discussion. */
  groupsThreadArchive(
    groupId: string,
    threadId: string,
    archived: boolean,
  ): Promise<{ ok: true }> {
    return this.rpc.call('groups.thread.archive', {
      group_id: groupId,
      thread_id: threadId,
      archived,
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
  groupsSetNickname(
    groupId: string,
    name: string,
    member?: string,
  ): Promise<{ ok: true }> {
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

  /**
   * Suppression groupée par un modérateur (`MANAGE_MESSAGES`) : jusqu'à 100
   * messages du salon en une opération. Rend le nombre effectivement supprimé.
   */
  groupsPurge(
    groupId: string,
    channelId: string,
    msgIds: string[],
  ): Promise<{ deleted: number }> {
    return this.rpc.call('groups.purge', {
      group_id: groupId,
      channel_id: channelId,
      msg_ids: msgIds,
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
  filesRead(
    merkleRoot: string,
    hint?: string,
    media?: boolean,
  ): Promise<FilesReadResult> {
    return this.rpc.call('files.read', {
      merkle_root: merkleRoot,
      ...(hint !== undefined ? { hint } : {}),
      // `media: true` plafonne le téléchargement déclenché à 8 Mio côté nœud
      // (rendu d'icône/bannière/avatar/émoji — anti-DoS 2 Gio d'un admin
      // malveillant). Les lectures en ligne sont de toute façon bornées à
      // 8 Mio, donc ce plafond n'affecte aucun contenu affichable légitime.
      ...(media === true ? { media: true } : {}),
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

  /**
   * Publie un fichier du disque par son chemin (`files.share`, jusqu'à 2 Gio) et
   * rend sa référence de pièce jointe — chemin obtenu via le sélecteur natif
   * Tauri (plugin dialog). Chemin d'envoi non plafonné, contrairement à
   * `files.share_bytes` (8 Mio décodés).
   */
  async filesShare(path: string): Promise<FileAttachment> {
    const { file } = await this.rpc.call<{ file: FileAttachment }>('files.share', {
      path,
    });
    return file;
  }

  /**
   * Copie le blob complet d'un fichier vers `path` (`files.save`, sans
   * plafond de taille). Le nœud rend une erreur `NotFound` si le contenu
   * n'est pas encore complet en local (déclencher d'abord son téléchargement).
   */
  async filesSave(merkleRoot: string, path: string): Promise<void> {
    await this.rpc.call('files.save', { merkle_root: merkleRoot, path });
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

  /**
   * Envoie un sticker de serveur (`groups.send` étendu, `sticker` référençant
   * un nom actuellement enregistré) : message dédié, `text`/`reply_to`/
   * `attachments` toujours ignorés côté nœud dès que `sticker` est fourni.
   */
  groupsSendSticker(
    groupId: string,
    channelId: string,
    sticker: string,
  ): Promise<{ msg_id: string }> {
    return this.rpc.call('groups.send', {
      group_id: groupId,
      channel_id: channelId,
      sticker,
    });
  }

  /**
   * Envoie un sondage de salon (`groups.send` étendu, D-048) : la question et
   * les options voyagent dans le message ; `text`/`reply_to`/`attachments`/
   * `sticker` toujours ignorés côté nœud dès que `poll` est fourni. Rend
   * l'identifiant du message et celui du sondage fraîchement généré côté nœud.
   */
  groupsSendPoll(
    groupId: string,
    channelId: string,
    question: string,
    options: string[],
  ): Promise<{ msg_id: string; poll_id: string }> {
    return this.rpc.call('groups.send', {
      group_id: groupId,
      channel_id: channelId,
      poll: { question, options },
    });
  }

  /**
   * Vote (ou change son vote) sur un sondage — choix unique, un second vote
   * du même membre remplace le précédent (pas d'accumulation, D-048 §6.1).
   */
  groupsPollVote(
    groupId: string,
    pollId: string,
    optionIndex: number,
  ): Promise<{ ok: true }> {
    return this.rpc.call('groups.polls.vote', {
      group_id: groupId,
      poll_id: pollId,
      option_index: optionIndex,
    });
  }

  /**
   * Clôture un sondage (auteur du sondage ou `MANAGE_CHANNELS`) — idempotent,
   * clore un sondage déjà clos ne change rien.
   */
  groupsPollClose(groupId: string, pollId: string): Promise<{ ok: true }> {
    return this.rpc.call('groups.polls.close', {
      group_id: groupId,
      poll_id: pollId,
    });
  }

  /**
   * Autorise une invitation à usage unique vers `pubkey` (consentement
   * explicite requis côté invité, D-045 — aucun force-join) ; rend
   * l'identifiant de l'invitation créée.
   */
  groupsInviteCreate(groupId: string, pubkey: string): Promise<{ invite_id: string }> {
    return this.rpc.call('groups.invite_create', { group_id: groupId, pubkey });
  }

  /**
   * Crée un lien d'invitation partageable (`accord://invite/…`) pour ce groupe.
   * `maxUses` absent ou `0` = illimité ; `expiresH` absent = 7 jours, `0` =
   * jamais. Rend le code à copier/partager.
   */
  groupsInviteLinkCreate(
    groupId: string,
    maxUses?: number,
    expiresH?: number,
  ): Promise<{ code: string }> {
    return this.rpc.call('groups.invite_link_create', {
      group_id: groupId,
      ...(maxUses !== undefined ? { max_uses: maxUses } : {}),
      ...(expiresH !== undefined ? { expires_h: expiresH } : {}),
    });
  }

  /**
   * Consomme un lien d'invitation partageable : rejoint le groupe si le code
   * est valide. Rend l'identifiant et le nom du groupe rejoint.
   */
  groupsInviteLinkRedeem(
    code: string,
  ): Promise<{ ok: boolean; group_id: string; group_name: string }> {
    return this.rpc.call('groups.invite_link_redeem', { code });
  }

  /**
   * Décode un lien d'invitation SANS le consommer ni rien télécharger : rend
   * les métadonnées du serveur (nom, icône, bannière, couleur) pour l'aperçu
   * riche affiché sous un message. Le nœud reste l'autorité de décodage.
   */
  groupsInviteLinkInfo(link: string): Promise<{
    group_id: string;
    invite_id: string;
    inviter: string;
    group_name: string;
    icon: string | null;
    banner: string | null;
    banner_color: number | null;
  }> {
    return this.rpc.call('groups.invite_link_info', { link });
  }

  /** Invitations entrantes en attente (reçues, ni acceptées ni refusées). */
  groupsInvitesList(): Promise<{ invites: PendingInvite[] }> {
    return this.rpc.call('groups.invites_list');
  }

  /**
   * Accepte une invitation reçue : le groupe se matérialise localement via
   * les événements `event.group_state`/`event.group_op`/`event.group_key`
   * qui suivent, une fois l'inviteur notifié.
   */
  groupsInviteAccept(groupId: string, inviteId: string): Promise<{ ok: true }> {
    return this.rpc.call('groups.invite_accept', {
      group_id: groupId,
      invite_id: inviteId,
    });
  }

  /** Refuse une invitation reçue. */
  groupsInviteDecline(groupId: string, inviteId: string): Promise<{ ok: true }> {
    return this.rpc.call('groups.invite_decline', {
      group_id: groupId,
      invite_id: inviteId,
    });
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
   * Ajoute (ou remplace) un son de soundboard (`MANAGE_EMOJIS`) : nom
   * `[a-z0-9_]` 2-32, clip audio ≤ 256 Kio décodés (OGG/MP3/MP4/WebM/WAV).
   * Rend la racine Merkle du clip publié.
   */
  groupsSoundsAdd(
    groupId: string,
    name: string,
    mime: string,
    dataB64: string,
  ): Promise<{ merkle_root: string }> {
    return this.rpc.call('groups.sounds.add', {
      group_id: groupId,
      name,
      mime,
      data_b64: dataB64,
    });
  }

  /** Supprime un son de soundboard par son nom (`MANAGE_EMOJIS`). */
  groupsSoundsDel(groupId: string, name: string): Promise<{ ok: true }> {
    return this.rpc.call('groups.sounds.del', { group_id: groupId, name });
  }

  /**
   * Joue un son de soundboard dans le salon vocal actif : le nœud le diffuse
   * aux participants présents. Refusé si l'appelant n'est pas connecté au
   * salon vocal `(group_id, channel_id)` — à n'appeler que depuis ce contexte.
   */
  groupsSoundboardPlay(
    groupId: string,
    channelId: string,
    soundName: string,
  ): Promise<{ ok: true }> {
    return this.rpc.call('groups.soundboard.play', {
      group_id: groupId,
      channel_id: channelId,
      sound_name: soundName,
    });
  }

  /**
   * Crée un événement planifié (`MANAGE_CHANNELS`) : `description` vide et
   * `channelId: null` (aucun salon associé) sont des valeurs explicites
   * valides, jamais omises — l'édition réécrit intégralement les mêmes champs.
   */
  groupsEventsCreate(
    groupId: string,
    fields: {
      title: string;
      description: string;
      startMs: number;
      channelId: string | null;
    },
  ): Promise<{ event_id: string }> {
    return this.rpc.call('groups.events.create', {
      group_id: groupId,
      title: fields.title,
      description: fields.description,
      start_ms: fields.startMs,
      channel_id: fields.channelId,
    });
  }

  /**
   * Réécrit intégralement un événement (`MANAGE_CHANNELS` ou auteur) — pas de
   * fusion partielle ; les RSVP existants sont conservés côté nœud.
   */
  groupsEventsEdit(
    groupId: string,
    eventId: string,
    fields: {
      title: string;
      description: string;
      startMs: number;
      channelId: string | null;
    },
  ): Promise<{ ok: true }> {
    return this.rpc.call('groups.events.edit', {
      group_id: groupId,
      event_id: eventId,
      title: fields.title,
      description: fields.description,
      start_ms: fields.startMs,
      channel_id: fields.channelId,
    });
  }

  /** Supprime un événement planifié (`MANAGE_CHANNELS` ou auteur). */
  groupsEventsDelete(groupId: string, eventId: string): Promise<{ ok: true }> {
    return this.rpc.call('groups.events.delete', {
      group_id: groupId,
      event_id: eventId,
    });
  }

  /**
   * RSVP « je suis intéressé·e » (`interested` par défaut `true`) sur son
   * propre RSVP ; `false` le retire. Dédoublonné côté nœud par
   * `(event_id, membre)`.
   */
  groupsEventsRsvp(
    groupId: string,
    eventId: string,
    interested?: boolean,
  ): Promise<{ ok: true }> {
    return this.rpc.call('groups.events.rsvp', {
      group_id: groupId,
      event_id: eventId,
      ...(interested !== undefined ? { interested } : {}),
    });
  }

  /**
   * Ajoute (ou remplace) un sticker de serveur (`MANAGE_EMOJIS`) : nom
   * `[a-z0-9_]` 2-32, image ≤ 512 Kio décodés. Rend la racine Merkle publiée.
   */
  groupsStickersAdd(
    groupId: string,
    name: string,
    dataB64: string,
    mime: string,
  ): Promise<{ merkle_root: string }> {
    return this.rpc.call('groups.stickers.add', {
      group_id: groupId,
      name,
      data_b64: dataB64,
      mime,
    });
  }

  /** Supprime un sticker de serveur par son nom (`MANAGE_EMOJIS`). */
  groupsStickersRemove(groupId: string, name: string): Promise<{ ok: true }> {
    return this.rpc.call('groups.stickers.remove', { group_id: groupId, name });
  }

  /** Liste des stickers de serveur (aussi exposée par `groups.state.stickers`). */
  groupsStickersList(groupId: string): Promise<{ stickers: ServerSticker[] }> {
    return this.rpc.call('groups.stickers.list', { group_id: groupId });
  }

  /**
   * Publie (ou efface, `image` omis) l'avatar de serveur — strictement
   * self-service, aucun paramètre pour désigner un autre membre. `image`
   * présent ⇒ ≤ 512 Kio décodés, `mime` libre `image/*`. Rend la nouvelle
   * racine Merkle, ou `null` une fois effacé.
   */
  groupsSetMemberAvatar(
    groupId: string,
    image?: { dataB64: string; mime: string },
  ): Promise<{ avatar: string | null }> {
    return this.rpc.call('groups.set_member_avatar', {
      group_id: groupId,
      ...(image !== undefined ? { data_b64: image.dataB64, mime: image.mime } : {}),
    });
  }

  /**
   * Fixe (ou efface avec `null`) la couleur de fond de la bannière de serveur
   * (`0xRRGGBB`, ≤ 24 bits). `color` est requis explicitement (contrat).
   */
  groupsSetBannerColor(groupId: string, color: number | null): Promise<{ ok: true }> {
    return this.rpc.call('groups.set_banner_color', { group_id: groupId, color });
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

  /**
   * État vocal courant (`active: null` hors salon), pour resynchronisation.
   * `dsp` : réglages de suppression de bruit / AGC persistés, toujours émis ;
   * optionnel par tolérance (nœud plus ancien).
   */
  voiceStatus(): Promise<{
    active: VoiceActive | null;
    master_volume: number;
    dsp?: VoiceDsp;
  }> {
    return this.rpc.call('voice.status');
  }

  /**
   * Présence de tous les salons vocaux connus (rejoints ou non) : occupants
   * visibles avant de rejoindre. Présence passive alimentée par les
   * diffusions `VoiceSignal` des pairs (TTL 90 s côté nœud).
   */
  voiceRooms(): Promise<{ rooms: VoiceRoomPresence[] }> {
    return this.rpc.call('voice.rooms');
  }

  /** Active/désactive la suppression de bruit (RNNoise) sur la capture locale, à chaud. */
  voiceSetNoiseSuppression(enabled: boolean): Promise<Record<string, never>> {
    return this.rpc.call('voice.set_noise_suppression', { enabled });
  }

  /** Active/désactive le contrôle automatique de gain sur la capture locale, à chaud. */
  voiceSetAgc(enabled: boolean): Promise<Record<string, never>> {
    return this.rpc.call('voice.set_agc', { enabled });
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
   * Démarre un appel 1-à-1 vers `peer` (ami confirmé requis) ; rend son
   * identifiant. Refusé si un appel est déjà en cours (toute phase confondue,
   * `calls.start` rend une erreur) — voir VOICE_CALLS.md §1.
   */
  callsStart(peer: string): Promise<{ call_id: string }> {
    return this.rpc.call('calls.start', { peer });
  }

  /** Accepte l'appel entrant en sonnerie (`call_id` doit correspondre). */
  callsAccept(callId: string): Promise<{ ok: true }> {
    return this.rpc.call('calls.accept', { call_id: callId });
  }

  /** Refuse l'appel entrant en sonnerie. */
  callsDecline(callId: string): Promise<{ ok: true }> {
    return this.rpc.call('calls.decline', { call_id: callId });
  }

  /**
   * Raccroche : couvre les trois phases (annule une sonnerie sortante,
   * refuse une sonnerie entrante, raccroche un appel actif). Idempotent au
   * repos.
   */
  callsHangup(): Promise<{ ok: true }> {
    return this.rpc.call('calls.hangup');
  }

  /** État de l'appel 1-à-1 courant, pour resynchronisation (connexion/reprise). */
  callsStatus(): Promise<CallStatus> {
    return this.rpc.call('calls.status');
  }

  /**
   * Force la sourdine (`mute`) et/ou la surdité (`deafen`) d'un membre dans
   * tous les salons vocaux du groupe (permission `KICK`, hiérarchie de rôles,
   * fondateur intouchable — vérifié côté nœud). Absents = `false` ;
   * `{ mute: false, deafen: false }` lève la modération du membre.
   */
  groupsVoiceModerate(
    groupId: string,
    pubkey: string,
    mute: boolean,
    deafen: boolean,
  ): Promise<{ ok: true }> {
    return this.rpc.call('groups.voice_moderate', {
      group_id: groupId,
      pubkey,
      mute,
      deafen,
    });
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
