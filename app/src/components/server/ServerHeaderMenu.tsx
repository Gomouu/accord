import { useEffect, useRef, useState, type ReactNode } from 'react';
import { copyToClipboard } from '../../lib/clipboard';
import { marquerServeurLu } from '../../lib/markServerRead';
import { useContextMenu, type ContextMenuItem } from '../../stores/contextMenu';
import { hasPerm, PERMISSIONS, useGroups } from '../../stores/groups';
import { serverLevel, useMute } from '../../stores/mute';
import { useSession } from '../../stores/session';
import { useT, useUi } from '../../stores/ui';
import {
  BellOffMenuIcon,
  buildNotifLevelItems,
  CheckMenuIcon,
  CopyMenuIcon,
  EditMenuIcon,
  EnvelopeMenuIcon,
  GearMenuIcon,
  LeaveMenuIcon,
  PlusMenuIcon,
} from '../ContextMenu';

interface ServerMenuItem {
  id: string;
  label: string;
  icon: ReactNode;
  onClick?: () => void;
  danger?: boolean;
  subtle?: boolean;
  separatorBefore?: boolean;
  transfersFocus?: boolean;
  checked?: boolean;
  submenu?: () => ContextMenuItem[];
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

function EventMenuIcon() {
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
    >
      <rect width="18" height="18" x="3" y="4" rx="2" />
      <path d="M16 2v4" />
      <path d="M8 2v4" />
      <path d="M3 10h18" />
    </svg>
  );
}

