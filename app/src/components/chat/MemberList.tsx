/**
 * Liste des membres d'un serveur, groupée par rôle le plus haut (façon
 * Discord) : présence, statut personnalisé, couleurs de rôle, menu contextuel
 * (profil, mention, MP, modération vocale, copie d'identifiant). Extraite de
 * `ChatView` (D-056) — comportement inchangé.
 */

import { interpolate } from '../../i18n';
import type { PresenceStatus } from '../../lib/api';
import { copyToClipboard } from '../../lib/clipboard';
import { estOuvertureMenu, pointAncrageMenu } from '../../lib/focus';
import { useContextMenu, type ContextMenuItem } from '../../stores/contextMenu';
import {
  useFriends,
  avatarDecorationOf,
  avatarOf,
  displayNameOf,
  presenceOf,
} from '../../stores/friends';
import {
  useGroups,
  canModerateVoice,
  memberColor,
  nicknameOf,
  serverAvatarOf,
  sortRoles,
} from '../../stores/groups';
import { selfDisplayName, useSession } from '../../stores/session';
import { useUi, useT } from '../../stores/ui';
import { Avatar } from '../Avatar';
import {
  CopyMenuIcon,
  EnvelopeMenuIcon,
  MentionMenuIcon,
  ProfileMenuIcon,
  VoiceDeafenMenuIcon,
  VoiceMuteMenuIcon,
} from '../ContextMenu';
import { PresenceDot } from '../PresenceDot';
import { ownDotStatus } from '../UserMenu';

