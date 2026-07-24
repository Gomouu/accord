/**
 * Tests de la logique pure du sélecteur rapide : classement flou, sources
 * (amis/salons visibles), destinations récentes et cycle de salon/MP.
 */

import { describe, expect, it } from 'vitest';
import type { Contact, GroupStateJson } from './api';
import type { CommandSwitchItem, QuickSwitchItem } from './quickSwitch';
import {
  buildQuickSwitchItems,
  serverItemId,
  buildRecentItems,
  channelItemId,
  cycleChannel,
  cycleDm,
  dmItemId,
  isMacPlatform,
  matchScore,
  nextUnreadItem,
  rankQuickSwitchItems,
  sectionQuickSwitchItems,
  visibleNavigableChannels,
} from './quickSwitch';

function contact(
  pubkey: string,
  displayName: string,
  state: Contact['state'] = 'friend',
): Contact {
  return {
    node_id: `n-${pubkey}`,
    pubkey,
    friend_code: `accord-${pubkey}`,
    display_name: displayName,
    bio: null,
    avatar: null,
    banner: null,
    state,
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

describe('matchScore', () => {
  it('retourne null pour une requête vide', () => {
    expect(matchScore('Général', '')).toBeNull();
    expect(matchScore('Général', '   ')).toBeNull();
  });

  it('retourne null en l’absence de correspondance', () => {
    expect(matchScore('Général', 'xyz')).toBeNull();
  });

  it('classe le préfixe au-dessus de la limite de mot', () => {
    const prefix = matchScore('General', 'gen')!;
    const wordBoundary = matchScore('Off Topic General', 'gen')!;
    expect(prefix).toBeGreaterThan(wordBoundary);
  });

  it('classe la limite de mot au-dessus de la simple sous-chaîne', () => {
    const wordBoundary = matchScore('Voice General', 'gen')!;
    const substring = matchScore('Legend', 'gen')!;
    expect(wordBoundary).toBeGreaterThan(substring);
  });

  it('classe la sous-chaîne au-dessus de la sous-séquence', () => {
    const substring = matchScore('Legend', 'gen')!;
    const subsequence = matchScore('Green Ends', 'gen')!;
    expect(substring).toBeGreaterThan(subsequence);
  });

  it('reconnaît une sous-séquence non contiguë', () => {
    expect(matchScore('General', 'gnrl')).not.toBeNull();
  });

  it('ignore la casse', () => {
    expect(matchScore('GENERAL', 'gen')).toBe(matchScore('general', 'GEN'));
  });

  it('matches text without requiring accents in the query', () => {
    expect(matchScore('Général', 'general')).toBe(matchScore('General', 'general'));
  });
});

describe('rankQuickSwitchItems', () => {
  it('filtre les non-correspondances et trie par pertinence puis alphabet', () => {
    const items = [
      { label: 'Legend' },
      { label: 'General' },
      { label: 'Off Topic General' },
      { label: 'Nope' },
    ];

    const ranked = rankQuickSwitchItems(items, 'gen');

    expect(ranked.map((i) => i.label)).toEqual([
      'General',
      'Off Topic General',
      'Legend',
    ]);
  });

  it('départage à égalité de score par ordre alphabétique', () => {
    const items = [{ label: 'Zebra' }, { label: 'Alpha' }];
    expect(rankQuickSwitchItems(items, 'a').map((i) => i.label)).toEqual([
      'Alpha',
      'Zebra',
    ]);
  });

  it('retourne une liste vide sans correspondance', () => {
    expect(rankQuickSwitchItems([{ label: 'Général' }], 'xyz')).toEqual([]);
  });

  it('matches command subtitles and fuzzy keywords', () => {
    const commands: CommandSwitchItem[] = [
      {
        id: 'command:create-channel',
        kind: 'command',
        label: 'Create channel',
        subtitle: 'Server tools',
        keywords: ['room'],
        run: () => undefined,
      },
      {
        id: 'command:privacy',
        kind: 'command',
        label: 'Open privacy settings',
        subtitle: 'Personal settings',
        keywords: ['safety'],
        run: () => undefined,
      },
    ];

    expect(rankQuickSwitchItems(commands, 'server tools')).toEqual([commands[0]]);
    expect(rankQuickSwitchItems(commands, 'sfty')).toEqual([commands[1]]);
  });
});

describe('sectionQuickSwitchItems', () => {
  it('groups ranked items into the fixed palette section order', () => {
    const items: QuickSwitchItem[] = [
      {
        id: 'command:settings',
        kind: 'command',
        label: 'Open settings',
        subtitle: 'Command',
        run: () => undefined,
      },
      { id: 'server:g1', kind: 'server', label: 'Guild', groupId: 'g1' },
      { id: 'friends', kind: 'friends', label: 'Friends', view: { kind: 'friends' } },
      {
        id: 'dm:alice',
        kind: 'dm',
        label: 'Alice',
        pubkey: 'alice',
        avatarHash: null,
        avatarDecoration: null,
        view: { kind: 'dm', peer: 'alice' },
      },
      {
        id: 'channel:g1/c1',
        kind: 'channel',
        label: 'general',
        subtitle: 'Guild',
        channelKind: 'text',
        groupId: 'g1',
        channelId: 'c1',
        view: { kind: 'group', groupId: 'g1', channelId: 'c1' },
      },
    ];

    expect(
      sectionQuickSwitchItems(items).map((section) => ({
        id: section.id,
        items: section.items.map((item) => item.id),
      })),
    ).toEqual([
      { id: 'channels', items: ['channel:g1/c1'] },
      { id: 'dms', items: ['friends', 'dm:alice'] },
      { id: 'servers', items: ['server:g1'] },
      { id: 'commands', items: ['command:settings'] },
    ]);
    expect(sectionQuickSwitchItems(items.slice(2), true)[0]?.id).toBe('recent');
  });
});

describe('buildQuickSwitchItems', () => {
  it('conserve l’avatar et sa décoration dans une destination MP', () => {
    const alice = {
      ...contact('alice', 'Alice'),
      avatar: 'avatar-hash',
      avatar_decoration: 'neon_ring',
    };

    const item = buildQuickSwitchItems({
      friendsLabel: 'Amis',
      contacts: [alice],
      groupIds: [],
      groupStates: {},
      selfPubkey: 'moi',
    }).find((candidate) => candidate.kind === 'dm');

    expect(item).toMatchObject({
      avatarHash: 'avatar-hash',
      avatarDecoration: 'neon_ring',
    });
  });

  it('inclut chaque serveur rejoint comme destination', () => {
    const items = buildQuickSwitchItems({
      friendsLabel: 'Amis',
      contacts: [],
      groupIds: ['g1'],
      groupStates: { g1: groupState() },
      selfPubkey: null,
    });
    expect(items.filter((i) => i.kind === 'server')).toEqual([
      { id: serverItemId('g1'), kind: 'server', label: 'Guilde', groupId: 'g1' },
    ]);
  });

  it('inclut toujours l’entrée Amis en tête', () => {
    const items = buildQuickSwitchItems({
      friendsLabel: 'Amis',
      contacts: [],
      groupIds: [],
      groupStates: {},
      selfPubkey: null,
    });
    expect(items[0]).toEqual({
      id: 'friends',
      kind: 'friends',
      label: 'Amis',
      view: { kind: 'friends' },
    });
  });

  it('n’inclut que les amis établis (pas les demandes ni les bloqués)', () => {
    const items = buildQuickSwitchItems({
      friendsLabel: 'Amis',
      contacts: [
        contact('a', 'Alice', 'friend'),
        contact('b', 'Bob', 'pending_in'),
        contact('c', 'Carl', 'blocked'),
      ],
      groupIds: [],
      groupStates: {},
      selfPubkey: null,
    });
    expect(items.filter((i) => i.kind === 'dm').map((i) => i.id)).toEqual([
      dmItemId('a'),
    ]);
  });

  it('replie sur le code ami quand le pseudo est vide', () => {
    const items = buildQuickSwitchItems({
      friendsLabel: 'Amis',
      contacts: [contact('a', '')],
      groupIds: [],
      groupStates: {},
      selfPubkey: null,
    });
    expect(items[1]!.label).toBe('accord-a');
  });

  it('inclut les salons visibles de chaque serveur rejoint, tous genres confondus', () => {
    const state = groupState({
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
          name: 'annonces',
          kind: 'announcement',
          category: null,
          position: 1,
          topic: '',
        },
        {
          channel_id: 'c3',
          name: 'vocal',
          kind: 'voice',
          category: null,
          position: 2,
          topic: '',
        },
      ],
    });
    const items = buildQuickSwitchItems({
      friendsLabel: 'Amis',
      contacts: [],
      groupIds: ['g1'],
      groupStates: { g1: state },
      selfPubkey: null,
    });
    const channelItems = items.filter((i) => i.kind === 'channel');
    expect(channelItems.map((i) => i.id)).toEqual([
      channelItemId('g1', 'c1'),
      channelItemId('g1', 'c2'),
      channelItemId('g1', 'c3'),
    ]);
    expect(
      channelItems.every((i) => i.kind === 'channel' && i.subtitle === 'Guilde'),
    ).toBe(true);
  });

  it('exclut un salon dont VIEW est refusé par override de rôle', () => {
    const state = groupState({
      my_permissions: 0,
      members: [
        {
          pubkey: 'moi',
          roles: ['r1'],
          nickname: null,
          avatar: null,
          timeout_until_ms: 0,
        },
      ],
      channels: [
        {
          channel_id: 'c1',
          name: 'secret',
          kind: 'text',
          category: null,
          position: 0,
          topic: '',
        },
      ],
      overrides: [{ channel_id: 'c1', role_id: 'r1', allow: 0, deny: 0x1 }],
    });
    const items = buildQuickSwitchItems({
      friendsLabel: 'Amis',
      contacts: [],
      groupIds: ['g1'],
      groupStates: { g1: state },
      selfPubkey: 'moi',
    });
    expect(items.some((i) => i.kind === 'channel')).toBe(false);
  });
});