export function ServerHeaderMenu({
  groupId,
  onClose,
}: {
  groupId: string;
  onClose: () => void;
}) {
  const t = useT();
  const toast = useUi((state) => state.toast);
  const openModal = useUi((state) => state.openModal);
  const hideMutedChannels = useUi((state) => state.hideMutedChannels);
  const serverLevels = useMute((state) => state.serverLevels);
  const state = useGroups((groups) => groups.states[groupId]);
  const unreadByChannel = useGroups((groups) => groups.unread[groupId]);
  const mentionCount = useGroups((groups) => groups.mentions[groupId] ?? 0);
  const self = useSession((session) => session.self);
  const menuRef = useRef<HTMLDivElement>(null);
  const itemRefs = useRef<Array<HTMLButtonElement | null>>([]);
  const restoreFocusRef = useRef(true);
  const [activeIndex, setActiveIndex] = useState(0);

  useEffect(() => {
    const closeOnEscape = (event: KeyboardEvent): void => {
      if (event.key === 'Escape') onClose();
    };
    const closeOutside = (event: MouseEvent): void => {
      if (menuRef.current !== null && !menuRef.current.contains(event.target as Node)) {
        onClose();
      }
    };
    window.addEventListener('keydown', closeOnEscape);
    document.addEventListener('mousedown', closeOutside);
    return () => {
      window.removeEventListener('keydown', closeOnEscape);
      document.removeEventListener('mousedown', closeOutside);
    };
  }, [onClose]);

  useEffect(() => {
    const trigger =
      document.activeElement instanceof HTMLElement ? document.activeElement : null;
    itemRefs.current[0]?.focus();
    return () => {
      if (restoreFocusRef.current && trigger !== null && trigger.isConnected) {
        trigger.focus();
      }
    };
  }, []);

  if (state === undefined) return null;

  const permissions = state.my_permissions;
  const canManageChannels = hasPerm(permissions, PERMISSIONS.MANAGE_CHANNELS);
  const canOpenSettings = [
    PERMISSIONS.MANAGE_CHANNELS,
    PERMISSIONS.MANAGE_ROLES,
    PERMISSIONS.MANAGE_EMOJIS,
    PERMISSIONS.KICK,
    PERMISSIONS.BAN,
  ].some((permission) => hasPerm(permissions, permission));
  const founderBlocked =
    self !== null && state.founder === self.pubkey && state.members.length > 1;
  const items: ServerMenuItem[] = [];

  /**
   * « Marquer comme lu » n'apparaît que si le serveur a réellement des
   * non-lus : proposer une action sans effet use la confiance dans le menu.
   * Même règle que le menu contextuel du rail, qui partage cette boucle.
   */
  const serverHasUnread =
    mentionCount > 0 || Object.values(unreadByChannel ?? {}).some((n) => n > 0);

  const addSection = (section: ServerMenuItem[]): void => {
    section.forEach((item, index) => {
      items.push({
        ...item,
        separatorBefore: index === 0 && items.length > 0,
      });
    });
  };

  if (serverHasUnread) {
    addSection([
      {
        id: 'mark-read',
        label: t.contextMenu.markAsRead,
        icon: <CheckMenuIcon />,
        onClick: () => {
          void marquerServeurLu(groupId);
        },
      },
    ]);
  }

  if (hasPerm(permissions, PERMISSIONS.INVITE)) {
    addSection([
      {
        id: 'invite',
        label: t.groups.invitePeople,
        icon: <EnvelopeMenuIcon />,
        transfersFocus: true,
        onClick: () => openModal({ kind: 'invite', groupId }),
      },
    ]);
  }

  const managementItems: ServerMenuItem[] = [];
  if (canOpenSettings) {
    managementItems.push({
      id: 'settings',
      label: t.serveur.settingsTitle,
      icon: <GearMenuIcon />,
      transfersFocus: true,
      onClick: () => openModal({ kind: 'serverSettings', groupId }),
    });
  }
  if (canManageChannels) {
    managementItems.push(
      {
        id: 'create-channel',
        label: t.groups.addChannel,
        icon: <PlusMenuIcon />,
        transfersFocus: true,
        onClick: () => openModal({ kind: 'createChannel', groupId }),
      },
      {
        id: 'create-category',
        label: t.serveur.createCategoryAction,
        icon: <PlusMenuIcon />,
        transfersFocus: true,
        onClick: () => openModal({ kind: 'createCategory', groupId }),
      },
      {
        id: 'create-event',
        label: t.groups.eventCreate,
        icon: <EventMenuIcon />,
        transfersFocus: true,
        onClick: () => openModal({ kind: 'events', groupId }),
      },
    );
  }
  addSection(managementItems);

  addSection([
    {
      id: 'notifications',
      label: t.notifLevel.title,
      icon: <BellOffMenuIcon />,
      submenu: () =>
        buildNotifLevelItems(
          t.notifLevel,
          serverLevel({ serverLevels, channelLevels: {} }, groupId),
          (level) => useMute.getState().setServerLevel(groupId, level),
        ),
    },
    {
      id: 'hide-muted',
      label: t.serveur.hideMutedChannels,
      icon: <BellOffMenuIcon />,
      checked: hideMutedChannels,
      onClick: () => useUi.getState().toggleHideMutedChannels(),
    },
    {
      id: 'edit-profile',
      label: t.serveur.editServerProfile,
      icon: <EditMenuIcon />,
      transfersFocus: true,
      onClick: () =>
        openModal({ kind: 'serverSettings', groupId, initialTab: 'members' }),
    },
  ]);

  if (!founderBlocked) {
    addSection([
      {
        id: 'leave',
        label: t.serveur.leave,
        icon: <LeaveMenuIcon />,
        danger: true,
        transfersFocus: true,
        onClick: () => openModal({ kind: 'leaveServer', groupId }),
      },
    ]);
  }

  addSection([
    {
      id: 'copy-id',
      label: t.contextMenu.copyServerId,
      icon: <CopyMenuIcon />,
      subtle: true,
      onClick: () =>
        copyToClipboard(
          groupId,
          () => toast('success', t.app.copied),
          () => toast('error', t.errors.actionFailed),
        ),
    },
  ]);

  const activate = (item: ServerMenuItem): void => {
    if (item.transfersFocus === true) restoreFocusRef.current = false;
    onClose();
    item.onClick?.();
  };

  const openSubmenu = (item: ServerMenuItem, trigger: HTMLElement): void => {
    if (item.submenu === undefined) return;
    const rect = trigger.getBoundingClientRect();
    restoreFocusRef.current = false;
    onClose();
    useContextMenu.getState().openMenu(rect.right, rect.top, item.submenu());
  };

  const moveActive = (next: number): void => {
    if (items.length === 0) return;
    const index = ((next % items.length) + items.length) % items.length;
    setActiveIndex(index);
    itemRefs.current[index]?.focus();
  };

  const focusedIndex = (): number => {
    const index = itemRefs.current.findIndex(
      (element) => element !== null && element === document.activeElement,
    );
    return index >= 0 ? index : activeIndex;
  };

  const onKeyDown = (event: React.KeyboardEvent<HTMLDivElement>): void => {
    if (event.key === 'Escape') {
      event.preventDefault();
      onClose();
    } else if (event.key === 'Tab') {
      onClose();
    } else if (event.key === 'ArrowDown') {
      event.preventDefault();
      moveActive(focusedIndex() + 1);
    } else if (event.key === 'ArrowUp') {
      event.preventDefault();
      moveActive(focusedIndex() - 1);
    } else if (event.key === 'Home') {
      event.preventDefault();
      moveActive(0);
    } else if (event.key === 'End') {
      event.preventDefault();
      moveActive(items.length - 1);
    } else if (event.key === 'ArrowRight') {
      const index = focusedIndex();
      const item = items[index];
      const trigger = itemRefs.current[index];
      if (item?.submenu !== undefined && trigger !== null && trigger !== undefined) {
        event.preventDefault();
        openSubmenu(item, trigger);
      }
    }
  };

  return (
    <div
      ref={menuRef}
      role="menu"
      aria-label={t.serveur.serverMenu}
      tabIndex={-1}
      onKeyDown={onKeyDown}
      className="server-menu-surface context-menu-enter absolute start-3 top-[calc(100%+6px)] z-50 w-[min(16.5rem,calc(100vw-6rem))] origin-top overflow-hidden rounded-lg focus:outline-none"
    >
      <div className="server-menu-scroll max-h-[calc(100dvh-9rem)] overflow-y-auto overscroll-contain p-1.5">
        {items.map((item, index) => (
          <div key={item.id}>
            {item.separatorBefore === true && (
              <div className="mx-1.5 my-1 h-px bg-input/70" role="separator" />
            )}
            <button
              ref={(element) => {
                itemRefs.current[index] = element;
              }}
              type="button"
              role={item.checked === undefined ? 'menuitem' : 'menuitemcheckbox'}
              aria-checked={item.checked}
              aria-haspopup={item.submenu !== undefined ? 'menu' : undefined}
              tabIndex={index === activeIndex ? 0 : -1}
              onMouseEnter={() => setActiveIndex(index)}
              onFocus={() => setActiveIndex(index)}
              onClick={(event) =>
                item.submenu !== undefined
                  ? openSubmenu(item, event.currentTarget)
                  : activate(item)
              }
              className={`server-menu-item group flex h-9 w-full items-center gap-3 rounded-md px-2.5 text-start text-sm font-medium transition-colors duration-fast focus-visible:outline-none active:scale-[0.98] ${
                item.danger === true
                  ? 'server-menu-danger'
                  : item.subtle === true
                    ? 'text-muted'
                    : 'text-norm'
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
