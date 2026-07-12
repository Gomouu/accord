/**
 * Tests du gestionnaire `event.soundboard_play` : ne joue le clip reçu que
 * dans le salon vocal correspondant et ignore l'écho de sa propre émission.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Mock } from 'vitest';

const h = vi.hoisted(() => ({
  voiceState: { active: null as { groupId: string; channelId: string } | null },
  sessionState: { self: null as { pubkey: string } | null },
}));

vi.mock('../lib/client', () => ({
  rpc: { onEvent: vi.fn() },
  api: {},
}));

vi.mock('../lib/files', () => ({
  lireFichier: vi.fn(() => Promise.resolve('data:audio/ogg;base64,AA==')),
}));

vi.mock('./voice', () => ({ useVoice: { getState: () => h.voiceState } }));
vi.mock('./session', () => ({ useSession: { getState: () => h.sessionState } }));

// Évite l'accès DOM (`new Audio(...).play()`) dans l'environnement de test.
vi.stubGlobal(
  'Audio',
  class {
    play(): Promise<void> {
      return Promise.resolve();
    }
  },
);

import { lireFichier } from '../lib/files';
import { handleSoundboardEvent } from './soundboard';

const lireMock = lireFichier as unknown as Mock;

/** Événement typique reçu d'un pair. */
function evt(overrides: Record<string, unknown> = {}): Record<string, unknown> {
  return { group_id: 'g1', channel_id: 'g1', sound: 'deadbeef', from: 'peer', ...overrides };
}

describe('handleSoundboardEvent', () => {
  beforeEach(() => {
    lireMock.mockClear();
    h.voiceState.active = { groupId: 'g1', channelId: 'g1' };
    h.sessionState.self = { pubkey: 'moi' };
  });

  it('joue le clip reçu dans le salon vocal correspondant', () => {
    handleSoundboardEvent('event.soundboard_play', evt());
    expect(lireMock).toHaveBeenCalledWith('deadbeef', 'peer');
  });

  it('ignore un autre type d’événement', () => {
    handleSoundboardEvent('event.voice_speaking', evt());
    expect(lireMock).not.toHaveBeenCalled();
  });

  it('ne joue rien hors du salon vocal actif', () => {
    h.voiceState.active = { groupId: 'g2', channelId: 'g2' };
    handleSoundboardEvent('event.soundboard_play', evt());
    expect(lireMock).not.toHaveBeenCalled();
  });

  it('ne joue rien quand aucun salon vocal n’est actif', () => {
    h.voiceState.active = null;
    handleSoundboardEvent('event.soundboard_play', evt());
    expect(lireMock).not.toHaveBeenCalled();
  });

  it('ignore l’écho de sa propre émission (évite le double)', () => {
    handleSoundboardEvent('event.soundboard_play', evt({ from: 'moi' }));
    expect(lireMock).not.toHaveBeenCalled();
  });

  it('ignore un payload malformé', () => {
    handleSoundboardEvent('event.soundboard_play', { group_id: 'g1' });
    expect(lireMock).not.toHaveBeenCalled();
  });
});
