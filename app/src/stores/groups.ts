/**
 * Groupes : liste, états matérialisés (salons, catégories, rôles, membres,
 * bannis, permissions), historiques paginés, épinglés et actions de gestion.
 * `event.group_state` (câblé dans AppShell) déclenche `handleGroupState`.
 */

import { create } from 'zustand';
import { api, rpc } from '../lib/client';
import type {
  Contact,
  FileAttachment,
  GroupCategory,
  GroupChannel,
  GroupChannelKind,
  GroupEvent,
  GroupMember,
  GroupMessage,
  GroupPoll,
  GroupRole,
  GroupStateJson,
  GroupThread,
  PendingInvite,
  ServerEmoji,
} from '../lib/api';
import {
  PAGE_SIZE,
  fetchGroupPage,
  mergeOlderPage,
  mergeRecentPage,
  sortAscending,
} from '../lib/history';
import { POLL_MAX_OPTIONS } from '../lib/poll';
import { avatarOf, useFriends } from './friends';

/** Clé d'index des historiques de salon (aussi comprise par lib/search). */
export function channelKey(groupId: string, channelId: string): string {
  return `${groupId}/${channelId}`;
}

/* ------------------------------------------------------------------ */
/* Aides pures : permissions, couleurs de rôle, tris par position.     */
/* ------------------------------------------------------------------ */

/** Bits de permission du contrat (API.md §Groupes). */
export const PERMISSIONS = {
  VIEW: 0x1,
  SEND: 0x2,
  MANAGE_MESSAGES: 0x4,
  MANAGE_CHANNELS: 0x8,
  INVITE: 0x10,
  KICK: 0x20,
  BAN: 0x40,
  MANAGE_ROLES: 0x80,
  ADMIN: 0x100,
  MANAGE_EMOJIS: 0x200,
} as const;

/** Vrai si `mask` accorde `bit` — `ADMIN` implique toutes les permissions. */
export function hasPerm(mask: number, bit: number): boolean {
  if ((mask & PERMISSIONS.ADMIN) !== 0) return true;
  return (mask & bit) === bit;
}

/** Couleur CSS (`#rrggbb`) d'un entier RGB du contrat (`0xRRGGBB`). */
export function roleColorCss(color: number): string {
  return `#${(color & 0xffffff).toString(16).padStart(6, '0')}`;
}

/**
 * Couleur affichée d'un membre : celle de son rôle de position la plus
 * haute dont la couleur n'est pas 0. `null` = couleur par défaut du thème.
 */
export function memberColor(
  member: GroupMember | undefined,
  roles: readonly GroupRole[],
): string | null {
  if (member === undefined) return null;
  const owned = new Set(member.roles);
  let best: GroupRole | null = null;
  for (const role of roles) {
    if (!owned.has(role.role_id) || role.color === 0) continue;
    if (best === null || role.position > best.position) best = role;
  }
  return best === null ? null : roleColorCss(best.color);
}

/** Position du rôle le plus haut d'un membre (−1 sans rôle). */
export function highestRolePosition(
  member: GroupMember | undefined,
  roles: readonly GroupRole[],
): number {
  if (member === undefined) return -1;
  const owned = new Set(member.roles);
  let best = -1;
  for (const role of roles) {
    if (owned.has(role.role_id) && role.position > best) best = role.position;
  }
  return best;
}

/**
 * Vrai si l'utilisateur local peut forcer la modération vocale
 * (`groups.voice_moderate`) de `targetPubkey` dans ce groupe : permission
 * `KICK`, cible ni le fondateur ni soi-même. Même convention côté UI que
 * kick/ban/timeout dans `ServerMembersTab` (pas de hiérarchie de rôles
 * calculée côté client) — le nœud revérifie la hiérarchie complète de toute
 * façon (permission vérifiée au rejeu ET à l'émission, voir VOICE_CALLS.md §3).
 */
export function canModerateVoice(
  state: Pick<GroupStateJson, 'my_permissions' | 'founder'>,
  selfPubkey: string | null,
  targetPubkey: string,
): boolean {
  if (selfPubkey === null || targetPubkey === selfPubkey) return false;
  if (state.founder === targetPubkey) return false;
  return hasPerm(state.my_permissions, PERMISSIONS.KICK);
}

/**
 * Pseudo de serveur d'un membre (`state.members[].nickname`), ou `null`
 * lorsqu'il est absent, vide ou ne contient que des espaces. Les composants
 * l'utilisent avec un repli sur le pseudo global.
 */
export function nicknameOf(
  state: Pick<GroupStateJson, 'members'> | undefined,
  pubkey: string,
): string | null {
  const nickname = state?.members.find((m) => m.pubkey === pubkey)?.nickname;
  return nickname != null && nickname.trim() !== '' ? nickname : null;
}

/**
 * Avatar de serveur affichable d'un membre : l'override self-service
 * (`state.members[].avatar`) s'il est présent, sinon l'avatar global du
 * contact ami connu (`avatarOf`). Ne connaît pas l'identité locale : pour
 * soi-même, l'appelant complète avec son propre repli (`self.avatar`) via
 * `serverAvatarOf(state, contacts, pubkey) ?? self.avatar` — même convention
 * que `nicknameOf`, qui ne sait pas non plus distinguer soi-même.
 */
export function serverAvatarOf(
  state: Pick<GroupStateJson, 'members'> | undefined,
  contacts: readonly Contact[],
  pubkey: string,
): string | null {
  const override = state?.members.find((m) => m.pubkey === pubkey)?.avatar;
  if (override != null) return override;
  return avatarOf(contacts, pubkey);
}

/**
 * Échéance murale (ms) de la sourdine active d'un membre, ou `null` si aucune
 * sourdine n'est active (`0`, absente ou échéance déjà passée). Comparée à
 * `now` — une échéance passée est sans effet, comme côté nœud.
 */
export function timeoutUntil(
  member: Pick<GroupMember, 'timeout_until_ms'> | undefined,
  now: number = Date.now(),
): number | null {
  const until = member?.timeout_until_ms ?? 0;
  return until > now ? until : null;
}

/** Salons triés par position croissante (départage stable par id). */
export function sortChannels(channels: readonly GroupChannel[]): GroupChannel[] {
  return [...channels].sort(
    (a, b) => a.position - b.position || a.channel_id.localeCompare(b.channel_id),
  );
}

/** Catégories triées par position croissante (départage stable par id). */
export function sortCategories(categories: readonly GroupCategory[]): GroupCategory[] {
  return [...categories].sort(
    (a, b) => a.position - b.position || a.category_id.localeCompare(b.category_id),
  );
}

/** Rôles triés du plus haut au plus bas (position décroissante). */
export function sortRoles(roles: readonly GroupRole[]): GroupRole[] {
  return [...roles].sort(
    (a, b) => b.position - a.position || a.role_id.localeCompare(b.role_id),
  );
}

