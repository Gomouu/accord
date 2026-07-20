/**
 * Barre latérale (240 px) : en mode accueil, navigation Amis + conversations
 * privées ; en mode groupe, nom du groupe + salons groupés par catégorie
 * (les sans-catégorie d'abord), boutons gérés par les permissions. Si le
 * serveur a une bannière (`groups.state.banner`), l'en-tête devient un
 * bandeau image façon Discord, nom et actions posés sur un scrim en bas.
 */

import { useEffect, useRef, useState } from 'react';
import { interpolate } from '../i18n';
import type { GroupChannel } from '../lib/api';
import { copyToClipboard } from '../lib/clipboard';
import { profileCardGradient } from '../lib/color';
import { draftKey } from '../lib/drafts';
import { marquerServeurLu } from '../lib/markServerRead';
import { hasDraft, useDrafts } from '../stores/drafts';
import { lireFichier } from '../lib/files';
import { estOuvertureMenu, pointAncrageMenu } from '../lib/focus';
import { useCalls } from '../stores/calls';
import { presenceOf, useFriends } from '../stores/friends';
import {
  useGroups,
  channelKey,
  channelsByCategory,
  hasPerm,
  isChannelRestricted,
  isChannelVisible,
  upcomingEvents,
  PERMISSIONS,
} from '../stores/groups';
import { channelLevel, serverLevel, useMute, type NotifLevel } from '../stores/mute';
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
  EnvelopeMenuIcon,
  GearMenuIcon,
  LeaveMenuIcon,
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
                  className="absolute -bottom-0.5 -right-0.5 rounded-full ring-2 ring-sidebar"
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

