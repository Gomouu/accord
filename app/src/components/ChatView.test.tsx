/**
 * Tests de la vue conversation privée : marquage lu (dm.mark_read) au
 * lamport du dernier message affiché à l'ouverture puis à chaque arrivée,
 * aucun marquage sur fil vide, indicateur de frappe du pair, et résolution
 * des émojis custom agrégés de tous les serveurs rejoints (aucun contexte de
 * serveur en MP).
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Mock } from 'vitest';
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';

vi.mock('../lib/client', () => ({
  rpc: { call: vi.fn(), onEvent: vi.fn(() => () => {}), onStatus: vi.fn() },
  api: {
    dmMarkRead: vi.fn(),
    friendsList: vi.fn(),
    filesShareBytes: vi.fn(),
    dmPins: vi.fn(),
    dmPin: vi.fn(),
    dmUnpin: vi.fn(),
    dmHistoryAround: vi.fn(),
    dmRetry: vi.fn(),
    groupsPins: vi.fn(() => Promise.resolve({ msg_ids: [] })),
    groupsMarkRead: vi.fn(() => Promise.resolve({ ok: true })),
    groupsList: vi.fn(() => Promise.resolve({ unread: {}, mentions: {} })),
    groupsPurge: vi.fn(() => Promise.resolve({ deleted: 0 })),
  },
}));

vi.mock('../lib/files', () => ({
  lireFichier: vi.fn(() => Promise.resolve('blob:emoji')),
}));

import { api, rpc } from '../lib/client';
import type { Contact, DmMessage, GroupStateJson } from '../lib/api';
import { useContextMenu } from '../stores/contextMenu';
import { useDms } from '../stores/dms';
import { useFriends } from '../stores/friends';
import { useGroups } from '../stores/groups';
import { useSession } from '../stores/session';
import { useTyping, dmTypingKey, TYPING_EXPIRY_MS } from '../stores/typing';
import { useUi } from '../stores/ui';
import { DmView, GroupView } from './ChatView';

const callMock = rpc.call as unknown as Mock;
const markReadMock = api.dmMarkRead as unknown as Mock;
const friendsListMock = api.friendsList as unknown as Mock;
const pinsMock = api.dmPins as unknown as Mock;
const unpinMock = api.dmUnpin as unknown as Mock;
const historyAroundMock = api.dmHistoryAround as unknown as Mock;
const purgeMock = api.groupsPurge as unknown as Mock;

const PEER = 'pair-pk';

function contact(pubkey: string, displayName: string): Contact {
  return {
    node_id: 'noeud',
    pubkey,
    friend_code: 'accord-lion-foret-12345',
    display_name: displayName,
    bio: null,
    avatar: null,
    banner: null,
    state: 'friend',
    last_seen_ms: 0,
  };
}

function dmMsg(id: string, lamport: number, text = `message ${id}`): DmMessage {
  return {
    msg_id: id,
    author: PEER,
    lamport,
    sent_ms: lamport * 1000,
    acked: true,
    deleted: false,
    body: { type: 'text', text, reply_to: null, attachments: 0 },
    edited: null,
  };
}

function makeGroupState(over: Partial<GroupStateJson> = {}): GroupStateJson {
  return {
    group_id: 'g1',
    name: 'Guilde',
    icon: null,
    founder: 'f',
    members: [],
    bans: [],
    channels: [],
    categories: [],
    roles: [],
    invites: [],
    emojis: [],
    my_permissions: 0x1ff,
    ...over,
  };
}

beforeEach(() => {
  useUi.setState({
    lang: 'fr',
    view: { kind: 'dm', peer: PEER },
    toasts: [],
    jump: null,
  });
  useDms.setState({ conversations: {}, hasMore: {}, loadingOlder: {}, pins: {} });
  useFriends.setState({ contacts: [contact(PEER, 'Alice')], loaded: false });
  useGroups.setState({ ids: [], states: {} });
  useTyping.setState({ writers: {} });
  callMock.mockReset();
  markReadMock.mockReset();
  friendsListMock.mockReset();
  pinsMock.mockReset();
  unpinMock.mockReset();
  historyAroundMock.mockReset();
  purgeMock.mockReset().mockResolvedValue({ deleted: 0 });
  useContextMenu.setState({ menu: null });
  markReadMock.mockResolvedValue({ ok: true });
  friendsListMock.mockResolvedValue({ contacts: [contact(PEER, 'Alice')] });
  pinsMock.mockResolvedValue({ msg_ids: [] });
});

describe('DmView — marquage lu', () => {
  it('marque la conversation lue au lamport du dernier message affiché', async () => {
    // Arrange : la page récente contient deux messages (lamports 5 et 7).
    callMock.mockResolvedValue({ messages: [dmMsg('b', 7), dmMsg('a', 5)] });

    // Act
    render(<DmView peer={PEER} />);

    // Assert : mark_read au dernier lamport, puis liste d'amis rafraîchie
    // (le compteur de non-lus retombe).
    await waitFor(() => expect(markReadMock).toHaveBeenCalledWith(PEER, 7));
    await waitFor(() => expect(useFriends.getState().loaded).toBe(true));
    expect(friendsListMock).toHaveBeenCalled();
  });

  it('ne marque rien tant que le fil est vide', async () => {
    // Arrange
    callMock.mockResolvedValue({ messages: [] });

    // Act
    render(<DmView peer={PEER} />);

    // Assert
    await waitFor(() => expect(callMock).toHaveBeenCalled());
    expect(markReadMock).not.toHaveBeenCalled();
  });

  it('marque à nouveau quand un message arrive dans la conversation ouverte', async () => {
    // Arrange
    callMock.mockResolvedValue({ messages: [dmMsg('a', 5)] });
    render(<DmView peer={PEER} />);
    await waitFor(() => expect(markReadMock).toHaveBeenCalledWith(PEER, 5));

    // Act : un événement rafraîchit le fil avec un message plus récent.
    callMock.mockResolvedValue({ messages: [dmMsg('b', 9), dmMsg('a', 5)] });
    await act(async () => {
      await useDms.getState().refresh(PEER);
    });

    // Assert
    await waitFor(() => expect(markReadMock).toHaveBeenCalledWith(PEER, 9));
  });
});

describe('DmView — épingles', () => {
  it('ouvre le volet et y liste le message épinglé résolu', async () => {
    callMock.mockResolvedValue({ messages: [dmMsg('p1', 3)] });
    pinsMock.mockResolvedValue({ msg_ids: ['p1'] });

    render(<DmView peer={PEER} />);
    await waitFor(() => expect(useDms.getState().pins[PEER]).toEqual(['p1']));

    const toggle = screen.getByRole('button', { name: 'Messages épinglés' });
    await act(async () => {
      toggle.click();
    });

    const dialog = screen.getByRole('dialog', { name: 'Messages épinglés' });
    expect(within(dialog).getByText('message p1')).toBeInTheDocument();
  });

  it('désépingle depuis le volet (dm.unpin puis rechargement)', async () => {
    callMock.mockResolvedValue({ messages: [dmMsg('p1', 3)] });
    pinsMock.mockResolvedValueOnce({ msg_ids: ['p1'] });
    unpinMock.mockResolvedValue({ ok: true });
    pinsMock.mockResolvedValueOnce({ msg_ids: [] });

    render(<DmView peer={PEER} />);
    await waitFor(() => expect(useDms.getState().pins[PEER]).toEqual(['p1']));

    await act(async () => {
      screen.getByRole('button', { name: 'Messages épinglés' }).click();
    });
    const dialog = screen.getByRole('dialog', { name: 'Messages épinglés' });
    await act(async () => {
      within(dialog).getByRole('button', { name: 'Désépingler' }).click();
    });

    expect(unpinMock).toHaveBeenCalledWith(PEER, 'p1');
    await waitFor(() => expect(useDms.getState().pins[PEER]).toEqual([]));
  });
});

describe('DmView — saut au message', () => {
  it('signale un message indisponible quand la fenêtre est introuvable', async () => {
    callMock.mockResolvedValue({ messages: [dmMsg('a', 5)] });
    historyAroundMock.mockResolvedValue({
      messages: [],
      found: false,
      peer_read_lamport: null,
    });

    render(<DmView peer={PEER} />);
    await waitFor(() => expect(callMock).toHaveBeenCalled());

    await act(async () => {
      useUi.getState().requestJump({ kind: 'dm', peer: PEER }, 'ghost');
    });

    await waitFor(() =>
      expect(useUi.getState().toasts.some((t) => t.text === 'Message indisponible')).toBe(
        true,
      ),
    );
    expect(historyAroundMock).toHaveBeenCalledWith(PEER, 'ghost');
  });
});

describe('DmView — indicateur de frappe', () => {
  it("affiche l'indicateur du pair sous la zone de saisie", async () => {
    // Arrange
    callMock.mockResolvedValue({ messages: [] });
    useTyping.setState({
      writers: { [dmTypingKey(PEER)]: { [PEER]: Date.now() + TYPING_EXPIRY_MS } },
    });

    // Act
    render(<DmView peer={PEER} />);

    // Assert
    expect(screen.getByText('Alice est en train d’écrire…')).toBeInTheDocument();
    await waitFor(() => expect(callMock).toHaveBeenCalled());
  });
});

describe('DmView — émojis custom agrégés', () => {
  it('rend en image un émoji custom d’un serveur rejoint', async () => {
    // Arrange : le membre local a rejoint g1, qui connaît :parrot:.
    useGroups.setState({
      ids: ['g1'],
      states: {
        g1: makeGroupState({ emojis: [{ name: 'parrot', merkle_root: 'racine' }] }),
      },
    });
    callMock.mockResolvedValue({ messages: [dmMsg('a', 5, 'salut :parrot:')] });

    // Act
    render(<DmView peer={PEER} />);

    // Assert
    expect(await screen.findByAltText(':parrot:')).toBeInTheDocument();
  });

  it('laisse le jeton en texte quand aucun serveur rejoint ne connaît l’émoji', async () => {
    // Arrange : aucun serveur rejoint ne publie :ghost:.
    callMock.mockResolvedValue({ messages: [dmMsg('a', 5, ':ghost:')] });

    // Act
    render(<DmView peer={PEER} />);

    // Assert : jeton littéral, jamais d'image cassée.
    expect(await screen.findByText(':ghost:')).toBeInTheDocument();
    expect(screen.queryByRole('img')).not.toBeInTheDocument();
  });
});

describe('GroupView — statut personnalisé dans la liste des membres', () => {
  beforeEach(() => {
    callMock.mockResolvedValue({ messages: [] });
    useUi.setState({
      view: { kind: 'group', groupId: 'g1', channelId: 'c1' },
      toasts: [],
      jump: null,
    });
  });

  it('affiche le texte de statut sous le nom d’un membre ami', async () => {
    useGroups.setState({
      ids: ['g1'],
      states: {
        g1: makeGroupState({
          channels: [
            {
              channel_id: 'c1',
              name: 'général',
              kind: 'text',
              category: null,
              position: 0,
              topic: '',
            },
          ],
          members: [{ pubkey: PEER, roles: [] }],
        }),
      },
    });
    useFriends.setState({
      contacts: [{ ...contact(PEER, 'Alice'), status_text: 'En pleine partie' }],
    });

    render(<GroupView groupId="g1" channelId="c1" />);

    expect(await screen.findByText('En pleine partie')).toBeInTheDocument();
  });

  it("n'affiche aucune ligne de statut quand il est absent", async () => {
    useGroups.setState({
      ids: ['g1'],
      states: {
        g1: makeGroupState({
          channels: [
            {
              channel_id: 'c1',
              name: 'général',
              kind: 'text',
              category: null,
              position: 0,
              topic: '',
            },
          ],
          members: [{ pubkey: PEER, roles: [] }],
        }),
      },
    });
    useFriends.setState({ contacts: [contact(PEER, 'Alice')] });

    render(<GroupView groupId="g1" channelId="c1" />);

    expect(await screen.findByText('Alice')).toBeInTheDocument();
    expect(screen.queryByText('En pleine partie')).not.toBeInTheDocument();
  });

  it('affiche son propre texte de statut personnalisé dans la liste des membres', async () => {
    useSession.setState({
      self: {
        node_id: 'nm',
        pubkey: 'moi',
        friend_code: 'accord-moi',
        name: 'Moi',
        bio: null,
        avatar: null,
        banner: null,
        pronouns: null,
        accent_color: null,
        banner_color: null,
        avatar_decoration: null,
        profile_effect: null,
      },
      phase: 'ready',
    });
    useGroups.setState({
      ids: ['g1'],
      states: {
        g1: makeGroupState({
          channels: [
            {
              channel_id: 'c1',
              name: 'général',
              kind: 'text',
              category: null,
              position: 0,
              topic: '',
            },
          ],
          members: [{ pubkey: 'moi', roles: [] }],
        }),
      },
    });
    useFriends.setState({ contacts: [], ownStatusText: 'De retour bientôt' });

    render(<GroupView groupId="g1" channelId="c1" />);

    expect(await screen.findByText('De retour bientôt')).toBeInTheDocument();
    useSession.setState({ self: null, phase: 'boot' });
  });
});

describe('GroupView — purge (mode sélection)', () => {
  const CHANNEL = {
    channel_id: 'c1',
    name: 'général',
    kind: 'text' as const,
    category: null,
    position: 0,
    topic: '',
  };

  /** Message de salon (le pair) avec horloge de Lamport. */
  function grpMsg(id: string, lamport: number) {
    return {
      msg_id: id,
      channel_id: 'c1',
      author: PEER,
      lamport,
      sent_ms: lamport * 1000,
      deleted: false,
      body: { type: 'text' as const, text: `message ${id}`, reply_to: null, attachments: 0 },
      edited: null,
    };
  }

  beforeEach(() => {
    useUi.setState({
      view: { kind: 'group', groupId: 'g1', channelId: 'c1' },
      toasts: [],
      jump: null,
    });
    useSession.setState({
      self: {
        node_id: 'nm',
        pubkey: 'moderateur',
        friend_code: 'accord-moi',
        name: 'Mod',
        bio: null,
        avatar: null,
        banner: null,
        pronouns: null,
        accent_color: null,
        banner_color: null,
        avatar_decoration: null,
        profile_effect: null,
      },
    });
    callMock.mockResolvedValue({ messages: [grpMsg('a', 5), grpMsg('b', 6)] });
  });

  /** Ouvre le menu contextuel d'un message et rend ses items du store. */
  function openMessageMenu(text: string) {
    const row = screen.getByText(text).closest('[data-msg-id]') as HTMLElement;
    fireEvent.contextMenu(row);
    return useContextMenu.getState().menu?.items ?? [];
  }

  it('gate la commande « Sélectionner » sur MANAGE_MESSAGES', async () => {
    useGroups.setState({
      ids: ['g1'],
      states: { g1: makeGroupState({ channels: [CHANNEL], my_permissions: 0 }) },
    });
    render(<GroupView groupId="g1" channelId="c1" />);
    await screen.findByText('message a');

    const items = openMessageMenu('message a');
    expect(items.some((i) => i.label === 'Sélectionner des messages')).toBe(false);
  });

  it('supprime les messages cochés via purge après confirmation', async () => {
    useGroups.setState({
      ids: ['g1'],
      states: { g1: makeGroupState({ channels: [CHANNEL], my_permissions: 0x1ff }) },
    });
    render(<GroupView groupId="g1" channelId="c1" />);
    await screen.findByText('message a');

    // Entrée en mode sélection depuis le menu contextuel (message « a » coché).
    const items = openMessageMenu('message a');
    const select = items.find((i) => i.label === 'Sélectionner des messages');
    expect(select).toBeDefined();
    act(() => select!.onClick());

    // La barre d'action affiche le compteur ; on coche aussi « b ».
    expect(screen.getByText('1 sélectionné(s)')).toBeInTheDocument();
    const boxes = screen.getAllByRole('checkbox', { name: 'Sélectionner le message' });
    fireEvent.click(boxes[1]!);
    expect(screen.getByText('2 sélectionné(s)')).toBeInTheDocument();

    // Suppression en deux temps : « Supprimer » puis « Confirmer ».
    fireEvent.click(screen.getByRole('button', { name: 'Supprimer' }));
    fireEvent.click(screen.getByRole('button', { name: 'Confirmer la suppression' }));

    await waitFor(() =>
      expect(purgeMock).toHaveBeenCalledWith('g1', 'c1', ['a', 'b']),
    );
    // Sortie du mode : la barre disparaît.
    await waitFor(() =>
      expect(screen.queryByText(/sélectionné/)).not.toBeInTheDocument(),
    );
  });
});
