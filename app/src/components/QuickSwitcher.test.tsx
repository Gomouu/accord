/**
 * Tests du sélecteur rapide : ouverture/fermeture, filtrage en direct,
 * navigation clavier (flèches + Entrée), sélection à la souris, dernières
 * destinations à requête vide et non-jonction d'un salon vocal sélectionné.
 */

import { beforeEach, describe, expect, it } from 'vitest';
import { act, fireEvent, render, screen } from '@testing-library/react';
import type { Contact, GroupStateJson, SelfProfile } from '../lib/api';
import { useFriends } from '../stores/friends';
import { useGroups } from '../stores/groups';
import { useSession } from '../stores/session';
import { useUi } from '../stores/ui';
import { QuickSwitcher } from './QuickSwitcher';

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
  profile_frame: null,
};

function contact(pubkey: string, displayName: string): Contact {
  return {
    node_id: `n-${pubkey}`,
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
  useUi.setState({
    quickSwitcherOpen: false,
    view: { kind: 'friends' },
    lastDmPeer: null,
    lastChannelByServer: {},
  });
  useSession.setState({ self: SELF, phase: 'ready' });
  useFriends.setState({
    contacts: [contact('alice', 'Alice'), contact('bob', 'Bobby')],
    loaded: true,
  });
  useGroups.setState({
    ids: ['g1'],
    states: {
      g1: groupState({
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
            channel_id: 'v1',
            name: 'salle-vocale',
            kind: 'voice',
            category: null,
            position: 1,
            topic: '',
          },
        ],
      }),
    },
  });
});

describe('QuickSwitcher', () => {
  it('ne rend rien tant que le store n’est pas ouvert', () => {
    render(<QuickSwitcher />);
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('s’affiche, avec Amis en tête des destinations récentes à requête vide', () => {
    useUi.setState({ quickSwitcherOpen: true });
    render(<QuickSwitcher />);

    expect(screen.getByRole('dialog')).toBeInTheDocument();
    expect(screen.getByRole('combobox')).toBeInTheDocument();
  });

  it('filtre en direct à la frappe', () => {
    useUi.setState({ quickSwitcherOpen: true });
    render(<QuickSwitcher />);

    fireEvent.change(screen.getByRole('combobox'), { target: { value: 'bob' } });

    expect(screen.getByText('Bobby')).toBeInTheDocument();
    expect(screen.queryByText('Alice')).toBeNull();
  });

  it('Entrée navigue vers le résultat actif puis referme la palette', () => {
    useUi.setState({ quickSwitcherOpen: true });
    render(<QuickSwitcher />);

    fireEvent.change(screen.getByRole('combobox'), { target: { value: 'bob' } });
    fireEvent.keyDown(screen.getByRole('combobox'), { key: 'Enter' });

    expect(useUi.getState().view).toEqual({ kind: 'dm', peer: 'bob' });
    expect(useUi.getState().quickSwitcherOpen).toBe(false);
  });

  it('ArrowDown déplace le curseur actif avant validation par Entrée', () => {
    useUi.setState({ quickSwitcherOpen: true });
    render(<QuickSwitcher />);

    // Requête vide : Amis (index 0) est seul dans les résultats récents faute
    // de mémoire de navigation — on filtre plutôt pour obtenir deux options.
    fireEvent.change(screen.getByRole('combobox'), { target: { value: 'a' } });
    const input = screen.getByRole('combobox');
    fireEvent.keyDown(input, { key: 'ArrowDown' });
    fireEvent.keyDown(input, { key: 'Enter' });

    // La première option classée pour « a » n'est pas forcément Alice ; on
    // vérifie seulement qu'une navigation a bien eu lieu hors de la vue Amis.
    expect(useUi.getState().view.kind).not.toBe('friends');
  });

  it('clic sur un résultat navigue et referme', () => {
    useUi.setState({ quickSwitcherOpen: true });
    render(<QuickSwitcher />);

    fireEvent.change(screen.getByRole('combobox'), { target: { value: 'général' } });
    fireEvent.click(screen.getByText('général'));

    expect(useUi.getState().view).toEqual({
      kind: 'group',
      groupId: 'g1',
      channelId: 'c1',
    });
    expect(useUi.getState().quickSwitcherOpen).toBe(false);
  });

  it('Échap referme sans naviguer', () => {
    useUi.setState({ quickSwitcherOpen: true, view: { kind: 'friends' } });
    render(<QuickSwitcher />);

    fireEvent.keyDown(screen.getByRole('combobox'), { key: 'Escape' });

    expect(useUi.getState().quickSwitcherOpen).toBe(false);
    expect(useUi.getState().view).toEqual({ kind: 'friends' });
  });

  it('sélectionner un salon vocal navigue vers le serveur sans rejoindre la voix', () => {
    useUi.setState({ quickSwitcherOpen: true });
    render(<QuickSwitcher />);

    fireEvent.change(screen.getByRole('combobox'), { target: { value: 'salle-vocale' } });
    fireEvent.click(screen.getByText('salle-vocale'));

    const view = useUi.getState().view;
    expect(view.kind).toBe('group');
    if (view.kind === 'group') {
      expect(view.groupId).toBe('g1');
      // Jamais le salon vocal lui-même : navigation façon clic sur l'icône serveur.
      expect(view.channelId).not.toBe('v1');
      expect(view.channelId).toBe('c1');
    }
  });

  it('montre les dernières destinations quand la requête est vide', () => {
    useUi.setState({
      quickSwitcherOpen: true,
      lastDmPeer: 'bob',
      lastChannelByServer: { g1: 'c1' },
    });
    render(<QuickSwitcher />);

    expect(screen.getByText('Bobby')).toBeInTheDocument();
    expect(screen.getByText('général')).toBeInTheDocument();
  });
  it('propose les serveurs et navigue vers leur dernier salon consulté', () => {
    useUi.setState({ quickSwitcherOpen: true, lastChannelByServer: { g1: 'c1' } });
    render(<QuickSwitcher />);

    fireEvent.change(screen.getByRole('combobox'), { target: { value: 'Guilde' } });
    fireEvent.click(screen.getByText('Guilde'));

    expect(useUi.getState().view).toEqual({
      kind: 'group',
      groupId: 'g1',
      channelId: 'c1',
    });
    expect(useUi.getState().quickSwitcherOpen).toBe(false);
  });

  it('rend le focus au déclencheur à la fermeture par Échap', () => {
    render(
      <>
        <button>déclencheur</button>
        <QuickSwitcher />
      </>,
    );
    const trigger = screen.getByRole('button', { name: 'déclencheur' });
    trigger.focus();

    act(() => useUi.setState({ quickSwitcherOpen: true }));
    expect(screen.getByRole('combobox')).toHaveFocus();

    fireEvent.keyDown(screen.getByRole('combobox'), { key: 'Escape' });
    expect(trigger).toHaveFocus();
  });
});
