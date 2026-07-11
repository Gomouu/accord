/**
 * Barre latérale (240 px) : en mode accueil, navigation Amis + conversations
 * privées ; en mode groupe, nom du groupe + salons groupés par catégorie
 * (les sans-catégorie d'abord), boutons gérés par les permissions.
 */

import { useState } from 'react';
import { interpolate } from '../i18n';
import type { GroupChannel } from '../lib/api';
import { copyToClipboard } from '../lib/clipboard';
import { useCalls } from '../stores/calls';
import { presenceOf, useFriends } from '../stores/friends';
import {
  useGroups,
  channelKey,
  channelsByCategory,
  hasPerm,
  PERMISSIONS,
} from '../stores/groups';
import { useContextMenu, type ContextMenuItem } from '../stores/contextMenu';
import { useUi, useT } from '../stores/ui';
import { useVoice } from '../stores/voice';
import { Avatar } from './Avatar';
import {
  CheckMenuIcon,
  CopyMenuIcon,
  DeleteMenuIcon,
  EditMenuIcon,
  PhoneOffIcon,
} from './ContextMenu';
import { MentionInbox } from './MentionInbox';
import { PresenceDot } from './PresenceDot';
import { SearchBar } from './SearchBar';
import { MentionBadge, UnreadBadge } from './UnreadBadge';
import { UserPanel } from './UserPanel';
import { VoiceSection } from './VoiceSection';

/** Bouton d'action de l'en-tête, taille fixe (icon spec) : conteneur carré centré. */
function HeaderIconButton({
  label,
  onClick,
  active = false,
  children,
}: {
  label: string;
  onClick: () => void;
  active?: boolean;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      onClick={onClick}
      className={`flex h-7 w-7 shrink-0 items-center justify-center rounded-md transition-colors duration-fast hover:bg-chat-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar active:scale-95 ${
        active ? 'text-header' : 'text-muted hover:text-norm'
      }`}
    >
      {children}
    </button>
  );
}

/** Bouton d'ouverture de la boîte de mentions (icône « @ »). */
function InboxButton({ onOpen }: { onOpen: () => void }) {
  const t = useT();
  return (
    <HeaderIconButton label={t.mentions.open} onClick={onOpen}>
      <svg
        width="18"
        height="18"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth={2}
        strokeLinecap="round"
        strokeLinejoin="round"
        aria-hidden
      >
        <circle cx="12" cy="12" r="4" />
        <path d="M16 8v5a3 3 0 0 0 6 0v-1a10 10 0 1 0-3.92 7.94" />
      </svg>
    </HeaderIconButton>
  );
}

/** Petit chevron décoratif (rotation animée sur `open`), icon spec 14 px. */
function Chevron({ open }: { open: boolean }) {
  return (
    <svg
      width="14"
      height="14"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
      className={`shrink-0 transition-transform duration-fast ease-expo ${open ? 'rotate-0' : '-rotate-90'}`}
    >
      <path d="m6 9 6 6 6-6" />
    </svg>
  );
}

