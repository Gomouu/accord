/**
 * Session store tests, focused on the logout (lock) transition: the store
 * must land on the unlock screen exactly like a fresh launch on an existing
 * vault, survive the RPC link closing underneath it, and allow an immediate
 * re-unlock afterwards.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Mock } from 'vitest';

vi.mock('../lib/client', () => ({
  rpc: {
    onStatus: vi.fn(),
    connect: vi.fn(async () => {}),
    close: vi.fn(),
  },
  api: {
    identitySelf: vi.fn(),
    profileSet: vi.fn(),
    profileSetAvatar: vi.fn(),
    profileSetBanner: vi.fn(),
  },
}));

vi.mock('../lib/bridge', () => ({
  createIdentity: vi.fn(),
  restoreIdentity: vi.fn(),
  unlockIdentity: vi.fn(),
  lockIdentity: vi.fn(),
  accountsList: vi.fn(),
  accountCreate: vi.fn(),
  accountRestore: vi.fn(),
  accountUnlock: vi.fn(),
  sessionClose: vi.fn(),
  // Appelées au chargement du module `stores/ui` (barre des
  // menus/systray) — la vraie implémentation touche Tauri, sans intérêt ici.
  traySetEnabled: vi.fn(async () => {}),
  registerCloseInterception: vi.fn(),
}));

import { api, rpc } from '../lib/client';
import type { SelfProfile } from '../lib/api';
import type { AccountMeta } from '../lib/bridge';
import {
  accountsList,
  accountUnlock,
  lockIdentity,
  sessionClose,
  unlockIdentity,
} from '../lib/bridge';
import {
  rememberNotifiedConversation,
  takePendingConversation,
} from '../lib/notifications';
import { useSession } from './session';
import { useDms } from './dms';
import { useGroups } from './groups';
import { useFriends } from './friends';
import { useUi } from './ui';

const lockIdentityMock = lockIdentity as unknown as Mock;
const unlockIdentityMock = unlockIdentity as unknown as Mock;
const accountsListMock = accountsList as unknown as Mock;
const accountUnlockMock = accountUnlock as unknown as Mock;
const sessionCloseMock = sessionClose as unknown as Mock;
const identitySelfMock = api.identitySelf as unknown as Mock;
const profileSetMock = api.profileSet as unknown as Mock;
const closeMock = rpc.close as unknown as Mock;
const connectMock = rpc.connect as unknown as Mock;

function account(id: string): AccountMeta {
  return {
    id,
    name: `Compte ${id}`,
    created_ms: 1,
    last_used_ms: 1,
    is_legacy: false,
    pubkey_short: null,
  };
}

const self: SelfProfile = {
  node_id: 'n-moi',
  pubkey: 'aa'.repeat(32),
  friend_code: 'accord-moi-12345',
  name: 'Alex',
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

/**
 * RPC status callback registered once at store creation — captured before
 * `vi.clearAllMocks()` wipes the recorded call.
 */
const statusCallback = (rpc.onStatus as unknown as Mock).mock.calls[0]?.[0] as (
  status: string,
) => void;

beforeEach(() => {
  vi.clearAllMocks();
  identitySelfMock.mockReset();
  profileSetMock.mockReset();
  lockIdentityMock.mockResolvedValue('locked');
  accountsListMock.mockResolvedValue([]);
  sessionCloseMock.mockResolvedValue('locked');
  useSession.setState({
    phase: 'ready',
    self,
    accounts: [],
    recoveryPhrase: null,
    askName: false,
    error: null,
  });
});

describe('useSession — personnalisation du profil', () => {
  it('fixe la décoration puis recharge identity.self', async () => {
    const updated = { ...self, avatar_decoration: 'neon_ring' };
    profileSetMock.mockResolvedValueOnce({});
    identitySelfMock.mockResolvedValueOnce(updated);

    await useSession.getState().setAvatarDecoration('neon_ring');

    expect(profileSetMock).toHaveBeenCalledWith({ avatar_decoration: 'neon_ring' });
    expect(identitySelfMock).toHaveBeenCalledTimes(1);
    expect(profileSetMock.mock.invocationCallOrder[0]!).toBeLessThan(
      identitySelfMock.mock.invocationCallOrder[0]!,
    );
    expect(useSession.getState().self).toEqual(updated);
  });

  it('fixe l’effet puis recharge identity.self', async () => {
    const updated = { ...self, profile_effect: 'aurora' };
    profileSetMock.mockResolvedValueOnce({});
    identitySelfMock.mockResolvedValueOnce(updated);

    await useSession.getState().setProfileEffect('aurora');

    expect(profileSetMock).toHaveBeenCalledWith({ profile_effect: 'aurora' });
    expect(identitySelfMock).toHaveBeenCalledTimes(1);
    expect(profileSetMock.mock.invocationCallOrder[0]!).toBeLessThan(
      identitySelfMock.mock.invocationCallOrder[0]!,
    );
    expect(useSession.getState().self).toEqual(updated);
  });

  it('fixe le cadre puis recharge identity.self', async () => {
    const updated = { ...self, profile_frame: 'crystal_crown' };
    profileSetMock.mockResolvedValueOnce({});
    identitySelfMock.mockResolvedValueOnce(updated);

    await useSession.getState().setProfileFrame('crystal_crown');

    expect(profileSetMock).toHaveBeenCalledWith({ profile_frame: 'crystal_crown' });
    expect(identitySelfMock).toHaveBeenCalledTimes(1);
    expect(profileSetMock.mock.invocationCallOrder[0]!).toBeLessThan(
      identitySelfMock.mock.invocationCallOrder[0]!,
    );
    expect(useSession.getState().self).toEqual(updated);
  });
});

