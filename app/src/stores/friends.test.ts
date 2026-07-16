/**
 * Tests de la logique du store contacts : nom affichable et hash d'avatar
 * d'un pair, application d'un profil annoncé (`event.profile`), marquage
 * lu d'une conversation (`dm.mark_read` puis rechargement de la liste),
 * retrait d'ami, présence riche et statut de présence local.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Mock } from 'vitest';

vi.mock('../lib/client', () => ({
  api: {
    dmMarkRead: vi.fn(),
    friendsList: vi.fn(),
    friendsRemove: vi.fn(),
    friendsGetStatus: vi.fn(),
    friendsSetStatus: vi.fn(),
  },
}));

import { api } from '../lib/client';
import type { Contact } from '../lib/api';
import {
  avatarDecorationOf,
  avatarOf,
  displayNameOf,
  handleFriendsNodeEvent,
  presenceOf,
  totalDmMentions,
  totalDmUnread,
  useFriends,
} from './friends';

const dmMarkReadMock = api.dmMarkRead as unknown as Mock;
const friendsListMock = api.friendsList as unknown as Mock;
const friendsRemoveMock = api.friendsRemove as unknown as Mock;
const friendsGetStatusMock = api.friendsGetStatus as unknown as Mock;
const friendsSetStatusMock = api.friendsSetStatus as unknown as Mock;

beforeEach(() => {
  dmMarkReadMock.mockReset();
  friendsListMock.mockReset();
  friendsRemoveMock.mockReset();
  friendsGetStatusMock.mockReset();
  friendsSetStatusMock.mockReset();
});

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

describe('displayNameOf', () => {
  it('rend le nom affiché du contact connu', () => {
    const contacts = [contact('aabbccddee', 'Alice')];
    expect(displayNameOf(contacts, 'aabbccddee')).toBe('Alice');
  });

  it('replie sur l’identifiant court pour un pair inconnu', () => {
    expect(displayNameOf([], 'aabbccddee')).toBe('aabbcc');
  });

  it('replie sur l’identifiant court si le nom affiché est vide', () => {
    const contacts = [contact('aabbccddee', '   ')];
    expect(displayNameOf(contacts, 'aabbccddee')).toBe('aabbcc');
  });
});

describe('avatarOf', () => {
  it('rend le hash d’avatar du contact connu', () => {
    const alice = { ...contact('aabbccddee', 'Alice'), avatar: 'cafe'.repeat(16) };
    expect(avatarOf([alice], 'aabbccddee')).toBe('cafe'.repeat(16));
  });

  it('rend null pour un pair inconnu ou sans avatar', () => {
    expect(avatarOf([], 'aabbccddee')).toBeNull();
    expect(avatarOf([contact('aabbccddee', 'Alice')], 'aabbccddee')).toBeNull();
  });
});

describe('avatarDecorationOf', () => {
  it('rend la décoration connue du contact', () => {
    const alice = {
      ...contact('aabbccddee', 'Alice'),
      avatar_decoration: 'neon_ring',
    };

    expect(avatarDecorationOf([alice], 'aabbccddee')).toBe('neon_ring');
  });

  it('rend null pour un pair inconnu ou une décoration absente ou nulle', () => {
    const absent = contact('aabbccddee', 'Alice');
    const nulle = { ...contact('1122334455', 'Bob'), avatar_decoration: null };

    expect(avatarDecorationOf([], 'inconnu')).toBeNull();
    expect(avatarDecorationOf([absent], 'aabbccddee')).toBeNull();
    expect(avatarDecorationOf([nulle], '1122334455')).toBeNull();
  });
});

describe('useFriends.applyProfile', () => {
  it('applique les ids de décoration, d’effet et de cadre reçus', () => {
    useFriends.setState({ contacts: [contact('alice-pk', 'Alice')] });

    useFriends.getState().applyProfile({
      pubkey: 'alice-pk',
      name: 'Alice',
      bio: null,
      avatar: null,
      banner: null,
      avatar_decoration: 'neon_ring',
      profile_effect: 'aurora',
      profile_frame: 'crystal_crown',
    });

    expect(useFriends.getState().contacts[0]).toMatchObject({
      avatar_decoration: 'neon_ring',
      profile_effect: 'aurora',
      profile_frame: 'crystal_crown',
    });
  });

  it('efface la décoration, l’effet et le cadre quand l’annonce contient null', () => {
    useFriends.setState({
      contacts: [
        {
          ...contact('alice-pk', 'Alice'),
          avatar_decoration: 'neon_ring',
          profile_effect: 'aurora',
          profile_frame: 'crystal_crown',
        },
      ],
    });

    useFriends.getState().applyProfile({
      pubkey: 'alice-pk',
      name: 'Alice',
      bio: null,
      avatar: null,
      banner: null,
      avatar_decoration: null,
      profile_effect: null,
      profile_frame: null,
    });

    expect(useFriends.getState().contacts[0]).toMatchObject({
      avatar_decoration: null,
      profile_effect: null,
      profile_frame: null,
    });
  });

  it('conserve les valeurs connues quand les trois champs sont absents', () => {
    useFriends.setState({
      contacts: [
        {
          ...contact('alice-pk', 'Alice'),
          avatar_decoration: 'neon_ring',
          profile_effect: 'aurora',
          profile_frame: 'crystal_crown',
        },
      ],
    });

    useFriends.getState().applyProfile({
      pubkey: 'alice-pk',
      name: 'Alice',
      bio: null,
      avatar: null,
      banner: null,
    });

    expect(useFriends.getState().contacts[0]).toMatchObject({
      avatar_decoration: 'neon_ring',
      profile_effect: 'aurora',
      profile_frame: 'crystal_crown',
    });
  });

  it('met à jour pseudo, bio et avatar du contact visé (event.profile)', () => {
    useFriends.setState({
      contacts: [contact('alice-pk', 'Alice'), contact('bob-pk', 'Bob')],
    });

    useFriends.getState().applyProfile({
      pubkey: 'alice-pk',
      name: 'Alicia',
      bio: 'salut !',
      avatar: 'ab'.repeat(32),
      banner: null,
    });

    const [alice, bob] = useFriends.getState().contacts;
    expect(alice).toMatchObject({
      display_name: 'Alicia',
      bio: 'salut !',
      avatar: 'ab'.repeat(32),
      banner: null,
    });
    expect(bob).toMatchObject({ display_name: 'Bob', bio: null, avatar: null });
  });

  it('efface bio et avatar quand l’annonce les rend nuls', () => {
    useFriends.setState({
      contacts: [
        { ...contact('alice-pk', 'Alice'), bio: 'ancienne', avatar: 'cd'.repeat(32) },
      ],
    });

    useFriends.getState().applyProfile({
      pubkey: 'alice-pk',
      name: 'Alice',
      bio: null,
      avatar: null,
      banner: null,
    });

    expect(useFriends.getState().contacts[0]).toMatchObject({
      bio: null,
      avatar: null,
      banner: null,
    });
  });

  it('ignore un profil de pair inconnu (le nœud n’annonce que des amis)', () => {
    const contacts = [contact('alice-pk', 'Alice')];
    useFriends.setState({ contacts });

    useFriends.getState().applyProfile({
      pubkey: 'inconnu-pk',
      name: 'Intrus',
      bio: null,
      avatar: null,
      banner: null,
    });

    expect(useFriends.getState().contacts).toEqual(contacts);
  });
});

describe('useFriends.markRead', () => {
  it('enregistre la position de lecture puis recharge la liste', async () => {
    // Arrange : après relecture, le nœud rend le contact sans non-lu.
    const alice = { ...contact('alice-pk', 'Alice'), unread: 0 };
    dmMarkReadMock.mockResolvedValueOnce({ ok: true });
    friendsListMock.mockResolvedValueOnce({ contacts: [alice] });

    // Act
    await useFriends.getState().markRead('alice-pk', 7);

    // Assert
    expect(dmMarkReadMock).toHaveBeenCalledWith('alice-pk', 7);
    expect(useFriends.getState().contacts).toEqual([alice]);
  });

  it('ne recharge pas la liste quand le nœud refuse le marquage', async () => {
    // Arrange
    dmMarkReadMock.mockRejectedValueOnce(new Error('pair inconnu'));

    // Act / Assert
    await expect(useFriends.getState().markRead('alice-pk', 7)).rejects.toThrow();
    expect(friendsListMock).not.toHaveBeenCalled();
  });
});

describe('useFriends.remove', () => {
  it('retire l’amitié puis recharge la liste (le contact disparaît)', async () => {
    // Arrange
    useFriends.setState({ contacts: [contact('alice-pk', 'Alice')] });
    friendsRemoveMock.mockResolvedValueOnce({ ok: true });
    friendsListMock.mockResolvedValueOnce({ contacts: [] });

    // Act
    await useFriends.getState().remove('alice-pk');

    // Assert
    expect(friendsRemoveMock).toHaveBeenCalledWith('alice-pk');
    expect(useFriends.getState().contacts).toEqual([]);
  });

  it('propage le refus du nœud sans recharger la liste', async () => {
    friendsRemoveMock.mockRejectedValueOnce(new Error('contact non ami'));

    await expect(useFriends.getState().remove('alice-pk')).rejects.toThrow();
    expect(friendsListMock).not.toHaveBeenCalled();
  });
});

describe('useFriends.applyPresence', () => {
  it('conserve le statut riche connu sur un appel historique à deux arguments', () => {
    useFriends.setState({
      contacts: [
        { ...contact('alice-pk', 'Alice'), status: 'dnd', status_text: 'focus' },
      ],
    });

    useFriends.getState().applyPresence('alice-pk', true);

    expect(useFriends.getState().contacts[0]).toMatchObject({
      online: true,
      status: 'dnd',
      status_text: 'focus',
    });
  });

  it('applique le statut riche et son texte quand ils sont fournis', () => {
    useFriends.setState({ contacts: [contact('alice-pk', 'Alice')] });

    useFriends.getState().applyPresence('alice-pk', true, 'idle', 'afk');

    expect(useFriends.getState().contacts[0]).toMatchObject({
      online: true,
      status: 'idle',
      status_text: 'afk',
    });
  });
});

describe('handleFriendsNodeEvent', () => {
  it('reflète une présence riche (event.presence)', () => {
    useFriends.setState({ contacts: [contact('alice-pk', 'Alice')] });

    handleFriendsNodeEvent('event.presence', {
      pubkey: 'alice-pk',
      online: true,
      status: 'dnd',
      status_text: 'occupée',
    });

    expect(useFriends.getState().contacts[0]).toMatchObject({
      status: 'dnd',
      status_text: 'occupée',
    });
  });

  it('recharge la liste sur event.friend_removed', () => {
    friendsListMock.mockResolvedValueOnce({ contacts: [] });

    handleFriendsNodeEvent('event.friend_removed', { peer: 'alice-pk' });

    expect(friendsListMock).toHaveBeenCalledTimes(1);
  });

  it('ignore les événements d’autres domaines', () => {
    handleFriendsNodeEvent('event.dm', { peer: 'alice-pk' });
    expect(friendsListMock).not.toHaveBeenCalled();
  });
});

describe('useFriends — statut de présence local', () => {
  it('charge le statut persisté (friends.get_status)', async () => {
    friendsGetStatusMock.mockResolvedValueOnce({ status: 'dnd', custom: 'focus' });

    await useFriends.getState().loadOwnStatus();

    expect(useFriends.getState().ownStatus).toBe('dnd');
    expect(useFriends.getState().ownStatusText).toBe('focus');
  });

  it('fixe le statut : texte inchangé sans `custom`, effacé si vide', async () => {
    friendsSetStatusMock.mockResolvedValue({ ok: true });
    useFriends.setState({ ownStatus: 'online', ownStatusText: 'salut' });

    await useFriends.getState().setOwnStatus('idle');
    expect(friendsSetStatusMock).toHaveBeenCalledWith('idle', undefined);
    expect(useFriends.getState()).toMatchObject({
      ownStatus: 'idle',
      ownStatusText: 'salut',
    });

    await useFriends.getState().setOwnStatus('invisible', '');
    expect(useFriends.getState()).toMatchObject({
      ownStatus: 'invisible',
      ownStatusText: null,
    });
  });

  it('n’applique rien localement quand le nœud refuse', async () => {
    friendsSetStatusMock.mockRejectedValueOnce(new Error('statut inconnu'));
    useFriends.setState({ ownStatus: 'online', ownStatusText: null });

    await expect(useFriends.getState().setOwnStatus('dnd', 'x')).rejects.toThrow();
    expect(useFriends.getState().ownStatus).toBe('online');
  });
});

describe('presenceOf', () => {
  it('préfère le statut riche annoncé, sinon replie sur la joignabilité', () => {
    expect(presenceOf({ ...contact('a', 'A'), status: 'idle' })).toBe('idle');
    expect(presenceOf({ ...contact('a', 'A'), online: true })).toBe('online');
    expect(presenceOf({ ...contact('a', 'A'), online: false })).toBe('offline');
    expect(presenceOf(undefined)).toBe('offline');
  });
});

describe('totalDmUnread / totalDmMentions', () => {
  it('additionne les non-lus et mentions des seuls amis établis', () => {
    const contacts: Contact[] = [
      { ...contact('alice-pk', 'Alice'), unread: 3, mention_count: 1 },
      { ...contact('bob-pk', 'Bob'), unread: 2, mention_count: 0 },
      {
        ...contact('carol-pk', 'Carol'),
        state: 'pending_in',
        unread: 9,
        mention_count: 9,
      },
    ];

    expect(totalDmUnread(contacts)).toBe(5);
    expect(totalDmMentions(contacts)).toBe(1);
  });

  it('rend zéro sans contact ou sans compteur connu', () => {
    expect(totalDmUnread([])).toBe(0);
    expect(totalDmMentions([contact('alice-pk', 'Alice')])).toBe(0);
  });
});