function HomeSidebar({ onOpenInbox }: { onOpenInbox: () => void }) {
  const t = useT();
  const view = useUi((s) => s.view);
  const setView = useUi((s) => s.setView);
  const contacts = useFriends((s) => s.contacts);
  const missedPeers = useCalls((s) => s.missedPeers);
  const friends = contacts.filter((c) => c.state === 'friend');

  return (
    <>
      <SearchBar />
      <div className="flex-1 space-y-0.5 overflow-y-auto p-2">
        <button
          type="button"
          onClick={() => setView({ kind: 'friends' })}
          className={`flex h-9 w-full items-center gap-3 rounded-md px-2 font-medium transition-colors duration-fast focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar ${
            view.kind === 'friends'
              ? 'bg-chat-hover text-norm'
              : 'text-muted hover:bg-chat-hover hover:text-norm'
          }`}
        >
          <span className="flex h-5 w-5 shrink-0 items-center justify-center">
            <svg
              width="20"
              height="20"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth={2}
              strokeLinecap="round"
              strokeLinejoin="round"
              aria-hidden
            >
              <path d="M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2" />
              <circle cx="9" cy="7" r="4" />
              <path d="M22 21v-2a4 4 0 0 0-3-3.87" />
              <path d="M16 3.13a4 4 0 0 1 0 7.75" />
            </svg>
          </span>
          {t.friends.title}
        </button>

        <div className="flex items-center justify-between px-2 pb-1 pt-4">
          <span className="text-[11px] font-medium uppercase tracking-wide text-muted">
            {t.dm.directMessages}
          </span>
          <InboxButton onOpen={onOpenInbox} />
        </div>
        {friends.map((c) => {
          const active = view.kind === 'dm' && view.peer === c.pubkey;
          const status = presenceOf(c);
          const mentionCount = c.mention_count ?? 0;
          return (
            <button
              key={c.pubkey}
              type="button"
              onClick={() => setView({ kind: 'dm', peer: c.pubkey })}
              className={`flex h-9 w-full items-center gap-2.5 rounded-md px-2 transition-colors duration-fast focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar ${
                active
                  ? 'bg-chat-hover text-norm'
                  : 'text-muted hover:bg-chat-hover hover:text-norm'
              }`}
            >
              <span className="relative shrink-0">
                <Avatar
                  id={c.pubkey}
                  name={c.display_name || c.friend_code}
                  size={32}
                  avatarHash={c.avatar}
                  hint={c.pubkey}
                />
                <PresenceDot
                  status={status}
                  label={t.profil[status]}
                  className="absolute -bottom-0.5 -right-0.5 rounded-full ring-2 ring-sidebar"
                />
              </span>
              <span className="min-w-0 truncate font-medium">
                {c.display_name || c.friend_code}
              </span>
              <span className="ml-auto flex shrink-0 items-center gap-1">
                {missedPeers.has(c.pubkey) && (
                  <span
                    role="img"
                    aria-label={interpolate(t.calls.missedFrom, {
                      name: c.display_name || c.friend_code,
                    })}
                    title={interpolate(t.calls.missedFrom, {
                      name: c.display_name || c.friend_code,
                    })}
                    className="text-red"
                  >
                    <PhoneOffIcon size={13} />
                  </span>
                )}
                {/* Une mention prime sur le simple non-lu (pastille distincte). */}
                {mentionCount > 0 ? (
                  <MentionBadge count={mentionCount} />
                ) : (
                  <UnreadBadge count={c.unread ?? 0} />
                )}
              </span>
            </button>
          );
        })}
      </div>
    </>
  );
}

/** Icône d'un salon selon son genre (texte, vocal, annonces). */
export function ChannelIcon({ kind }: { kind: GroupChannel['kind'] }) {
  if (kind === 'voice') {
    return (
      <svg
        width="16"
        height="16"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth={2}
        strokeLinecap="round"
        strokeLinejoin="round"
        aria-hidden
      >
        <polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5" />
        <path d="M15.54 8.46a5 5 0 0 1 0 7.07" />
        <path d="M19.07 4.93a10 10 0 0 1 0 14.14" />
      </svg>
    );
  }
  if (kind === 'announcement') {
    return (
      <svg
        width="16"
        height="16"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth={2}
        strokeLinecap="round"
        strokeLinejoin="round"
        aria-hidden
      >
        <path d="m3 11 18-5v12L3 14v-3z" />
        <path d="M11.6 16.8a3 3 0 1 1-5.8-1.6" />
      </svg>
    );
  }
  return (
    <span aria-hidden className="text-[17px] font-medium leading-none">
      #
    </span>
  );
}