export function MemberList({
  groupId,
  fill = false,
}: {
  groupId: string;
  fill?: boolean;
}) {
  const t = useT();
  const contacts = useFriends((s) => s.contacts);
  const self = useSession((s) => s.self);
  const phase = useSession((s) => s.phase);
  const ownStatus = useFriends((s) => s.ownStatus);
  const ownStatusText = useFriends((s) => s.ownStatusText);
  const state = useGroups((s) => s.states[groupId]);
  const openProfile = useUi((s) => s.openProfile);
  const activeProfile = useUi((s) => s.profile);
  const requestMentionInsert = useUi((s) => s.requestMentionInsert);
  const toast = useUi((s) => s.toast);
  const membersWidth = useUi((s) => s.membersWidth);
  if (!state) return null;

  /** Statut de présence d'un membre — le sien (fiable), sinon celui du contact ami connu. */
  const statusOf = (pubkey: string): PresenceStatus => {
    if (self && pubkey === self.pubkey) {
      return phase === 'ready' ? ownDotStatus(ownStatus) : 'offline';
    }
    return presenceOf(contacts.find((c) => c.pubkey === pubkey));
  };

  const nameOf = (pubkey: string): string => {
    const nick = nicknameOf(state, pubkey);
    if (self && pubkey === self.pubkey) {
      return `${nick ?? selfDisplayName(self)}`;
    }
    return nick ?? displayNameOf(contacts, pubkey);
  };

  /** Texte de statut personnalisé — le sien, sinon celui du contact ami connu. */
  const statusTextOf = (pubkey: string): string | null => {
    if (self && pubkey === self.pubkey) return ownStatusText;
    return contacts.find((c) => c.pubkey === pubkey)?.status_text ?? null;
  };

  /**
   * Items du menu contextuel d'une entrée de la liste des membres : profil,
   * mention, MP, copie d'identifiant — même schéma que le menu « utilisateur »
   * de `MessageList` (avatar/pseudo d'un message).
   */
  const buildMemberItems = (pubkey: string, target: HTMLElement): ContextMenuItem[] => {
    const isSelfMember = self !== null && pubkey === self.pubkey;
    const contact = contacts.find((c) => c.pubkey === pubkey);
    const canMessage = !isSelfMember && contact?.state === 'friend';
    const items: ContextMenuItem[] = [
      {
        label: t.contextMenu.viewProfile,
        icon: <ProfileMenuIcon />,
        onClick: () => {
          const r = target.getBoundingClientRect();
          openProfile(
            pubkey,
            { top: r.top, left: r.left, bottom: r.bottom, right: r.right },
            groupId,
          );
        },
      },
      {
        label: interpolate(t.contextMenu.mention, { name: nameOf(pubkey) }),
        icon: <MentionMenuIcon />,
        onClick: () => requestMentionInsert(nameOf(pubkey)),
      },
    ];
    if (canMessage) {
      items.push({
        label: t.friends.sendDm,
        icon: <EnvelopeMenuIcon />,
        onClick: () => useUi.getState().setView({ kind: 'dm', peer: pubkey }),
      });
    }
    // Modération vocale serveur (op 0x1F) : permission KICK requise, fondateur
    // et soi-même intouchables côté UI (même convention que kick/ban/timeout
    // dans ServerMembersTab — le nœud revérifie la hiérarchie de toute façon).
    if (canModerateVoice(state, self?.pubkey ?? null, pubkey)) {
      const targetMember = state.members.find((m) => m.pubkey === pubkey);
      const serverMuted = targetMember?.voice_muted === true;
      const serverDeafened = targetMember?.voice_deafened === true;
      const onModerateError = (): void => toast('error', t.errors.actionFailed);
      items.push({
        label: serverMuted
          ? t.contextMenu.voiceUnmuteServer
          : t.contextMenu.voiceMuteServer,
        icon: <VoiceMuteMenuIcon />,
        separatorBefore: true,
        onClick: () => {
          useGroups
            .getState()
            .voiceModerate(groupId, pubkey, !serverMuted, serverDeafened)
            .catch(onModerateError);
        },
      });
      items.push({
        label: serverDeafened
          ? t.contextMenu.voiceUndeafenServer
          : t.contextMenu.voiceDeafenServer,
        icon: <VoiceDeafenMenuIcon />,
        onClick: () => {
          useGroups
            .getState()
            .voiceModerate(groupId, pubkey, serverMuted, !serverDeafened)
            .catch(onModerateError);
        },
      });
    }
    items.push({
      label: t.contextMenu.copyUserId,
      icon: <CopyMenuIcon />,
      separatorBefore: true,
      onClick: () =>
        copyToClipboard(
          pubkey,
          () => toast('info', t.app.copied),
          () => toast('error', t.errors.actionFailed),
        ),
    });
    return items;
  };

  /** Noms des rôles d'un membre, du plus haut au plus bas. */
  const roleNamesOf = (roleIds: readonly string[]): string[] => {
    const owned = new Set(roleIds);
    return sortRoles(state.roles)
      .filter((r) => owned.has(r.role_id))
      .map((r) => r.name);
  };

  /**
   * Regroupe les membres par rôle le plus haut détenu (façon Discord) : une
   * section par rôle occupé, dans l'ordre de priorité, puis un groupe final
   * pour les membres sans rôle (libellé générique réutilisé du titre du
   * panneau — pas de nouvelle chaîne i18n).
   */
  const sortedRoles = sortRoles(state.roles);
  const sections: { key: string; label: string; members: typeof state.members }[] =
    sortedRoles.map((role) => ({ key: role.role_id, label: role.name, members: [] }));
  const withoutRole: { key: string; label: string; members: typeof state.members } = {
    key: '__sans_role__',
    label: t.groups.members,
    members: [],
  };
  for (const member of state.members) {
    const owned = new Set(member.roles);
    const top = sortedRoles.find((r) => owned.has(r.role_id));
    const bucket =
      top === undefined ? withoutRole : sections.find((s) => s.key === top.role_id);
    (bucket ?? withoutRole).members.push(member);
  }
  const populatedSections = [...sections, withoutRole]
    .filter((section) => section.members.length > 0)
    .map((section) => ({
      ...section,
      members: [...section.members].sort((a, b) =>
        nameOf(a.pubkey).localeCompare(nameOf(b.pubkey)),
      ),
    }));

  return (
    <aside
      className="theme-surface-sidebar accord-members h-full shrink-0 overflow-y-auto bg-sidebar p-2"
      style={{ width: fill ? '100%' : membersWidth }}
      aria-label={t.groups.members}
    >
      {populatedSections.map((section) => (
        <div key={section.key}>
          <div className="px-1.5 pb-1 pt-3 text-[11px] font-medium uppercase tracking-wide text-muted first:pt-1">
            {section.label} — {section.members.length}
          </div>
          {section.members.map((member) => {
            const color = memberColor(member, state.roles);
            const roleNames = roleNamesOf(member.roles);
            // Avatar d'un membre : avatar de serveur self-service s'il est
            // défini, sinon l'avatar global (le sien, ou celui du contact ami
            // connu — les avatars des non-amis ne circulent pas, limite du
            // protocole).
            const globalAvatarHash =
              self && member.pubkey === self.pubkey
                ? self.avatar
                : avatarOf(contacts, member.pubkey);
            const avatarHash =
              serverAvatarOf(state, contacts, member.pubkey) ?? globalAvatarHash;
            const avatarDecoration =
              self && member.pubkey === self.pubkey
                ? self.avatar_decoration
                : avatarDecorationOf(contacts, member.pubkey);
            const status = statusOf(member.pubkey);
            const statusText = statusTextOf(member.pubkey);
            return (
              <button
                key={member.pubkey}
                type="button"
                aria-label={interpolate(t.profil.openProfile, {
                  name: nameOf(member.pubkey),
                })}
                aria-haspopup="dialog"
                aria-expanded={
                  activeProfile?.pubkey === member.pubkey &&
                  activeProfile.surface === 'profile-card'
                }
                onClick={(e) => {
                  const r = e.currentTarget.getBoundingClientRect();
                  openProfile(
                    member.pubkey,
                    { top: r.top, left: r.left, bottom: r.bottom, right: r.right },
                    groupId,
                  );
                }}
                onContextMenu={(e) => {
                  e.preventDefault();
                  useContextMenu
                    .getState()
                    .openMenu(
                      e.clientX,
                      e.clientY,
                      buildMemberItems(member.pubkey, e.currentTarget),
                    );
                }}
                onKeyDown={(e) => {
                  // Équivalent clavier du clic droit (Maj+F10 / touche Menu).
                  if (!estOuvertureMenu(e)) return;
                  e.preventDefault();
                  const { x, y } = pointAncrageMenu(e.currentTarget);
                  useContextMenu
                    .getState()
                    .openMenu(x, y, buildMemberItems(member.pubkey, e.currentTarget));
                }}
                className="flex min-h-9 w-full items-center gap-2.5 rounded-md px-1.5 py-1.5 text-left transition-colors duration-fast hover:bg-chat-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar"
              >
                <span className="relative shrink-0">
                  <Avatar
                    id={member.pubkey}
                    name={nameOf(member.pubkey)}
                    size={32}
                    avatarHash={avatarHash}
                    hint={member.pubkey}
                    decoration={avatarDecoration}
                  />
                  <PresenceDot
                    status={status}
                    label={t.profil[status]}
                    className="absolute -bottom-0.5 -right-0.5 rounded-full ring-2 ring-sidebar"
                  />
                </span>
                <div className="min-w-0">
                  <div
                    className="truncate text-sm font-medium text-muted"
                    style={color !== null ? { color } : undefined}
                  >
                    {nameOf(member.pubkey)}
                  </div>
                  {statusText !== null && statusText !== '' && (
                    <div className="truncate text-xs text-muted">{statusText}</div>
                  )}
                  {state.founder === member.pubkey && (
                    <div className="text-[10px] uppercase text-yellow">
                      {t.groups.founder}
                    </div>
                  )}
                  {roleNames.length > 0 && (
                    <div className="truncate text-[11px] text-faint">
                      {roleNames.join(' · ')}
                    </div>
                  )}
                </div>
              </button>
            );
          })}
        </div>
      ))}
    </aside>
  );
}

/**
 * Volet des messages épinglés (MP ou salon) : messages déjà résolus dans
 * l'historique chargé + nombre d'épinglés hors-page. Chaque entrée saute vers
 * le message d'un clic ; `canUnpin` expose le retrait de l'épingle.
 */
