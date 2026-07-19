/**
 * Tests de la garde anti-id-périmé du rail des serveurs : la restauration du
 * dernier salon consulté ne doit jamais renvoyer un salon supprimé ou devenu
 * vocal, et replie proprement sur le premier salon disponible. Couvre aussi
 * les pastilles de non-lu/mention posées sur les icônes du rail (Accueil/MP
 * et serveurs).
 */

import { beforeEach, describe, expect, it } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import type { Contact, GroupChannel, GroupStateJson } from '../lib/api';
import { useContextMenu } from '../stores/contextMenu';
import { useFriends } from '../stores/friends';
import { PERMISSIONS, useGroups } from '../stores/groups';
import { useMute } from '../stores/mute';
import { useUi } from '../stores/ui';
import { ContextMenu } from './ContextMenu';
import { channelToRestore, ServerRail } from './ServerRail';

function contact(pubkey: string, unread?: number, mentionCount?: number): Contact {
  return {
    node_id: 'noeud',
    pubkey,
    friend_code: 'accord-lion-foret-12345',
    display_name: pubkey,
    bio: null,
    avatar: null,
    banner: null,
    state: 'friend',
    last_seen_ms: 0,
    ...(unread !== undefined ? { unread } : {}),
    ...(mentionCount !== undefined ? { mention_count: mentionCount } : {}),
  };
}

/** État minimal d'un serveur nommé, sans salon (pastilles : le nom suffit). */
function serverState(name: string): GroupStateJson {
  return {
    group_id: 'g1',
    name,
    icon: null,
    founder: null,
    members: [],
    bans: [],
    channels: [],
    categories: [],
    roles: [],
    invites: [],
    my_permissions: 0,
  };
}

function channel(
  id: string,
  position: number,
  kind: GroupChannel['kind'] = 'text',
): GroupChannel {
  return { channel_id: id, name: id, kind, category: null, position, topic: '' };
}

function groupState(channels: GroupChannel[]): GroupStateJson {
  return {
    group_id: 'g1',
    name: 'Guilde',
    icon: null,
    founder: null,
    members: [],
    bans: [],
    channels,
    categories: [],
    roles: [],
    invites: [],
    my_permissions: 0,
  };
}

describe('channelToRestore', () => {
  it('restaure le salon mémorisé quand il existe toujours', () => {
    const state = groupState([channel('c1', 0), channel('c2', 1)]);

    expect(channelToRestore(state, 'c2')).toBe('c2');
  });

  it('replie sur le premier salon quand le salon mémorisé a été supprimé', () => {
    const state = groupState([channel('c1', 0), channel('c2', 1)]);

    expect(channelToRestore(state, 'c-disparu')).toBe('c1');
  });

  it('replie sur le premier salon quand le salon mémorisé est devenu vocal', () => {
    const state = groupState([channel('c1', 0), channel('c2', 1, 'voice')]);

    expect(channelToRestore(state, 'c2')).toBe('c1');
  });

  it('renvoie null sans aucun salon disponible', () => {
    const state = groupState([]);

    expect(channelToRestore(state, 'c-disparu')).toBeNull();
  });

  it('renvoie null quand l’état du serveur n’est pas encore chargé', () => {
    expect(channelToRestore(undefined, 'c1')).toBeNull();
  });

  it('utilise le premier salon quand aucun salon n’est mémorisé', () => {
    const state = groupState([channel('c1', 0), channel('c2', 1)]);

    expect(channelToRestore(state, undefined)).toBe('c1');
  });
});

