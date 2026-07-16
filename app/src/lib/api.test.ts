/**
 * Tests des wrappers `groups.*` de modération de serveur : chaque méthode doit
 * appeler la bonne méthode JSON-RPC avec les paramètres attendus (et, pour le
 * pseudo, omettre `member` quand il vaut « soi-même »).
 */

import { describe, expect, it, vi } from 'vitest';
import { Api } from './api';
import type { RpcClient } from './rpc';

/** Api câblée sur un `rpc.call` espionné. */
function makeApi() {
  const call = vi.fn().mockResolvedValue({ ok: true });
  const api = new Api({ call } as unknown as RpcClient);
  return { api, call };
}

describe('profileSet — champs de personnalisation tri-état', () => {
  it('transmet les ids définis', async () => {
    const { api, call } = makeApi();

    await api.profileSet({
      avatar_decoration: 'neon_ring',
      profile_effect: 'aurora',
      profile_frame: 'crystal_crown',
    });

    expect(call).toHaveBeenCalledWith('profile.set', {
      avatar_decoration: 'neon_ring',
      profile_effect: 'aurora',
      profile_frame: 'crystal_crown',
    });
  });

  it('transmet explicitement null pour effacer les trois champs', async () => {
    const { api, call } = makeApi();

    await api.profileSet({
      avatar_decoration: null,
      profile_effect: null,
      profile_frame: null,
    });

    expect(call).toHaveBeenCalledWith('profile.set', {
      avatar_decoration: null,
      profile_effect: null,
      profile_frame: null,
    });
  });

  it('omet les trois clés quand elles ne figurent pas dans les changements', async () => {
    const { api, call } = makeApi();

    await api.profileSet({ bio: 'mise à jour' });

    expect(call).toHaveBeenCalledWith('profile.set', { bio: 'mise à jour' });
    const params = call.mock.calls[0]![1] as Record<string, unknown>;
    expect('avatar_decoration' in params).toBe(false);
    expect('profile_effect' in params).toBe(false);
    expect('profile_frame' in params).toBe(false);
  });
});

describe('groups moderation wrappers', () => {
  it('groupsTimeout forwards group_id, pubkey and until_ms', async () => {
    const { api, call } = makeApi();
    await api.groupsTimeout('g1', 'pk1', 10_000);
    expect(call).toHaveBeenCalledWith('groups.timeout', {
      group_id: 'g1',
      pubkey: 'pk1',
      until_ms: 10_000,
    });
  });

  it('groupsTimeoutClear forwards group_id and pubkey', async () => {
    const { api, call } = makeApi();
    await api.groupsTimeoutClear('g1', 'pk1');
    expect(call).toHaveBeenCalledWith('groups.timeout_clear', {
      group_id: 'g1',
      pubkey: 'pk1',
    });
  });

  it('groupsSetNickname includes member when targeting someone else', async () => {
    const { api, call } = makeApi();
    await api.groupsSetNickname('g1', 'Capitaine', 'pk2');
    expect(call).toHaveBeenCalledWith('groups.set_nickname', {
      group_id: 'g1',
      name: 'Capitaine',
      member: 'pk2',
    });
  });

  it('groupsSetNickname omits member for self (no undefined key)', async () => {
    const { api, call } = makeApi();
    await api.groupsSetNickname('g1', 'Moi');
    expect(call).toHaveBeenCalledWith('groups.set_nickname', {
      group_id: 'g1',
      name: 'Moi',
    });
    // exactOptionalPropertyTypes: `member` must be absent, not `undefined`.
    const params = call.mock.calls[0]![1] as Record<string, unknown>;
    expect('member' in params).toBe(false);
  });

  it('groupsPurge forwards group_id, channel_id and msg_ids', async () => {
    const { api, call } = makeApi();
    await api.groupsPurge('g1', 'c1', ['m1', 'm2']);
    expect(call).toHaveBeenCalledWith('groups.purge', {
      group_id: 'g1',
      channel_id: 'c1',
      msg_ids: ['m1', 'm2'],
    });
  });
});
