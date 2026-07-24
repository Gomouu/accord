import { useMemo } from 'react';
import { interpolate, type Dict } from '../i18n';
import { copyToClipboard } from '../lib/clipboard';
import { markAllRead } from '../lib/markRead';
import { marquerServeurLu } from '../lib/markServerRead';
import { openSettingsTab } from '../lib/settingsNavigation';
import {
  channelItemId,
  dmItemId,
  isMacPlatform,
  nextUnreadItem,
  serverItemId,
  type CommandSwitchItem,
  type QuickSwitchCommandIcon,
  type QuickSwitchItem,
} from '../lib/quickSwitch';
import { useFriends } from '../stores/friends';
import { hasPerm, PERMISSIONS, useGroups } from '../stores/groups';
import { useSession } from '../stores/session';
import { THEME_IDS, type View, useT, useUi } from '../stores/ui';
import { useVoice } from '../stores/voice';
import { THEME_LABEL_KEYS } from './settings/AppearanceTab';

const SETTINGS_TABS = [
  { id: 'appearance', label: 'appearance', icon: 'appearance' },
  { id: 'voice', label: 'voice', icon: 'voice' },
  { id: 'privacy', label: 'privacy', icon: 'privacy' },
  { id: 'notifications', label: 'notifications', icon: 'notifications' },
] as const satisfies readonly {
  id: 'appearance' | 'voice' | 'privacy' | 'notifications';
  label: keyof Dict['settings'];
  icon: QuickSwitchCommandIcon;
}[];

function currentNavigationId(view: View): string {
  if (view.kind === 'friends') return 'friends';
  if (view.kind === 'dm') return dmItemId(view.peer);
  return view.channelId === null
    ? serverItemId(view.groupId)
    : channelItemId(view.groupId, view.channelId);
}

function canOpenServerSettings(permissions: number): boolean {
  return [
    PERMISSIONS.MANAGE_CHANNELS,
    PERMISSIONS.MANAGE_ROLES,
    PERMISSIONS.MANAGE_EMOJIS,
    PERMISSIONS.KICK,
    PERMISSIONS.BAN,
  ].some((permission) => hasPerm(permissions, permission));
}