describe('useSession.lock', () => {
  it('lands on the unlock screen with the session state wiped', async () => {
    useSession.setState({ recoveryPhrase: 'douze mots', askName: true, error: 'old' });

    await useSession.getState().lock();

    const s = useSession.getState();
    expect(s.phase).toBe('locked');
    expect(s.self).toBeNull();
    expect(s.recoveryPhrase).toBeNull();
    expect(s.askName).toBe(false);
    expect(s.error).toBeNull();
    expect(closeMock).toHaveBeenCalledTimes(1);
    expect(lockIdentityMock).toHaveBeenCalledTimes(1);
  });

  it('drops any pending notification navigation', async () => {
    rememberNotifiedConversation({ kind: 'dm', peer: 'pair-1' });

    await useSession.getState().lock();

    expect(takePendingConversation()).toBeNull();
  });

  it('ignores the RPC link closing after logout (no offline bounce)', async () => {
    await useSession.getState().lock();

    statusCallback('closed');

    expect(useSession.getState().phase).toBe('locked');
  });

  it('falls back to onboarding when the vault file disappeared', async () => {
    lockIdentityMock.mockResolvedValue('absent');

    await useSession.getState().lock();

    expect(useSession.getState().phase).toBe('setup');
  });

  it('stays on the unlock screen with the error surfaced when locking fails', async () => {
    lockIdentityMock.mockRejectedValue(new Error('boom'));

    await useSession.getState().lock();

    const s = useSession.getState();
    expect(s.phase).toBe('locked');
    expect(s.error).toBe('boom');
  });

  it('allows an immediate re-unlock, exactly like a fresh launch', async () => {
    unlockIdentityMock.mockResolvedValue({ port: 4242, token: 'jeton' });
    identitySelfMock.mockResolvedValue(self);

    await useSession.getState().lock();
    await useSession.getState().unlock('phrase-de-passe');

    const s = useSession.getState();
    expect(s.phase).toBe('ready');
    expect(s.self).toEqual(self);
    expect(s.askName).toBe(false);
    expect(connectMock).toHaveBeenCalledWith(4242, 'jeton');
  });
});

describe('useSession.init — routing by account count', () => {
  it('routes to setup when no local account is known', async () => {
    accountsListMock.mockResolvedValue([]);

    await useSession.getState().init();

    const s = useSession.getState();
    expect(s.phase).toBe('setup');
    expect(s.accounts).toEqual([]);
  });

  it('routes to locked (direct unlock) with exactly one local account', async () => {
    accountsListMock.mockResolvedValue([account('a1')]);

    await useSession.getState().init();

    const s = useSession.getState();
    expect(s.phase).toBe('locked');
    expect(s.accounts).toHaveLength(1);
  });

  it('routes to the welcome account picker with two or more local accounts', async () => {
    accountsListMock.mockResolvedValue([account('a1'), account('a2')]);

    await useSession.getState().init();

    const s = useSession.getState();
    expect(s.phase).toBe('welcome');
    expect(s.accounts).toHaveLength(2);
  });

  it('falls back to setup with the error surfaced when the registry cannot be read', async () => {
    accountsListMock.mockRejectedValue(new Error('boom'));

    await useSession.getState().init();

    const s = useSession.getState();
    expect(s.phase).toBe('setup');
    expect(s.error).toBe('boom');
  });
});