/** Maximum wire position of a role (u16). */
const MAX_ROLE_POSITION = 0xffff;

/**
 * Position edits required to move a role one step up or down in the
 * displayed order (descending positions). Distinct positions are swapped;
 * on a tie (display order decided by id) the role that must end up higher
 * is raised by one. Returns `[]` when there is no neighbor.
 */
export function planRoleMove(
  roles: readonly GroupRole[],
  roleId: string,
  direction: 'up' | 'down',
): Array<{ role_id: string; position: number }> {
  const sorted = sortRoles(roles);
  const i = sorted.findIndex((r) => r.role_id === roleId);
  if (i === -1) return [];
  const moving = sorted[i];
  const neighbor = sorted[direction === 'up' ? i - 1 : i + 1];
  if (moving === undefined || neighbor === undefined) return [];
  if (moving.position !== neighbor.position) {
    return [
      { role_id: moving.role_id, position: neighbor.position },
      { role_id: neighbor.role_id, position: moving.position },
    ];
  }
  const raised = direction === 'up' ? moving : neighbor;
  return [
    {
      role_id: raised.role_id,
      position: Math.min(moving.position + 1, MAX_ROLE_POSITION),
    },
  ];
}

/**
 * Override courant d'un rôle sur un salon (`{ allow: 0, deny: 0 }` si
 * aucun) — l'état peut omettre `overrides` (nœud plus ancien).
 */
export function overrideOf(
  state: Pick<GroupStateJson, 'overrides'> | undefined,
  channelId: string,
  roleId: string,
): { allow: number; deny: number } {
  const found = (state?.overrides ?? []).find(
    (o) => o.channel_id === channelId && o.role_id === roleId,
  );
  return found === undefined
    ? { allow: 0, deny: 0 }
    : { allow: found.allow, deny: found.deny };
}

/**
 * Permissions effectives de l'utilisateur local dans un salon donné : la base
 * globale (`my_permissions`) enrichie des overrides des rôles qu'il porte dans
 * ce salon (`deny` prioritaire sur `allow`). Reflète `GroupState::permissions_in`
 * côté nœud. ADMIN/fondateur (bit ADMIN présent) court-circuite les overrides.
 */
export function myChannelPermissions(
  state: Pick<GroupStateJson, 'my_permissions' | 'members' | 'overrides'>,
  channelId: string,
  selfPubkey: string | null,
): number {
  const base = state.my_permissions;
  if (hasPerm(base, PERMISSIONS.ADMIN) || selfPubkey === null) return base;
  const member = state.members.find((m) => m.pubkey === selfPubkey);
  if (member === undefined) return base;
  const owned = new Set(member.roles);
  let allow = 0;
  let deny = 0;
  for (const o of state.overrides ?? []) {
    if (o.channel_id !== channelId || !owned.has(o.role_id)) continue;
    allow |= o.allow;
    deny |= o.deny;
  }
  return (base | allow) & ~deny;
}

/**
 * Vrai si `channel` est un salon d'annonces où l'utilisateur local ne peut pas
 * écrire (pas de `MANAGE_CHANNELS` effectif) : le composeur passe en lecture
 * seule tandis que le salon reste consultable. Symétrique de la porte
 * d'émission côté nœud (`ChannelKind::Announcement` + `MANAGE_CHANNELS`).
 */
export function isChannelReadOnly(
  state: Pick<GroupStateJson, 'my_permissions' | 'members' | 'overrides'>,
  channel: Pick<GroupChannel, 'channel_id' | 'kind'>,
  selfPubkey: string | null,
): boolean {
  if (channel.kind !== 'announcement') return false;
  const eff = myChannelPermissions(state, channel.channel_id, selfPubkey);
  return !hasPerm(eff, PERMISSIONS.MANAGE_CHANNELS);
}

/**
 * Vrai si `channelId` porte au moins un override de rôle qui refuse VIEW ou
 * SEND (`overrides[].deny`, prioritaire sur `allow` — `GroupState::apply`
 * côté nœud). Accord n'a pas de rôle « @everyone » implicite : VIEW+SEND sont
 * accordés à tout membre par défaut (D-015) et un override ne s'applique
 * qu'aux membres portant le rôle concerné ([`myChannelPermissions`]). Ce
 * drapeau signale donc un salon dont l'accès n'est pas uniforme pour tous les
 * rôles (au moins une exception existe), pas un refus opposable à tout le
 * monde — c'est l'information la plus proche que l'état matérialisé expose
 * pour un indicateur « salon restreint » dans la barre latérale.
 */
export function isChannelRestricted(
  state: Pick<GroupStateJson, 'overrides'> | undefined,
  channelId: string,
): boolean {
  const restrictedBits = PERMISSIONS.VIEW | PERMISSIONS.SEND;
  return (state?.overrides ?? []).some(
    (o) => o.channel_id === channelId && (o.deny & restrictedBits) !== 0,
  );
}

/**
 * Vrai si l'utilisateur local voit `channelId` (VIEW effectif via
 * [`myChannelPermissions`]). Le nœud envoie tous les salons du groupe à tout
 * membre (`groups.state` ne filtre pas par permission) : ce filtre reproduit
 * côté UI la visibilité que `GroupState::permissions_in` calcule côté nœud,
 * pour que la barre latérale masque les salons où VIEW est refusé.
 */
export function isChannelVisible(
  state: Pick<GroupStateJson, 'my_permissions' | 'members' | 'overrides'> | undefined,
  channelId: string,
  selfPubkey: string | null,
): boolean {
  if (state === undefined) return true;
  return hasPerm(myChannelPermissions(state, channelId, selfPubkey), PERMISSIONS.VIEW);
}

/** Section de salons : `category` vaut `null` pour les sans-catégorie. */
export interface ChannelGroup {
  category: GroupCategory | null;
  channels: GroupChannel[];
}

/**
 * Regroupe les salons par catégorie : les sans-catégorie d'abord (y compris
 * ceux dont la catégorie n'existe plus), puis les catégories par position.
 */
export function channelsByCategory(
  channels: readonly GroupChannel[],
  categories: readonly GroupCategory[],
): ChannelGroup[] {
  const sorted = sortChannels(channels);
  const known = new Set(categories.map((c) => c.category_id));
  const groups: ChannelGroup[] = [
    {
      category: null,
      channels: sorted.filter((c) => c.category === null || !known.has(c.category)),
    },
  ];
  for (const category of sortCategories(categories)) {
    groups.push({
      category,
      channels: sorted.filter((c) => c.category === category.category_id),
    });
  }
  return groups;
}

/** Émoji custom agrégé : nom, racine Merkle et groupe d'origine (indice de pair). */
export interface AggregatedEmoji extends ServerEmoji {
  groupId: string;
}