export function useQuickSwitchCommands(
  navigationItems: readonly QuickSwitchItem[],
): CommandSwitchItem[] {
  const t = useT();
  const view = useUi((state) => state.view);
  const openModal = useUi((state) => state.openModal);
  const setView = useUi((state) => state.setView);
  const setTheme = useUi((state) => state.setTheme);
  const toast = useUi((state) => state.toast);
  const hideMutedChannels = useUi((state) => state.hideMutedChannels);
  const toggleHideMutedChannels = useUi((state) => state.toggleHideMutedChannels);
  const contacts = useFriends((state) => state.contacts);
  const ownStatus = useFriends((state) => state.ownStatus);
  const ownStatusText = useFriends((state) => state.ownStatusText);
  const setOwnStatus = useFriends((state) => state.setOwnStatus);
  const groupIds = useGroups((state) => state.ids);
  const groupStates = useGroups((state) => state.states);
  const groupUnread = useGroups((state) => state.unread);
  const self = useSession((state) => state.self);
  const activeVoice = useVoice((state) => state.active);
  const selfDeafened = useVoice((state) => state.selfDeafened);
  const toggleMute = useVoice((state) => state.toggleMute);
  const toggleDeafen = useVoice((state) => state.toggleDeafen);

  return useMemo(() => {
    const commands: CommandSwitchItem[] = [];
    const onError = (): void => toast('error', t.errors.actionFailed);
    const run = (promise: Promise<unknown>): void => void promise.catch(onError);
    const dmPeers = contacts
      .filter((contact) => contact.state === 'friend' && (contact.unread ?? 0) > 0)
      .map((contact) => contact.pubkey);
    const dmUnread = new Set(dmPeers);
    const unreadGroupIds = groupIds.filter((groupId) =>
      Object.values(groupUnread[groupId] ?? {}).some((count) => count > 0),
    );
    const unreadItem = nextUnreadItem(
      navigationItems,
      currentNavigationId(view),
      (item) =>
        (item.kind === 'dm' && dmUnread.has(item.pubkey)) ||
        (item.kind === 'channel' &&
          item.channelKind !== 'voice' &&
          (groupUnread[item.groupId]?.[item.channelId] ?? 0) > 0),
    );
    const unreadView =
      unreadItem?.kind === 'dm' || unreadItem?.kind === 'channel'
        ? unreadItem.view
        : null;

    for (const contact of contacts) {
      if (contact.state !== 'friend') continue;
      const name =
        contact.display_name.trim() === '' ? contact.friend_code : contact.display_name;
      commands.push({
        id: `command:start-dm:${contact.pubkey}`,
        kind: 'command',
        label: interpolate(t.quickSwitch.commandStartDm, { name }),
        subtitle: t.quickSwitch.commandNavigation,
        keywords: ['message', 'dm', 'friend', name, contact.friend_code],
        icon: 'dm',
        run: () => setView({ kind: 'dm', peer: contact.pubkey }),
      });
    }

    if (unreadView !== null) {
      commands.push({
        id: 'command:next-unread',
        kind: 'command',
        label: t.quickSwitch.commandNextUnread,
        subtitle: t.quickSwitch.commandNavigation,
        keywords: ['next', 'unread', 'message', 'mention'],
        icon: 'unread',
        featured: true,
        run: () => setView(unreadView),
      });
    }

    if (unreadGroupIds.length > 0 || dmPeers.length > 0) {
      commands.push({
        id: 'command:mark-all-read',
        kind: 'command',
        label: t.quickSwitch.commandMarkAllRead,
        subtitle: t.quickSwitch.commandNavigation,
        keywords: ['mark', 'all', 'read', 'unread', 'clear'],
        icon: 'mark-read',
        run: () => run(markAllRead(unreadGroupIds, dmPeers)),
      });
    }

    commands.push(
      {
        id: 'command:settings',
        kind: 'command',
        label: t.quickSwitch.commandOpenSettings,
        subtitle: t.quickSwitch.commandPersonal,
        keywords: ['settings', 'preferences', 'account'],
        icon: 'settings',
        featured: true,
        run: () => openModal({ kind: 'settings' }),
      },
      {
        id: 'command:create-server',
        kind: 'command',
        label: t.quickSwitch.commandCreateServer,
        subtitle: t.quickSwitch.commandCreation,
        keywords: ['create', 'new', 'server', 'group'],
        icon: 'server',
        featured: true,
        run: () => openModal({ kind: 'createGroup' }),
      },
      {
        id: 'command:add-friend',
        kind: 'command',
        label: t.quickSwitch.commandAddFriend,
        subtitle: t.quickSwitch.commandPersonal,
        keywords: ['add', 'friend', 'contact'],
        icon: 'invite',
        run: () => setView({ kind: 'friends' }),
      },
    );

    const statusCommands = [
      ['online', t.quickSwitch.commandStatusOnline],
      ['idle', t.quickSwitch.commandStatusIdle],
      ['dnd', t.quickSwitch.commandStatusDnd],
      ['invisible', t.quickSwitch.commandStatusInvisible],
    ] as const;
    for (const [status, label] of statusCommands) {
      commands.push({
        id: `command:status:${status}`,
        kind: 'command',
        label,
        subtitle: t.quickSwitch.commandStatus,
        keywords: ['presence', 'status', status],
        icon: 'status',
        run: () => run(setOwnStatus(status)),
      });
    }
    commands.push({
      id: 'command:custom-status',
      kind: 'command',
      label: t.quickSwitch.commandSetCustomStatus,
      subtitle: t.quickSwitch.commandStatus,
      keywords: ['presence', 'status', 'custom', 'message'],
      icon: 'status',
      customStatusMode: 'edit',
      run: () => undefined,
    });
    if (ownStatusText !== null && ownStatusText !== '') {
      commands.push({
        id: 'command:clear-custom-status',
        kind: 'command',
        label: t.quickSwitch.commandClearCustomStatus,
        subtitle: t.quickSwitch.commandStatus,
        keywords: ['presence', 'status', 'custom', 'clear', 'remove'],
        icon: 'status',
        run: () => run(setOwnStatus(ownStatus, '')),
      });
    }

    for (const theme of THEME_IDS) {
      const name = t.settings[THEME_LABEL_KEYS[theme]];
      commands.push({
        id: `command:theme:${theme}`,
        kind: 'command',
        label: interpolate(t.quickSwitch.commandSetTheme, { name }),
        subtitle: t.quickSwitch.commandTheme,
        keywords: ['appearance', 'theme', theme, name],
        icon: 'theme',
        run: () => setTheme(theme),
      });
    }

    for (const tab of SETTINGS_TABS) {
      const name = t.settings[tab.label];
      commands.push({
        id: `command:settings:${tab.id}`,
        kind: 'command',
        label: interpolate(t.quickSwitch.commandOpenSettingsTab, { name }),
        subtitle: t.quickSwitch.commandPersonal,
        keywords: ['settings', 'preferences', tab.id, name],
        icon: tab.icon,
        run: () => openSettingsTab(tab.id),
      });
    }

    if (activeVoice !== null) {
      commands.push(
        {
          id: 'command:voice:mute',
          kind: 'command',
          label: activeVoice.muted
            ? t.quickSwitch.commandUnmute
            : t.quickSwitch.commandMute,
          subtitle: t.quickSwitch.commandVoice,
          keywords: ['voice', 'microphone', 'mic', 'mute', 'unmute'],
          icon: 'microphone',
          shortcut: isMacPlatform() ? '⌘⇧M' : 'Ctrl+Shift+M',
          run: () => run(toggleMute()),
        },
        {
          id: 'command:voice:deafen',
          kind: 'command',
          label: selfDeafened
            ? t.quickSwitch.commandUndeafen
            : t.quickSwitch.commandDeafen,
          subtitle: t.quickSwitch.commandVoice,
          keywords: ['voice', 'audio', 'headphones', 'deafen', 'undeafen'],
          icon: 'headphones',
          run: () => run(toggleDeafen()),
        },
      );
    }

    if (view.kind === 'group') {
      const groupId = view.groupId;
      const state = groupStates[groupId];
      if (state !== undefined) {
        const permissions = state.my_permissions;
        if (unreadGroupIds.includes(groupId)) {
          commands.push({
            id: 'command:server:mark-read',
            kind: 'command',
            label: t.quickSwitch.commandMarkServerRead,
            subtitle: t.quickSwitch.commandServer,
            keywords: ['server', 'mark', 'read', 'unread', state.name],
            icon: 'mark-read',
            run: () => run(marquerServeurLu(groupId)),
          });
        }
        if (hasPerm(permissions, PERMISSIONS.MANAGE_CHANNELS)) {
          commands.push(
            {
              id: 'command:server:create-channel',
              kind: 'command',
              label: t.quickSwitch.commandCreateChannel,
              subtitle: t.quickSwitch.commandCreation,
              keywords: ['server', 'create', 'new', 'channel', state.name],
              icon: 'channel',
              run: () => openModal({ kind: 'createChannel', groupId }),
            },
            {
              id: 'command:server:create-category',
              kind: 'command',
              label: t.quickSwitch.commandCreateCategory,
              subtitle: t.quickSwitch.commandCreation,
              keywords: ['server', 'create', 'new', 'category', state.name],
              icon: 'category',
              run: () => openModal({ kind: 'createCategory', groupId }),
            },
            {
              id: 'command:server:create-event',
              kind: 'command',
              label: t.quickSwitch.commandCreateEvent,
              subtitle: t.quickSwitch.commandCreation,
              keywords: ['server', 'create', 'new', 'event', 'calendar', state.name],
              icon: 'calendar',
              run: () => openModal({ kind: 'events', groupId }),
            },
          );
        }
        if (canOpenServerSettings(permissions)) {
          commands.push({
            id: 'command:server:settings',
            kind: 'command',
            label: t.quickSwitch.commandServerSettings,
            subtitle: t.quickSwitch.commandServer,
            keywords: ['server', 'settings', 'manage', state.name],
            icon: 'settings',
            run: () => openModal({ kind: 'serverSettings', groupId }),
          });
        }
        if (hasPerm(permissions, PERMISSIONS.INVITE)) {
          commands.push({
            id: 'command:server:invite',
            kind: 'command',
            label: t.quickSwitch.commandInvite,
            subtitle: t.quickSwitch.commandServer,
            keywords: ['server', 'invite', 'people', 'friend', state.name],
            icon: 'invite',
            run: () => openModal({ kind: 'invite', groupId }),
          });
        }
        commands.push(
          {
            id: 'command:server:copy-id',
            kind: 'command',
            label: t.quickSwitch.commandCopyServerId,
            subtitle: t.quickSwitch.commandServer,
            keywords: ['server', 'copy', 'id', 'identifier', state.name],
            icon: 'copy',
            run: () =>
              copyToClipboard(groupId, () => toast('success', t.app.copied), onError),
          },
          {
            id: 'command:server:muted-channels',
            kind: 'command',
            label: hideMutedChannels
              ? t.quickSwitch.commandShowMutedChannels
              : t.quickSwitch.commandHideMutedChannels,
            subtitle: t.quickSwitch.commandPersonal,
            keywords: ['server', 'channel', 'muted', 'hide', 'show'],
            icon: 'muted-channels',
            run: toggleHideMutedChannels,
          },
        );
        if (self !== null && state.founder !== self.pubkey) {
          commands.push({
            id: 'command:server:leave',
            kind: 'command',
            label: t.quickSwitch.commandLeaveServer,
            subtitle: t.quickSwitch.commandServer,
            keywords: ['server', 'leave', 'quit', 'exit', state.name],
            icon: 'leave',
            danger: true,
            run: () => openModal({ kind: 'leaveServer', groupId }),
          });
        }
      }
    }

    return commands;
  }, [
    activeVoice,
    contacts,
    groupIds,
    groupStates,
    groupUnread,
    hideMutedChannels,
    navigationItems,
    openModal,
    ownStatus,
    ownStatusText,
    self,
    selfDeafened,
    setOwnStatus,
    setTheme,
    setView,
    t,
    toast,
    toggleDeafen,
    toggleHideMutedChannels,
    toggleMute,
    view,
  ]);
}