function ChannelRow({
  channel,
  active,
  unread,
  groupId,
  canManage,
  onOpen,
}: {
  channel: GroupChannel;
  active: boolean;
  /** Nombre de messages non lus du salon (absent ou 0 : pas de pastille). */
  unread?: number | undefined;
  groupId: string;
  /** Renommage/suppression permis (MANAGE_CHANNELS). */
  canManage: boolean;
  onOpen: (channel: GroupChannel) => void;
}) {
  const t = useT();
  const toast = useUi((s) => s.toast);

  /**
   * Items du menu contextuel d'un salon : copie d'identifiant, marquage lu
   * (charge la page récente puis réutilise `markRead`, comme à l'ouverture du
   * salon) et, si permis, édition (paramètres du serveur) / suppression.
   */
  const buildItems = (): ContextMenuItem[] => {
    const items: ContextMenuItem[] = [
      {
        label: t.contextMenu.copyChannelId,
        icon: <CopyMenuIcon />,
        onClick: () =>
          copyToClipboard(
            channel.channel_id,
            () => toast('info', t.app.copied),
            () => toast('error', t.errors.actionFailed),
          ),
      },
    ];
    if (channel.kind !== 'voice' && (unread ?? 0) > 0) {
      items.push({
        label: t.contextMenu.markAsRead,
        icon: <CheckMenuIcon />,
        onClick: () => {
          void (async () => {
            try {
              await useGroups.getState().refreshHistory(groupId, channel.channel_id);
              const key = channelKey(groupId, channel.channel_id);
              const last = (useGroups.getState().messages[key] ?? []).at(-1);
              if (last !== undefined) {
                await useGroups
                  .getState()
                  .markRead(groupId, channel.channel_id, last.lamport);
              }
            } catch {
              toast('error', t.errors.actionFailed);
            }
          })();
        },
      });
    }
    if (canManage) {
      items.push({
        label: t.contextMenu.editChannel,
        icon: <EditMenuIcon />,
        separatorBefore: true,
        onClick: () => useUi.getState().openModal({ kind: 'serverSettings', groupId }),
      });
      items.push({
        label: t.serveur.deleteChannel,
        icon: <DeleteMenuIcon />,
        danger: true,
        onClick: () => {
          if (
            !window.confirm(
              interpolate(t.serveur.deleteChannelConfirm, { name: channel.name }),
            )
          ) {
            return;
          }
          useGroups
            .getState()
            .deleteChannel(groupId, channel.channel_id)
            .catch(() => toast('error', t.errors.actionFailed));
        },
      });
    }
    return items;
  };

  return (
    <button
      type="button"
      onClick={() => onOpen(channel)}
      onContextMenu={(e) => {
        e.preventDefault();
        useContextMenu.getState().openMenu(e.clientX, e.clientY, buildItems());
      }}
      className={`flex h-9 w-full items-center gap-1.5 rounded-md px-2 font-medium transition-colors duration-fast focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar ${
        active
          ? 'bg-chat-hover text-norm'
          : 'text-muted hover:bg-chat-hover hover:text-norm'
      }`}
    >
      <span
        aria-hidden
        className="flex h-4 w-4 shrink-0 items-center justify-center text-faint"
      >
        <ChannelIcon kind={channel.kind} />
      </span>
      <span className="min-w-0 truncate">{channel.name}</span>
      <UnreadBadge count={unread ?? 0} />
    </button>
  );
}