/**
 * Agrège les émojis custom de tous les groupes rejoints (`ids`), dédupliqués
 * par nom : en cas de collision entre serveurs, le premier groupe rencontré
 * dans `ids` l'emporte (ordre stable et déterministe). Sert à proposer et
 * afficher les émojis custom en MP, où aucun `groupId` de contexte de serveur
 * n'existe — l'image reste ensuite chargée par sa racine Merkle, indépendante
 * du groupe (contenu adressé, voir `lib/files.lireFichier`).
 */
export function aggregateEmojis(
  ids: readonly string[],
  states: Readonly<Record<string, GroupStateJson>>,
): AggregatedEmoji[] {
  const seen = new Set<string>();
  const out: AggregatedEmoji[] = [];
  for (const groupId of ids) {
    for (const emoji of states[groupId]?.emojis ?? []) {
      if (seen.has(emoji.name)) continue;
      seen.add(emoji.name);
      out.push({ ...emoji, groupId });
    }
  }
  return out;
}

/** Carte nom → racine Merkle agrégée sur tous les groupes rejoints (voir `aggregateEmojis`). */
export function aggregateEmojiMap(
  ids: readonly string[],
  states: Readonly<Record<string, GroupStateJson>>,
): Map<string, string> {
  return new Map(
    aggregateEmojis(ids, states).map((e) => [e.name, e.merkle_root] as const),
  );
}

/** Événements triés par échéance croissante (départage stable par id). */
export function sortEvents(events: readonly GroupEvent[]): GroupEvent[] {
  return [...events].sort(
    (a, b) => a.start_ms - b.start_ms || a.event_id.localeCompare(b.event_id),
  );
}

/**
 * Événements planifiés dont l'échéance n'est pas encore passée, triés par
 * date croissante — alimente le badge de compte de la barre latérale et la
 * mise en avant du panneau d'événements.
 */
export function upcomingEvents(
  state: Pick<GroupStateJson, 'events'> | undefined,
  now: number = Date.now(),
): GroupEvent[] {
  return sortEvents((state?.events ?? []).filter((e) => e.start_ms > now));
}

/**
 * Sondage d'un groupe par son identifiant (`groups.state.polls`), ou
 * `undefined` si l'état n'a pas encore convergé (sondage tout juste envoyé,
 * `event.group_state` pas encore rejoué) — voir `pollResults` pour un repli
 * sûr et toujours de la bonne forme.
 */
export function pollOf(
  state: Pick<GroupStateJson, 'polls'> | undefined,
  pollId: string,
): GroupPoll | undefined {
  return state?.polls?.find((p) => p.poll_id === pollId);
}

/** Dépouillement d'un sondage prêt à l'affichage (voir `pollResults`). */
export interface PollResults {
  /**
   * Décomptes bornés au nombre RÉEL d'options du message — jamais les 10
   * cases brutes de `GroupPoll.counts`. Un vote forgé sur un `option_index`
   * hors bornes réelles (accepté structurellement au repli, D-048 §6.1)
   * n'apparaît donc jamais dans une case affichée.
   */
  counts: number[];
  /** Somme des décomptes réels seulement (ignore tout vote fantôme éventuellement compté dans `total_votes`). */
  total: number;
  /** Pourcentage entier par option réelle (0 sans aucun vote). */
  percentages: number[];
  /** Option votée localement, `null` si absente ou si son index dépasse les options réelles. */
  myVote: number | null;
}

/**
 * Dépouillement défensif d'un sondage : clampe `poll.counts` (toujours large
 * de `MAX_POLL_OPTIONS`, quel que soit le nombre réel d'options du message)
 * au nombre réel d'options (`optionCount`, tiré du corps du message, jamais
 * de l'op-log). `poll` absent (état pas encore convergé) rend un
 * dépouillement à zéro, toujours de la forme attendue par l'UI.
 */
export function pollResults(
  poll: GroupPoll | undefined,
  optionCount: number,
): PollResults {
  const safeCount = Math.max(0, Math.min(optionCount, POLL_MAX_OPTIONS));
  const counts = Array.from({ length: safeCount }, (_, i) => poll?.counts[i] ?? 0);
  const total = counts.reduce((sum, c) => sum + c, 0);
  const percentages = counts.map((c) => (total > 0 ? Math.round((c / total) * 100) : 0));
  const myVote =
    poll?.my_vote !== undefined && poll.my_vote !== null && poll.my_vote < safeCount
      ? poll.my_vote
      : null;
  return { counts, total, percentages, myVote };
}

/**
 * Fils d'un salon (`state.threads` filtré par `parent_channel`), ordre du nœud
 * conservé. Rend `[]` tant que l'état n'expose pas de fils (nœud plus ancien ou
 * état pas encore chargé).
 */
export function channelThreads(
  state: Pick<GroupStateJson, 'threads'> | undefined,
  channelId: string,
): GroupThread[] {
  return (state?.threads ?? []).filter((th) => th.parent_channel === channelId);
}

/**
 * Fil dont `root_msg` est `msgId`, ou `undefined` si ce message n'a pas encore
 * de fil — sert au menu (« Créer » vs « Ouvrir ») et à la pastille de racine.
 */
export function threadOfRoot(
  state: Pick<GroupStateJson, 'threads'> | undefined,
  msgId: string,
): GroupThread | undefined {
  return (state?.threads ?? []).find((th) => th.root_msg === msgId);
}

/* ------------------------------------------------------------------ */
/* Store.                                                              */
/* ------------------------------------------------------------------ */

/** Copie des historiques où `msgId` (dans `key`) est transformé par `patch`. */
function patchMessages(
  messages: Record<string, GroupMessage[]>,
  key: string,
  msgId: string,
  patch: (message: GroupMessage) => GroupMessage,
): Record<string, GroupMessage[]> {
  const existing = messages[key];
  if (existing === undefined) return messages;
  return {
    ...messages,
    [key]: existing.map((m) => (m.msg_id === msgId ? patch(m) : m)),
  };
}

