/**
 * Barre latérale (240 px) : en mode accueil, navigation Amis + conversations
 * privées ; en mode groupe, nom du groupe + salons groupés par catégorie
 * (les sans-catégorie d'abord), boutons gérés par les permissions. Si le
 * serveur a une bannière (`groups.state.banner`), l'en-tête devient un
 * bandeau image façon Discord, nom et actions posés sur un scrim en bas.
 */

import { useEffect, useState } from 'react';
import { interpolate } from '../i18n';
import type { GroupChannel } from '../lib/api';
import { copyToClipboard } from '../lib/clipboard';
import { profileCardGradient } from '../lib/color';
import { draftKey } from '../lib/drafts';
import { hasDraft, useDrafts } from '../stores/drafts';
import { lireFichier } from '../lib/files';
import { estOuvertureMenu, pointAncrageMenu } from '../lib/focus';
import { useCalls } from '../stores/calls';
import { presenceOf, useFriends } from '../stores/friends';
import {
  useGroups,
  channelKey,
  channelsByCategory,
  dmThreadId,
  groupUnreadTotal,
  hasPerm,
  isChannelRestricted,
  isChannelVisible,
  isDmGroup,
  splitGroups,
  upcomingEvents,
  PERMISSIONS,
} from '../stores/groups';
import { channelLevel, useMute, type NotifLevel } from '../stores/mute';
import { sortPinnedFirst, usePinnedDms } from '../stores/pinnedDms';
import { useSession } from '../stores/session';
import { useContextMenu, type ContextMenuItem } from '../stores/contextMenu';
import { useUi, useT } from '../stores/ui';
import { useVoice } from '../stores/voice';
import { Avatar } from './Avatar';
import { buildContactMenu } from './contactMenu';
import {
  BellOffMenuIcon,
  buildNotifLevelItems,
  CheckMenuIcon,
  CopyMenuIcon,
  DeleteMenuIcon,
  EditMenuIcon,
  PhoneOffIcon,
  PlusMenuIcon,
} from './ContextMenu';
import { MentionInbox } from './MentionInbox';
import { SavedMessages } from './SavedMessages';
import { PresenceDot } from './PresenceDot';
import { SearchBar } from './SearchBar';
import { MentionBadge, UnreadBadge } from './UnreadBadge';
import { UserPanel } from './UserPanel';
import { VoiceSection } from './VoiceSection';
import { ServerHeaderMenu } from './server/ServerHeaderMenu';

