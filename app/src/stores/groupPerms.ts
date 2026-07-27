/**
 * Aides pures des groupes : permissions (masques, héritage par salon),
 * couleurs de rôle, tris par position. Aucun état, aucun appel réseau —
 * extraites de `stores/groups` qui dépassait largement les 800 lignes.
 */

import type {
  Contact,
  GroupCategory,
  GroupChannel,
  GroupMember,
  GroupRole,
  GroupStateJson,
} from '../lib/api';
import { avatarOf } from './friends';
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