interface GroupsState {
  ids: string[];
  states: Record<string, GroupStateJson>;
  /** Messages par `groupId/channelId`, du plus ancien au plus récent. */
  messages: Record<string, GroupMessage[]>;
  /** Vrai si des messages plus anciens existent probablement côté nœud. */
  hasMore: Record<string, boolean>;
  /** Garde anti-rafale du chargement vers le haut. */
  loadingOlder: Record<string, boolean>;
  /** Identifiants épinglés par `groupId/channelId` (ordre du nœud). */
  pins: Record<string, string[]>;
  /** Non-lus par groupe puis salon (`groups.list`) : seuls les > 0 figurent. */
  unread: Record<string, Record<string, number>>;
  /** Mentions non lues par groupe (`groups.list`) : seuls les > 0 figurent. */
  mentions: Record<string, number>;
  /**
   * Invitations de serveur entrantes en attente (consentement explicite,
   * D-045) : reçues, ni acceptées ni refusées. Chargées au démarrage et
   * complétées par `handleInvitePending` (`event.group_invite_pending`).
   */
  pendingInvites: PendingInvite[];
  loadList: () => Promise<void>;
  /** Recharge uniquement les invitations entrantes en attente. */
  loadPendingInvites: () => Promise<void>;
  /**
   * Ajoute une invitation reçue (`event.group_invite_pending`), dédupliquée
   * par couple `(group_id, invite_id)`.
   */
  handleInvitePending: (invite: PendingInvite) => void;
  /**
   * Accepte une invitation reçue : retire l'entrée localement (optimiste),
   * la restaure et propage l'erreur si le nœud refuse.
   */
  acceptInvite: (groupId: string, inviteId: string) => Promise<void>;
  /** Refuse une invitation reçue — même sémantique optimiste qu'`acceptInvite`. */
  declineInvite: (groupId: string, inviteId: string) => Promise<void>;
  /** Recharge uniquement les compteurs de non-lus (sans recharger les états). */
  refreshUnread: () => Promise<void>;
  /** Marque le salon lu jusqu'à `lamport` puis rafraîchit les non-lus. */
  markRead: (groupId: string, channelId: string, lamport: number) => Promise<void>;
  loadState: (groupId: string) => Promise<void>;
  /** Recharge l'état (et les épinglés consultés) sur `event.group_state`. */
  handleGroupState: (groupId: string) => Promise<void>;
  /** Charge (ou rafraîchit) la page récente, fusionnée sans rechargement. */
  refreshHistory: (groupId: string, channelId: string) => Promise<void>;
  /** Charge la page précédant le plus ancien message connu du salon. */
  loadOlderHistory: (groupId: string, channelId: string) => Promise<void>;
  /**
   * S'assure que `msgId` est chargé (fenêtre `groups.history_around` fusionnée
   * si besoin). Rend `true` si le message est disponible localement au retour,
   * `false` si le nœud l'ignore (fenêtre `found: false`).
   */
  jumpTo: (groupId: string, channelId: string, msgId: string) => Promise<boolean>;
  create: (name: string, defaultChannel: string) => Promise<string>;
  rename: (groupId: string, name: string) => Promise<void>;
  setIcon: (groupId: string, dataB64: string, mime: string) => Promise<void>;
  /**
   * Publie la bannière du serveur (image ≤ 512 Kio) puis recharge l'état ;
   * `dataB64` et `mime` à `null` retirent la bannière (même flux que l'icône).
   */
  setBanner: (
    groupId: string,
    dataB64: string | null,
    mime: string | null,
  ) => Promise<void>;
  setTopic: (groupId: string, channelId: string, topic: string) => Promise<void>;
  addChannel: (
    groupId: string,
    name: string,
    kind?: GroupChannelKind,
    category?: string,
  ) => Promise<string>;
  renameChannel: (groupId: string, channelId: string, name: string) => Promise<void>;
  /** Déplace un salon dans une catégorie (`null` = sans catégorie). */
  setChannelCategory: (
    groupId: string,
    channelId: string,
    category: string | null,
  ) => Promise<void>;
  /**
   * Fixe l'override d'un rôle sur un salon (`allow`/`deny` bitfields,
   * `deny` prioritaire) ; `allow = deny = 0` efface l'override.
   */
  setChannelPerms: (
    groupId: string,
    channelId: string,
    roleId: string,
    allow: number,
    deny: number,
  ) => Promise<void>;
  /** Fixe le mode lent d'un salon en secondes (`0` = désactivé) puis recharge. */
  setSlowmode: (groupId: string, channelId: string, seconds: number) => Promise<void>;
  /**
   * Remplace la liste des mots filtrés par l'AutoMod (masquage au rendu côté
   * clients honnêtes) puis recharge l'état.
   */
  setAutomodWords: (groupId: string, words: string[]) => Promise<void>;
  deleteChannel: (groupId: string, channelId: string) => Promise<void>;
  /**
   * Ouvre un fil sur `rootMsg` dans `parentChannel` puis recharge l'état (le
   * fil apparaît dans `states[groupId].threads`). Rend le `thread_id` créé.
   */
  createThread: (
    groupId: string,
    parentChannel: string,
    rootMsg: string,
    name: string,
  ) => Promise<string>;
  /** Archive ou désarchive un fil puis recharge l'état. */
  archiveThread: (
    groupId: string,
    threadId: string,
    archived: boolean,
  ) => Promise<void>;
  addCategory: (groupId: string, name: string) => Promise<string>;
  renameCategory: (groupId: string, categoryId: string, name: string) => Promise<void>;
  /** Supprime la catégorie ; ses salons restent, sans catégorie. */
  deleteCategory: (groupId: string, categoryId: string) => Promise<void>;
  kick: (groupId: string, pubkey: string) => Promise<void>;
  ban: (groupId: string, pubkey: string) => Promise<void>;
  unban: (groupId: string, pubkey: string) => Promise<void>;
  /** Met un membre en sourdine jusqu'à `untilMs` (permission KICK) puis recharge. */
  timeout: (groupId: string, pubkey: string, untilMs: number) => Promise<void>;
  /** Lève la sourdine d'un membre puis recharge l'état. */
  clearTimeout: (groupId: string, pubkey: string) => Promise<void>;
  /**
   * Force (ou lève avec `mute: false, deafen: false`) la modération vocale
   * d'un membre dans tous les salons vocaux du groupe (permission `KICK`,
   * vérifiée côté nœud) puis recharge l'état.
   */
  voiceModerate: (
    groupId: string,
    pubkey: string,
    mute: boolean,
    deafen: boolean,
  ) => Promise<void>;
  /**
   * Fixe (ou efface avec une chaîne vide) le pseudo de serveur d'un membre —
   * `member` absent = soi-même — puis recharge l'état.
   */
  setNickname: (groupId: string, name: string, member?: string) => Promise<void>;
  /** Quitte le groupe et l'efface localement (liste, état, historiques). */
  leave: (groupId: string) => Promise<void>;
  addRole: (
    groupId: string,
    name: string,
    color: number,
    permissions: number,
  ) => Promise<string>;
  editRole: (
    groupId: string,
    roleId: string,
    changes: { name?: string; color?: number; position?: number; permissions?: number },
  ) => Promise<void>;
  deleteRole: (groupId: string, roleId: string) => Promise<void>;
  /** Monte ou descend un rôle d'un cran dans la hiérarchie affichée. */
  moveRole: (groupId: string, roleId: string, direction: 'up' | 'down') => Promise<void>;
  /** Attribue (`assign: true`) ou retire un rôle à un membre. */
  setMemberRole: (
    groupId: string,
    roleId: string,
    pubkey: string,
    assign: boolean,
  ) => Promise<void>;
  loadPins: (groupId: string, channelId: string) => Promise<void>;
  /** Épingle ou désépingle selon l'état courant `pinned`. */
  togglePin: (
    groupId: string,
    channelId: string,
    msgId: string,
    pinned: boolean,
  ) => Promise<void>;
  /**
   * Envoie un message de salon, éventuellement en réponse à `replyTo` (msg_id)
   * et avec des pièces jointes déjà publiées (texte vide admis avec pièces).
   */
  send: (
    groupId: string,
    channelId: string,
    text: string,
    replyTo?: string,
    attachments?: FileAttachment[],
  ) => Promise<void>;
  /** Remplace le texte d'un de ses propres messages. */
  editMessage: (
    groupId: string,
    channelId: string,
    msgId: string,
    text: string,
  ) => Promise<void>;
  /** Supprime un message (le sien, ou celui d'autrui avec MANAGE_MESSAGES). */
  deleteMessage: (groupId: string, channelId: string, msgId: string) => Promise<void>;
  /**
   * Suppression groupée (≤100) par un modérateur (`MANAGE_MESSAGES`) : appelle
   * `groups.purge` puis pose localement les tombstones des messages visés
   * (l'état convergera par ailleurs via la réplication / `refreshHistory`).
   * Rend le nombre supprimé par le nœud.
   */
  purge: (
    groupId: string,
    channelId: string,
    msgIds: string[],
  ) => Promise<{ deleted: number }>;
  /** Ajoute ou retire (bascule) sa réaction `emoji` sur un message. */
  toggleReaction: (
    groupId: string,
    channelId: string,
    msgId: string,
    emoji: string,
    selfPubkey: string,
  ) => Promise<void>;
  /** Autorise une invitation vers `pubkey` (consentement explicite requis, D-045). */
  invite: (groupId: string, pubkey: string) => Promise<void>;
  /** Ajoute (ou remplace) un émoji de serveur puis recharge l'état. */
  addEmoji: (
    groupId: string,
    name: string,
    dataB64: string,
    mime: string,
  ) => Promise<void>;
  /** Supprime un émoji de serveur par son nom puis recharge l'état. */
  delEmoji: (groupId: string, name: string) => Promise<void>;
  /** Crée un événement planifié (MANAGE_CHANNELS) puis recharge l'état. */
  createEvent: (
    groupId: string,
    fields: {
      title: string;
      description: string;
      startMs: number;
      channelId: string | null;
    },
  ) => Promise<string>;
  /** Réécrit intégralement un événement (MANAGE_CHANNELS ou auteur) puis recharge l'état. */
  editEvent: (
    groupId: string,
    eventId: string,
    fields: {
      title: string;
      description: string;
      startMs: number;
      channelId: string | null;
    },
  ) => Promise<void>;
  /** Supprime un événement (MANAGE_CHANNELS ou auteur) puis recharge l'état. */
  deleteEvent: (groupId: string, eventId: string) => Promise<void>;
  /**
   * Bascule son propre RSVP (optimiste : `rsvped`/`rsvp_count` mis à jour
   * localement avant la réponse du nœud, restaurés en cas d'échec).
   */
  rsvpEvent: (groupId: string, eventId: string, interested: boolean) => Promise<void>;
  /** Ajoute (ou remplace) un sticker de serveur puis recharge l'état. */
  addSticker: (
    groupId: string,
    name: string,
    dataB64: string,
    mime: string,
  ) => Promise<void>;
  /** Supprime un sticker de serveur par son nom puis recharge l'état. */
  removeSticker: (groupId: string, name: string) => Promise<void>;
  /** Ajoute (ou remplace) un son de soundboard puis recharge l'état. */
  addSound: (
    groupId: string,
    name: string,
    mime: string,
    dataB64: string,
  ) => Promise<void>;
  /** Supprime un son de soundboard par son nom puis recharge l'état. */
  delSound: (groupId: string, name: string) => Promise<void>;
  /**
   * Publie (ou efface, `image` omis) son avatar de serveur self-service puis
   * recharge l'état.
   */
  setMemberAvatar: (
    groupId: string,
    image?: { dataB64: string; mime: string },
  ) => Promise<void>;
  /** Fixe (ou efface avec `null`) la couleur de bannière de serveur puis recharge l'état. */
  setBannerColor: (groupId: string, color: number | null) => Promise<void>;
  /** Envoie un sticker de serveur (message dédié) puis rafraîchit l'historique. */
  sendSticker: (groupId: string, channelId: string, name: string) => Promise<void>;
  /**
   * Envoie un sondage (message dédié, D-048) puis rafraîchit l'historique et
   * l'état du groupe (le nœud enregistre `PollCreate` en même temps que le
   * message). Rend l'identifiant du sondage fraîchement créé.
   */
  sendPoll: (
    groupId: string,
    channelId: string,
    question: string,
    options: string[],
  ) => Promise<string>;
  /**
   * Vote (ou change son vote) sur un sondage — optimiste : `counts`/
   * `my_vote`/`total_votes` mis à jour localement avant la réponse du nœud,
   * restaurés en cas d'échec (même schéma que `rsvpEvent`).
   */
  votePoll: (groupId: string, pollId: string, optionIndex: number) => Promise<void>;
  /** Clôture un sondage (auteur ou MANAGE_CHANNELS) puis recharge l'état. */
  closePoll: (groupId: string, pollId: string) => Promise<void>;
}

