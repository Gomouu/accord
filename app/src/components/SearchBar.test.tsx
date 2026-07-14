/**
 * Tests de la barre de recherche : pastilles de filtres à la frappe, rendu des
 * résultats par métadonnées (avec extrait local quand disponible) et saut au
 * message au clic (demande `requestJump` du store d'interface).
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';

vi.mock('../lib/client', () => ({
  rpc: { call: vi.fn(), onEvent: vi.fn(), onStatus: vi.fn() },
  api: { searchQuery: vi.fn() },
}));

import { api } from '../lib/client';
import type { Mock } from 'vitest';
import type { Contact, SelfProfile } from '../lib/api';
import { SearchBar } from './SearchBar';
import { useDms } from '../stores/dms';
import { useFriends } from '../stores/friends';
import { useGroups } from '../stores/groups';
import { useSession } from '../stores/session';
import { useUi } from '../stores/ui';

const searchMock = api.searchQuery as unknown as Mock;

const SELF: SelfProfile = {
  node_id: 'noeud',
  pubkey: 'moi',
  friend_code: 'accord-moi',
  name: null,
  bio: null,
  avatar: null,
  banner: null,
  pronouns: null,
  accent_color: null,
  banner_color: null,
  avatar_decoration: null,
  profile_effect: null,
};

function contact(pubkey: string, name: string): Contact {
  return {
    node_id: `n-${pubkey}`,
    pubkey,
    friend_code: `accord-${pubkey}`,
    display_name: name,
    bio: null,
    avatar: null,
    banner: null,
    state: 'friend',
    last_seen_ms: 0,
  };
}

beforeEach(() => {
  useUi.setState({ lang: 'fr', jump: null, view: { kind: 'friends' } });
  useFriends.setState({ contacts: [contact('peer', 'Alice')] });
  useSession.setState({ self: SELF });
  useDms.setState({ conversations: {} });
  useGroups.setState({ states: {}, messages: {} });
  searchMock.mockReset();
});

describe('SearchBar — pastilles de filtres', () => {
  it('affiche les filtres reconnus à la frappe', () => {
    render(<SearchBar />);

    fireEvent.change(screen.getByLabelText('Rechercher'), {
      target: { value: 'from:alice has:image bonjour' },
    });

    const region = screen.getByLabelText('Filtres actifs');
    expect(region).toHaveTextContent('from:alice');
    expect(region).toHaveTextContent('has:image');
  });

  it('n’affiche aucune pastille sans filtre', () => {
    render(<SearchBar />);

    fireEvent.change(screen.getByLabelText('Rechercher'), {
      target: { value: 'juste des mots' },
    });

    expect(screen.queryByLabelText('Filtres actifs')).not.toBeInTheDocument();
  });
});

describe('SearchBar — résultats et saut', () => {
  const dmHit = {
    msg_id: 'm1',
    author: 'peer',
    lamport: 1,
    timestamp: 1000,
    conversation: { type: 'dm', peer: 'peer' },
  };

  it('rend les résultats avec conversation, auteur et extrait local', async () => {
    useDms.setState({
      conversations: {
        peer: [
          {
            msg_id: 'm1',
            author: 'peer',
            lamport: 1,
            sent_ms: 1000,
            acked: true,
            deleted: false,
            body: { type: 'text', text: 'bonjour Alice', reply_to: null, attachments: 0 },
            edited: null,
          },
        ],
      },
    });
    searchMock.mockResolvedValueOnce({ msg_ids: ['m1'], hits: [dmHit] });
    render(<SearchBar />);

    fireEvent.change(screen.getByLabelText('Rechercher'), {
      target: { value: 'bonjour' },
    });
    fireEvent.keyDown(screen.getByLabelText('Rechercher'), { key: 'Enter' });

    expect(await screen.findByText('bonjour Alice')).toBeInTheDocument();
    expect(screen.getByText('@Alice')).toBeInTheDocument();
  });

  it('affiche un repère de saut quand la conversation n’est pas chargée', async () => {
    searchMock.mockResolvedValueOnce({ msg_ids: ['m1'], hits: [dmHit] });
    render(<SearchBar />);

    fireEvent.change(screen.getByLabelText('Rechercher'), {
      target: { value: 'bonjour' },
    });
    fireEvent.keyDown(screen.getByLabelText('Rechercher'), { key: 'Enter' });

    expect(await screen.findByText('Aller au message')).toBeInTheDocument();
  });

  it('demande le saut vers le message au clic sur un résultat', async () => {
    searchMock.mockResolvedValueOnce({ msg_ids: ['m1'], hits: [dmHit] });
    render(<SearchBar />);

    fireEvent.change(screen.getByLabelText('Rechercher'), {
      target: { value: 'bonjour' },
    });
    fireEvent.keyDown(screen.getByLabelText('Rechercher'), { key: 'Enter' });
    fireEvent.click(await screen.findByText('Aller au message'));

    expect(useUi.getState().jump).toMatchObject({
      msgId: 'm1',
      view: { kind: 'dm', peer: 'peer' },
    });
  });
});