/** Icône « calendrier » du jeu de menu (14 px, création d'événement — en-tête). */
function EventMenuIcon() {
  return (
    <svg
      width={14}
      height={14}
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

function MenuChevronIcon() {
  return (
    <svg
      width="14"
      height="14"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2.25"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
    >
      <path d="m9 18 6-6-6-6" />
    </svg>
  );
}

interface ServerMenuItem {
  label: string;
  icon: React.ReactNode;
  /** Action directe ; absente pour un item à sous-menu (voir `submenu`). */
  onClick?: () => void;
  danger?: boolean;
  separatorBefore?: boolean;
  /** Item à cocher (case) : rendu `menuitemcheckbox`, coche à droite si `true`. */
  checked?: boolean;
  /**
   * Ouvre un sous-menu (menu contextuel générique) au lieu d'agir directement —
   * réutilise `buildNotifLevelItems`. Le menu déroulant se ferme et le sous-menu
   * s'ancre sur la ligne cliquée.
   */
  submenu?: () => ContextMenuItem[];
}

/**
 * Menu déroulant du nom de serveur (façon Discord), ancré sous l'en-tête et
 * large de la barre latérale (moins ses marges) — même langage visuel que
 * `ContextMenu`/`UserMenu` (`.glass-strong`, icônes partagées, danger rouge),
 * mais positionné en dropdown plutôt qu'au point de clic. Icônes en fin de
 * ligne (façon menu de serveur Discord). Items construits en réutilisant
 * exclusivement des actions déjà existantes des stores (modales, sourdine,
 * copie) ; le niveau de notification passe par le sous-menu générique et le
 * masquage des salons muets bascule une simple préférence locale.
 */
function ServerHeaderMenu({
  groupId,
  name,
  onClose,
}: {
  groupId: string;
  name: string;
  onClose: () => void;
}) {
  const t = useT();
  const toast = useUi((s) => s.toast);
  const openModal = useUi((s) => s.openModal);
  const setView = useUi((s) => s.setView);
  const hideMutedChannels = useUi((s) => s.hideMutedChannels);
  const serverLevels = useMute((s) => s.serverLevels);
  const state = useGroups((s) => s.states[groupId]);
  const unreadByChannel = useGroups((s) => s.unread[groupId]);
  const menuMentionCount = useGroups((s) => s.mentions[groupId]) ?? 0;
  const self = useSession((s) => s.self);
  const ref = useRef<HTMLDivElement>(null);
  const itemRefs = useRef<Array<HTMLButtonElement | null>>([]);
  const [activeIndex, setActiveIndex] = useState(0);

  // Fermeture au clic extérieur et à Échap (même approche que ContextMenu/UserMenu).
  useEffect(() => {
    const onKey = (e: KeyboardEvent): void => {
      if (e.key === 'Escape') onClose();
    };
    const onDown = (e: MouseEvent): void => {
      if (ref.current !== null && !ref.current.contains(e.target as Node)) onClose();
    };
    window.addEventListener('keydown', onKey);
    document.addEventListener('mousedown', onDown);
    return () => {
      window.removeEventListener('keydown', onKey);
      document.removeEventListener('mousedown', onDown);
    };
  }, [onClose]);

  useEffect(() => {
    // Focus pris à l'ouverture, rendu au déclencheur (le bouton du nom de
    // serveur) à la fermeture — sauf s'il a quitté le DOM entre-temps.
    const precedent =
      document.activeElement instanceof HTMLElement ? document.activeElement : null;
    itemRefs.current[0]?.focus();
    return () => {
      if (precedent !== null && precedent.isConnected) precedent.focus();
    };
  }, []);

  if (state === undefined) return null;

  const myPerms = state.my_permissions;
  const isFounder = self !== null && state.founder === self.pubkey;
  // Même garde que le menu contextuel du rail (ServerRail) : le fondateur ne
  // peut pas quitter tant que d'autres membres restent (règle du contrat).
  const founderBlocked = isFounder && state.members.length > 1;

  const canManageChannels = hasPerm(myPerms, PERMISSIONS.MANAGE_CHANNELS);
  const serverHasUnread =
    menuMentionCount > 0 || Object.values(unreadByChannel ?? {}).some((n) => n > 0);

  const items: ServerMenuItem[] = [];
  // Marquer comme lu — seulement si le serveur a des non-lus (jamais un
  // no-op), en tête comme Discord ; même boucle que le menu du rail.
  if (serverHasUnread) {
    items.push({
      label: t.contextMenu.markAsRead,
      icon: <CheckMenuIcon />,
      onClick: () => {
        void marquerServeurLu(groupId);
      },
    });
  }
  if (hasPerm(myPerms, PERMISSIONS.INVITE)) {
    items.push({
      label: t.groups.invitePeople,
      icon: <EnvelopeMenuIcon />,
      separatorBefore: items.length === 1,
      onClick: () => openModal({ kind: 'invite', groupId }),
    });
  }
  items.push({
    label: t.serveur.settingsTitle,
    icon: <GearMenuIcon />,
    separatorBefore: items.length === 1,
    onClick: () => openModal({ kind: 'serverSettings', groupId }),
  });
  if (canManageChannels) {
    items.push({
      label: t.groups.addChannel,
      icon: <PlusMenuIcon />,
      onClick: () => openModal({ kind: 'createChannel', groupId }),
    });
    // Pas de modale dédiée : réutilise le formulaire de création de
    // catégorie déjà existant dans Paramètres du serveur → Salons
    // (`ServerChannelsTab`, action `groups.addCategory`) plutôt que
    // dupliquer sa logique dans une nouvelle modale.
    items.push({
      label: t.serveur.createCategoryAction,
      icon: <PlusMenuIcon />,
      onClick: () =>
        openModal({ kind: 'serverSettings', groupId, initialTab: 'channels' }),
    });
    items.push({
      label: t.groups.eventCreate,
      icon: <EventMenuIcon />,
      onClick: () => openModal({ kind: 'events', groupId }),
    });
  }
  // Notifications : ouvre le sous-menu générique (mêmes trois niveaux que le
  // rail, voir `stores/mute.ts`) plutôt qu'une modale — ce menu déroulant ne
  // gère pas lui-même les items imbriqués.
  items.push({
    label: t.notifLevel.title,
    icon: <BellOffMenuIcon />,
    separatorBefore: true,
    submenu: () =>
      buildNotifLevelItems(
        t.notifLevel,
        serverLevel({ serverLevels, channelLevels: {} }, groupId),
        (lvl) => useMute.getState().setServerLevel(groupId, lvl),
      ),
  });
  items.push({
    label: t.serveur.hideMutedChannels,
    icon: <BellOffMenuIcon />,
    checked: hideMutedChannels,
    onClick: () => useUi.getState().toggleHideMutedChannels(),
  });
  // Profil de serveur (pseudo + avatar propres à ce serveur) : le formulaire
  // vit dans Paramètres du serveur → Membres (ligne de soi) — pas de modale
  // dédiée, on ouvre l'onglet directement.
  items.push({
    label: t.serveur.editServerProfile,
    icon: <EditMenuIcon />,
    separatorBefore: true,
    onClick: () => openModal({ kind: 'serverSettings', groupId, initialTab: 'members' }),
  });
  items.push({
    label: t.contextMenu.copyServerId,
    icon: <CopyMenuIcon />,
    separatorBefore: true,
    onClick: () =>
      copyToClipboard(
        groupId,
        () => toast('success', t.app.copied),
        () => toast('error', t.errors.actionFailed),
      ),
  });
  if (!founderBlocked) {
    items.push({
      label: t.serveur.leave,
      icon: <LeaveMenuIcon />,
      danger: true,
      separatorBefore: true,
      onClick: () => {
        if (!window.confirm(interpolate(t.serveur.leaveConfirm, { name }))) return;
        useGroups
          .getState()
          .leave(groupId)
          .then(() => {
            toast('info', t.serveur.left);
            setView({ kind: 'friends' });
          })
          .catch(() => toast('error', t.errors.actionFailed));
      },
    });
  }

  const activate = (item: ServerMenuItem): void => {
    onClose();
    item.onClick?.();
  };

  /**
   * Ouvre le sous-menu générique d'un item (notifications) ancré sur la ligne
   * cliquée, après fermeture du menu déroulant — le sous-menu vit dans le
   * `ContextMenu` global (monté dans `AppShell`).
   */
  const openSubmenu = (item: ServerMenuItem, trigger: HTMLElement): void => {
    if (item.submenu === undefined) return;
    const r = trigger.getBoundingClientRect();
    onClose();
    useContextMenu.getState().openMenu(r.right, r.top, item.submenu());
  };

  const moveActive = (next: number): void => {
    if (items.length === 0) return;
    const bounded = ((next % items.length) + items.length) % items.length;
    setActiveIndex(bounded);
    itemRefs.current[bounded]?.focus();
  };

  // Base des raccourcis clavier : l'élément réellement FOCALISÉ, jamais
  // `activeIndex` seul — le survol le déplace sans bouger le focus, et les
  // flèches (ou ArrowRight vers un sous-menu) agiraient alors depuis la ligne
  // survolée au lieu de celle que l'anneau de focus désigne.
  const focusedIndex = (): number => {
    const i = itemRefs.current.findIndex(
      (el) => el !== null && el === document.activeElement,
    );
    return i >= 0 ? i : activeIndex;
  };

  const onKeyDown = (e: React.KeyboardEvent<HTMLDivElement>): void => {
    if (e.key === 'Escape') {
      e.preventDefault();
      onClose();
    } else if (e.key === 'Tab') {
      onClose();
    } else if (e.key === 'ArrowDown') {
      e.preventDefault();
      moveActive(focusedIndex() + 1);
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      moveActive(focusedIndex() - 1);
    } else if (e.key === 'Home') {
      e.preventDefault();
      moveActive(0);
    } else if (e.key === 'End') {
      e.preventDefault();
      moveActive(items.length - 1);
    } else if (e.key === 'ArrowRight') {
      const index = focusedIndex();
      const item = items[index];
      const trigger = itemRefs.current[index];
      if (item?.submenu !== undefined && trigger !== null && trigger !== undefined) {
        e.preventDefault();
        openSubmenu(item, trigger);
      }
    }
  };

  return (
    <div
      ref={ref}
      role="menu"
      aria-label={t.serveur.serverMenu}
      tabIndex={-1}
      onKeyDown={onKeyDown}
      className="server-menu-surface context-menu-enter absolute left-3 top-[calc(100%+6px)] z-50 w-[min(16.5rem,calc(100vw-6rem))] origin-top overflow-hidden rounded-lg focus:outline-none"
    >
      <div className="server-menu-scroll max-h-[calc(100dvh-9rem)] overflow-y-auto overscroll-contain p-1.5">
        {items.map((item, i) => (
          <div key={`${i}-${item.label}`}>
            {item.separatorBefore === true && (
              <div className="mx-1.5 my-1 h-px bg-input/70" role="separator" />
            )}
            <button
              ref={(el) => {
                itemRefs.current[i] = el;
              }}
              type="button"
              role={item.checked === undefined ? 'menuitem' : 'menuitemcheckbox'}
              aria-checked={item.checked}
              aria-haspopup={item.submenu !== undefined ? 'menu' : undefined}
              tabIndex={i === activeIndex ? 0 : -1}
              onMouseEnter={() => setActiveIndex(i)}
              onFocus={() => setActiveIndex(i)}
              onClick={(e) =>
                item.submenu !== undefined
                  ? openSubmenu(item, e.currentTarget)
                  : activate(item)
              }
              className={`server-menu-item group flex h-9 w-full items-center gap-3 rounded-md px-2.5 text-left text-sm font-medium transition-colors duration-fast focus-visible:outline-none ${
                item.danger === true ? 'server-menu-danger' : 'text-norm'
              }`}
            >
              <span className="min-w-0 flex-1 truncate">{item.label}</span>
              {item.checked === undefined ? (
                <span
                  aria-hidden
                  className={`flex shrink-0 items-center gap-1 ${
                    item.danger === true
                      ? ''
                      : 'text-muted transition-colors duration-fast group-hover:text-white group-focus-visible:text-white'
                  }`}
                >
                  <span className="flex h-5 w-5 items-center justify-center [&>svg]:h-[18px] [&>svg]:w-[18px]">
                    {item.icon}
                  </span>
                  {item.submenu !== undefined && (
                    <span className="flex h-4 w-3.5 items-center justify-center text-faint transition-colors duration-fast group-hover:text-white group-focus-visible:text-white">
                      <MenuChevronIcon />
                    </span>
                  )}
                </span>
              ) : (
                <span
                  aria-hidden
                  className={`flex h-[18px] w-[18px] shrink-0 items-center justify-center rounded-xs border transition-colors duration-fast [&>svg]:h-3.5 [&>svg]:w-3.5 ${
                    item.checked
                      ? 'border-blurple bg-blurple text-white group-hover:border-white group-hover:bg-white group-hover:text-blurple group-focus-visible:border-white group-focus-visible:bg-white group-focus-visible:text-blurple'
                      : 'border-faint/70 bg-transparent group-hover:border-white group-focus-visible:border-white'
                  }`}
                >
                  {item.checked && <CheckMenuIcon />}
                </span>
              )}
            </button>
          </div>
        ))}
      </div>
    </div>
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
            className={`flex h-9 min-w-0 flex-1 items-center gap-1 rounded-md pr-1 text-left transition-colors duration-fast focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar ${
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
              name={state?.name ?? ''}
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
          <span className="min-w-0 flex-1 truncate text-left">
            {t.groups.eventsEntry}
          </span>
          {upcomingCount > 0 && (
            <span
              aria-label={interpolate(t.groups.eventsBadge, {
                count: String(upcomingCount),
              })}
              className="badge-pop ml-auto min-w-4 shrink-0 rounded-full bg-red px-1.5 text-center text-[11px] font-semibold leading-4 text-on-red"
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
      {view.kind === 'group' ? (
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
