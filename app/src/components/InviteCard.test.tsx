/**
 * Tests de la carte d'invitation de serveur en MP : états dérivés (en
 * attente, membre, carte de l'inviteur, obsolète) et bascule des boutons vers
 * les actions du store des groupes.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';

vi.mock('../lib/client', () => ({
  rpc: { onEvent: vi.fn(() => () => {}), onStatus: vi.fn(() => () => {}) },
  api: {},
}));

import type { GroupStateJson, MsgBody, PendingInvite } from '../lib/api';
import { useGroups } from '../stores/groups';
import { useUi } from '../stores/ui';
import { InviteCard } from './InviteCard';

type InviteBody = Extract<MsgBody, { type: 'invite' }>;

const acceptInvite = vi.fn(() => Promise.resolve());
const declineInvite = vi.fn(() => Promise.resolve());
const setView = vi.fn();

function body(): InviteBody {
  return {
    type: 'invite',
    group_id: 'g1',
    invite_id: 'i1',
    inviter: 'pk_bob',
    group_name: 'Guilde',
  };
}

function pending(): PendingInvite {
  return {
    group_id: 'g1',
    invite_id: 'i1',
    group_name: 'Guilde',
    inviter: 'pk_bob',
    expires_ms: 9999,
  };
}

function etatGroupe(membres: string[]): GroupStateJson {
  return {
    group_id: 'g1',
    name: 'Guilde',
    members: membres.map((pubkey) => ({
      pubkey,
      roles: [],
      nickname: null,
      timeout_until_ms: 0,
    })),
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
  } as unknown as GroupStateJson;
}

beforeEach(() => {
  useUi.setState({
    lang: 'fr',
    lastChannelByServer: {},
    setView: setView as unknown as ReturnType<typeof useUi.getState>['setView'],
  });
  useGroups.setState({
    ids: [],
    states: {},
    pendingInvites: [],
    acceptInvite: acceptInvite as unknown as ReturnType<
      typeof useGroups.getState
    >['acceptInvite'],
    declineInvite: declineInvite as unknown as ReturnType<
      typeof useGroups.getState
    >['declineInvite'],
  });
  acceptInvite.mockClear();
  declineInvite.mockClear();
  setView.mockClear();
});

describe('InviteCard', () => {
  it('invitation en attente : Rejoindre déclenche l’acceptation', () => {
    useGroups.setState({ pendingInvites: [pending()] });
    render(<InviteCard body={body()} isOwn={false} peer="pk_bob" />);

    expect(screen.getByText('Guilde')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Rejoindre le serveur Guilde' }));
    expect(acceptInvite).toHaveBeenCalledWith('g1', 'i1');
  });

  it('invitation en attente : Refuser déclenche le refus', () => {
    useGroups.setState({ pendingInvites: [pending()] });
    render(<InviteCard body={body()} isOwn={false} peer="pk_bob" />);

    fireEvent.click(
      screen.getByRole('button', { name: 'Refuser l’invitation au serveur Guilde' }),
    );
    expect(declineInvite).toHaveBeenCalledWith('g1', 'i1');
  });

  it('déjà membre : Rejoint et navigation vers le serveur', () => {
    useGroups.setState({
      ids: ['g1'],
      states: { g1: etatGroupe(['moi']) },
    });
    render(<InviteCard body={body()} isOwn={false} peer="pk_bob" />);

    expect(screen.getByText('Rejoint')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Aller au serveur' }));
    expect(setView).toHaveBeenCalledWith({
      kind: 'group',
      groupId: 'g1',
      channelId: 'c1',
    });
  });

  it('carte de l’inviteur : Invitation envoyée, puis Rejoint quand l’invité est membre', () => {
    const { rerender } = render(<InviteCard body={body()} isOwn={true} peer="pk_ami" />);
    expect(screen.getByText('Invitation envoyée')).toBeInTheDocument();
    expect(screen.queryByRole('button')).not.toBeInTheDocument();

    useGroups.setState({ ids: ['g1'], states: { g1: etatGroupe(['pk_ami']) } });
    rerender(<InviteCard body={body()} isOwn={true} peer="pk_ami" />);
    expect(screen.getByText('Rejoint')).toBeInTheDocument();
    expect(screen.queryByRole('button')).not.toBeInTheDocument();
  });

  it('ni en attente ni membre : invitation obsolète, aucun bouton', () => {
    render(<InviteCard body={body()} isOwn={false} peer="pk_bob" />);
    expect(screen.getByText('Invitation expirée ou déjà traitée')).toBeInTheDocument();
    expect(screen.queryByRole('button')).not.toBeInTheDocument();
  });
});
