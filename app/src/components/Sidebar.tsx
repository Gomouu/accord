/**
 * Barre latérale (240 px) : en mode accueil, navigation Amis + conversations
 * privées ; en mode groupe, nom du groupe + salons groupés par catégorie
 * (les sans-catégorie d'abord), boutons gérés par les permissions.
 */

import { useState } from 'react';
import { interpolate } from '../i18n';
import type { GroupChannel } from '../lib/api';
import { copyToClipboard } from '../lib/clipboard';
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
import { CheckMenuIcon, CopyMenuIcon, DeleteMenuIcon, EditMenuIcon } from './ContextMenu';
import { MentionInbox } from './MentionInbox';
import { PresenceDot } from './PresenceDot';
import { SearchBar } from './SearchBar';
import { MentionBadge, UnreadBadge } from './UnreadBadge';
import { UserPanel } from './UserPanel';
import { VoiceSection } from './VoiceSection';

/** Bouton d'ouverture de la boîte de mentions (icône « @ »). */
function InboxButton({ onOpen }: { onOpen: () => void }) {
  const t = useT();
  return (
    <button
      type="button"
      aria-label={t.mentions.open}
      title={t.mentions.open}
      onClick={onOpen}
      className="shrink-0 rounded p-1.5 text-muted transition-colors duration-fast hover:bg-chat-hover hover:text-norm active:scale-95"
    >
      <svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor" aria-hidden>
        <path d="M12 2a10 10 0 0 0 0 20 9.9 9.9 0 0 0 5-1.3 1 1 0 1 0-1-1.7A7.9 7.9 0 0 1 12 20a8 8 0 1 1 8-8v1a1.5 1.5 0 0 1-3 0V8a1 1 0 1 0-2 0v.5A4 4 0 1 0 16 14a3.5 3.5 0 0 0 6-2v-.9A10 10 0 0 0 12 2Zm0 12a2 2 0 1 1 0-4 2 2 0 0 1 0 4Z" />
      </svg>
    </button>
  );
}

