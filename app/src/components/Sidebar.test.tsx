/**
 * Tests de la barre latérale : pastilles de non-lus sur les conversations
 * privées (champ `unread` de friends.list) et sur les salons d'un serveur
 * (compteurs de groups.list), absentes sans non-lu.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import type { Contact, GroupStateJson } from '../lib/api';
import { useFriends } from '../stores/friends';
import { PERMISSIONS, useGroups } from '../stores/groups';
import { useSession } from '../stores/session';
import { useUi } from '../stores/ui';
import { Sidebar } from './Sidebar';

function contact(
  pubkey: string,
  displayName: string,
  unread?: number,
  statusText?: string,
): Contact {
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
    ...(unread !== undefined ? { unread } : {}),
    ...(statusText !== undefined ? { status_text: statusText } : {}),
  };
}

const SELF = {
  node_id: 'n',
  pubkey: 'moi',
  friend_code: 'accord-moi',
  name: 'Moi',
  bio: null,
  avatar: null,
  banner: null,
  pronouns: null,
  accent_color: null,
  banner_color: null,
};

function groupState(over: Partial<GroupStateJson> = {}): GroupStateJson {
  return {
    group_id: 'g1',
    name: 'Guilde',
    icon: null,
    founder: null,
    members: [],
    bans: [],
    channels: [
      {
        channel_id: 'c1',
        name: 'général',
        kind: 'text',
        category: null,
        position: 0,
        topic: '',
      },
      {
        channel_id: 'c2',
        name: 'projets',
        kind: 'text',
        category: null,
        position: 1,
        topic: '',
      },
    ],
    categories: [],
    roles: [],
    invites: [],
    // Base réaliste (D-015) : tout membre porte VIEW+SEND par défaut côté
    // nœud (`GroupState::base_permissions`) — un `my_permissions` à 0 ne
    // reflète aucun membre réel et masquerait à tort tous les salons du
    // filtre de visibilité (`isChannelVisible`).
    my_permissions: PERMISSIONS.VIEW | PERMISSIONS.SEND,
    ...over,
  };
}

beforeEach(() => {
  useUi.setState({ lang: 'fr', view: { kind: 'friends' } });
  useSession.setState({ self: null });
  useFriends.setState({ contacts: [] });
  useGroups.setState({ ids: [], states: {}, unread: {} });
});

describe('Sidebar — non-lus des conversations privées', () => {
  it('affiche la pastille avec le compte du contact', () => {
    // Arrange
    useFriends.setState({
      contacts: [contact('alice-pk', 'Alice', 3), contact('bob-pk', 'Bob')],
    });

    // Act
    render(<Sidebar />);

    // Assert
    const badge = screen.getByLabelText('3 message(s) non lu(s)');
    expect(badge).toHaveTextContent('3');
  });

  it("n'affiche aucune pastille sans non-lu", () => {
    // Arrange
    useFriends.setState({
      contacts: [contact('alice-pk', 'Alice', 0), contact('bob-pk', 'Bob')],
    });

    // Act
    render(<Sidebar />);

    // Assert
    expect(screen.queryByLabelText(/non lu/)).not.toBeInTheDocument();
  });
});

describe('Sidebar — non-lus des salons', () => {
  it('affiche la pastille sur le seul salon ayant des non-lus', () => {
    // Arrange
    useUi.setState({ view: { kind: 'group', groupId: 'g1', channelId: 'c1' } });
    useGroups.setState({
      ids: ['g1'],
      states: { g1: groupState() },
      unread: { g1: { c2: 5 } },
    });

    // Act
    render(<Sidebar />);

    // Assert
    const badge = screen.getByLabelText('5 message(s) non lu(s)');
    expect(badge).toHaveTextContent('5');
    expect(screen.getAllByLabelText(/non lu/)).toHaveLength(1);
  });
});

describe('Sidebar — salons restreints et masqués', () => {
  beforeEach(() => {
    useUi.setState({ view: { kind: 'group', groupId: 'g1', channelId: 'c1' } });
  });

  it('affiche un cadenas sur un salon portant un override refusant VIEW ou SEND', () => {
    useSession.setState({ self: SELF });
    useGroups.setState({
      ids: ['g1'],
      states: {
        g1: groupState({
          my_permissions: PERMISSIONS.VIEW | PERMISSIONS.SEND,
          members: [{ pubkey: 'moi', roles: [] }],
          overrides: [
            { channel_id: 'c2', role_id: 'r', allow: 0, deny: PERMISSIONS.SEND },
          ],
        }),
      },
    });

    render(<Sidebar />);

    expect(
      screen.getByLabelText('Salon restreint : accès limité selon les rôles'),
    ).toBeInTheDocument();
  });

  it("n'affiche aucun cadenas sans override refusant VIEW ou SEND", () => {
    useSession.setState({ self: SELF });
    useGroups.setState({
      ids: ['g1'],
      states: {
        g1: groupState({
          my_permissions: PERMISSIONS.VIEW | PERMISSIONS.SEND,
          members: [{ pubkey: 'moi', roles: [] }],
        }),
      },
    });

    render(<Sidebar />);

    expect(
      screen.queryByLabelText('Salon restreint : accès limité selon les rôles'),
    ).not.toBeInTheDocument();
  });

  it("masque un salon où VIEW est effectivement refusé à l'utilisateur local", () => {
    useSession.setState({ self: SELF });
    useGroups.setState({
      ids: ['g1'],
      states: {
        g1: groupState({
          my_permissions: PERMISSIONS.VIEW | PERMISSIONS.SEND,
          members: [{ pubkey: 'moi', roles: ['r'] }],
          overrides: [
            { channel_id: 'c2', role_id: 'r', allow: 0, deny: PERMISSIONS.VIEW },
          ],
        }),
      },
    });

    render(<Sidebar />);

    expect(screen.getByText('général')).toBeInTheDocument();
    expect(screen.queryByText('projets')).not.toBeInTheDocument();
  });

  it('garde le salon visible si ADMIN court-circuite l’override VIEW refusé', () => {
    useSession.setState({ self: SELF });
    useGroups.setState({
      ids: ['g1'],
      states: {
        g1: groupState({
          my_permissions: PERMISSIONS.ADMIN,
          members: [{ pubkey: 'moi', roles: ['r'] }],
          overrides: [
            { channel_id: 'c2', role_id: 'r', allow: 0, deny: PERMISSIONS.VIEW },
          ],
        }),
      },
    });

    render(<Sidebar />);

    expect(screen.getByText('général')).toBeInTheDocument();
    expect(screen.getByText('projets')).toBeInTheDocument();
  });
});

describe('Sidebar — statut personnalisé des conversations privées', () => {
  it('affiche le texte de statut sous le nom quand il est défini', () => {
    useFriends.setState({
      contacts: [contact('alice-pk', 'Alice', undefined, 'En pleine partie')],
    });

    render(<Sidebar />);

    expect(screen.getByText('En pleine partie')).toBeInTheDocument();
  });

  it("n'affiche rien de plus sans statut personnalisé", () => {
    useFriends.setState({ contacts: [contact('bob-pk', 'Bob')] });

    const { container } = render(<Sidebar />);

    expect(screen.getByText('Bob')).toBeInTheDocument();
    // Aucune deuxième ligne de statut personnalisé sous le nom.
    expect(container.querySelectorAll('.text-xs.text-muted')).toHaveLength(0);
  });
});

describe('Sidebar — menu du nom de serveur', () => {
  beforeEach(() => {
    useUi.setState({
      view: { kind: 'group', groupId: 'g1', channelId: 'c1' },
      modal: null,
    });
  });

  it("ouvre le menu et n'affiche que les items permis sans permission", () => {
    useGroups.setState({ ids: ['g1'], states: { g1: groupState() } });

    render(<Sidebar />);
    fireEvent.click(screen.getByRole('button', { name: /Guilde/ }));

    expect(screen.getByRole('menu', { name: 'Menu du serveur' })).toBeInTheDocument();
    expect(
      screen.getByRole('menuitem', { name: 'Paramètres du serveur' }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole('menuitem', { name: 'Copier l’ID du serveur' }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole('menuitem', { name: 'Quitter le serveur' }),
    ).toBeInTheDocument();
    // Ni invitation ni création de salon/catégorie sans les permissions requises.
    expect(
      screen.queryByRole('menuitem', { name: 'Inviter des personnes' }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole('menuitem', { name: 'Créer un salon' }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole('menuitem', { name: 'Créer la catégorie' }),
    ).not.toBeInTheDocument();
  });

  it('affiche « Inviter » et « Créer un salon »/« Créer la catégorie » avec les permissions', () => {
    useGroups.setState({
      ids: ['g1'],
      states: {
        g1: groupState({
          my_permissions: PERMISSIONS.INVITE | PERMISSIONS.MANAGE_CHANNELS,
        }),
      },
    });

    render(<Sidebar />);
    fireEvent.click(screen.getByRole('button', { name: /Guilde/ }));

    expect(
      screen.getByRole('menuitem', { name: 'Inviter des personnes' }),
    ).toBeInTheDocument();
    expect(screen.getByRole('menuitem', { name: 'Créer un salon' })).toBeInTheDocument();
    expect(
      screen.getByRole('menuitem', { name: 'Créer la catégorie' }),
    ).toBeInTheDocument();
  });

  it('« Créer la catégorie » ouvre les paramètres du serveur sur l’onglet Salons', () => {
    useGroups.setState({
      ids: ['g1'],
      states: { g1: groupState({ my_permissions: PERMISSIONS.MANAGE_CHANNELS }) },
    });

    render(<Sidebar />);
    fireEvent.click(screen.getByRole('button', { name: /Guilde/ }));
    fireEvent.click(screen.getByRole('menuitem', { name: 'Créer la catégorie' }));

    expect(useUi.getState().modal).toEqual({
      kind: 'serverSettings',
      groupId: 'g1',
      initialTab: 'channels',
    });
  });

  it('« Quitter le serveur » demande confirmation puis appelle leave()', () => {
    const original = useGroups.getState().leave;
    const leave = vi.fn(() => Promise.resolve());
    useGroups.setState({ ids: ['g1'], states: { g1: groupState() }, leave });
    vi.spyOn(window, 'confirm').mockReturnValue(true);

    render(<Sidebar />);
    fireEvent.click(screen.getByRole('button', { name: /Guilde/ }));
    fireEvent.click(screen.getByRole('menuitem', { name: 'Quitter le serveur' }));

    expect(leave).toHaveBeenCalledWith('g1');
    vi.restoreAllMocks();
    useGroups.setState({ leave: original });
  });

  it('se ferme avec Échap', () => {
    useGroups.setState({ ids: ['g1'], states: { g1: groupState() } });

    render(<Sidebar />);
    fireEvent.click(screen.getByRole('button', { name: /Guilde/ }));
    expect(screen.getByRole('menu', { name: 'Menu du serveur' })).toBeInTheDocument();

    fireEvent.keyDown(window, { key: 'Escape' });

    expect(
      screen.queryByRole('menu', { name: 'Menu du serveur' }),
    ).not.toBeInTheDocument();
  });
});
