/**
 * Tests du store des groupes : aides pures (permissions, couleur de rôle,
 * tris par position, regroupement par catégorie), rechargement sur
 * `event.group_state` (handleGroupState), épinglés et actions de message.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Mock } from 'vitest';

vi.mock('../lib/client', () => ({
  rpc: { call: vi.fn() },
  api: {
    groupsList: vi.fn(),
    groupsMarkRead: vi.fn(),
    groupsState: vi.fn(),
    groupsRename: vi.fn(),
    groupsLeave: vi.fn(),
    groupsChannelAdd: vi.fn(),
    groupsChannelEdit: vi.fn(),
    groupsChannelPerms: vi.fn(),
    groupsCategoryEdit: vi.fn(),
    groupsCategoryDel: vi.fn(),
    groupsRoleEdit: vi.fn(),
    groupsPins: vi.fn(),
    groupsPin: vi.fn(),
    groupsUnpin: vi.fn(),
    groupsHistoryAround: vi.fn(),
    groupsEdit: vi.fn(),
    groupsDelete: vi.fn(),
    groupsReact: vi.fn(),
    groupsSend: vi.fn(),
    groupsEmojiAdd: vi.fn(),
    groupsEmojiDel: vi.fn(),
    groupsTimeout: vi.fn(),
    groupsTimeoutClear: vi.fn(),
    groupsVoiceModerate: vi.fn(),
    groupsSetNickname: vi.fn(),
    groupsInviteCreate: vi.fn(),
    groupsInvitesList: vi.fn(),
    groupsInviteAccept: vi.fn(),
    groupsInviteDecline: vi.fn(),
  },
}));

import { api, rpc } from '../lib/client';
import type { GroupMessage, GroupRole, GroupStateJson, PendingInvite } from '../lib/api';
import {
  useGroups,
  canModerateVoice,
  channelKey,
  channelsByCategory,
  handleMentionNodeEvent,
  hasPerm,
  highestRolePosition,
  isChannelReadOnly,
  isChannelRestricted,
  isChannelVisible,
  memberColor,
  myChannelPermissions,
  nicknameOf,
  overrideOf,
  planRoleMove,
  roleColorCss,
  sortCategories,
  sortChannels,
  sortRoles,
  timeoutUntil,
  PERMISSIONS,
} from './groups';

const listMock = api.groupsList as unknown as Mock;
const channelEditMock = api.groupsChannelEdit as unknown as Mock;
const channelPermsMock = api.groupsChannelPerms as unknown as Mock;
const categoryEditMock = api.groupsCategoryEdit as unknown as Mock;
const categoryDelMock = api.groupsCategoryDel as unknown as Mock;
const roleEditMock = api.groupsRoleEdit as unknown as Mock;
const markReadMock = api.groupsMarkRead as unknown as Mock;
const stateMock = api.groupsState as unknown as Mock;
const renameMock = api.groupsRename as unknown as Mock;
const leaveMock = api.groupsLeave as unknown as Mock;
const channelAddMock = api.groupsChannelAdd as unknown as Mock;
const pinsMock = api.groupsPins as unknown as Mock;
const pinMock = api.groupsPin as unknown as Mock;
const unpinMock = api.groupsUnpin as unknown as Mock;
const editMock = api.groupsEdit as unknown as Mock;
const deleteMock = api.groupsDelete as unknown as Mock;
const reactMock = api.groupsReact as unknown as Mock;
const sendMock = api.groupsSend as unknown as Mock;
const emojiAddMock = api.groupsEmojiAdd as unknown as Mock;
const emojiDelMock = api.groupsEmojiDel as unknown as Mock;
const historyAroundMock = api.groupsHistoryAround as unknown as Mock;
const timeoutMock = api.groupsTimeout as unknown as Mock;
const timeoutClearMock = api.groupsTimeoutClear as unknown as Mock;
const voiceModerateMock = api.groupsVoiceModerate as unknown as Mock;
const nicknameMock = api.groupsSetNickname as unknown as Mock;
const inviteCreateMock = api.groupsInviteCreate as unknown as Mock;
const invitesListMock = api.groupsInvitesList as unknown as Mock;
const inviteAcceptMock = api.groupsInviteAccept as unknown as Mock;
const inviteDeclineMock = api.groupsInviteDecline as unknown as Mock;
const callMock = rpc.call as unknown as Mock;

function role(id: string, position: number, color = 0): GroupRole {
  return { role_id: id, name: `rôle-${id}`, color, position, permissions: 0 };
}

function groupState(over: Partial<GroupStateJson> = {}): GroupStateJson {
  return {
    group_id: 'g1',
    name: 'Guilde',
    icon: null,
    founder: 'fondateur',
    members: [{ pubkey: 'moi', roles: [] }],
    bans: [],
    channels: [],
    categories: [],
    roles: [],
    invites: [],
    my_permissions: 0x3,
    ...over,
  };
}

function groupMsg(id: string, lamport: number): GroupMessage {
  return {
    msg_id: id,
    channel_id: 'c1',
    author: 'pair',
    lamport,
    sent_ms: lamport * 1000,
    deleted: false,
    body: { type: 'text', text: `message ${id}`, reply_to: null, attachments: 0 },
    edited: null,
  };
}

beforeEach(() => {
  useGroups.setState({
    ids: [],
    states: {},
    messages: {},
    hasMore: {},
    loadingOlder: {},
    pins: {},
    unread: {},
    mentions: {},
    pendingInvites: [],
  });
  for (const mock of [
    listMock,
    markReadMock,
    stateMock,
    renameMock,
    leaveMock,
    channelAddMock,
    pinsMock,
    pinMock,
    unpinMock,
    editMock,
    deleteMock,
    reactMock,
    sendMock,
    emojiAddMock,
    emojiDelMock,
    channelEditMock,
    channelPermsMock,
    categoryEditMock,
    categoryDelMock,
    roleEditMock,
    historyAroundMock,
    timeoutMock,
    timeoutClearMock,
    voiceModerateMock,
    nicknameMock,
    inviteCreateMock,
    invitesListMock,
    inviteAcceptMock,
    inviteDeclineMock,
    callMock,
  ]) {
    mock.mockReset();
  }
  // Défaut sûr : `loadList` appelle toujours `invites_list` en parallèle des
  // états de groupe — la plupart des tests ne portent pas sur les invitations.
  invitesListMock.mockResolvedValue({ invites: [] });
});

describe('hasPerm', () => {
  it('vérifie un bit simple', () => {
    expect(hasPerm(PERMISSIONS.VIEW | PERMISSIONS.SEND, PERMISSIONS.SEND)).toBe(true);
    expect(hasPerm(PERMISSIONS.VIEW, PERMISSIONS.KICK)).toBe(false);
  });

  it('ADMIN implique toutes les permissions', () => {
    expect(hasPerm(PERMISSIONS.ADMIN, PERMISSIONS.BAN)).toBe(true);
    expect(hasPerm(PERMISSIONS.ADMIN, PERMISSIONS.MANAGE_ROLES)).toBe(true);
  });

  it('le bitfield du fondateur (0x1FF) accorde tout', () => {
    for (const bit of Object.values(PERMISSIONS)) {
      expect(hasPerm(0x1ff, bit)).toBe(true);
    }
  });

  it('un masque nul n’accorde rien', () => {
    expect(hasPerm(0, PERMISSIONS.VIEW)).toBe(false);
  });
});

describe('roleColorCss', () => {
  it('convertit l’entier RGB en couleur CSS, zéros de tête compris', () => {
    expect(roleColorCss(0xff0000)).toBe('#ff0000');
    expect(roleColorCss(0x00040f)).toBe('#00040f');
  });
});

describe('memberColor', () => {
  const roles = [
    role('bas', 1, 0x00ff00),
    role('haut', 9, 0),
    role('milieu', 5, 0xff0000),
  ];

  it('prend la couleur du rôle coloré de position la plus haute', () => {
    const member = { pubkey: 'moi', roles: ['bas', 'haut', 'milieu'] };
    // `haut` est plus haut mais sans couleur (0) : c'est `milieu` qui gagne.
    expect(memberColor(member, roles)).toBe('#ff0000');
  });

  it('rend null sans rôle coloré ou pour un membre inconnu', () => {
    expect(memberColor({ pubkey: 'moi', roles: ['haut'] }, roles)).toBeNull();
    expect(memberColor(undefined, roles)).toBeNull();
    expect(memberColor({ pubkey: 'moi', roles: [] }, roles)).toBeNull();
  });
});

describe('highestRolePosition', () => {
  it('rend la position du rôle le plus haut, −1 sans rôle', () => {
    const roles = [role('a', 2), role('b', 7), role('c', 4)];
    expect(highestRolePosition({ pubkey: 'x', roles: ['a', 'c'] }, roles)).toBe(4);
    expect(highestRolePosition({ pubkey: 'x', roles: [] }, roles)).toBe(-1);
    expect(highestRolePosition(undefined, roles)).toBe(-1);
  });
});

describe('canModerateVoice', () => {
  const withKick = { my_permissions: PERMISSIONS.KICK, founder: 'fondateur' };
  const withoutKick = { my_permissions: PERMISSIONS.VIEW, founder: 'fondateur' };
  const admin = { my_permissions: PERMISSIONS.ADMIN, founder: 'fondateur' };

  it('autorise avec la permission KICK, cible ni soi ni le fondateur', () => {
    expect(canModerateVoice(withKick, 'moi', 'autre')).toBe(true);
  });

  it('ADMIN implique KICK', () => {
    expect(canModerateVoice(admin, 'moi', 'autre')).toBe(true);
  });

  it('refuse sans la permission KICK', () => {
    expect(canModerateVoice(withoutKick, 'moi', 'autre')).toBe(false);
  });

  it('refuse sur soi-même', () => {
    expect(canModerateVoice(withKick, 'moi', 'moi')).toBe(false);
  });

  it('refuse sur le fondateur (intouchable)', () => {
    expect(canModerateVoice(withKick, 'moi', 'fondateur')).toBe(false);
  });

  it('refuse sans identité locale connue', () => {
    expect(canModerateVoice(withKick, null, 'autre')).toBe(false);
  });
});

describe('tris par position', () => {
  it('trie salons et catégories par position croissante', () => {
    const channels = [
      {
        channel_id: 'b',
        name: 'b',
        kind: 'text' as const,
        category: null,
        position: 2,
        topic: '',
      },
      {
        channel_id: 'a',
        name: 'a',
        kind: 'text' as const,
        category: null,
        position: 1,
        topic: '',
      },
    ];
    expect(sortChannels(channels).map((c) => c.channel_id)).toEqual(['a', 'b']);

    const categories = [
      { category_id: 'y', name: 'y', position: 3 },
      { category_id: 'x', name: 'x', position: 0 },
    ];
    expect(sortCategories(categories).map((c) => c.category_id)).toEqual(['x', 'y']);
  });

  it('trie les rôles du plus haut au plus bas', () => {
    expect(
      sortRoles([role('a', 1), role('b', 9), role('c', 5)]).map((r) => r.role_id),
    ).toEqual(['b', 'c', 'a']);
  });
});

describe('channelsByCategory', () => {
  const categories = [
    { category_id: 'c2', name: 'Deux', position: 2 },
    { category_id: 'c1', name: 'Un', position: 1 },
  ];
  const channels = [
    {
      channel_id: 'k4',
      name: 'd',
      kind: 'text' as const,
      category: 'c1',
      position: 0,
      topic: '',
    },
    {
      channel_id: 'k1',
      name: 'a',
      kind: 'text' as const,
      category: null,
      position: 1,
      topic: '',
    },
    {
      channel_id: 'k2',
      name: 'b',
      kind: 'voice' as const,
      category: null,
      position: 0,
      topic: '',
    },
    {
      channel_id: 'k3',
      name: 'c',
      kind: 'text' as const,
      category: 'c2',
      position: 0,
      topic: '',
    },
    // Catégorie disparue : rattaché aux sans-catégorie.
    {
      channel_id: 'k5',
      name: 'e',
      kind: 'text' as const,
      category: 'fantôme',
      position: 2,
      topic: '',
    },
  ];

  it('place les sans-catégorie d’abord puis les catégories par position', () => {
    const groups = channelsByCategory(channels, categories);
    expect(groups.map((g) => g.category?.category_id ?? null)).toEqual([
      null,
      'c1',
      'c2',
    ]);
    expect(groups[0]?.channels.map((c) => c.channel_id)).toEqual(['k2', 'k1', 'k5']);
    expect(groups[1]?.channels.map((c) => c.channel_id)).toEqual(['k4']);
    expect(groups[2]?.channels.map((c) => c.channel_id)).toEqual(['k3']);
  });
});

describe('useGroups.handleGroupState', () => {
  it('recharge groups.state (refetch sur event.group_state)', async () => {
    const fresh = groupState({ name: 'Renommée' });
    stateMock.mockResolvedValueOnce(fresh);

    await useGroups.getState().handleGroupState('g1');

    expect(stateMock).toHaveBeenCalledWith('g1');
    expect(useGroups.getState().states['g1']).toEqual(fresh);
  });

  it('recharge aussi les épinglés déjà consultés du groupe', async () => {
    useGroups.setState({ pins: { [channelKey('g1', 'c1')]: ['m1'] } });
    stateMock.mockResolvedValueOnce(groupState());
    pinsMock.mockResolvedValueOnce({ msg_ids: ['m1', 'm2'] });

    await useGroups.getState().handleGroupState('g1');

    expect(pinsMock).toHaveBeenCalledWith('g1', 'c1');
    expect(useGroups.getState().pins[channelKey('g1', 'c1')]).toEqual(['m1', 'm2']);
  });

  it('repart de groups.list quand l’état n’est plus accessible', async () => {
    stateMock.mockRejectedValueOnce(new Error('refusé : non membre'));
    listMock.mockResolvedValueOnce({ groups: [] });

    await useGroups.getState().handleGroupState('g1');

    expect(listMock).toHaveBeenCalledTimes(1);
    expect(useGroups.getState().ids).toEqual([]);
  });
});

describe('useGroups — actions de gestion', () => {
  it('rename appelle le nœud puis recharge l’état', async () => {
    renameMock.mockResolvedValueOnce({ ok: true });
    stateMock.mockResolvedValueOnce(groupState({ name: 'Après' }));

    await useGroups.getState().rename('g1', 'Après');

    expect(renameMock).toHaveBeenCalledWith('g1', 'Après');
    expect(useGroups.getState().states['g1']?.name).toBe('Après');
  });

  it('addChannel transmet le genre et la catégorie', async () => {
    channelAddMock.mockResolvedValueOnce({ channel_id: 'nouveau' });
    stateMock.mockResolvedValueOnce(groupState());

    const id = await useGroups.getState().addChannel('g1', 'blabla', 'voice', 'cat');

    expect(channelAddMock).toHaveBeenCalledWith('g1', 'blabla', 'voice', 'cat');
    expect(id).toBe('nouveau');
  });

  it('leave efface le groupe localement (liste, état, fils, épinglés)', async () => {
    const key = channelKey('g1', 'c1');
    useGroups.setState({
      ids: ['g1', 'g2'],
      states: { g1: groupState(), g2: groupState({ group_id: 'g2' }) },
      messages: { [key]: [groupMsg('m1', 1)], 'g2/c9': [groupMsg('m2', 2)] },
      pins: { [key]: ['m1'] },
    });
    leaveMock.mockResolvedValueOnce({ ok: true });

    await useGroups.getState().leave('g1');

    const s = useGroups.getState();
    expect(leaveMock).toHaveBeenCalledWith('g1');
    expect(s.ids).toEqual(['g2']);
    expect(s.states['g1']).toBeUndefined();
    expect(s.messages[key]).toBeUndefined();
    expect(s.messages['g2/c9']).toHaveLength(1);
    expect(s.pins[key]).toBeUndefined();
  });

  it('ne touche à rien localement quand le nœud refuse le départ', async () => {
    useGroups.setState({ ids: ['g1'], states: { g1: groupState() } });
    leaveMock.mockRejectedValueOnce(new Error('refusé : fondateur'));

    await expect(useGroups.getState().leave('g1')).rejects.toThrow();

    expect(useGroups.getState().ids).toEqual(['g1']);
  });
});

describe('useGroups — épinglés', () => {
  it('togglePin épingle puis recharge la liste', async () => {
    pinMock.mockResolvedValueOnce({ ok: true });
    pinsMock.mockResolvedValueOnce({ msg_ids: ['m1'] });

    await useGroups.getState().togglePin('g1', 'c1', 'm1', false);

    expect(pinMock).toHaveBeenCalledWith('g1', 'c1', 'm1');
    expect(unpinMock).not.toHaveBeenCalled();
    expect(useGroups.getState().pins[channelKey('g1', 'c1')]).toEqual(['m1']);
  });

  it('togglePin désépingle un message déjà épinglé', async () => {
    unpinMock.mockResolvedValueOnce({ ok: true });
    pinsMock.mockResolvedValueOnce({ msg_ids: [] });

    await useGroups.getState().togglePin('g1', 'c1', 'm1', true);

    expect(unpinMock).toHaveBeenCalledWith('g1', 'c1', 'm1');
    expect(pinMock).not.toHaveBeenCalled();
  });
});

describe('useGroups — actions de message', () => {
  const key = channelKey('g1', 'c1');

  it('editMessage reflète le nouveau texte localement', async () => {
    useGroups.setState({ messages: { [key]: [groupMsg('a', 1), groupMsg('b', 2)] } });
    editMock.mockResolvedValueOnce({ ok: true });

    await useGroups.getState().editMessage('g1', 'c1', 'a', 'corrigé');

    expect(editMock).toHaveBeenCalledWith('g1', 'c1', 'a', 'corrigé');
    expect(useGroups.getState().messages[key]?.[0]?.edited).toBe('corrigé');
    expect(useGroups.getState().messages[key]?.[1]?.edited).toBeNull();
  });

  it('deleteMessage pose le tombstone localement', async () => {
    useGroups.setState({ messages: { [key]: [groupMsg('a', 1)] } });
    deleteMock.mockResolvedValueOnce({ ok: true });

    await useGroups.getState().deleteMessage('g1', 'c1', 'a');

    expect(deleteMock).toHaveBeenCalledWith('g1', 'c1', 'a');
    expect(useGroups.getState().messages[key]?.[0]?.deleted).toBe(true);
  });

  it('toggleReaction ajoute sa réaction absente (add: true)', async () => {
    useGroups.setState({
      messages: {
        [key]: [{ ...groupMsg('a', 1), reactions: [{ emoji: '👍', author: 'pair' }] }],
      },
    });
    reactMock.mockResolvedValueOnce({ ok: true });

    await useGroups.getState().toggleReaction('g1', 'c1', 'a', '👍', 'moi');

    expect(reactMock).toHaveBeenCalledWith('g1', 'c1', 'a', '👍', true);
    expect(useGroups.getState().messages[key]?.[0]?.reactions).toEqual([
      { emoji: '👍', author: 'pair' },
      { emoji: '👍', author: 'moi' },
    ]);
  });

  it('toggleReaction retire sa réaction déjà posée (add: false)', async () => {
    useGroups.setState({
      messages: {
        [key]: [{ ...groupMsg('a', 1), reactions: [{ emoji: '👍', author: 'moi' }] }],
      },
    });
    reactMock.mockResolvedValueOnce({ ok: true });

    await useGroups.getState().toggleReaction('g1', 'c1', 'a', '👍', 'moi');

    expect(reactMock).toHaveBeenCalledWith('g1', 'c1', 'a', '👍', false);
    expect(useGroups.getState().messages[key]?.[0]?.reactions).toEqual([]);
  });

  it('toggleReaction ignore un message inconnu localement', async () => {
    await useGroups.getState().toggleReaction('g1', 'c1', 'fantôme', '👍', 'moi');

    expect(reactMock).not.toHaveBeenCalled();
  });
});

describe('useGroups — envoi et réponse en salon', () => {
  it('send transmet reply_to au nœud puis rafraîchit le fil', async () => {
    sendMock.mockResolvedValueOnce({ msg_id: 'x' });
    callMock.mockResolvedValueOnce({ messages: [groupMsg('x', 5)] });

    await useGroups.getState().send('g1', 'c1', 'coucou', 'orig');

    expect(sendMock).toHaveBeenCalledWith('g1', 'c1', 'coucou', 'orig', undefined);
    expect(useGroups.getState().messages[channelKey('g1', 'c1')]).toHaveLength(1);
  });

  it('send sans réponse passe reply_to indéfini', async () => {
    sendMock.mockResolvedValueOnce({ msg_id: 'y' });
    callMock.mockResolvedValueOnce({ messages: [] });

    await useGroups.getState().send('g1', 'c1', 'salut');

    expect(sendMock).toHaveBeenCalledWith('g1', 'c1', 'salut', undefined, undefined);
  });
});

describe('useGroups — émojis de serveur', () => {
  it('MANAGE_EMOJIS vaut 0x200 et est impliqué par ADMIN', () => {
    expect(PERMISSIONS.MANAGE_EMOJIS).toBe(0x200);
    expect(hasPerm(PERMISSIONS.ADMIN, PERMISSIONS.MANAGE_EMOJIS)).toBe(true);
    expect(hasPerm(PERMISSIONS.SEND, PERMISSIONS.MANAGE_EMOJIS)).toBe(false);
  });

  it('addEmoji publie l’émoji puis recharge l’état', async () => {
    emojiAddMock.mockResolvedValueOnce({ merkle_root: 'ab'.repeat(32) });
    stateMock.mockResolvedValueOnce(groupState());

    await useGroups.getState().addEmoji('g1', 'parrot', 'QUJD', 'image/png');

    expect(emojiAddMock).toHaveBeenCalledWith('g1', 'parrot', 'QUJD', 'image/png');
    expect(stateMock).toHaveBeenCalledWith('g1');
  });

  it('delEmoji supprime puis recharge l’état', async () => {
    emojiDelMock.mockResolvedValueOnce({ ok: true });
    stateMock.mockResolvedValueOnce(groupState());

    await useGroups.getState().delEmoji('g1', 'parrot');

    expect(emojiDelMock).toHaveBeenCalledWith('g1', 'parrot');
    expect(stateMock).toHaveBeenCalledWith('g1');
  });
});

describe('useGroups — non-lus', () => {
  it('mémorise les compteurs de groups.list au chargement', async () => {
    // Arrange
    listMock.mockResolvedValueOnce({ groups: ['g1'], unread: { g1: { c1: 2 } } });
    stateMock.mockResolvedValueOnce(groupState());

    // Act
    await useGroups.getState().loadList();

    // Assert
    expect(useGroups.getState().unread).toEqual({ g1: { c1: 2 } });
  });

  it('replie sur aucun non-lu quand groups.list omet le champ', async () => {
    // Arrange
    useGroups.setState({ unread: { g1: { c1: 2 } } });
    listMock.mockResolvedValueOnce({ groups: [] });

    // Act
    await useGroups.getState().loadList();

    // Assert
    expect(useGroups.getState().unread).toEqual({});
  });

  it('markRead enregistre la position puis rafraîchit les compteurs', async () => {
    // Arrange
    useGroups.setState({ unread: { g1: { c1: 4 } } });
    markReadMock.mockResolvedValueOnce({ ok: true });
    listMock.mockResolvedValueOnce({ groups: ['g1'], unread: {} });

    // Act
    await useGroups.getState().markRead('g1', 'c1', 9);

    // Assert
    expect(markReadMock).toHaveBeenCalledWith('g1', 'c1', 9);
    expect(useGroups.getState().unread).toEqual({});
  });

  it('refreshUnread ne recharge que les compteurs (pas les états)', async () => {
    // Arrange
    listMock.mockResolvedValueOnce({ groups: ['g1'], unread: { g1: { c2: 1 } } });

    // Act
    await useGroups.getState().refreshUnread();

    // Assert
    expect(useGroups.getState().unread).toEqual({ g1: { c2: 1 } });
    expect(stateMock).not.toHaveBeenCalled();
  });
});

describe('useGroups — mentions', () => {
  it('mémorise les mentions par groupe au chargement', async () => {
    listMock.mockResolvedValueOnce({ groups: ['g1'], unread: {}, mentions: { g1: 3 } });
    stateMock.mockResolvedValueOnce(groupState());

    await useGroups.getState().loadList();

    expect(useGroups.getState().mentions).toEqual({ g1: 3 });
  });

  it('replie sur aucune mention quand le champ est omis', async () => {
    useGroups.setState({ mentions: { g1: 2 } });
    listMock.mockResolvedValueOnce({ groups: [] });

    await useGroups.getState().loadList();

    expect(useGroups.getState().mentions).toEqual({});
  });

  it('refreshUnread met aussi à jour les mentions', async () => {
    listMock.mockResolvedValueOnce({ groups: ['g1'], unread: {}, mentions: { g1: 4 } });

    await useGroups.getState().refreshUnread();

    expect(useGroups.getState().mentions).toEqual({ g1: 4 });
  });

  it('event.mention rafraîchit les compteurs de mentions', async () => {
    listMock.mockResolvedValue({ groups: [], unread: {}, mentions: { g9: 5 } });

    handleMentionNodeEvent('event.mention');

    await vi.waitFor(() => expect(useGroups.getState().mentions).toEqual({ g9: 5 }));
  });

  it('ignore les événements autres que event.mention', () => {
    handleMentionNodeEvent('event.dm');

    expect(listMock).not.toHaveBeenCalled();
  });
});

describe('useGroups — invitations en attente (D-045)', () => {
  function invite(over: Partial<PendingInvite> = {}): PendingInvite {
    return {
      group_id: 'g1',
      invite_id: 'i1',
      group_name: 'Guilde',
      inviter: 'alice-pk',
      expires_ms: 9999,
      ...over,
    };
  }

  it('handleInvitePending ajoute une invitation reçue (event.group_invite_pending)', () => {
    useGroups.getState().handleInvitePending(invite());

    expect(useGroups.getState().pendingInvites).toEqual([invite()]);
  });

  it('handleInvitePending déduplique par (group_id, invite_id)', () => {
    useGroups.setState({ pendingInvites: [invite()] });

    useGroups
      .getState()
      .handleInvitePending(invite({ group_name: 'Renommée entre-temps' }));

    expect(useGroups.getState().pendingInvites).toHaveLength(1);
    expect(useGroups.getState().pendingInvites[0]?.group_name).toBe('Guilde');
  });

  it('loadPendingInvites remplace la liste depuis le nœud', async () => {
    invitesListMock.mockResolvedValueOnce({ invites: [invite()] });

    await useGroups.getState().loadPendingInvites();

    expect(useGroups.getState().pendingInvites).toEqual([invite()]);
  });

  it('acceptInvite retire l’invitation localement puis appelle le nœud', async () => {
    useGroups.setState({ pendingInvites: [invite()] });
    inviteAcceptMock.mockResolvedValueOnce({ ok: true });

    await useGroups.getState().acceptInvite('g1', 'i1');

    expect(inviteAcceptMock).toHaveBeenCalledWith('g1', 'i1');
    expect(useGroups.getState().pendingInvites).toEqual([]);
  });

  it('acceptInvite restaure l’invitation et propage l’erreur si le nœud refuse', async () => {
    useGroups.setState({ pendingInvites: [invite()] });
    inviteAcceptMock.mockRejectedValueOnce(new Error('refusé'));

    await expect(useGroups.getState().acceptInvite('g1', 'i1')).rejects.toThrow();

    expect(useGroups.getState().pendingInvites).toEqual([invite()]);
  });

  it('declineInvite retire l’invitation localement puis appelle le nœud', async () => {
    useGroups.setState({ pendingInvites: [invite()] });
    inviteDeclineMock.mockResolvedValueOnce({ ok: true });

    await useGroups.getState().declineInvite('g1', 'i1');

    expect(inviteDeclineMock).toHaveBeenCalledWith('g1', 'i1');
    expect(useGroups.getState().pendingInvites).toEqual([]);
  });

  it('declineInvite restaure l’invitation et propage l’erreur si le nœud refuse', async () => {
    useGroups.setState({ pendingInvites: [invite()] });
    inviteDeclineMock.mockRejectedValueOnce(new Error('refusé'));

    await expect(useGroups.getState().declineInvite('g1', 'i1')).rejects.toThrow();

    expect(useGroups.getState().pendingInvites).toEqual([invite()]);
  });

  it('invite (créer, D-045) appelle invite_create puis recharge l’état — plus de force-join', async () => {
    inviteCreateMock.mockResolvedValueOnce({ invite_id: 'i2' });
    stateMock.mockResolvedValueOnce(groupState());

    await useGroups.getState().invite('g1', 'bob-pk');

    expect(inviteCreateMock).toHaveBeenCalledWith('g1', 'bob-pk');
    expect(stateMock).toHaveBeenCalledWith('g1');
  });
});

describe('planRoleMove', () => {
  const roles = [role('haut', 9), role('milieu', 5), role('bas', 1)];

  it('échange les positions avec le voisin du dessus', () => {
    expect(planRoleMove(roles, 'milieu', 'up')).toEqual([
      { role_id: 'milieu', position: 9 },
      { role_id: 'haut', position: 5 },
    ]);
  });

  it('échange les positions avec le voisin du dessous', () => {
    expect(planRoleMove(roles, 'milieu', 'down')).toEqual([
      { role_id: 'milieu', position: 1 },
      { role_id: 'bas', position: 5 },
    ]);
  });

  it('rend [] sans voisin (extrémités) ou pour un rôle inconnu', () => {
    expect(planRoleMove(roles, 'haut', 'up')).toEqual([]);
    expect(planRoleMove(roles, 'bas', 'down')).toEqual([]);
    expect(planRoleMove(roles, 'fantome', 'up')).toEqual([]);
  });

  it('à égalité de position, élève celui qui doit finir au-dessus', () => {
    // Ordre affiché (départage par id) : a(0) puis b(0).
    const tied = [role('a', 0), role('b', 0)];
    expect(planRoleMove(tied, 'b', 'up')).toEqual([{ role_id: 'b', position: 1 }]);
    expect(planRoleMove(tied, 'a', 'down')).toEqual([{ role_id: 'b', position: 1 }]);
  });
});

describe('overrideOf', () => {
  const state = {
    overrides: [{ channel_id: 'c1', role_id: 'r1', allow: 0x1, deny: 0x2 }],
  };

  it('rend l’override existant du couple (salon, rôle)', () => {
    expect(overrideOf(state, 'c1', 'r1')).toEqual({ allow: 0x1, deny: 0x2 });
  });

  it('rend un override neutre sans entrée ou sans champ overrides', () => {
    expect(overrideOf(state, 'c1', 'r2')).toEqual({ allow: 0, deny: 0 });
    expect(overrideOf({}, 'c1', 'r1')).toEqual({ allow: 0, deny: 0 });
    expect(overrideOf(undefined, 'c1', 'r1')).toEqual({ allow: 0, deny: 0 });
  });
});

describe('nicknameOf', () => {
  it('rend le pseudo de serveur défini', () => {
    const s = groupState({ members: [{ pubkey: 'moi', roles: [], nickname: 'Bibou' }] });
    expect(nicknameOf(s, 'moi')).toBe('Bibou');
  });

  it('rend null quand absent, vide, espaces, membre ou état inconnus', () => {
    const s = groupState({
      members: [
        { pubkey: 'a', roles: [] },
        { pubkey: 'b', roles: [], nickname: '' },
        { pubkey: 'c', roles: [], nickname: '   ' },
      ],
    });
    expect(nicknameOf(s, 'a')).toBeNull();
    expect(nicknameOf(s, 'b')).toBeNull();
    expect(nicknameOf(s, 'c')).toBeNull();
    expect(nicknameOf(s, 'inconnu')).toBeNull();
    expect(nicknameOf(undefined, 'a')).toBeNull();
  });
});

describe('timeoutUntil', () => {
  it('rend l’échéance active, null si passée, nulle ou absente', () => {
    expect(timeoutUntil({ timeout_until_ms: 2000 }, 1000)).toBe(2000);
    expect(timeoutUntil({ timeout_until_ms: 500 }, 1000)).toBeNull();
    expect(timeoutUntil({ timeout_until_ms: 1000 }, 1000)).toBeNull();
    expect(timeoutUntil({ timeout_until_ms: 0 }, 1000)).toBeNull();
    expect(timeoutUntil({}, 1000)).toBeNull();
    expect(timeoutUntil(undefined, 1000)).toBeNull();
  });
});

describe('myChannelPermissions', () => {
  it('rend la base globale sans override applicable', () => {
    const s = groupState({
      my_permissions: 0x3,
      members: [{ pubkey: 'moi', roles: ['r'] }],
    });
    expect(myChannelPermissions(s, 'c1', 'moi')).toBe(0x3);
  });

  it('applique allow puis deny des rôles portés dans ce salon (deny gagne)', () => {
    const s = groupState({
      my_permissions: PERMISSIONS.VIEW | PERMISSIONS.SEND,
      members: [{ pubkey: 'moi', roles: ['r'] }],
      overrides: [
        {
          channel_id: 'c1',
          role_id: 'r',
          allow: PERMISSIONS.MANAGE_CHANNELS,
          deny: PERMISSIONS.SEND,
        },
        { channel_id: 'c2', role_id: 'r', allow: PERMISSIONS.BAN, deny: 0 },
      ],
    });
    const eff = myChannelPermissions(s, 'c1', 'moi');
    expect(hasPerm(eff, PERMISSIONS.MANAGE_CHANNELS)).toBe(true);
    expect(hasPerm(eff, PERMISSIONS.VIEW)).toBe(true);
    expect(hasPerm(eff, PERMISSIONS.SEND)).toBe(false);
    // L'override de l'autre salon (c2) n'est pas appliqué.
    expect(hasPerm(eff, PERMISSIONS.BAN)).toBe(false);
  });

  it('ignore les overrides des rôles non portés', () => {
    const s = groupState({
      my_permissions: PERMISSIONS.VIEW,
      members: [{ pubkey: 'moi', roles: [] }],
      overrides: [
        { channel_id: 'c1', role_id: 'r', allow: PERMISSIONS.MANAGE_CHANNELS, deny: 0 },
      ],
    });
    expect(myChannelPermissions(s, 'c1', 'moi')).toBe(PERMISSIONS.VIEW);
  });

  it('ADMIN court-circuite les overrides (deny ignoré)', () => {
    const s = groupState({
      my_permissions: PERMISSIONS.ADMIN,
      members: [{ pubkey: 'moi', roles: ['r'] }],
      overrides: [
        { channel_id: 'c1', role_id: 'r', allow: 0, deny: PERMISSIONS.MANAGE_CHANNELS },
      ],
    });
    expect(myChannelPermissions(s, 'c1', 'moi')).toBe(PERMISSIONS.ADMIN);
  });
});

describe('isChannelReadOnly', () => {
  it('faux hors salon d’annonces', () => {
    const s = groupState({
      my_permissions: PERMISSIONS.VIEW | PERMISSIONS.SEND,
      members: [{ pubkey: 'moi', roles: [] }],
    });
    expect(isChannelReadOnly(s, { channel_id: 'c1', kind: 'text' }, 'moi')).toBe(false);
  });

  it('vrai en salon d’annonces sans MANAGE_CHANNELS effectif', () => {
    const s = groupState({
      my_permissions: PERMISSIONS.VIEW | PERMISSIONS.SEND,
      members: [{ pubkey: 'moi', roles: [] }],
    });
    expect(isChannelReadOnly(s, { channel_id: 'c1', kind: 'announcement' }, 'moi')).toBe(
      true,
    );
  });

  it('faux en annonces avec MANAGE_CHANNELS global ou via override', () => {
    const global = groupState({
      my_permissions: PERMISSIONS.MANAGE_CHANNELS,
      members: [{ pubkey: 'moi', roles: [] }],
    });
    expect(
      isChannelReadOnly(global, { channel_id: 'c1', kind: 'announcement' }, 'moi'),
    ).toBe(false);

    const viaOverride = groupState({
      my_permissions: PERMISSIONS.VIEW,
      members: [{ pubkey: 'moi', roles: ['r'] }],
      overrides: [
        { channel_id: 'c1', role_id: 'r', allow: PERMISSIONS.MANAGE_CHANNELS, deny: 0 },
      ],
    });
    expect(
      isChannelReadOnly(viaOverride, { channel_id: 'c1', kind: 'announcement' }, 'moi'),
    ).toBe(false);
  });
});

describe('isChannelRestricted', () => {
  it('faux sans override sur ce salon', () => {
    const s = groupState({
      overrides: [{ channel_id: 'autre', role_id: 'r', allow: 0, deny: PERMISSIONS.VIEW }],
    });
    expect(isChannelRestricted(s, 'c1')).toBe(false);
  });

  it('faux sans champ overrides ou état indéfini', () => {
    expect(isChannelRestricted(groupState(), 'c1')).toBe(false);
    expect(isChannelRestricted(undefined, 'c1')).toBe(false);
  });

  it('vrai si un rôle se voit refuser VIEW sur ce salon', () => {
    const s = groupState({
      overrides: [{ channel_id: 'c1', role_id: 'r', allow: 0, deny: PERMISSIONS.VIEW }],
    });
    expect(isChannelRestricted(s, 'c1')).toBe(true);
  });

  it('vrai si un rôle se voit refuser SEND sur ce salon', () => {
    const s = groupState({
      overrides: [{ channel_id: 'c1', role_id: 'r', allow: 0, deny: PERMISSIONS.SEND }],
    });
    expect(isChannelRestricted(s, 'c1')).toBe(true);
  });

  it('faux quand seule une autre permission est refusée (ex. MANAGE_MESSAGES)', () => {
    const s = groupState({
      overrides: [
        { channel_id: 'c1', role_id: 'r', allow: 0, deny: PERMISSIONS.MANAGE_MESSAGES },
      ],
    });
    expect(isChannelRestricted(s, 'c1')).toBe(false);
  });

  it('faux quand le rôle est seulement autorisé (allow), pas refusé', () => {
    const s = groupState({
      overrides: [{ channel_id: 'c1', role_id: 'r', allow: PERMISSIONS.VIEW, deny: 0 }],
    });
    expect(isChannelRestricted(s, 'c1')).toBe(false);
  });
});

describe('isChannelVisible', () => {
  it('vrai avec VIEW effectif (base sans override)', () => {
    const s = groupState({
      my_permissions: PERMISSIONS.VIEW | PERMISSIONS.SEND,
      members: [{ pubkey: 'moi', roles: [] }],
    });
    expect(isChannelVisible(s, 'c1', 'moi')).toBe(true);
  });

  it('faux quand un rôle porté se voit refuser VIEW sur ce salon', () => {
    const s = groupState({
      my_permissions: PERMISSIONS.VIEW | PERMISSIONS.SEND,
      members: [{ pubkey: 'moi', roles: ['r'] }],
      overrides: [{ channel_id: 'c1', role_id: 'r', allow: 0, deny: PERMISSIONS.VIEW }],
    });
    expect(isChannelVisible(s, 'c1', 'moi')).toBe(false);
  });

  it('vrai pour ADMIN même avec un override VIEW refusé', () => {
    const s = groupState({
      my_permissions: PERMISSIONS.ADMIN,
      members: [{ pubkey: 'moi', roles: ['r'] }],
      overrides: [{ channel_id: 'c1', role_id: 'r', allow: 0, deny: PERMISSIONS.VIEW }],
    });
    expect(isChannelVisible(s, 'c1', 'moi')).toBe(true);
  });

  it('vrai par défaut sans état matérialisé', () => {
    expect(isChannelVisible(undefined, 'c1', 'moi')).toBe(true);
  });
});

describe('useGroups — sourdine et pseudos de serveur', () => {
  it('timeout met en sourdine puis recharge l’état', async () => {
    timeoutMock.mockResolvedValueOnce({ ok: true });
    stateMock.mockResolvedValueOnce(groupState());

    await useGroups.getState().timeout('g1', 'autre', 4242);

    expect(timeoutMock).toHaveBeenCalledWith('g1', 'autre', 4242);
    expect(useGroups.getState().states['g1']).toBeDefined();
  });

  it('clearTimeout lève la sourdine puis recharge l’état', async () => {
    timeoutClearMock.mockResolvedValueOnce({ ok: true });
    stateMock.mockResolvedValueOnce(groupState());

    await useGroups.getState().clearTimeout('g1', 'autre');

    expect(timeoutClearMock).toHaveBeenCalledWith('g1', 'autre');
    expect(useGroups.getState().states['g1']).toBeDefined();
  });

  it('setNickname transmet le membre ciblé puis recharge l’état', async () => {
    nicknameMock.mockResolvedValueOnce({ ok: true });
    stateMock.mockResolvedValueOnce(groupState());

    await useGroups.getState().setNickname('g1', 'Bibou', 'autre');

    expect(nicknameMock).toHaveBeenCalledWith('g1', 'Bibou', 'autre');
  });

  it('setNickname sans membre vise soi-même (membre undefined)', async () => {
    nicknameMock.mockResolvedValueOnce({ ok: true });
    stateMock.mockResolvedValueOnce(groupState());

    await useGroups.getState().setNickname('g1', '');

    expect(nicknameMock).toHaveBeenCalledWith('g1', '', undefined);
  });
});

describe('useGroups — modération vocale (groups.voice_moderate)', () => {
  it('voiceModerate transmet mute/deafen puis recharge l’état', async () => {
    voiceModerateMock.mockResolvedValueOnce({ ok: true });
    stateMock.mockResolvedValueOnce(groupState());

    await useGroups.getState().voiceModerate('g1', 'autre', true, false);

    expect(voiceModerateMock).toHaveBeenCalledWith('g1', 'autre', true, false);
    expect(useGroups.getState().states['g1']).toBeDefined();
  });

  it('voiceModerate(false, false) lève la modération', async () => {
    voiceModerateMock.mockResolvedValueOnce({ ok: true });
    stateMock.mockResolvedValueOnce(groupState());

    await useGroups.getState().voiceModerate('g1', 'autre', false, false);

    expect(voiceModerateMock).toHaveBeenCalledWith('g1', 'autre', false, false);
  });

  it('ne recharge pas l’état quand le nœud refuse', async () => {
    voiceModerateMock.mockRejectedValueOnce(new Error('refusé : hiérarchie'));

    await expect(
      useGroups.getState().voiceModerate('g1', 'autre', true, false),
    ).rejects.toThrow();

    expect(stateMock).not.toHaveBeenCalled();
  });
});

describe('useGroups — catégories et overrides', () => {
  it('renameCategory édite puis recharge l’état', async () => {
    categoryEditMock.mockResolvedValueOnce({ ok: true });
    stateMock.mockResolvedValueOnce(groupState());

    await useGroups.getState().renameCategory('g1', 'cat1', 'Papotage');

    expect(categoryEditMock).toHaveBeenCalledWith('g1', 'cat1', { name: 'Papotage' });
    expect(stateMock).toHaveBeenCalledWith('g1');
  });

  it('deleteCategory supprime puis recharge l’état', async () => {
    categoryDelMock.mockResolvedValueOnce({ ok: true });
    stateMock.mockResolvedValueOnce(groupState());

    await useGroups.getState().deleteCategory('g1', 'cat1');

    expect(categoryDelMock).toHaveBeenCalledWith('g1', 'cat1');
    expect(stateMock).toHaveBeenCalledWith('g1');
  });

  it('setChannelCategory passe null pour « sans catégorie »', async () => {
    channelEditMock.mockResolvedValue({ ok: true });
    stateMock.mockResolvedValue(groupState());

    await useGroups.getState().setChannelCategory('g1', 'c1', 'cat1');
    await useGroups.getState().setChannelCategory('g1', 'c1', null);

    expect(channelEditMock).toHaveBeenNthCalledWith(1, 'g1', 'c1', {
      category: 'cat1',
    });
    expect(channelEditMock).toHaveBeenNthCalledWith(2, 'g1', 'c1', { category: null });
  });

  it('setChannelPerms fixe l’override puis recharge l’état', async () => {
    channelPermsMock.mockResolvedValueOnce({ ok: true });
    stateMock.mockResolvedValueOnce(groupState());

    await useGroups.getState().setChannelPerms('g1', 'c1', 'r1', 0, PERMISSIONS.SEND);

    expect(channelPermsMock).toHaveBeenCalledWith('g1', 'c1', 'r1', 0, 0x2);
    expect(stateMock).toHaveBeenCalledWith('g1');
  });
});

describe('useGroups — moveRole', () => {
  it('émet un edit par rôle échangé puis recharge l’état', async () => {
    useGroups.setState({
      states: {
        g1: groupState({ roles: [role('haut', 9), role('bas', 1)] }),
      },
    });
    roleEditMock.mockResolvedValue({ ok: true });
    stateMock.mockResolvedValueOnce(groupState());

    await useGroups.getState().moveRole('g1', 'bas', 'up');

    expect(roleEditMock).toHaveBeenNthCalledWith(1, 'g1', 'bas', { position: 9 });
    expect(roleEditMock).toHaveBeenNthCalledWith(2, 'g1', 'haut', { position: 1 });
    expect(stateMock).toHaveBeenCalledWith('g1');
  });

  it('sans voisin : aucune requête, aucun rechargement', async () => {
    useGroups.setState({
      states: { g1: groupState({ roles: [role('seul', 3)] }) },
    });

    await useGroups.getState().moveRole('g1', 'seul', 'up');

    expect(roleEditMock).not.toHaveBeenCalled();
    expect(stateMock).not.toHaveBeenCalled();
  });
});

describe('useGroups — jumpTo (saut au message)', () => {
  const key = channelKey('g1', 'c1');

  it('rend true sans requête quand la cible est déjà chargée', async () => {
    useGroups.setState({ messages: { [key]: [groupMsg('a', 1), groupMsg('b', 2)] } });

    const found = await useGroups.getState().jumpTo('g1', 'c1', 'b');

    expect(found).toBe(true);
    expect(historyAroundMock).not.toHaveBeenCalled();
  });

  it('fusionne la fenêtre du nœud et rend true quand found', async () => {
    useGroups.setState({ messages: { [key]: [groupMsg('recent', 100)] } });
    historyAroundMock.mockResolvedValueOnce({
      messages: [groupMsg('cible', 10), groupMsg('voisin', 11)],
      found: true,
    });

    const found = await useGroups.getState().jumpTo('g1', 'c1', 'cible');

    expect(found).toBe(true);
    expect(historyAroundMock).toHaveBeenCalledWith('g1', 'c1', 'cible');
    expect(useGroups.getState().messages[key]?.map((m) => m.msg_id)).toEqual([
      'cible',
      'voisin',
      'recent',
    ]);
  });

  it('rend false et ne touche à rien quand found est false', async () => {
    historyAroundMock.mockResolvedValueOnce({ messages: [], found: false });

    const found = await useGroups.getState().jumpTo('g1', 'c1', 'inconnu');

    expect(found).toBe(false);
    expect(useGroups.getState().messages[key]).toBeUndefined();
  });
});