describe('ServerRail — pastilles', () => {
  beforeEach(() => {
    useUi.setState({
      lang: 'fr',
      view: { kind: 'friends' },
      lastChannelByServer: {},
      lastDmPeer: null,
    });
    useFriends.setState({ contacts: [] });
    useGroups.setState({ ids: [], states: {}, mentions: {} });
    useMute.setState({ serverLevels: {}, channelLevels: {} });
  });

  it('affiche le non-lu agrégé des MP sur le bouton Accueil', () => {
    useFriends.setState({ contacts: [contact('alice-pk', 3), contact('bob-pk', 2)] });

    render(<ServerRail />);

    expect(screen.getByLabelText(/5 message\(s\) non lu\(s\)/)).toBeInTheDocument();
    expect(screen.getByText('5')).toBeInTheDocument();
  });

  it('une mention en MP prime sur le simple non-lu sur le bouton Accueil', () => {
    useFriends.setState({ contacts: [contact('alice-pk', 3, 1)] });

    render(<ServerRail />);

    const button = screen.getByLabelText(/1 mention\(s\) non lue\(s\)/);
    // Le badge rouge visuel (posé à côté du bouton) porte le préfixe « @ »
    // propre aux mentions ; le texte « @1 » est réparti sur deux nœuds
    // (icône « @ » + compte), d'où `toHaveTextContent` sur le conteneur
    // plutôt que `getByText` (qui ne recompose pas le texte fragmenté).
    expect(button.parentElement).toHaveTextContent('@1');
  });

  it("n'affiche aucune pastille Accueil sans non-lu ni mention", () => {
    useFriends.setState({ contacts: [contact('alice-pk')] });

    render(<ServerRail />);

    expect(screen.queryByLabelText(/non lu/)).not.toBeInTheDocument();
  });

  it('une mention en salon pose un badge rouge « @ » sur l’icône du serveur', () => {
    useGroups.setState({
      ids: ['g1'],
      states: { g1: serverState('Guilde') },
      mentions: { g1: 4 },
    });

    render(<ServerRail />);

    // Nom accessible du bouton enrichi du compte de mentions...
    const button = screen.getByLabelText(/Guilde .* 4 mention\(s\) non lue\(s\)/);
    // ...et badge visuel rouge « @4 » posé sur l'icône (texte fragmenté sur
    // deux nœuds : `toHaveTextContent` plutôt que `getByText`).
    expect(button.parentElement).toHaveTextContent('@4');
  });

  it("n'affiche aucun badge sur l'icône du serveur sans mention", () => {
    useGroups.setState({
      ids: ['g1'],
      states: { g1: serverState('Guilde') },
      mentions: {},
    });

    render(<ServerRail />);

    expect(screen.getByLabelText('Guilde')).toBeInTheDocument();
    expect(screen.queryByLabelText(/mention/)).not.toBeInTheDocument();
  });
});

describe('ServerRail — actions du rail', () => {
  beforeEach(() => {
    useUi.setState({
      lang: 'fr',
      view: { kind: 'friends' },
      modal: null,
      lastChannelByServer: {},
      lastDmPeer: null,
    });
    useFriends.setState({ contacts: [] });
    useGroups.setState({ ids: [], states: {}, mentions: {} });
    useMute.setState({ serverLevels: {}, channelLevels: {} });
  });

  it('le bouton « + » ouvre la modale créer/rejoindre un serveur', () => {
    render(<ServerRail />);

    fireEvent.click(screen.getByLabelText('Créer un groupe'));

    expect(useUi.getState().modal).toEqual({ kind: 'createGroup' });
  });

  it('n’affiche plus de bouton dédié « Rejoindre un serveur » (déplacé dans « + »)', () => {
    render(<ServerRail />);

    expect(screen.queryByLabelText('Rejoindre un serveur')).not.toBeInTheDocument();
  });
});

describe('ServerRail — sourdine des notifications', () => {
  beforeEach(() => {
    useUi.setState({
      lang: 'fr',
      view: { kind: 'friends' },
      lastChannelByServer: {},
      lastDmPeer: null,
    });
    useFriends.setState({ contacts: [] });
    useGroups.setState({
      ids: ['g1'],
      states: { g1: serverState('Guilde') },
      mentions: {},
    });
    useMute.setState({ serverLevels: {}, channelLevels: {} });
    useContextMenu.setState({ menu: null });
  });

  it('atténue l’icône (opacité) d’un serveur en sourdine', () => {
    useMute.setState({ serverLevels: { g1: 'none' } });

    render(<ServerRail />);

    const button = screen.getByLabelText(/Guilde.*En sourdine/);
    expect(button.className).toMatch(/opacity-50/);
  });

  it('n’atténue pas un serveur qui n’est pas en sourdine', () => {
    render(<ServerRail />);

    const button = screen.getByLabelText('Guilde');
    expect(button.className).not.toMatch(/opacity-50/);
  });

  it('le sous-menu « Notifications » règle le niveau du serveur (Rien puis Tout)', () => {
    render(
      <>
        <ServerRail />
        <ContextMenu />
      </>,
    );

    // Rendu réel du menu (plutôt qu'appeler `onClick` à la main) : le clic
    // passe par les gestionnaires React normaux, donc par `act()` via
    // `fireEvent`, ce qui garantit que le re-rendu déclenché par le store est
    // bien reflété avant l'assertion suivante.
    fireEvent.contextMenu(screen.getByLabelText('Guilde'));
    fireEvent.click(screen.getByRole('menuitem', { name: 'Notifications' }));

    // Sous-menu à trois choix, coche sur le niveau actif (« Tout » par défaut).
    expect(screen.getByRole('menuitemradio', { name: 'Tout' })).toHaveAttribute(
      'aria-checked',
      'true',
    );
    expect(screen.getByRole('menuitemradio', { name: 'Rien' })).toHaveAttribute(
      'aria-checked',
      'false',
    );

    fireEvent.click(screen.getByRole('menuitemradio', { name: 'Rien' }));
    expect(useMute.getState().serverLevels).toEqual({ g1: 'none' });

    // Réouverture : la coche suit désormais « Rien », et l'on repasse à « Tout ».
    fireEvent.contextMenu(screen.getByLabelText(/Guilde.*En sourdine/));
    fireEvent.click(screen.getByRole('menuitem', { name: 'Notifications' }));
    expect(screen.getByRole('menuitemradio', { name: 'Rien' })).toHaveAttribute(
      'aria-checked',
      'true',
    );

    fireEvent.click(screen.getByRole('menuitemradio', { name: 'Tout' }));
    expect(useMute.getState().serverLevels).toEqual({ g1: 'all' });
  });
});

