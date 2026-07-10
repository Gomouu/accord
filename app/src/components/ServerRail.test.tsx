/**
 * Tests de la garde anti-id-périmé du rail des serveurs : la restauration du
 * dernier salon consulté ne doit jamais renvoyer un salon supprimé ou devenu
 * vocal, et replie proprement sur le premier salon disponible.
 */

import { describe, expect, it } from 'vitest';
import type { GroupChannel, GroupStateJson } from '../lib/api';
import { channelToRestore } from './ServerRail';

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