/**
 * Séquences « dernier gagne » (cf. `stores/dms`) : `loadState` (par groupe) et
 * `refreshHistory` (par salon) sont déclenchés sur événement et peuvent
 * répondre dans le désordre ; on ignore une réponse périmée pour ne pas
 * réécraser un état/des messages plus frais déjà appliqués.
 */
const stateSeq = new Map<string, number>();
const historySeq = new Map<string, number>();

export const useGroups = create<GroupsState>((set, get) => ({
  ids: [],
  states: {},
  messages: {},
  hasMore: {},
  loadingOlder: {},
  pins: {},
  unread: {},
  mentions: {},
  pendingInvites: [],

  loadList: async () => {
    const { groups, unread, mentions } = await api.groupsList();
    set({ ids: groups, unread: unread ?? {}, mentions: mentions ?? {} });
    await Promise.all([
      ...groups.map((id) => get().loadState(id)),
      get().loadPendingInvites(),
    ]);
  },

  loadPendingInvites: async () => {
    const { invites } = await api.groupsInvitesList();
    set({ pendingInvites: invites });
  },

  handleInvitePending: (invite) =>
    set((s) => {
      const exists = s.pendingInvites.some(
        (i) => i.group_id === invite.group_id && i.invite_id === invite.invite_id,
      );
      return exists ? {} : { pendingInvites: [...s.pendingInvites, invite] };
    }),

  acceptInvite: async (groupId, inviteId) => {
    const previous = get().pendingInvites;
    set({
      pendingInvites: previous.filter(
        (i) => !(i.group_id === groupId && i.invite_id === inviteId),
      ),
    });
    try {
      await api.groupsInviteAccept(groupId, inviteId);
    } catch (err) {
      // Échec : ré-insère UNIQUEMENT l'invitation retirée dans la liste
      // COURANTE (préserve celles arrivées entre-temps), sans doublon.
      const removed = previous.find(
        (i) => i.group_id === groupId && i.invite_id === inviteId,
      );
      if (removed !== undefined) {
        set((s) =>
          s.pendingInvites.some(
            (i) => i.group_id === groupId && i.invite_id === inviteId,
          )
            ? {}
            : { pendingInvites: [...s.pendingInvites, removed] },
        );
      }
      throw err;
    }
  },

  declineInvite: async (groupId, inviteId) => {
    const previous = get().pendingInvites;
    set({
      pendingInvites: previous.filter(
        (i) => !(i.group_id === groupId && i.invite_id === inviteId),
      ),
    });
    try {
      await api.groupsInviteDecline(groupId, inviteId);
    } catch (err) {
      // Échec : ré-insère UNIQUEMENT l'invitation retirée dans la liste
      // COURANTE (préserve celles arrivées entre-temps), sans doublon.
      const removed = previous.find(
        (i) => i.group_id === groupId && i.invite_id === inviteId,
      );
      if (removed !== undefined) {
        set((s) =>
          s.pendingInvites.some(
            (i) => i.group_id === groupId && i.invite_id === inviteId,
          )
            ? {}
            : { pendingInvites: [...s.pendingInvites, removed] },
        );
      }
      throw err;
    }
  },

  refreshUnread: async () => {
    const { unread, mentions } = await api.groupsList();
    set({ unread: unread ?? {}, mentions: mentions ?? {} });
  },

  markRead: async (groupId, channelId, lamport) => {
    await api.groupsMarkRead(groupId, channelId, lamport);
    await get().refreshUnread();
  },

  loadState: async (groupId) => {
    const seq = (stateSeq.get(groupId) ?? 0) + 1;
    stateSeq.set(groupId, seq);
    const state = await api.groupsState(groupId);
    // Réponse périmée (un `loadState` plus récent a démarré depuis) : ignorée.
    if (stateSeq.get(groupId) !== seq) return;
    set((s) => ({ states: { ...s.states, [groupId]: state } }));
  },

  handleGroupState: async (groupId) => {
    try {
      await get().loadState(groupId);
    } catch {
      // Groupe devenu inaccessible (expulsion, bannissement…) : on repart
      // de la liste du nœud, qui fait foi.
      await get().loadList();
      return;
    }
    // Les épinglés peuvent avoir changé (op pin/unpin) : on recharge ceux
    // déjà consultés de ce groupe, en best effort.
    const prefix = `${groupId}/`;
    const keys = Object.keys(get().pins).filter((k) => k.startsWith(prefix));
    await Promise.all(
      keys.map((k) =>
        get()
          .loadPins(groupId, k.slice(prefix.length))
          .catch(() => {}),
      ),
    );
  },

  refreshHistory: async (groupId, channelId) => {
    const key = channelKey(groupId, channelId);
    const seq = (historySeq.get(key) ?? 0) + 1;
    historySeq.set(key, seq);
    const { messages } = await fetchGroupPage(rpc, groupId, channelId);
    // Réponse périmée (un rafraîchissement plus récent a démarré depuis) : ignorée.
    if (historySeq.get(key) !== seq) return;
    const pageFull = messages.length === PAGE_SIZE;
    set((s) => {
      const existing = s.messages[key];
      if (existing === undefined || existing.length === 0) {
        return {
          messages: { ...s.messages, [key]: sortAscending(messages) },
          hasMore: { ...s.hasMore, [key]: pageFull },
        };
      }
      const merged = mergeRecentPage(existing, messages, pageFull);
      return {
        messages: { ...s.messages, [key]: merged.messages },
        hasMore: merged.gapDetected ? { ...s.hasMore, [key]: pageFull } : s.hasMore,
      };
    });
  },

  loadOlderHistory: async (groupId, channelId) => {
    const key = channelKey(groupId, channelId);
    const state = get();
    const oldest = (state.messages[key] ?? [])[0];
    if (
      oldest === undefined ||
      state.loadingOlder[key] === true ||
      state.hasMore[key] !== true
    ) {
      return;
    }
    set((s) => ({ loadingOlder: { ...s.loadingOlder, [key]: true } }));
    try {
      const { messages } = await fetchGroupPage(rpc, groupId, channelId, oldest.lamport);
      set((s) => ({
        messages: {
          ...s.messages,
          [key]: mergeOlderPage(s.messages[key] ?? [], messages),
        },
        hasMore: { ...s.hasMore, [key]: messages.length === PAGE_SIZE },
      }));
    } finally {
      set((s) => ({ loadingOlder: { ...s.loadingOlder, [key]: false } }));
    }
  },

  jumpTo: async (groupId, channelId, msgId) => {
    const key = channelKey(groupId, channelId);
    const existing = get().messages[key] ?? [];
    if (existing.some((m) => m.msg_id === msgId)) return true;
    const res = await api.groupsHistoryAround(groupId, channelId, msgId);
    if (!res.found) return false;
    set((s) => {
      const merged = mergeOlderPage(s.messages[key] ?? [], res.messages);
      const knownHasMore = s.hasMore[key];
      return {
        messages: { ...s.messages, [key]: merged },
        hasMore: {
          ...s.hasMore,
          [key]:
            knownHasMore === undefined ? res.messages.length >= PAGE_SIZE : knownHasMore,
        },
      };
    });
    return true;
  },

  create: async (name, defaultChannel) => {
    const { group_id } = await api.groupsCreate(name);
    await api.groupsChannelAdd(group_id, defaultChannel);
    await get().loadList();
    return group_id;
  },

  rename: async (groupId, name) => {
    await api.groupsRename(groupId, name);
    await get().loadState(groupId);
  },

  setIcon: async (groupId, dataB64, mime) => {
    await api.groupsSetIcon(groupId, dataB64, mime);
    await get().loadState(groupId);
  },

  setBanner: async (groupId, dataB64, mime) => {
    await api.groupsSetBanner(groupId, mime, dataB64);
    await get().loadState(groupId);
  },

  setTopic: async (groupId, channelId, topic) => {
    await api.groupsSetTopic(groupId, channelId, topic);
    await get().loadState(groupId);
  },

  addChannel: async (groupId, name, kind, category) => {
    const { channel_id } = await api.groupsChannelAdd(groupId, name, kind, category);
    await get().loadState(groupId);
    return channel_id;
  },

  renameChannel: async (groupId, channelId, name) => {
    await api.groupsChannelEdit(groupId, channelId, { name });
    await get().loadState(groupId);
  },

  setChannelCategory: async (groupId, channelId, category) => {
    await api.groupsChannelEdit(groupId, channelId, { category });
    await get().loadState(groupId);
  },

  setChannelPerms: async (groupId, channelId, roleId, allow, deny) => {
    await api.groupsChannelPerms(groupId, channelId, roleId, allow, deny);
    await get().loadState(groupId);
  },

  setSlowmode: async (groupId, channelId, seconds) => {
    await api.groupsChannelSlowmode(groupId, channelId, seconds);
    await get().loadState(groupId);
  },

  setAutomodWords: async (groupId, words) => {
    await api.groupsAutomodSet(groupId, words);
    await get().loadState(groupId);
  },

  deleteChannel: async (groupId, channelId) => {
    await api.groupsChannelDel(groupId, channelId);
    await get().loadState(groupId);
  },

  createThread: async (groupId, parentChannel, rootMsg, name) => {
    const { thread_id } = await api.groupsThreadCreate(
      groupId,
      parentChannel,
      rootMsg,
      name,
    );
    await get().loadState(groupId);
    return thread_id;
  },

  archiveThread: async (groupId, threadId, archived) => {
    await api.groupsThreadArchive(groupId, threadId, archived);
    await get().loadState(groupId);
  },

  addCategory: async (groupId, name) => {
    const { category_id } = await api.groupsCategoryAdd(groupId, name);
    await get().loadState(groupId);
    return category_id;
  },

  renameCategory: async (groupId, categoryId, name) => {
    await api.groupsCategoryEdit(groupId, categoryId, { name });
    await get().loadState(groupId);
  },

  deleteCategory: async (groupId, categoryId) => {
    await api.groupsCategoryDel(groupId, categoryId);
    await get().loadState(groupId);
  },

  kick: async (groupId, pubkey) => {
    await api.groupsKick(groupId, pubkey);
    await get().loadState(groupId);
  },

  ban: async (groupId, pubkey) => {
    await api.groupsBan(groupId, pubkey);
    await get().loadState(groupId);
  },

  unban: async (groupId, pubkey) => {
    await api.groupsUnban(groupId, pubkey);
    await get().loadState(groupId);
  },

  timeout: async (groupId, pubkey, untilMs) => {
    await api.groupsTimeout(groupId, pubkey, untilMs);
    await get().loadState(groupId);
  },

  clearTimeout: async (groupId, pubkey) => {
    await api.groupsTimeoutClear(groupId, pubkey);
    await get().loadState(groupId);
  },

  voiceModerate: async (groupId, pubkey, mute, deafen) => {
    await api.groupsVoiceModerate(groupId, pubkey, mute, deafen);
    await get().loadState(groupId);
  },

  setNickname: async (groupId, name, member) => {
    await api.groupsSetNickname(groupId, name, member);
    await get().loadState(groupId);
  },

  leave: async (groupId) => {
    await api.groupsLeave(groupId);
    set((s) => ({
      ids: s.ids.filter((id) => id !== groupId),
      states: Object.fromEntries(
        Object.entries(s.states).filter(([id]) => id !== groupId),
      ),
      messages: Object.fromEntries(
        Object.entries(s.messages).filter(([key]) => !key.startsWith(`${groupId}/`)),
      ),
      pins: Object.fromEntries(
        Object.entries(s.pins).filter(([key]) => !key.startsWith(`${groupId}/`)),
      ),
      unread: Object.fromEntries(
        Object.entries(s.unread).filter(([id]) => id !== groupId),
      ),
      mentions: Object.fromEntries(
        Object.entries(s.mentions).filter(([id]) => id !== groupId),
      ),
    }));
  },

  addRole: async (groupId, name, color, permissions) => {
    const { role_id } = await api.groupsRoleAdd(groupId, name, color, permissions);
    await get().loadState(groupId);
    return role_id;
  },

  editRole: async (groupId, roleId, changes) => {
    await api.groupsRoleEdit(groupId, roleId, changes);
    await get().loadState(groupId);
  },

  deleteRole: async (groupId, roleId) => {
    await api.groupsRoleDel(groupId, roleId);
    await get().loadState(groupId);
  },

  moveRole: async (groupId, roleId, direction) => {
    const state = get().states[groupId];
    if (state === undefined) return;
    const edits = planRoleMove(state.roles, roleId, direction);
    // Sequential on purpose: the node replays each op on the current state.
    for (const edit of edits) {
      await api.groupsRoleEdit(groupId, edit.role_id, { position: edit.position });
    }
    if (edits.length > 0) await get().loadState(groupId);
  },

  setMemberRole: async (groupId, roleId, pubkey, assign) => {
    if (assign) await api.groupsRoleAssign(groupId, roleId, pubkey);
    else await api.groupsRoleUnassign(groupId, roleId, pubkey);
    await get().loadState(groupId);
  },

  loadPins: async (groupId, channelId) => {
    const { msg_ids } = await api.groupsPins(groupId, channelId);
    set((s) => ({
      pins: { ...s.pins, [channelKey(groupId, channelId)]: msg_ids },
    }));
  },

  togglePin: async (groupId, channelId, msgId, pinned) => {
    if (pinned) await api.groupsUnpin(groupId, channelId, msgId);
    else await api.groupsPin(groupId, channelId, msgId);
    await get().loadPins(groupId, channelId);
  },

  send: async (groupId, channelId, text, replyTo, attachments) => {
    await api.groupsSend(groupId, channelId, text, replyTo, attachments);
    await get().refreshHistory(groupId, channelId);
  },

  editMessage: async (groupId, channelId, msgId, text) => {
    await api.groupsEdit(groupId, channelId, msgId, text);
    const key = channelKey(groupId, channelId);
    set((s) => ({
      messages: patchMessages(s.messages, key, msgId, (m) => ({
        ...m,
        edited: text,
      })),
    }));
  },

  deleteMessage: async (groupId, channelId, msgId) => {
    await api.groupsDelete(groupId, channelId, msgId);
    const key = channelKey(groupId, channelId);
    set((s) => ({
      messages: patchMessages(s.messages, key, msgId, (m) => ({
        ...m,
        deleted: true,
      })),
    }));
  },

  purge: async (groupId, channelId, msgIds) => {
    const result = await api.groupsPurge(groupId, channelId, msgIds);
    const key = channelKey(groupId, channelId);
    set((s) => {
      let messages = s.messages;
      for (const id of msgIds) {
        messages = patchMessages(messages, key, id, (m) => ({ ...m, deleted: true }));
      }
      return { messages };
    });
    return result;
  },

  toggleReaction: async (groupId, channelId, msgId, emoji, selfPubkey) => {
    const key = channelKey(groupId, channelId);
    const message = (get().messages[key] ?? []).find((m) => m.msg_id === msgId);
    if (message === undefined) return;
    const already = (message.reactions ?? []).some(
      (r) => r.emoji === emoji && r.author === selfPubkey,
    );
    await api.groupsReact(groupId, channelId, msgId, emoji, !already);
    set((s) => ({
      messages: patchMessages(s.messages, key, msgId, (m) => {
        // Idempotent : deux clics rapides lisent tous deux `already=false` puis
        // ajoutent ; on garde contre le doublon en relisant l'état COURANT.
        const has = (m.reactions ?? []).some(
          (r) => r.emoji === emoji && r.author === selfPubkey,
        );
        return {
          ...m,
          reactions: already
            ? (m.reactions ?? []).filter(
                (r) => !(r.emoji === emoji && r.author === selfPubkey),
              )
            : has
              ? (m.reactions ?? [])
              : [...(m.reactions ?? []), { emoji, author: selfPubkey }],
        };
      }),
    }));
  },

  invite: async (groupId, pubkey) => {
    await api.groupsInviteCreate(groupId, pubkey);
    await get().loadState(groupId);
  },

  addEmoji: async (groupId, name, dataB64, mime) => {
    await api.groupsEmojiAdd(groupId, name, dataB64, mime);
    await get().loadState(groupId);
  },

  delEmoji: async (groupId, name) => {
    await api.groupsEmojiDel(groupId, name);
    await get().loadState(groupId);
  },

  createEvent: async (groupId, fields) => {
    const { event_id } = await api.groupsEventsCreate(groupId, fields);
    await get().loadState(groupId);
    return event_id;
  },

  editEvent: async (groupId, eventId, fields) => {
    await api.groupsEventsEdit(groupId, eventId, fields);
    await get().loadState(groupId);
  },

  deleteEvent: async (groupId, eventId) => {
    await api.groupsEventsDelete(groupId, eventId);
    await get().loadState(groupId);
  },

  rsvpEvent: async (groupId, eventId, interested) => {
    const state = get().states[groupId];
    if (state === undefined) {
      await api.groupsEventsRsvp(groupId, eventId, interested);
      return;
    }
    const events = (state.events ?? []).map((e) =>
      e.event_id === eventId && e.rsvped !== interested
        ? { ...e, rsvped: interested, rsvp_count: e.rsvp_count + (interested ? 1 : -1) }
        : e,
    );
    set((s) => ({ states: { ...s.states, [groupId]: { ...state, events } } }));
    try {
      await api.groupsEventsRsvp(groupId, eventId, interested);
    } catch (err) {
      // Échec : annule UNIQUEMENT ce RSVP sur l'état COURANT (préserve les MAJ
      // concurrentes arrivées pendant l'appel), au lieu de restaurer tout
      // l'instantané pré-clic.
      const prev = (state.events ?? []).find((e) => e.event_id === eventId);
      if (prev !== undefined) {
        set((s) => {
          const cur = s.states[groupId];
          if (cur === undefined) return {};
          return {
            states: {
              ...s.states,
              [groupId]: {
                ...cur,
                events: (cur.events ?? []).map((e) =>
                  e.event_id === eventId ? prev : e,
                ),
              },
            },
          };
        });
      }
      throw err;
    }
  },

  addSticker: async (groupId, name, dataB64, mime) => {
    await api.groupsStickersAdd(groupId, name, dataB64, mime);
    await get().loadState(groupId);
  },

  removeSticker: async (groupId, name) => {
    await api.groupsStickersRemove(groupId, name);
    await get().loadState(groupId);
  },

  addSound: async (groupId, name, mime, dataB64) => {
    await api.groupsSoundsAdd(groupId, name, mime, dataB64);
    await get().loadState(groupId);
  },

  delSound: async (groupId, name) => {
    await api.groupsSoundsDel(groupId, name);
    await get().loadState(groupId);
  },

  setMemberAvatar: async (groupId, image) => {
    await api.groupsSetMemberAvatar(groupId, image);
    await get().loadState(groupId);
  },

  setBannerColor: async (groupId, color) => {
    await api.groupsSetBannerColor(groupId, color);
    await get().loadState(groupId);
  },

  sendSticker: async (groupId, channelId, name) => {
    await api.groupsSendSticker(groupId, channelId, name);
    await get().refreshHistory(groupId, channelId);
  },

  sendPoll: async (groupId, channelId, question, options) => {
    const { poll_id } = await api.groupsSendPoll(groupId, channelId, question, options);
    // Le message (question/options) et l'état (PollCreate, dépouillement à
    // zéro) sont deux sources distinctes — les deux doivent être rechargées.
    await Promise.all([
      get().refreshHistory(groupId, channelId),
      get().loadState(groupId),
    ]);
    return poll_id;
  },

  votePoll: async (groupId, pollId, optionIndex) => {
    const state = get().states[groupId];
    const poll = state?.polls?.find((p) => p.poll_id === pollId);
    if (state === undefined || poll === undefined) {
      // État pas encore chargé/convergé pour ce sondage : vote à l'aveugle,
      // aucun patch optimiste possible sans dépouillement de départ.
      await api.groupsPollVote(groupId, pollId, optionIndex);
      return;
    }
    if (poll.my_vote === optionIndex) return;
    const counts = [...poll.counts];
    if (poll.my_vote !== null) {
      counts[poll.my_vote] = Math.max(0, (counts[poll.my_vote] ?? 0) - 1);
    }
    counts[optionIndex] = (counts[optionIndex] ?? 0) + 1;
    const total_votes = poll.total_votes + (poll.my_vote === null ? 1 : 0);
    const polls = (state.polls ?? []).map((p) =>
      p.poll_id === pollId ? { ...p, counts, my_vote: optionIndex, total_votes } : p,
    );
    set((s) => ({ states: { ...s.states, [groupId]: { ...state, polls } } }));
    try {
      await api.groupsPollVote(groupId, pollId, optionIndex);
    } catch (err) {
      // Échec : restaure UNIQUEMENT ce sondage sur l'état COURANT (préserve les
      // MAJ concurrentes arrivées pendant l'appel), au lieu de tout l'instantané.
      set((s) => {
        const cur = s.states[groupId];
        if (cur === undefined) return {};
        return {
          states: {
            ...s.states,
            [groupId]: {
              ...cur,
              polls: (cur.polls ?? []).map((p) => (p.poll_id === pollId ? poll : p)),
            },
          },
        };
      });
      throw err;
    }
  },

  closePoll: async (groupId, pollId) => {
    await api.groupsPollClose(groupId, pollId);
    await get().loadState(groupId);
  },
}));

/**
 * Événement `event.mention` : une mention vient d'être détectée localement.
 * Rafraîchit les compteurs de mentions/non-lus par groupe et la liste des
 * contacts (le compteur de mentions par MP y est plié) pour actualiser les
 * pastilles en direct. Exporté pour les tests ; câblé au chargement du module.
 */
export function handleMentionNodeEvent(method: string): void {
  if (method !== 'event.mention') return;
  void useGroups
    .getState()
    .refreshUnread()
    .catch(() => {
      // Best effort : les compteurs seront corrigés au prochain passage.
    });
  void useFriends
    .getState()
    .load()
    .catch(() => {
      // Best effort : la liste sera rechargée au prochain événement.
    });
}

// Garde d'environnement : les tests qui simulent `../lib/client` sans
// `rpc.onEvent` doivent pouvoir importer ce module sans câblage.
try {
  rpc.onEvent(handleMentionNodeEvent);
} catch {
  // Client simulé (tests) : pas d'événements à câbler.
}