describe('ServerRail — accessibilité clavier', () => {
  beforeEach(() => {
    useUi.setState({
      lang: 'fr',
      view: { kind: 'friends' },
      lastChannelByServer: {},
      lastDmPeer: null,
    });
    useFriends.setState({ contacts: [] });
    useGroups.setState({
      ids: ['g1'],
      states: { g1: serverState('Guilde') },
      mentions: {},
    });
    useMute.setState({ serverLevels: {}, channelLevels: {} });
    useContextMenu.setState({ menu: null });
  });

  it('expose le serveur actif via aria-current="page"', () => {
    useUi.setState({ view: { kind: 'group', groupId: 'g1', channelId: null } });

    render(<ServerRail />);

    const serverButton = screen.getByLabelText('Guilde');
    expect(serverButton).toHaveAttribute('aria-current', 'page');
    expect(serverButton.previousElementSibling).toHaveClass('bg-header');
    // Le bouton Accueil/MP, inactif, ne porte pas l'attribut.
    expect(screen.getByLabelText('Messages privés')).not.toHaveAttribute('aria-current');
  });

  it('Maj+F10 ouvre le menu contextuel du serveur au clavier', () => {
    render(
      <>
        <ServerRail />
        <ContextMenu />
      </>,
    );

    fireEvent.keyDown(screen.getByLabelText('Guilde'), { key: 'F10', shiftKey: true });

    expect(screen.getByRole('menu')).toBeInTheDocument();
    expect(
      screen.getByRole('menuitem', { name: 'Copier l’ID du serveur' }),
    ).toBeInTheDocument();
  });

  it('propose créations et masquage des salons muets avec les permissions', () => {
    useGroups.setState({
      ids: ['g1'],
      states: {
        g1: {
          ...serverState('Guilde'),
          my_permissions: PERMISSIONS.INVITE | PERMISSIONS.MANAGE_CHANNELS,
        },
      },
      mentions: {},
      unread: {},
    });

    render(
      <>
        <ServerRail />
        <ContextMenu />
      </>,
    );
    fireEvent.keyDown(screen.getByLabelText('Guilde'), { key: 'F10', shiftKey: true });

    expect(
      screen.getByRole('menuitem', { name: 'Inviter des personnes' }),
    ).toBeInTheDocument();
    expect(screen.getByRole('menuitem', { name: 'Créer un salon' })).toBeInTheDocument();
    expect(
      screen.getByRole('menuitem', { name: 'Créer une catégorie' }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole('menuitem', { name: 'Créer un événement' }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole('menuitemradio', { name: 'Masquer les salons muets' }),
    ).toBeInTheDocument();
    // Aucun non-lu : pas d'entrée « Marquer comme lu » (jamais un no-op).
    expect(
      screen.queryByRole('menuitem', { name: 'Marquer comme lu' }),
    ).not.toBeInTheDocument();
  });

  it('« Marquer comme lu » n’apparaît que si le serveur a des non-lus', () => {
    useGroups.setState({
      ids: ['g1'],
      states: { g1: serverState('Guilde') },
      mentions: {},
      unread: { g1: { c1: 3 } },
    });

    render(
      <>
        <ServerRail />
        <ContextMenu />
      </>,
    );
    fireEvent.keyDown(screen.getByLabelText('Guilde'), { key: 'F10', shiftKey: true });

    expect(
      screen.getByRole('menuitem', { name: 'Marquer comme lu' }),
    ).toBeInTheDocument();
  });
});