function HomeSidebar({ onOpenInbox }: { onOpenInbox: () => void }) {
  const t = useT();
  const view = useUi((s) => s.view);
  const setView = useUi((s) => s.setView);
  const contacts = useFriends((s) => s.contacts);
  const friends = contacts.filter((c) => c.state === 'friend');

  return (
    <>
      <SearchBar />
      <div className="flex-1 space-y-0.5 overflow-y-auto p-2">
        <button
          type="button"
          onClick={() => setView({ kind: 'friends' })}
          className={`flex w-full items-center gap-3 rounded px-2 py-2 font-medium transition-colors duration-fast ${
            view.kind === 'friends'
              ? 'bg-chat-hover text-header'
              : 'text-muted hover:bg-chat-hover hover:text-norm'
          }`}
        >
          <svg width="22" height="22" viewBox="0 0 24 24" fill="currentColor" aria-hidden>
            <path d="M9 11a4 4 0 1 0 0-8 4 4 0 0 0 0 8Zm0 2c-3.3 0-7 1.7-7 4v2a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-2c0-2.3-3.7-4-7-4Zm7.5-2.2a3.6 3.6 0 0 0 0-6.6 5.5 5.5 0 0 1 0 6.6ZM19 13.3c1.8.8 3 2 3 3.7v2a1 1 0 0 1-1 1h-3.3c.2-.3.3-.6.3-1v-2c0-1.5-.6-2.7-1.6-3.7.9 0 1.8 0 2.6.1Z" />
          </svg>
          {t.friends.title}
        </button>

        <div className="flex items-center justify-between px-2 pb-1 pt-4">
          <span className="text-xs font-semibold uppercase tracking-wide text-faint">
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
              className={`flex w-full items-center gap-2.5 rounded px-2 py-1.5 transition-colors duration-fast ${
                active
                  ? 'bg-chat-hover text-header'
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
              {/* Une mention prime sur le simple non-lu (pastille distincte). */}
              {mentionCount > 0 ? (
                <MentionBadge count={mentionCount} />
              ) : (
                <UnreadBadge count={c.unread ?? 0} />
              )}
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
      <svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor" aria-hidden>
        <path d="M11.4 4.1 7 8H4a1 1 0 0 0-1 1v6a1 1 0 0 0 1 1h3l4.4 3.9a1 1 0 0 0 1.6-.8V4.9a1 1 0 0 0-1.6-.8Z" />
        <path d="M15.5 8.5a1 1 0 0 1 1.4 0 5 5 0 0 1 0 7 1 1 0 1 1-1.4-1.4 3 3 0 0 0 0-4.2 1 1 0 0 1 0-1.4Z" />
      </svg>
    );
  }
  if (kind === 'announcement') {
    return (
      <svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor" aria-hidden>
        <path d="M19 4a1 1 0 0 1 1.5.9v14.2a1 1 0 0 1-1.5.9L13 17H7a4 4 0 0 1 0-8h6l6-5Zm-9 15a2 2 0 0 1-2-2v-1h4v1a2 2 0 0 1-2 2Z" />
      </svg>
    );
  }
  return (
    <span aria-hidden className="text-lg font-normal leading-none">
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
                await useGroups.getState().markRead(groupId, channel.channel_id, last.lamport);
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
          if (!window.confirm(interpolate(t.serveur.deleteChannelConfirm, { name: channel.name }))) {
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
      className={`flex w-full items-center gap-1.5 rounded px-2 py-1.5 font-medium transition-colors duration-fast ${
        active
          ? 'bg-chat-hover text-header'
          : 'text-muted hover:bg-chat-hover hover:text-norm'
      }`}
    >
      <span aria-hidden className="flex w-5 shrink-0 justify-center text-faint">
        <ChannelIcon kind={channel.kind} />
      </span>
      <span className="min-w-0 truncate">{channel.name}</span>
      <UnreadBadge count={unread ?? 0} />
    </button>
  );
}

function GroupSidebar({
  groupId,
  onOpenInbox,
}: {
  groupId: string;
  onOpenInbox: () => void;
}) {
  const t = useT();
  const view = useUi((s) => s.view);
  const setView = useUi((s) => s.setView);
  const openModal = useUi((s) => s.openModal);
  const toast = useUi((s) => s.toast);
  const state = useGroups((s) => s.states[groupId]);
  const unread = useGroups((s) => s.unread[groupId]);
  const mentionCount = useGroups((s) => s.mentions[groupId]) ?? 0;
  const joinVoice = useVoice((s) => s.join);

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
      <div className="flex h-12 items-center justify-between gap-1 border-b border-rail px-4 shadow-sm">
        <span className="min-w-0 flex-1 truncate font-semibold text-header">
          {state?.name ?? '…'}
        </span>
        {mentionCount > 0 && <MentionBadge count={mentionCount} />}
        <InboxButton onOpen={onOpenInbox} />
        {hasPerm(myPerms, PERMISSIONS.INVITE) && (
          <button
            type="button"
            aria-label={t.groups.invite}
            title={t.groups.invite}
            onClick={() => openModal({ kind: 'invite', groupId })}
            className="rounded p-1 text-muted transition-colors duration-fast hover:text-norm active:scale-95"
          >
            <svg
              width="18"
              height="18"
              viewBox="0 0 24 24"
              fill="currentColor"
              aria-hidden
            >
              <path d="M15 8a4 4 0 1 1-8 0 4 4 0 0 1 8 0Zm-4 6c-3.3 0-7 1.7-7 4v1a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-1c0-2.3-3.7-4-7-4Zm9-8a1 1 0 0 1 1 1v2h2a1 1 0 1 1 0 2h-2v2a1 1 0 1 1-2 0v-2h-2a1 1 0 1 1 0-2h2V7a1 1 0 0 1 1-1Z" />
            </svg>
          </button>
        )}
        <button
          type="button"
          aria-label={t.serveur.settingsTitle}
          title={t.serveur.settingsTitle}
          onClick={() => openModal({ kind: 'serverSettings', groupId })}
          className="rounded p-1 text-muted transition-colors duration-fast hover:text-norm active:scale-95"
        >
          <svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor" aria-hidden>
            <path d="M10.3 3.6a2 2 0 0 1 3.4 0l.6 1a2 2 0 0 0 2.2.9l1.1-.3a2 2 0 0 1 2.4 2.4l-.3 1.1a2 2 0 0 0 .9 2.2l1 .6a2 2 0 0 1 0 3.4l-1 .6a2 2 0 0 0-.9 2.2l.3 1.1a2 2 0 0 1-2.4 2.4l-1.1-.3a2 2 0 0 0-2.2.9l-.6 1a2 2 0 0 1-3.4 0l-.6-1a2 2 0 0 0-2.2-.9l-1.1.3a2 2 0 0 1-2.4-2.4l.3-1.1a2 2 0 0 0-.9-2.2l-1-.6a2 2 0 0 1 0-3.4l1-.6a2 2 0 0 0 .9-2.2l-.3-1.1a2 2 0 0 1 2.4-2.4l1.1.3a2 2 0 0 0 2.2-.9l.6-1ZM12 15.5a3.5 3.5 0 1 0 0-7 3.5 3.5 0 0 0 0 7Z" />
          </svg>
        </button>
      </div>
      <div className="flex-1 overflow-y-auto p-2">
        <div className="flex items-center justify-between px-2 pb-1 pt-2">
          <span className="text-xs font-semibold uppercase tracking-wide text-faint">
            {t.groups.channels}
          </span>
          {hasPerm(myPerms, PERMISSIONS.MANAGE_CHANNELS) && (
            <button
              type="button"
              aria-label={t.groups.addChannel}
              title={t.groups.addChannel}
              onClick={() => openModal({ kind: 'createChannel', groupId })}
              className="text-faint transition-colors duration-fast hover:text-norm active:scale-95"
            >
              <svg
                width="16"
                height="16"
                viewBox="0 0 24 24"
                fill="currentColor"
                aria-hidden
              >
                <path d="M11 5a1 1 0 1 1 2 0v6h6a1 1 0 1 1 0 2h-6v6a1 1 0 1 1-2 0v-6H5a1 1 0 1 1 0-2h6V5Z" />
              </svg>
            </button>
          )}
        </div>
        {!hasChannels && (
          <p className="px-2 py-1 text-sm text-faint">{t.groups.noChannel}</p>
        )}
        {sections.map((section) => (
          <div key={section.category?.category_id ?? 'sans-categorie'}>
            {section.category !== null && section.channels.length > 0 && (
              <div className="truncate px-2 pb-1 pt-3 text-xs font-semibold uppercase tracking-wide text-faint">
                {section.category.name}
              </div>
            )}
            {section.channels.map((ch) => (
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
        ))}
        <VoiceSection groupId={groupId} />
      </div>
    </>
  );
}

export function Sidebar() {
  const view = useUi((s) => s.view);
  const [inboxOpen, setInboxOpen] = useState(false);
  const openInbox = (): void => setInboxOpen(true);
  return (
    <aside className="flex h-full w-60 flex-col bg-sidebar">
      {view.kind === 'group' ? (
        <GroupSidebar groupId={view.groupId} onOpenInbox={openInbox} />
      ) : (
        <HomeSidebar onOpenInbox={openInbox} />
      )}
      <UserPanel />
      {inboxOpen && <MentionInbox onClose={() => setInboxOpen(false)} />}
    </aside>
  );
}
