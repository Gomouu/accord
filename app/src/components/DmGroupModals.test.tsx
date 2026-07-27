/**
 * Tests des surfaces de groupes de MP (jalon 5) : bornes du sélecteur d'amis
 * (de trois à vingt personnes, soi comprise), création, puis les trois actions
 * que le nœud accepte dans un tel groupe — renommer, changer l'icône, partir.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';

vi.mock('../lib/files', () => ({
  lireFichier: vi.fn(() => new Promise(() => {})),
}));

import type { Contact, GroupStateJson, SelfProfile } from '../lib/api';
import { useFriends } from '../stores/friends';
import { useGroups } from '../stores/groups';
import { useSession } from '../stores/session';
import { useUi } from '../stores/ui';
import { CreateDmGroupModal, DmGroupModal } from './DmGroupModals';

const SELF: SelfProfile = {
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

function contact(pubkey: string, displayName: string): Contact {
  return {
    node_id: 'noeud',
    pubkey,
    friend_code: `accord-${pubkey}`,
    display_name: displayName,
    bio: null,
    avatar: null,
    banner: null,
    state: 'friend',
    last_seen_ms: 0,
  };
}

function dmState(over: Partial<GroupStateJson> = {}): GroupStateJson {
  return {
    group_id: 'mp1',
    name: 'Nous trois',
    is_dm: true,
    icon: null,
    founder: 'moi',
    members: [
      { pubkey: 'moi', roles: [] },
      { pubkey: 'alice', roles: [] },
      { pubkey: 'bob', roles: [] },
    ],
    bans: [],
    channels: [
      {
        channel_id: 'fil',
        name: 'Nous trois',
        kind: 'text',
        category: null,
        position: 0,
        topic: '',
      },
    ],
    categories: [],
    roles: [],
    invites: [],
    my_permissions: 0x3ff,
    ...over,
  };
}

beforeEach(() => {
  useUi.setState({ lang: 'fr', modal: null, toasts: [], view: { kind: 'friends' } });
  useSession.setState({ self: SELF });
  useFriends.setState({
    contacts: [
      contact('alice', 'Alice'),
      contact('bob', 'Bob'),
      contact('carol', 'Carol'),
    ],
  });
  useGroups.setState({ ids: [], states: {} });
});

describe('CreateDmGroupModal', () => {
  it('exige au moins deux amis en plus de soi', () => {
    render(<CreateDmGroupModal />);

    const creer = screen.getByRole('button', { name: 'Créer le groupe' });
    fireEvent.change(screen.getByLabelText('Nom du groupe'), {
      target: { value: 'Nous trois' },
    });
    expect(creer).toBeDisabled();

    // Un seul ami : on serait deux — le message privé existe déjà pour ça.
    fireEvent.click(screen.getByRole('button', { name: /Alice/ }));
    expect(creer).toBeDisabled();

    fireEvent.click(screen.getByRole('button', { name: /Bob/ }));
    expect(creer).toBeEnabled();
  });

  it('exige aussi un nom, même avec assez d’amis', () => {
    render(<CreateDmGroupModal />);

    fireEvent.click(screen.getByRole('button', { name: /Alice/ }));
    fireEvent.click(screen.getByRole('button', { name: /Bob/ }));

    expect(screen.getByRole('button', { name: 'Créer le groupe' })).toBeDisabled();
  });

  it('crée le groupe puis ouvre son fil unique', async () => {
    const createDm = vi.fn(async () => {
      useGroups.setState({ ids: ['mp1'], states: { mp1: dmState() } });
      return 'mp1';
    });
    useGroups.setState({ createDm });

    render(<CreateDmGroupModal />);
    fireEvent.click(screen.getByRole('button', { name: /Alice/ }));
    fireEvent.click(screen.getByRole('button', { name: /Bob/ }));
    fireEvent.change(screen.getByLabelText('Nom du groupe'), {
      target: { value: '  Nous trois  ' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Créer le groupe' }));

    await waitFor(() => expect(createDm).toHaveBeenCalled());
    // Le nom est rogné et le fondateur ne figure pas dans `members`.
    expect(createDm).toHaveBeenCalledWith('Nous trois', ['alice', 'bob']);
    await waitFor(() =>
      expect(useUi.getState().view).toEqual({
        kind: 'group',
        groupId: 'mp1',
        channelId: 'fil',
      }),
    );
    expect(useUi.getState().modal).toBeNull();
  });

  it('bascule la sélection au second clic', () => {
    render(<CreateDmGroupModal />);

    const alice = screen.getByRole('button', { name: /Alice/ });
    fireEvent.click(alice);
    expect(alice).toHaveAttribute('aria-pressed', 'true');
    fireEvent.click(alice);
    expect(alice).toHaveAttribute('aria-pressed', 'false');
  });

  it('affiche le décompte des places restantes (dix-neuf, soi non comprise)', () => {
    render(<CreateDmGroupModal />);

    fireEvent.click(screen.getByRole('button', { name: /Alice/ }));

    expect(screen.getByText('1 sélectionné(s) sur 19')).toBeInTheDocument();
  });
});

describe('DmGroupModal', () => {
  it('renomme le groupe — aucun rôle consulté', async () => {
    const rename = vi.fn(async () => {});
    useGroups.setState({ ids: ['mp1'], states: { mp1: dmState() }, rename });

    render(<DmGroupModal groupId="mp1" />);
    fireEvent.change(screen.getByLabelText('Nom du groupe'), {
      target: { value: 'Nous quatre' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Renommer' }));

    await waitFor(() => expect(rename).toHaveBeenCalledWith('mp1', 'Nous quatre'));
  });

  it('n’active « Renommer » que sur un nom réellement changé', () => {
    useGroups.setState({ ids: ['mp1'], states: { mp1: dmState() } });

    render(<DmGroupModal groupId="mp1" />);

    const bouton = screen.getByRole('button', { name: 'Renommer' });
    expect(bouton).toBeDisabled();

    fireEvent.change(screen.getByLabelText('Nom du groupe'), {
      target: { value: '   ' },
    });
    expect(bouton).toBeDisabled();

    fireEvent.change(screen.getByLabelText('Nom du groupe'), {
      target: { value: 'Nous quatre' },
    });
    expect(bouton).toBeEnabled();
  });

  it('liste les membres et marque l’utilisateur local', () => {
    useGroups.setState({ ids: ['mp1'], states: { mp1: dmState() } });

    render(<DmGroupModal groupId="mp1" />);

    expect(screen.getByText('Membres (3)')).toBeInTheDocument();
    const liste = screen.getByRole('list');
    expect(within(liste).getByText('Alice')).toBeInTheDocument();
    expect(within(liste).getByText('Bob')).toBeInTheDocument();
    expect(within(liste).getByText('vous')).toBeInTheDocument();
  });

  it('fait partir le membre en deux temps, y compris le fondateur', async () => {
    const leave = vi.fn(async () => {});
    // 🔒 `founder: 'moi'` : le fondateur d'un groupe de MP peut partir comme
    // les autres — il n'a pas de rôle à transmettre.
    useGroups.setState({ ids: ['mp1'], states: { mp1: dmState() }, leave });

    render(<DmGroupModal groupId="mp1" />);
    fireEvent.click(screen.getByRole('button', { name: 'Quitter le groupe' }));

    // Premier clic : confirmation, rien n'est encore envoyé.
    expect(leave).not.toHaveBeenCalled();
    expect(screen.getByText(/Quitter « Nous trois »/)).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'Confirmer' }));

    await waitFor(() => expect(leave).toHaveBeenCalledWith('mp1'));
    await waitFor(() => expect(useUi.getState().view).toEqual({ kind: 'friends' }));
  });

  it('ne rend rien tant que l’état du groupe est inconnu', () => {
    const { container } = render(<DmGroupModal groupId="inconnu" />);

    expect(container).toBeEmptyDOMElement();
  });
});