describe('buildRecentItems', () => {
  it('place la dernière conversation privée en tête, puis les derniers salons par serveur (ordre du rail)', () => {
    const items = buildQuickSwitchItems({
      friendsLabel: 'Amis',
      contacts: [contact('a', 'Alice')],
      groupIds: ['g1', 'g2'],
      groupStates: {
        g1: groupState({
          group_id: 'g1',
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
        }),
        g2: groupState({
          group_id: 'g2',
          name: 'Autre',
          channels: [
            {
              channel_id: 'c2',
              name: 'accueil',
              kind: 'text',
              category: null,
              position: 0,
              topic: '',
            },
          ],
        }),
      },
      selfPubkey: null,
    });

    const recent = buildRecentItems(items, ['g1', 'g2'], { g1: 'c1', g2: 'c2' }, 'a');

    expect(recent.map((i) => i.id)).toEqual([
      dmItemId('a'),
      channelItemId('g1', 'c1'),
      channelItemId('g2', 'c2'),
    ]);
  });

  it('ignore une mémoire pointant vers une destination qui n’existe plus', () => {
    const items = buildQuickSwitchItems({
      friendsLabel: 'Amis',
      contacts: [],
      groupIds: [],
      groupStates: {},
      selfPubkey: null,
    });
    expect(buildRecentItems(items, [], {}, 'disparu')).toEqual([]);
  });

  it('retourne une liste vide sans aucune mémoire de navigation', () => {
    const items = buildQuickSwitchItems({
      friendsLabel: 'Amis',
      contacts: [],
      groupIds: [],
      groupStates: {},
      selfPubkey: null,
    });
    expect(buildRecentItems(items, [], {}, null)).toEqual([]);
  });
});

