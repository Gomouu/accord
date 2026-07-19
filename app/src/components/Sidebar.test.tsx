/**
 * Tests de la barre latérale : pastilles de non-lus sur les conversations
 * privées (champ `unread` de friends.list) et sur les salons d'un serveur
 * (compteurs de groups.list), absentes sans non-lu.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Mock } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import type { Contact, GroupStateJson } from '../lib/api';
import { lireFichier } from '../lib/files';
import { useContextMenu } from '../stores/contextMenu';
import { useFriends } from '../stores/friends';
import { PERMISSIONS, useGroups } from '../stores/groups';
import { useMute } from '../stores/mute';
import { useSession } from '../stores/session';
import { useUi } from '../stores/ui';
import { ContextMenu } from './ContextMenu';
import { Sidebar } from './Sidebar';

vi.mock('../lib/files', () => ({
  lireFichier: vi.fn(() => new Promise(() => {})),
}));

const lireFichierMock = lireFichier as unknown as Mock;

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
  avatar_decoration: null,
  profile_effect: null,
  profile_frame: null,
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
  useMute.setState({ serverLevels: {}, channelLevels: {} });
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
    const leave = screen.getByRole('menuitem', { name: 'Quitter le serveur' });
    expect(leave).toHaveClass('server-menu-danger');
    expect(screen.getAllByRole('separator')).toHaveLength(3);
    // Ni invitation ni création de salon/catégorie sans les permissions requises.
    expect(
      screen.queryByRole('menuitem', { name: 'Inviter des personnes' }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole('menuitem', { name: 'Créer un salon' }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole('menuitem', { name: 'Créer une catégorie' }),
    ).not.toBeInTheDocument();
  });

  it('affiche « Inviter » et « Créer un salon »/« Créer une catégorie » avec les permissions', () => {
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
      screen.getByRole('menuitem', { name: 'Créer une catégorie' }),
    ).toBeInTheDocument();
  });

  it('le bandeau ne porte plus de raccourci Paramètres — seul le menu les propose', () => {
    useGroups.setState({
      ids: ['g1'],
      states: { g1: groupState({ my_permissions: PERMISSIONS.INVITE }) },
    });

    render(<Sidebar />);

    // L'engrenage direct du bandeau a été retiré (doublon du menu du serveur)…
    expect(
      screen.queryByRole('button', { name: 'Paramètres du serveur' }),
    ).not.toBeInTheDocument();
    // …mais l'icône d'invitation (personne+) reste avec la permission INVITE.
    expect(screen.getByRole('button', { name: 'Inviter' })).toBeInTheDocument();
  });

  it('« Créer une catégorie » ouvre les paramètres du serveur sur l’onglet Salons', () => {
    useGroups.setState({
      ids: ['g1'],
      states: { g1: groupState({ my_permissions: PERMISSIONS.MANAGE_CHANNELS }) },
    });

    render(<Sidebar />);
    fireEvent.click(screen.getByRole('button', { name: /Guilde/ }));
    fireEvent.click(screen.getByRole('menuitem', { name: 'Créer une catégorie' }));

    expect(useUi.getState().modal).toEqual({
      kind: 'serverSettings',
      groupId: 'g1',
      initialTab: 'channels',
    });
  });

  it('affiche « Créer un événement » avec MANAGE_CHANNELS et ouvre la modale', () => {
    useGroups.setState({
      ids: ['g1'],
      states: { g1: groupState({ my_permissions: PERMISSIONS.MANAGE_CHANNELS }) },
    });

    render(<Sidebar />);
    fireEvent.click(screen.getByRole('button', { name: /Guilde/ }));
    fireEvent.click(screen.getByRole('menuitem', { name: 'Créer un événement' }));

    expect(useUi.getState().modal).toEqual({ kind: 'events', groupId: 'g1' });
  });

  it('« Masquer les salons muets » bascule la préférence locale', () => {
    useGroups.setState({ ids: ['g1'], states: { g1: groupState() } });
    useUi.setState({ hideMutedChannels: false });

    render(<Sidebar />);
    fireEvent.click(screen.getByRole('button', { name: /Guilde/ }));
    const item = screen.getByRole('menuitemcheckbox', {
      name: 'Masquer les salons muets',
    });
    expect(item).toHaveAttribute('aria-checked', 'false');
    expect(item.querySelector('span[aria-hidden]')).toHaveClass(
      'h-[18px]',
      'w-[18px]',
      'border-faint/70',
    );

    fireEvent.click(item);
    expect(useUi.getState().hideMutedChannels).toBe(true);
    useUi.setState({ hideMutedChannels: false });
  });

  it('« Notifications » ouvre le sous-menu de niveau (serveur)', () => {
    useGroups.setState({ ids: ['g1'], states: { g1: groupState() } });

    render(
      <>
        <Sidebar />
        <ContextMenu />
      </>,
    );
    fireEvent.click(screen.getByRole('button', { name: /Guilde/ }));
    const notifications = screen.getByRole('menuitem', { name: 'Notifications' });
    expect(notifications.querySelectorAll('svg')).toHaveLength(2);
    fireEvent.click(notifications);

    fireEvent.click(screen.getByRole('menuitemradio', { name: 'Rien' }));
    expect(useMute.getState().serverLevels.g1).toBe('none');
    useMute.setState({ serverLevels: {} });
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
    const menu = screen.getByRole('menu', { name: 'Menu du serveur' });
    expect(menu).toHaveClass('server-menu-surface', 'overflow-hidden');
    expect(menu.querySelector('.server-menu-scroll')).toHaveClass(
      'overflow-y-auto',
      'overscroll-contain',
    );

    fireEvent.keyDown(window, { key: 'Escape' });

    expect(
      screen.queryByRole('menu', { name: 'Menu du serveur' }),
    ).not.toBeInTheDocument();
  });

  it('gère le focus, Home, End et Tab au clavier', () => {
    useGroups.setState({ ids: ['g1'], states: { g1: groupState() } });

    render(<Sidebar />);
    fireEvent.click(screen.getByRole('button', { name: /Guilde/ }));

    const settings = screen.getByRole('menuitem', { name: 'Paramètres du serveur' });
    expect(settings).toHaveFocus();

    fireEvent.keyDown(settings, { key: 'End' });
    const leave = screen.getByRole('menuitem', { name: 'Quitter le serveur' });
    expect(leave).toHaveFocus();

    fireEvent.keyDown(leave, { key: 'Home' });
    expect(settings).toHaveFocus();

    fireEvent.keyDown(settings, { key: 'Tab' });
    expect(
      screen.queryByRole('menu', { name: 'Menu du serveur' }),
    ).not.toBeInTheDocument();
  });

  it('le clavier repart de l’item focalisé malgré un survol ailleurs', () => {
    useGroups.setState({ ids: ['g1'], states: { g1: groupState() } });

    render(
      <>
        <Sidebar />
        <ContextMenu />
      </>,
    );
    fireEvent.click(screen.getByRole('button', { name: /Guilde/ }));

    const settings = screen.getByRole('menuitem', { name: 'Paramètres du serveur' });
    expect(settings).toHaveFocus();
    const notifications = screen.getByRole('menuitem', { name: 'Notifications' });
    fireEvent.keyDown(settings, { key: 'ArrowDown' });
    expect(notifications).toHaveFocus();

    fireEvent.mouseEnter(
      screen.getByRole('menuitem', { name: 'Copier l’ID du serveur' }),
    );
    expect(notifications).toHaveFocus();

    fireEvent.keyDown(notifications, { key: 'ArrowRight' });
    expect(screen.getByRole('menuitemradio', { name: 'Rien' })).toBeInTheDocument();
  });
});

describe('Sidebar — sourdine des notifications (salon)', () => {
  beforeEach(() => {
    useUi.setState({ view: { kind: 'group', groupId: 'g1', channelId: 'c1' } });
    useGroups.setState({ ids: ['g1'], states: { g1: groupState() } });
    useContextMenu.setState({ menu: null });
  });

  it('atténue la ligne et affiche l’icône cloche barrée sur un salon en sourdine', () => {
    useMute.setState({ channelLevels: { 'g1/c1': 'none' } });

    render(<Sidebar />);

    expect(
      screen.getByLabelText('Salon en sourdine : notifications désactivées'),
    ).toBeInTheDocument();
    const row = screen.getByText('général').closest('button');
    expect(row?.className).toMatch(/opacity-50/);
  });

  it('n’atténue pas un salon qui n’est pas en sourdine', () => {
    render(<Sidebar />);

    expect(
      screen.queryByLabelText('Salon en sourdine : notifications désactivées'),
    ).not.toBeInTheDocument();
    const row = screen.getByText('général').closest('button');
    expect(row?.className).not.toMatch(/opacity-50/);
  });

  it('masque un salon muet non actif quand « Masquer les salons muets » est actif', () => {
    useMute.setState({ channelLevels: { 'g1/c2': 'none' } });
    useUi.setState({ hideMutedChannels: true });

    render(<Sidebar />);

    // c1 (général) est le salon actif ; c2 (projets) est muet → masqué.
    expect(screen.getByText('général')).toBeInTheDocument();
    expect(screen.queryByText('projets')).not.toBeInTheDocument();
    useUi.setState({ hideMutedChannels: false });
  });

  it('garde le salon actif visible même en sourdine', () => {
    useMute.setState({ channelLevels: { 'g1/c1': 'none', 'g1/c2': 'none' } });
    useUi.setState({ hideMutedChannels: true });

    render(<Sidebar />);

    // c1 est actif : conservé bien que muet ; c2 muet non actif → masqué.
    expect(screen.getByText('général')).toBeInTheDocument();
    expect(screen.queryByText('projets')).not.toBeInTheDocument();
    useUi.setState({ hideMutedChannels: false });
  });

  it('le sous-menu « Notifications » règle le niveau du salon (Rien puis Tout)', () => {
    render(
      <>
        <Sidebar />
        <ContextMenu />
      </>,
    );

    // Rendu réel du menu (plutôt qu'appeler `onClick` à la main) : le clic
    // passe par les gestionnaires React normaux, donc par `act()` via
    // `fireEvent`, ce qui garantit que le re-rendu déclenché par le store est
    // bien reflété avant l'assertion suivante.
    fireEvent.contextMenu(screen.getByText('général'));
    fireEvent.click(screen.getByRole('menuitem', { name: 'Notifications' }));
    expect(screen.getByRole('menuitemradio', { name: 'Tout' })).toHaveAttribute(
      'aria-checked',
      'true',
    );

    fireEvent.click(screen.getByRole('menuitemradio', { name: 'Rien' }));
    expect(useMute.getState().channelLevels).toEqual({ 'g1/c1': 'none' });

    fireEvent.contextMenu(screen.getByText('général'));
    fireEvent.click(screen.getByRole('menuitem', { name: 'Notifications' }));
    fireEvent.click(screen.getByRole('menuitemradio', { name: 'Tout' }));
    expect(useMute.getState().channelLevels).toEqual({ 'g1/c1': 'all' });
  });

  it('ne règle que le salon ciblé, pas ses voisins', () => {
    render(
      <>
        <Sidebar />
        <ContextMenu />
      </>,
    );

    fireEvent.contextMenu(screen.getByText('général'));
    fireEvent.click(screen.getByRole('menuitem', { name: 'Notifications' }));
    fireEvent.click(screen.getByRole('menuitemradio', { name: 'Rien' }));

    expect(useMute.getState().channelLevels).toEqual({ 'g1/c1': 'none' });
    const rows = screen.getAllByLabelText(
      'Salon en sourdine : notifications désactivées',
    );
    expect(rows).toHaveLength(1);
  });
});

describe('Sidebar — bannière du serveur', () => {
  const BANNER_HASH = 'ab'.repeat(32);

  beforeEach(() => {
    useUi.setState({ view: { kind: 'group', groupId: 'g1', channelId: 'c1' } });
    lireFichierMock.mockReset();
  });

  it("réserve immédiatement la hauteur du bandeau pendant le chargement de l'image", () => {
    lireFichierMock.mockReturnValue(new Promise(() => {}));
    useGroups.setState({
      ids: ['g1'],
      states: { g1: groupState({ banner: BANNER_HASH }) },
    });

    render(<Sidebar />);

    expect(screen.getByTestId('server-header')).toHaveClass('h-24', 'bg-tooltip');
    expect(screen.getByTestId('server-header')).toHaveStyle({
      backgroundImage:
        'linear-gradient(135deg, rgb(var(--color-blurple) / 0.72), rgb(var(--color-tooltip)))',
    });
    expect(screen.queryByTestId('server-banner')).not.toBeInTheDocument();
  });

  it('affiche le bandeau image avec scrim, nom par-dessus et menu intact', async () => {
    // Arrange
    lireFichierMock.mockResolvedValue('data:image/png;base64,YWJj');
    useGroups.setState({
      ids: ['g1'],
      states: { g1: groupState({ banner: BANNER_HASH }) },
    });

    // Act
    render(<Sidebar />);

    // Assert — image chargée par sa racine Merkle, scrim de lisibilité posé.
    const img = await screen.findByTestId('server-banner');
    expect(img).toHaveAttribute('src', 'data:image/png;base64,YWJj');
    expect(img).toHaveClass('animate-[fade-in_var(--duration-normal)_var(--ease-out)]');
    expect(lireFichierMock).toHaveBeenCalledWith(BANNER_HASH);
    expect(screen.getByTestId('server-banner-scrim')).toBeInTheDocument();

    // Le nom reste superposé et le menu déroulant s'ouvre comme avant.
    fireEvent.click(screen.getByRole('button', { name: /Guilde/ }));
    expect(screen.getByRole('menu', { name: 'Menu du serveur' })).toBeInTheDocument();
  });

  it("garde l'en-tête simple sans bannière", () => {
    useGroups.setState({ ids: ['g1'], states: { g1: groupState() } });

    render(<Sidebar />);

    expect(screen.queryByTestId('server-banner')).not.toBeInTheDocument();
    expect(screen.queryByTestId('server-banner-scrim')).not.toBeInTheDocument();
    expect(screen.getByTestId('server-header')).toHaveClass('h-12', 'bg-sidebar');
    expect(lireFichierMock).not.toHaveBeenCalled();
    expect(screen.getByRole('button', { name: /Guilde/ })).toBeInTheDocument();
  });

  it('conserve le bandeau de repli sans saut si la lecture échoue', async () => {
    lireFichierMock.mockRejectedValue(new Error('indisponible'));
    useGroups.setState({
      ids: ['g1'],
      states: { g1: groupState({ banner: BANNER_HASH }) },
    });

    render(<Sidebar />);

    await waitFor(() => expect(lireFichierMock).toHaveBeenCalled());
    expect(screen.queryByTestId('server-banner')).not.toBeInTheDocument();
    expect(screen.getByTestId('server-header')).toHaveClass('h-24', 'bg-tooltip');
    // Une seule tentative — les reprises vivent dans lib/files, pas ici.
    expect(lireFichierMock).toHaveBeenCalledTimes(1);
    expect(screen.getByRole('button', { name: /Guilde/ })).toBeInTheDocument();
  });
});

describe('Sidebar — accessibilité clavier des salons', () => {
  beforeEach(() => {
    useUi.setState({ view: { kind: 'group', groupId: 'g1', channelId: 'c1' } });
    useGroups.setState({ ids: ['g1'], states: { g1: groupState() }, unread: {} });
    useContextMenu.setState({ menu: null });
  });

  it('expose le salon actif via aria-current="page"', () => {
    render(<Sidebar />);

    expect(screen.getByRole('button', { name: 'général' })).toHaveAttribute(
      'aria-current',
      'page',
    );
    expect(screen.getByRole('button', { name: 'projets' })).not.toHaveAttribute(
      'aria-current',
    );
  });

  it('Maj+F10 ouvre le menu contextuel du salon au clavier', () => {
    render(
      <>
        <Sidebar />
        <ContextMenu />
      </>,
    );

    fireEvent.keyDown(screen.getByRole('button', { name: 'projets' }), {
      key: 'F10',
      shiftKey: true,
    });

    expect(screen.getByRole('menu')).toBeInTheDocument();
    expect(
      screen.getByRole('menuitem', { name: 'Copier l’ID du salon' }),
    ).toBeInTheDocument();
  });

  it('expose l’entrée Amis active via aria-current en vue accueil', () => {
    useUi.setState({ view: { kind: 'friends' } });

    render(<Sidebar />);

    expect(screen.getByRole('button', { name: 'Amis' })).toHaveAttribute(
      'aria-current',
      'page',
    );
  });
});