describe('useSession.switchAccount', () => {
  it('closes the session, lands on welcome and refreshes the account list', async () => {
    accountsListMock.mockResolvedValue([account('a1'), account('a2')]);

    await useSession.getState().switchAccount();

    const s = useSession.getState();
    expect(sessionCloseMock).toHaveBeenCalledTimes(1);
    expect(closeMock).toHaveBeenCalledTimes(1);
    expect(s.phase).toBe('welcome');
    expect(s.self).toBeNull();
    expect(s.accounts).toHaveLength(2);
  });

  it('still lands on welcome with the error surfaced when closing the session fails', async () => {
    sessionCloseMock.mockRejectedValue(new Error('boom'));

    await useSession.getState().switchAccount();

    const s = useSession.getState();
    expect(s.phase).toBe('welcome');
    expect(s.error).toBe('boom');
  });
});

describe('useSession.activateAccount', () => {
  it('garde la session courante intacte si la phrase de passe est incorrecte', async () => {
    accountUnlockMock.mockRejectedValue(new Error('phrase de passe incorrecte'));
    useDms.setState({ conversations: { ami: [] } });

    await expect(
      useSession.getState().activateAccount('a2', 'mauvaise-phrase'),
    ).rejects.toThrow('phrase de passe incorrecte');

    expect(useSession.getState().phase).toBe('ready');
    expect(useSession.getState().self).toEqual(self);
    expect(useDms.getState().conversations).toEqual({ ami: [] });
    expect(closeMock).not.toHaveBeenCalled();
    expect(sessionCloseMock).not.toHaveBeenCalled();
  });

  it('purge les données puis hydrate le compte choisi après validation', async () => {
    const nextSelf = { ...self, pubkey: 'bb'.repeat(32), name: 'Béa' };
    accountUnlockMock.mockResolvedValue({ port: 5252, token: 'autre-jeton' });
    identitySelfMock.mockResolvedValue(nextSelf);
    useGroups.setState({ ids: ['ancien'] });

    await useSession.getState().activateAccount('a2', 'phrase-correcte');

    expect(accountUnlockMock).toHaveBeenCalledWith('a2', 'phrase-correcte');
    expect(closeMock).toHaveBeenCalledTimes(1);
    expect(connectMock).toHaveBeenCalledWith(5252, 'autre-jeton');
    expect(useGroups.getState().ids).toEqual([]);
    expect(useSession.getState().phase).toBe('ready');
    expect(useSession.getState().self).toEqual(nextSelf);
  });

  it('ferme la nouvelle session si son profil ne peut pas être hydraté', async () => {
    accountUnlockMock.mockResolvedValue({ port: 5252, token: 'autre-jeton' });
    identitySelfMock.mockRejectedValue(new Error('profil indisponible'));

    await expect(
      useSession.getState().activateAccount('a2', 'phrase-correcte'),
    ).rejects.toThrow('profil indisponible');

    expect(closeMock).toHaveBeenCalledTimes(2);
    expect(sessionCloseMock).toHaveBeenCalledTimes(1);
    expect(useSession.getState().phase).toBe('welcome');
    expect(useSession.getState().self).toBeNull();
  });
});

describe('useSession.unlockAccount', () => {
  it('unlocks the selected account and lands on ready', async () => {
    accountUnlockMock.mockResolvedValue({ port: 4242, token: 'jeton' });
    identitySelfMock.mockResolvedValue(self);
    useSession.setState({ phase: 'welcome' });

    await useSession.getState().unlockAccount('a1', 'phrase-de-passe');

    const s = useSession.getState();
    expect(accountUnlockMock).toHaveBeenCalledWith('a1', 'phrase-de-passe');
    expect(s.phase).toBe('ready');
    expect(s.self).toEqual(self);
  });

  it('stays on welcome with the error surfaced on a wrong passphrase', async () => {
    accountUnlockMock.mockRejectedValue(new Error('phrase de passe incorrecte'));
    useSession.setState({ phase: 'welcome' });

    await useSession.getState().unlockAccount('a1', 'mauvaise-phrase');

    const s = useSession.getState();
    expect(s.phase).toBe('welcome');
    expect(s.error).toBe('phrase de passe incorrecte');
  });

  it('purge les stores account-scoped au verrouillage (anti-fuite inter-comptes)', async () => {
    // Données du « compte A ».
    useDms.setState({ conversations: { p: [] } });
    useGroups.setState({ ids: ['g1'], pins: { g1: ['m'] } });
    useFriends.setState({ loaded: true });
    useUi.getState().setView({ kind: 'group', groupId: 'g1', channelId: 'c1' });

    lockIdentityMock.mockResolvedValue('present');
    await useSession.getState().lock();

    // Rien du compte A ne subsiste ; la vue retombe sur « Amis ».
    expect(useDms.getState().conversations).toEqual({});
    expect(useGroups.getState().ids).toEqual([]);
    expect(useGroups.getState().pins).toEqual({});
    expect(useFriends.getState().loaded).toBe(false);
    expect(useUi.getState().view).toEqual({ kind: 'friends' });
  });
});