describe('nextUnreadItem', () => {
  it('finds the next unread item and wraps to the beginning', () => {
    const items = [
      { id: 'first', unread: true },
      { id: 'middle', unread: false },
      { id: 'last', unread: true },
    ];
    const isUnread = (item: (typeof items)[number]): boolean => item.unread;

    expect(nextUnreadItem(items, 'first', isUnread)?.id).toBe('last');
    expect(nextUnreadItem(items, 'last', isUnread)?.id).toBe('first');
  });

  it('returns null when no item is unread', () => {
    expect(
      nextUnreadItem([{ id: 'read', unread: false }], 'read', (item) => item.unread),
    ).toBeNull();
  });
});

describe('visibleNavigableChannels', () => {
  it('respecte l’ordre de la barre latérale (sans catégorie, puis catégories par position)', () => {
    const state = groupState({
      categories: [{ category_id: 'cat1', name: 'Cat', position: 0 }],
      channels: [
        {
          channel_id: 'c2',
          name: 'dans-categorie',
          kind: 'text',
          category: 'cat1',
          position: 0,
          topic: '',
        },
        {
          channel_id: 'c1',
          name: 'sans-categorie',
          kind: 'text',
          category: null,
          position: 0,
          topic: '',
        },
      ],
    });
    const ordered = visibleNavigableChannels(state, null);
    expect(ordered.map((c) => c.channel_id)).toEqual(['c1', 'c2']);
  });
});

