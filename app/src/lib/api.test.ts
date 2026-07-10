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
});
