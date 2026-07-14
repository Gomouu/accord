/**
 * Tests des raccourcis clavier globaux câblés dans `AppShell` : Ctrl/Cmd+K
 * bascule le sélecteur rapide (même champ de saisie focalisé), Alt+↑/↓ fait
 * défiler les salons visibles d'un serveur en ignorant les salons vocaux, et
 * les raccourcis autres que Ctrl/Cmd+K sont inertes tant qu'un champ éditable
 * a le focus. Les voisins lourds (rail, barre latérale, fil de discussion,
 * modales) sont neutralisés : seul le câblage clavier propre à `AppShell`
 * est sous test ici (le sélecteur rapide lui-même est testé dans
 * `QuickSwitcher.test.tsx`).
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Mock } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import type { Contact, GroupStateJson, SelfProfile } from '../lib/api';

vi.mock('../lib/client', () => ({
  rpc: { onEvent: vi.fn(() => () => {}), onStatus: vi.fn(() => () => {}) },
  api: {
    friendsList: vi.fn(() => Promise.resolve({ contacts: [] })),
    groupsList: vi.fn(() => Promise.resolve({ groups: [], unread: {}, mentions: {} })),
    groupsInvitesList: vi.fn(() => Promise.resolve({ invites: [] })),
    voiceStatus: vi.fn(() =>
      Promise.resolve({ active: null, master_volume: 100, dsp: { noise_suppression: false, agc: false } }),
    ),
    callsStatus: vi.fn(() => Promise.resolve({ state: 'idle', peer: null, call_id: null })),
    voiceMute: vi.fn(() => Promise.resolve()),
  },
}));

vi.mock('./ServerRail', () => ({
  ServerRail: () => null,
  channelToRestore: () => null,
}));
vi.mock('./Sidebar', () => ({ Sidebar: () => null, ChannelIcon: () => null }));
vi.mock('./ChatView', () => ({ DmView: () => null, GroupView: () => null }));
vi.mock('./FriendsView', () => ({ FriendsView: () => null }));
vi.mock('./Modals', () => ({ Modals: () => null }));
vi.mock('./ProfilePopover', () => ({ ProfilePopover: () => null }));
vi.mock('./ContextMenu', () => ({
  ContextMenu: () => null,
  SearchIcon: () => null,
  CloseIcon: () => null,
}));
vi.mock('./IncomingCall', () => ({ IncomingCall: () => null }));

import { api } from '../lib/client';
import { useFriends } from '../stores/friends';
import { useGroups } from '../stores/groups';
import { useSession } from '../stores/session';
import { useUi } from '../stores/ui';
import { useVoice } from '../stores/voice';
import { AppShell } from './AppShell';

const voiceStatusMock = api.voiceStatus as unknown as Mock;

const SELF: SelfProfile = {
  node_id: 'n-moi',
  pubkey: 'moi',
  friend_code: 'accord-moi-12345',
  name: 'Moi',
  bio: null,
  avatar: null,
  banner: null,
  pronouns: null,
  accent_color: null,
  banner_color: null,
  avatar_decoration: null,
  profile_effect: null,
};

function contact(pubkey: string): Contact {
  return {
    node_id: `n-${pubkey}`,
    pubkey,
    friend_code: `accord-${pubkey}`,
    display_name: pubkey,
    bio: null,
    avatar: null,
    banner: null,
    state: 'friend',
    last_seen_ms: 0,
  };
}

function groupState(over: Partial<GroupStateJson> = {}): GroupStateJson {
  return {
    group_id: 'g1',
    name: 'Guilde',
    icon: null,
    founder: null,
    members: [],
    bans: [],
    channels: [],
    categories: [],
    roles: [],
    invites: [],
    my_permissions: 0x1,
    ...over,
  };
}

beforeEach(() => {
  window.localStorage.clear();
  useUi.setState({
    quickSwitcherOpen: false,
    view: { kind: 'friends' },
    lastDmPeer: null,
    lastChannelByServer: {},
    modal: null,
  });
  useUi.getState().setPttEnabled(false);
  useSession.setState({ self: SELF, phase: 'ready' });
  useFriends.setState({ contacts: [], loaded: true });
  useGroups.setState({ ids: [], states: {} });
  useVoice.setState({ active: null, participants: new Map() });
});

afterEach(() => {
  vi.clearAllMocks();
});

describe('AppShell — raccourcis globaux', () => {
  it('Ctrl+K bascule le sélecteur rapide', () => {
    render(<AppShell />);
    expect(screen.queryByRole('dialog')).toBeNull();

    fireEvent.keyDown(window, { key: 'k', ctrlKey: true });
    expect(useUi.getState().quickSwitcherOpen).toBe(true);
    expect(screen.getByRole('dialog')).toBeInTheDocument();

    fireEvent.keyDown(window, { key: 'k', ctrlKey: true });
    expect(useUi.getState().quickSwitcherOpen).toBe(false);
  });

  it('Ctrl+K reste actif même quand un champ de saisie a le focus', () => {
    render(<AppShell />);
    render(<input aria-label="champ de test" />, { container: document.body.appendChild(document.createElement('div')) });
    const input = screen.getByLabelText('champ de test');
    input.focus();

    fireEvent.keyDown(input, { key: 'k', ctrlKey: true });

    expect(useUi.getState().quickSwitcherOpen).toBe(true);
  });

  it('Alt+ArrowDown fait défiler au salon suivant en ignorant les salons vocaux', () => {
    useGroups.setState({
      ids: ['g1'],
      states: {
        g1: groupState({
          channels: [
            { channel_id: 't1', name: 'texte-1', kind: 'text', category: null, position: 0, topic: '' },
            { channel_id: 'v1', name: 'vocal-1', kind: 'voice', category: null, position: 1, topic: '' },
            { channel_id: 't2', name: 'texte-2', kind: 'text', category: null, position: 2, topic: '' },
          ],
        }),
      },
    });
    useUi.setState({ view: { kind: 'group', groupId: 'g1', channelId: 't1' } });
    render(<AppShell />);

    fireEvent.keyDown(window, { key: 'ArrowDown', altKey: true });

    expect(useUi.getState().view).toEqual({ kind: 'group', groupId: 'g1', channelId: 't2' });
  });

  it('Alt+ArrowUp/Down fait défiler les conversations privées en vue Accueil', () => {
    useFriends.setState({ contacts: [contact('alice'), contact('bob')], loaded: true });
    useUi.setState({ view: { kind: 'dm', peer: 'alice' } });
    render(<AppShell />);

    fireEvent.keyDown(window, { key: 'ArrowDown', altKey: true });

    expect(useUi.getState().view).toEqual({ kind: 'dm', peer: 'bob' });
  });

  it('les raccourcis autres que Ctrl/Cmd+K sont inertes quand un champ éditable a le focus', () => {
    useGroups.setState({
      ids: ['g1'],
      states: {
        g1: groupState({
          channels: [
            { channel_id: 't1', name: 'texte-1', kind: 'text', category: null, position: 0, topic: '' },
            { channel_id: 't2', name: 'texte-2', kind: 'text', category: null, position: 1, topic: '' },
          ],
        }),
      },
    });
    useUi.setState({ view: { kind: 'group', groupId: 'g1', channelId: 't1' } });
    render(<AppShell />);
    const host = document.body.appendChild(document.createElement('div'));
    render(<textarea aria-label="composeur de test" />, { container: host });
    const textarea = screen.getByLabelText('composeur de test');
    textarea.focus();

    fireEvent.keyDown(textarea, { key: 'ArrowDown', altKey: true });

    expect(useUi.getState().view).toEqual({ kind: 'group', groupId: 'g1', channelId: 't1' });
  });

  it('Ctrl+Maj+M bascule le micro en salon vocal', async () => {
    // La resynchronisation vocale au montage d'AppShell (`voiceStatus`)
    // écraserait un `useVoice.setState` posé avant le rendu : on fait plutôt
    // porter le salon actif par la réponse simulée du nœud.
    voiceStatusMock.mockResolvedValueOnce({
      active: { group_id: 'g1', channel_id: 'v1', muted: false, is_call: false, participants: [] },
      master_volume: 100,
      dsp: { noise_suppression: false, agc: false },
    });
    render(<AppShell />);
    await waitFor(() => expect(useVoice.getState().active?.groupId).toBe('g1'));

    fireEvent.keyDown(window, { key: 'M', ctrlKey: true, shiftKey: true });

    await waitFor(() => expect(useVoice.getState().active?.muted).toBe(true));
  });

  it('Ctrl+Maj+M reste sans effet hors salon vocal', () => {
    render(<AppShell />);

    fireEvent.keyDown(window, { key: 'M', ctrlKey: true, shiftKey: true });

    expect(useVoice.getState().active).toBeNull();
  });
});