describe('cycleChannel', () => {
  const channels = [
    {
      channel_id: 't1',
      name: 'texte-1',
      kind: 'text' as const,
      category: null,
      position: 0,
      topic: '',
    },
    {
      channel_id: 'v1',
      name: 'vocal-1',
      kind: 'voice' as const,
      category: null,
      position: 1,
      topic: '',
    },
    {
      channel_id: 't2',
      name: 'texte-2',
      kind: 'text' as const,
      category: null,
      position: 2,
      topic: '',
    },
  ];

  it('avance au salon textuel suivant, en ignorant le vocal', () => {
    expect(cycleChannel(channels, 't1', 1)).toBe('t2');
  });

  it('boucle du dernier au premier', () => {
    expect(cycleChannel(channels, 't2', 1)).toBe('t1');
  });

  it('recule en bouclant du premier au dernier', () => {
    expect(cycleChannel(channels, 't1', -1)).toBe('t2');
  });

  it('démarre au premier salon navigable sans salon actif connu', () => {
    expect(cycleChannel(channels, null, 1)).toBe('t1');
    expect(cycleChannel(channels, null, -1)).toBe('t2');
  });

  it('retourne null sans aucun salon navigable', () => {
    const onlyVoice = [
      {
        channel_id: 'v1',
        name: 'vocal',
        kind: 'voice' as const,
        category: null,
        position: 0,
        topic: '',
      },
    ];
    expect(cycleChannel(onlyVoice, null, 1)).toBeNull();
  });
});

describe('cycleDm', () => {
  it('avance et boucle', () => {
    expect(cycleDm(['a', 'b', 'c'], 'a', 1)).toBe('b');
    expect(cycleDm(['a', 'b', 'c'], 'c', 1)).toBe('a');
  });

  it('recule et boucle', () => {
    expect(cycleDm(['a', 'b', 'c'], 'a', -1)).toBe('c');
  });

  it('démarre au premier/dernier pair sans conversation active connue', () => {
    expect(cycleDm(['a', 'b'], null, 1)).toBe('a');
    expect(cycleDm(['a', 'b'], null, -1)).toBe('b');
  });

  it('retourne null sans aucun ami', () => {
    expect(cycleDm([], null, 1)).toBeNull();
  });
});

describe('isMacPlatform', () => {
  it('reconnaît une plateforme macOS', () => {
    expect(isMacPlatform('MacIntel')).toBe(true);
  });

  it('rejette une plateforme non-macOS', () => {
    expect(isMacPlatform('Win32')).toBe(false);
    expect(isMacPlatform('Linux x86_64')).toBe(false);
  });
});