/** Bouton d'action de l'en-tête, taille fixe (icon spec) : conteneur carré centré. */
function HeaderIconButton({
  label,
  onClick,
  active = false,
  onBanner = false,
  children,
}: {
  label: string;
  onClick: () => void;
  active?: boolean;
  /** Posé sur la bannière du serveur : teintes claires lisibles sur le scrim. */
  onBanner?: boolean;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      onClick={onClick}
      className={`flex h-9 w-9 shrink-0 items-center justify-center rounded-md transition-colors duration-fast focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar active:scale-95 ${
        onBanner
          ? 'text-white/80 hover:bg-white/10 hover:text-white'
          : `hover:bg-chat-hover ${active ? 'text-header' : 'text-muted hover:text-norm'}`
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

/** Bouton d'ouverture du panneau des messages enregistrés (favoris locaux). */
function SavedButton({ onOpen }: { onOpen: () => void }) {
  const t = useT();
  return (
    <HeaderIconButton label={t.saved.button} onClick={onOpen}>
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
        <path d="M19 21l-7-5-7 5V5a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2z" />
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

/**
 * Rangée d'un groupe de MP dans la liste des conversations : icône du groupe
 * (initiales en repli), nom, brouillon et pastille de non-lus. Un clic ouvre
 * le fil unique directement — un groupe de MP n'a pas de liste de salons.
 */
function DmGroupRow({ groupId }: { groupId: string }) {
  const t = useT();
  const view = useUi((s) => s.view);
  const setView = useUi((s) => s.setView);
  const state = useGroups((s) => s.states[groupId]);
  const unread = useGroups((s) => s.unread[groupId]);
  const mentions = useGroups((s) => s.mentions[groupId]) ?? 0;
  const draftKeys = useDrafts((s) => s.keys);
  if (state === undefined) return null;

  const channelId = dmThreadId(state);
  const active = view.kind === 'group' && view.groupId === groupId;
  const draft =
    !active &&
    channelId !== null &&
    hasDraft(draftKeys, draftKey({ kind: 'group', groupId, channelId }));

  return (
    <button
      type="button"
      aria-current={active ? 'page' : undefined}
      onClick={() => setView({ kind: 'group', groupId, channelId })}
      className={`flex h-9 w-full items-center gap-2.5 rounded-md px-2 transition-colors duration-fast focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar ${
        active
          ? 'bg-blurple/15 text-header ring-1 ring-inset ring-blurple/20'
          : 'text-muted hover:bg-chat-hover hover:text-norm'
      }`}
    >
      <span className="shrink-0">
        <Avatar id={groupId} name={state.name} size={32} avatarHash={state.icon} />
      </span>
      <span className="min-w-0 flex-1 truncate text-start font-medium">{state.name}</span>
      {draft && (
        <span
          role="img"
          aria-label={t.dm.draftBadge}
          title={t.dm.draftBadge}
          className="shrink-0 text-faint"
        >
          <DraftIcon />
        </span>
      )}
      {/* Une mention prime sur le simple non-lu, comme partout ailleurs. */}
      {mentions > 0 ? (
        <MentionBadge count={mentions} />
      ) : (
        <UnreadBadge count={groupUnreadTotal(unread)} />
      )}
    </button>
  );
}

/**
 * Section « Groupes » de l'accueil : les groupes de MP rejoints, et le bouton
 * de création. L'en-tête reste visible sans aucun groupe — c'est le seul
 * chemin vers la création, il ne doit pas dépendre d'en avoir déjà un.
 */
function DmGroupsSection() {
  const t = useT();
  const openModal = useUi((s) => s.openModal);
  const ids = useGroups((s) => s.ids);
  const states = useGroups((s) => s.states);
  const { dms } = splitGroups(ids, states);

  return (
    <>
      <div className="flex items-center justify-between px-2 pb-1 pt-4">
        <span className="text-[11px] font-medium uppercase tracking-wide text-muted">
          {t.dmGroups.section}
        </span>
        <HeaderIconButton
          label={t.dmGroups.create}
          onClick={() => openModal({ kind: 'createDmGroup' })}
        >
          {/* « + » du jeu de menu partagé : déjà dans le bundle initial. */}
          <PlusMenuIcon />
        </HeaderIconButton>
      </div>
      {dms.length === 0 ? (
        <p className="px-2 py-1 text-sm text-faint">{t.dmGroups.empty}</p>
      ) : (
        dms.map((id) => <DmGroupRow key={id} groupId={id} />)
      )}
    </>
  );
}

function HomeSidebar({
  onOpenInbox,
  onOpenSaved,
}: {
  onOpenInbox: () => void;
  onOpenSaved: () => void;
}) {
  const t = useT();
  const view = useUi((s) => s.view);
  const setView = useUi((s) => s.setView);
  const contacts = useFriends((s) => s.contacts);
  const missedPeers = useCalls((s) => s.missedPeers);
  const draftKeys = useDrafts((s) => s.keys);
  const pinnedDms = usePinnedDms((s) => s.pinned);
  const friends = sortPinnedFirst(
    contacts.filter((c) => c.state === 'friend'),
    new Set(pinnedDms),
  );

  return (
    <>
      <SearchBar />
      <div className="flex-1 space-y-0.5 overflow-y-auto p-2">
        <button
          type="button"
          aria-current={view.kind === 'friends' ? 'page' : undefined}
          onClick={() => setView({ kind: 'friends' })}
          className={`flex h-9 w-full items-center gap-3 rounded-md px-2 font-medium transition-colors duration-fast focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar ${
            view.kind === 'friends'
              ? 'bg-blurple/15 text-header ring-1 ring-inset ring-blurple/20'
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

        <DmGroupsSection />

        <div className="flex items-center justify-between px-2 pb-1 pt-4">
          <span className="text-[11px] font-medium uppercase tracking-wide text-muted">
            {t.dm.directMessages}
          </span>
          <div className="flex items-center gap-0.5">
            <SavedButton onOpen={onOpenSaved} />
            <InboxButton onOpen={onOpenInbox} />
          </div>
        </div>
        {friends.map((c) => {
          const active = view.kind === 'dm' && view.peer === c.pubkey;
          const status = presenceOf(c);
          const mentionCount = c.mention_count ?? 0;
          const statusText = c.status_text ?? null;
          const hasStatusText = statusText !== null && statusText !== '';
          return (
            <button
              key={c.pubkey}
              type="button"
              aria-current={active ? 'page' : undefined}
              onClick={() => setView({ kind: 'dm', peer: c.pubkey })}
              onContextMenu={(e) => {
                e.preventDefault();
                useContextMenu
                  .getState()
                  .openMenu(
                    e.clientX,
                    e.clientY,
                    buildContactMenu(t, c, e.currentTarget),
                  );
              }}
              className={`flex ${hasStatusText ? 'h-11' : 'h-9'} w-full items-center gap-2.5 rounded-md px-2 transition-colors duration-fast focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar ${
                active
                  ? 'bg-blurple/15 text-header ring-1 ring-inset ring-blurple/20'
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
                  decoration={c.avatar_decoration ?? null}
                />
                <PresenceDot
                  status={status}
                  label={t.profil[status]}
                  className="absolute -bottom-0.5 -end-0.5 rounded-full ring-2 ring-sidebar"
                />
              </span>
              <span className="min-w-0">
                <span className="block truncate font-medium">
                  {c.display_name || c.friend_code}
                </span>
                {hasStatusText && (
                  <span className="block truncate text-xs text-muted">{statusText}</span>
                )}
              </span>
              <span className="flex shrink-0 items-center gap-1">
                {!active &&
                  hasDraft(draftKeys, draftKey({ kind: 'dm', peer: c.pubkey })) && (
                    <span
                      role="img"
                      aria-label={t.dm.draftBadge}
                      title={t.dm.draftBadge}
                      className="text-faint"
                    >
                      <DraftIcon />
                    </span>
                  )}
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
  if (kind === 'forum') {
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
        <path d="M17 6.1H3" />
        <path d="M21 12.1H3" />
        <path d="M15.1 18H3" />
      </svg>
    );
  }
  return (
    <span aria-hidden className="text-[17px] font-medium leading-none">
      #
    </span>
  );
}

/** Icône « calendrier » (entrée Événements), icon spec 20 px. */
function CalendarIcon() {
  return (
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
      <rect width="18" height="18" x="3" y="4" rx="2" />
      <path d="M16 2v4" />
      <path d="M8 2v4" />
      <path d="M3 10h18" />
    </svg>
  );
}

/** Icône de cadenas (salon restreint par au moins un override de rôle), icon spec 14 px. */
function LockIcon() {
  return (
    <svg
      width="14"
      height="14"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
    >
      <rect x="3" y="11" width="18" height="11" rx="2" ry="2" />
      <path d="M7 11V7a5 5 0 0 1 10 0v4" />
    </svg>
  );
}

/** Petit crayon « brouillon en cours » des listes de conversations. */
function DraftIcon() {
  return (
    <svg
      width="12"
      height="12"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
    >
      <path d="M21.174 6.812a1 1 0 0 0-3.986-3.987L3.842 16.174a2 2 0 0 0-.5.83l-1.321 4.352a.5.5 0 0 0 .623.622l4.353-1.32a2 2 0 0 0 .83-.497z" />
    </svg>
  );
}

function ChannelRow({
  channel,
  active,
  unread,
  mentions,
  groupId,
  canManage,
  restricted,
  level,
  onOpen,
}: {
  channel: GroupChannel;
  active: boolean;
  /** Nombre de messages non lus du salon (absent ou 0 : pas de pastille). */
  unread?: number | undefined;
  /** Mentions non lues du salon (prime sur le non-lu simple ; absent ou 0 : rien). */
  mentions?: number | undefined;
  groupId: string;
  /** Renommage/suppression permis (MANAGE_CHANNELS). */
  canManage: boolean;
  /** Au moins un override de rôle refuse VIEW ou SEND sur ce salon. */
  restricted: boolean;
  /**
   * Niveau de notification effectif de ce salon (voir `stores/mute.ts`,
   * héritage salon←serveur déjà appliqué par l'appelant) : 'none' atténue la
   * ligne et affiche l'icône cloche barrée. Réglable indépendamment du serveur
   * entier (`ServerRail`), les deux se combinent à l'exécution côté
   * notification (`isConversationSilenced`).
   */
  level: NotifLevel;
  onOpen: (channel: GroupChannel) => void;
}) {
  const t = useT();
  const toast = useUi((s) => s.toast);
  const draftKeys = useDrafts((s) => s.keys);
  const muted = level === 'none';
  const channelDraft =
    !active &&
    channel.kind !== 'voice' &&
    hasDraft(
      draftKeys,
      draftKey({ kind: 'group', groupId, channelId: channel.channel_id }),
    );

  /**
   * Items du menu contextuel d'un salon : copie d'identifiant, niveau de
   * notification (sous-menu local à trois choix, ce salon uniquement), marquage
   * lu (charge la page récente puis réutilise `markRead`, comme à l'ouverture du
   * salon) et, si permis, édition (paramètres du serveur) / suppression. `x`/`y`
   * : point de clic, pour rouvrir le sous-menu au même endroit.
   */
  const buildItems = (x: number, y: number): ContextMenuItem[] => {
    const items: ContextMenuItem[] = [
      {
        label: t.contextMenu.copyChannelId,
        icon: <CopyMenuIcon />,
        onClick: () =>
          copyToClipboard(
            channel.channel_id,
            () => toast('success', t.app.copied),
            () => toast('error', t.errors.actionFailed),
          ),
      },
      {
        label: t.notifLevel.title,
        icon: <BellOffMenuIcon />,
        onClick: () =>
          useContextMenu.getState().openMenu(
            x,
            y,
            buildNotifLevelItems(t.notifLevel, level, (lvl) =>
              useMute.getState().setChannelLevel(groupId, channel.channel_id, lvl),
            ),
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
      aria-current={active ? 'page' : undefined}
      onClick={() => onOpen(channel)}
      onContextMenu={(e) => {
        e.preventDefault();
        useContextMenu
          .getState()
          .openMenu(e.clientX, e.clientY, buildItems(e.clientX, e.clientY));
      }}
      onKeyDown={(e) => {
        // Équivalent clavier du clic droit (Maj+F10 / touche Menu).
        if (!estOuvertureMenu(e)) return;
        e.preventDefault();
        const { x, y } = pointAncrageMenu(e.currentTarget);
        useContextMenu.getState().openMenu(x, y, buildItems(x, y));
      }}
      className={`flex h-9 w-full items-center gap-1.5 rounded-md px-2 font-medium transition-colors duration-fast focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar ${
        active
          ? 'bg-blurple/15 text-header ring-1 ring-inset ring-blurple/20'
          : 'text-muted hover:bg-chat-hover hover:text-norm'
      } ${muted ? 'opacity-50' : ''}`}
    >
      <span
        aria-hidden
        className="flex h-4 w-4 shrink-0 items-center justify-center text-faint"
      >
        <ChannelIcon kind={channel.kind} />
      </span>
      <span className="min-w-0 truncate">{channel.name}</span>
      {/* Une mention prime sur le simple non-lu (pastille distincte), posée à
          côté du nom (le nom tronque, la pastille ne rétrécit pas). */}
      {channelDraft && (
        <span
          role="img"
          aria-label={t.dm.draftBadge}
          title={t.dm.draftBadge}
          className="shrink-0 text-faint"
        >
          <DraftIcon />
        </span>
      )}
      {(mentions ?? 0) > 0 ? (
        <MentionBadge count={mentions ?? 0} />
      ) : (
        <UnreadBadge count={unread ?? 0} />
      )}
      {restricted && (
        <span
          role="img"
          aria-label={t.serveur.channelRestrictedLabel}
          title={t.serveur.channelRestrictedLabel}
          className="shrink-0 text-faint"
        >
          <LockIcon />
        </span>
      )}
      {muted && (
        <span
          role="img"
          aria-label={t.serveur.mutedChannelLabel}
          title={t.serveur.mutedChannelLabel}
          className="shrink-0 text-faint"
        >
          <BellOffMenuIcon />
        </span>
      )}
    </button>
  );
}

/** Petit chevron du bouton d'en-tête serveur : pointe vers le bas, 180° une fois ouvert. */
function HeaderChevronIcon({
  open,
  colorClassName = 'text-faint',
}: {
  open: boolean;
  /** Teinte du chevron (claire sur la bannière du serveur). */
  colorClassName?: string;
}) {
  return (
    <svg
      width="16"
      height="16"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
      className={`shrink-0 ${colorClassName} transition-transform duration-fast ease-expo ${open ? 'rotate-180' : 'rotate-0'}`}
    >
      <path d="m6 9 6 6 6-6" />
    </svg>
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
  const channelMentions = useGroups((s) => s.channelMentions[groupId]);
  const serverLevels = useMute((s) => s.serverLevels);
  const channelLevels = useMute((s) => s.channelLevels);
  const hideMutedChannels = useUi((s) => s.hideMutedChannels);
  const joinVoice = useVoice((s) => s.join);
  const self = useSession((s) => s.self);
  /** Menu déroulant du nom de serveur (ouvert/fermé). */
  const [serverMenuOpen, setServerMenuOpen] = useState(false);
  /**
   * URL `data:` de la bannière du serveur, chargée par sa racine Merkle
   * (`groups.state.banner`). Tant qu'elle n'est pas résolue (chargement en
   * cours, échec ou absence de bannière), l'en-tête simple s'affiche —
   * `lib/files` gère lui-même les reprises, aucun re-essai ici.
   */
  const [bannerUrl, setBannerUrl] = useState<string | null>(null);
  const banner = state?.banner ?? null;
  useEffect(() => {
    let alive = true;
    setBannerUrl(null);
    if (banner === null) return undefined;
    lireFichier(banner)
      .then((url) => {
        if (alive) setBannerUrl(url);
      })
      .catch(() => {
        // Bannière indisponible : repli silencieux sur l'en-tête simple.
      });
    return () => {
      alive = false;
    };
  }, [banner]);
  /** Catégories repliées (état d'affichage local, propre à ce panneau). */
  const [collapsed, setCollapsed] = useState<Record<string, boolean>>({});
  const toggleCategory = (categoryId: string): void =>
    setCollapsed((prev) => ({ ...prev, [categoryId]: !(prev[categoryId] ?? false) }));

  const myPerms = state?.my_permissions ?? 0;
  // Le nœud envoie tous les salons du groupe (`groups.state` ne filtre pas
  // par VIEW) : on masque ici ceux que l'utilisateur local ne peut pas voir,
  // que l'onglet Salons des paramètres continue lui d'afficher intégralement
  // aux porteurs de MANAGE_CHANNELS (`ServerChannelsTab`, non filtré).
  const visibleChannels = (state?.channels ?? []).filter((c) =>
    isChannelVisible(state, c.channel_id, self?.pubkey ?? null),
  );
  const activeChannel = view.kind === 'group' ? view.channelId : null;
  // Masquage des salons muets (préférence locale, menu du serveur) : retire les
  // salons au niveau effectif 'none' (héritage salon←serveur compris), en
  // gardant toujours le salon actif pour ne jamais cacher la conversation
  // ouverte (façon Discord).
  const shownChannels = hideMutedChannels
    ? visibleChannels.filter(
        (c) =>
          c.channel_id === activeChannel ||
          channelLevel({ serverLevels, channelLevels }, groupId, c.channel_id) !== 'none',
      )
    : visibleChannels;
  const sections = channelsByCategory(shownChannels, state?.categories ?? []);
  const hasChannels = sections.some((section) => section.channels.length > 0);

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

  const bannerGradient = profileCardGradient(state?.banner_color ?? null);
  const upcomingCount = upcomingEvents(state).length;
  /**
   * Un hash suffit à réserver le bandeau : la liste des salons ne saute plus
   * de 80 px lorsque l'image finit de charger. Le fond reste sombre et teinté
   * pendant le chargement (ou si le fichier est indisponible), puis l'image
   * apparaît en fondu au-dessus.
   */
  const hasBanner = banner !== null;
  const bannerBackground = hasBanner
    ? (bannerGradient ??
      'linear-gradient(135deg, rgb(var(--color-blurple) / 0.72), rgb(var(--color-tooltip)))')
    : bannerGradient;

  return (
    <>
      <div
        data-testid="server-header"
        className={`accord-server-header relative shrink-0 border-b border-rail shadow-1 ${
          hasBanner ? 'h-24 bg-tooltip' : 'h-12 bg-sidebar'
        }`}
        style={
          bannerBackground !== null ? { backgroundImage: bannerBackground } : undefined
        }
      >
        {bannerUrl !== null && (
          <>
            {/* `rounded-t-[inherit]` : l'image et le scrim épousent les coins
                hauts arrondis de l'en-tête (voir `--accord-header-radius` dans
                identity-refresh.css) au lieu de déborder en angles droits. */}
            <img
              src={bannerUrl}
              alt=""
              aria-hidden
              data-testid="server-banner"
              className="absolute inset-0 h-full w-full animate-[fade-in_var(--duration-normal)_var(--ease-out)] rounded-t-[inherit] object-cover"
            />
            {/* Scrim de lisibilité sous le nom : dégradé noir → transparent. */}
            <div
              aria-hidden
              data-testid="server-banner-scrim"
              className="pointer-events-none absolute inset-0 rounded-t-[inherit] bg-gradient-to-t from-black/60 via-black/20 to-transparent"
            />
          </>
        )}
        <div
          className={`relative flex h-full gap-1 px-4 ${hasBanner ? 'items-end pb-2' : 'items-center'}`}
        >
          <button
            type="button"
            aria-haspopup="menu"
            aria-expanded={serverMenuOpen}
            onMouseDown={(e) => e.stopPropagation()}
            onClick={() => setServerMenuOpen((open) => !open)}
            className={`flex h-9 min-w-0 flex-1 items-center gap-1 rounded-md pe-1 text-start transition-colors duration-fast focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar ${
              serverMenuOpen
                ? hasBanner
                  ? 'bg-black/25'
                  : 'bg-chat-hover'
                : hasBanner
                  ? 'hover:bg-white/10'
                  : 'hover:bg-chat-hover'
            }`}
          >
            <span
              className={`min-w-0 flex-1 truncate text-[15px] font-semibold ${
                hasBanner ? 'text-white' : 'text-header'
              }`}
            >
              {state?.name ?? '…'}
            </span>
            <HeaderChevronIcon
              open={serverMenuOpen}
              colorClassName={hasBanner ? 'text-white/80' : 'text-faint'}
            />
          </button>
          {serverMenuOpen && (
            <ServerHeaderMenu
              groupId={groupId}
              onClose={() => setServerMenuOpen(false)}
            />
          )}
          {mentionCount > 0 && <MentionBadge count={mentionCount} />}
          {hasPerm(myPerms, PERMISSIONS.INVITE) && (
            <HeaderIconButton
              label={t.groups.invite}
              onClick={() => openModal({ kind: 'invite', groupId })}
              onBanner={hasBanner}
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
        </div>
      </div>
      <div className="flex-1 overflow-y-auto p-2">
        <button
          type="button"
          onClick={() => openModal({ kind: 'events', groupId })}
          className="flex h-9 w-full items-center gap-3 rounded-md px-2 font-medium text-muted transition-colors duration-fast hover:bg-chat-hover hover:text-norm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar"
        >
          <span className="flex h-5 w-5 shrink-0 items-center justify-center">
            <CalendarIcon />
          </span>
          <span className="min-w-0 flex-1 truncate text-start">
            {t.groups.eventsEntry}
          </span>
          {upcomingCount > 0 && (
            <span
              aria-label={interpolate(t.groups.eventsBadge, {
                count: String(upcomingCount),
              })}
              className="badge-pop ms-auto min-w-4 shrink-0 rounded-full bg-red px-1.5 text-center text-[11px] font-semibold leading-4 text-on-red"
            >
              {upcomingCount}
            </span>
          )}
        </button>
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
              className="flex h-9 w-9 shrink-0 items-center justify-center rounded-md text-faint transition-colors duration-fast hover:bg-chat-hover hover:text-norm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar active:scale-95"
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
                    mentions={channelMentions?.[ch.channel_id]}
                    groupId={groupId}
                    canManage={hasPerm(myPerms, PERMISSIONS.MANAGE_CHANNELS)}
                    restricted={isChannelRestricted(state, ch.channel_id)}
                    level={channelLevel(
                      { serverLevels, channelLevels },
                      groupId,
                      ch.channel_id,
                    )}
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
  const t = useT();
  const view = useUi((s) => s.view);
  const sidebarWidth = useUi((s) => s.sidebarWidth);
  /**
   * Un groupe de MP garde la barre latérale d'accueil : pas de liste de
   * salons, pas de catégories, pas de bannière de serveur — sa conversation
   * se lit depuis la liste des conversations, où elle est rangée.
   */
  const isDm = useGroups((s) =>
    view.kind === 'group' ? isDmGroup(s.states[view.groupId]) : false,
  );
  const [inboxOpen, setInboxOpen] = useState(false);
  const [savedOpen, setSavedOpen] = useState(false);
  const openInbox = (): void => setInboxOpen(true);
  const openSaved = (): void => setSavedOpen(true);
  return (
    <aside
      aria-label={t.layout.sidebarLabel}
      className="theme-surface-sidebar accord-sidebar flex h-full shrink-0 flex-col bg-sidebar"
      style={{ width: sidebarWidth }}
    >
      {view.kind === 'group' && !isDm ? (
        <GroupSidebar groupId={view.groupId} />
      ) : (
        <HomeSidebar onOpenInbox={openInbox} onOpenSaved={openSaved} />
      )}
      <UserPanel />
      {inboxOpen && <MentionInbox onClose={() => setInboxOpen(false)} />}
      {savedOpen && <SavedMessages onClose={() => setSavedOpen(false)} />}
    </aside>
  );
}