function GroupSidebar({ groupId }: { groupId: string }) {
  const t = useT();
  const view = useUi((s) => s.view);
  const setView = useUi((s) => s.setView);
  const openModal = useUi((s) => s.openModal);
  const toast = useUi((s) => s.toast);
  const state = useGroups((s) => s.states[groupId]);
  const unread = useGroups((s) => s.unread[groupId]);
  const mentionCount = useGroups((s) => s.mentions[groupId]) ?? 0;
  const joinVoice = useVoice((s) => s.join);
  /** Catégories repliées (état d'affichage local, propre à ce panneau). */
  const [collapsed, setCollapsed] = useState<Record<string, boolean>>({});
  const toggleCategory = (categoryId: string): void =>
    setCollapsed((prev) => ({ ...prev, [categoryId]: !(prev[categoryId] ?? false) }));

  const myPerms = state?.my_permissions ?? 0;
  const sections = channelsByCategory(state?.channels ?? [], state?.categories ?? []);
  const hasChannels = sections.some((section) => section.channels.length > 0);
  const activeChannel = view.kind === 'group' ? view.channelId : null;

  /** Ouvre un salon : conversation pour texte/annonces, vocal sinon. */
  const openChannel = (channel: GroupChannel): void => {
    if (channel.kind === 'voice') {
      joinVoice(groupId, channel.channel_id).catch(() =>
        toast('error', t.errors.actionFailed),
      );
      return;
    }
    setView({ kind: 'group', groupId, channelId: channel.channel_id });
  };

  return (
    <>
      <div className="flex h-12 items-center gap-1 border-b border-rail bg-sidebar px-4 shadow-1">
        <span className="min-w-0 flex-1 truncate text-[15px] font-semibold text-header">
          {state?.name ?? '…'}
        </span>
        {mentionCount > 0 && <MentionBadge count={mentionCount} />}
        {hasPerm(myPerms, PERMISSIONS.INVITE) && (
          <HeaderIconButton
            label={t.groups.invite}
            onClick={() => openModal({ kind: 'invite', groupId })}
          >
            <svg
              width="18"
              height="18"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth={2}
              strokeLinecap="round"
              strokeLinejoin="round"
              aria-hidden
            >
              <path d="M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2" />
              <circle cx="9" cy="7" r="4" />
              <line x1="19" x2="19" y1="8" y2="14" />
              <line x1="22" x2="16" y1="11" y2="11" />
            </svg>
          </HeaderIconButton>
        )}
        <HeaderIconButton
          label={t.serveur.settingsTitle}
          onClick={() => openModal({ kind: 'serverSettings', groupId })}
        >
          <svg
            width="18"
            height="18"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth={2}
            strokeLinecap="round"
            strokeLinejoin="round"
            aria-hidden
          >
            <path d="M10.3 3.6a2 2 0 0 1 3.4 0l.4.7a2 2 0 0 0 2.2.9l.8-.2a2 2 0 0 1 2.4 2.4l-.2.8a2 2 0 0 0 .9 2.2l.7.4a2 2 0 0 1 0 3.4l-.7.4a2 2 0 0 0-.9 2.2l.2.8a2 2 0 0 1-2.4 2.4l-.8-.2a2 2 0 0 0-2.2.9l-.4.7a2 2 0 0 1-3.4 0l-.4-.7a2 2 0 0 0-2.2-.9l-.8.2a2 2 0 0 1-2.4-2.4l.2-.8a2 2 0 0 0-.9-2.2l-.7-.4a2 2 0 0 1 0-3.4l.7-.4a2 2 0 0 0 .9-2.2l-.2-.8a2 2 0 0 1 2.4-2.4l.8.2a2 2 0 0 0 2.2-.9l.4-.7Z" />
            <circle cx="12" cy="12" r="3" />
          </svg>
        </HeaderIconButton>
      </div>
      <div className="flex-1 overflow-y-auto p-2">
        <div className="flex items-center justify-between px-2 pb-1 pt-2">
          <span className="text-[11px] font-medium uppercase tracking-wide text-muted">
            {t.groups.channels}
          </span>
          {hasPerm(myPerms, PERMISSIONS.MANAGE_CHANNELS) && (
            <button
              type="button"
              aria-label={t.groups.addChannel}
              title={t.groups.addChannel}
              onClick={() => openModal({ kind: 'createChannel', groupId })}
              className="flex h-6 w-6 shrink-0 items-center justify-center rounded-md text-faint transition-colors duration-fast hover:bg-chat-hover hover:text-norm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar active:scale-95"
            >
              <svg
                width="16"
                height="16"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                strokeWidth={2}
                strokeLinecap="round"
                strokeLinejoin="round"
                aria-hidden
              >
                <path d="M5 12h14" />
                <path d="M12 5v14" />
              </svg>
            </button>
          )}
        </div>
        {!hasChannels && (
          <p className="px-2 py-1 text-sm text-faint">{t.groups.noChannel}</p>
        )}
        {sections.map((section) => {
          const categoryId = section.category?.category_id ?? 'sans-categorie';
          const isOpen = !(collapsed[categoryId] ?? false);
          return (
            <div key={categoryId}>
              {section.category !== null && section.channels.length > 0 && (
                <button
                  type="button"
                  onClick={() => toggleCategory(categoryId)}
                  aria-expanded={isOpen}
                  className="flex w-full items-center gap-1 truncate rounded-md px-2 pb-1 pt-3 text-[11px] font-medium uppercase tracking-wide text-muted transition-colors duration-fast hover:text-norm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar"
                >
                  <Chevron open={isOpen} />
                  <span className="truncate">{section.category.name}</span>
                </button>
              )}
              {isOpen &&
                section.channels.map((ch) => (
                  <ChannelRow
                    key={ch.channel_id}
                    channel={ch}
                    active={activeChannel === ch.channel_id}
                    unread={unread?.[ch.channel_id]}
                    groupId={groupId}
                    canManage={hasPerm(myPerms, PERMISSIONS.MANAGE_CHANNELS)}
                    onOpen={openChannel}
                  />
                ))}
            </div>
          );
        })}
        <VoiceSection groupId={groupId} />
      </div>
    </>
  );
}

export function Sidebar() {
  const view = useUi((s) => s.view);
  const sidebarWidth = useUi((s) => s.sidebarWidth);
  const [inboxOpen, setInboxOpen] = useState(false);
  const openInbox = (): void => setInboxOpen(true);
  return (
    <aside
      className="flex h-full shrink-0 flex-col bg-sidebar"
      style={{ width: sidebarWidth }}
    >
      {view.kind === 'group' ? (
        <GroupSidebar groupId={view.groupId} />
      ) : (
        <HomeSidebar onOpenInbox={openInbox} />
      )}
      <UserPanel />
      {inboxOpen && <MentionInbox onClose={() => setInboxOpen(false)} />}
    </aside>
  );
}
