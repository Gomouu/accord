/**
 * Tests du store vocal : rejoindre/quitter, bascule micro, deafen (sémantique
 * Discord), volumes de sortie, application des événements `event.voice_*` et
 * resynchronisation via `voice.status`.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Mock } from 'vitest';

vi.mock('../lib/client', () => ({
  api: {
    voiceJoin: vi.fn(),
    voiceLeave: vi.fn(),
    voiceMute: vi.fn(),
    voiceDeafen: vi.fn(),
    voiceSetVolume: vi.fn(),
    voiceStatus: vi.fn(),
    voiceSetNoiseSuppression: vi.fn(),
    voiceSetAgc: vi.fn(),
  },
}));

import { api } from '../lib/client';
import { clampVolume, useVoice, type ParticipantState } from './voice';

const joinMock = api.voiceJoin as unknown as Mock;
const leaveMock = api.voiceLeave as unknown as Mock;
const muteMock = api.voiceMute as unknown as Mock;
const deafenMock = api.voiceDeafen as unknown as Mock;
const setVolumeMock = api.voiceSetVolume as unknown as Mock;
const statusMock = api.voiceStatus as unknown as Mock;
const setNoiseSuppressionMock = api.voiceSetNoiseSuppression as unknown as Mock;
const setAgcMock = api.voiceSetAgc as unknown as Mock;

/** Participant sans état particulier (personne ne parle, volume neutre). */
function idle(overrides: Partial<ParticipantState> = {}): ParticipantState {
  return {
    speaking: false,
    muted: false,
    deafened: false,
    volume: 100,
    serverMuted: false,
    serverDeafened: false,
    prioritySpeaker: false,
    ...overrides,
  };
}

/** Pose un salon actif avec des participants, sans passer par l'API. */
function seedActive(
  groupId = 'g1',
  participants: Array<[string, Partial<ParticipantState>]> = [
    ['moi', {}],
    ['alice', {}],
  ],
): void {
  useVoice.setState({
    active: { groupId, channelId: groupId, muted: false, isCall: false },
    selfDeafened: false,
    participants: new Map(participants.map(([pk, state]) => [pk, idle(state)])),
  });
}

function participantKeys(): string[] {
  return [...useVoice.getState().participants.keys()];
}

beforeEach(() => {
  useVoice.setState({
    active: null,
    selfDeafened: false,
    masterVolume: 100,
    participants: new Map(),
    dsp: { noiseSuppression: false, agc: false },
  });
  joinMock.mockReset();
  leaveMock.mockReset();
  muteMock.mockReset();
  deafenMock.mockReset();
  setVolumeMock.mockReset();
  statusMock.mockReset();
  setNoiseSuppressionMock.mockReset();
  setAgcMock.mockReset();
});

describe('useVoice.join', () => {
  it('rejoint le salon et enregistre les participants (personne ne parle)', async () => {
    joinMock.mockResolvedValueOnce({ participants: ['moi', 'alice'] });

    await useVoice.getState().join('g1', 'g1');

    expect(joinMock).toHaveBeenCalledWith('g1', 'g1');
    expect(useVoice.getState().active).toEqual({
      groupId: 'g1',
      channelId: 'g1',
      muted: false,
      isCall: false,
    });
    expect(participantKeys()).toEqual(['moi', 'alice']);
    expect(useVoice.getState().participants.get('alice')).toEqual(idle());
  });

  it('réinitialise le deafen de session en rejoignant', async () => {
    useVoice.setState({ selfDeafened: true });
    joinMock.mockResolvedValueOnce({ participants: ['moi'] });

    await useVoice.getState().join('g1', 'g1');

    expect(useVoice.getState().selfDeafened).toBe(false);
  });

  it('remplace le salon précédent (join implique leave côté nœud)', async () => {
    seedActive('g0');
    joinMock.mockResolvedValueOnce({ participants: ['moi'] });

    await useVoice.getState().join('g1', 'g1');

    expect(useVoice.getState().active?.groupId).toBe('g1');
    expect(participantKeys()).toEqual(['moi']);
  });

  it('ne change rien localement quand le nœud refuse (salon plein)', async () => {
    joinMock.mockRejectedValueOnce(new Error('salon plein'));

    await expect(useVoice.getState().join('g1', 'g1')).rejects.toThrow();

    expect(useVoice.getState().active).toBeNull();
    expect(participantKeys()).toEqual([]);
  });
});

describe('useVoice.leave', () => {
  it('quitte le salon et vide les participants', async () => {
    seedActive();
    useVoice.setState({ selfDeafened: true });
    leaveMock.mockResolvedValueOnce({});

    await useVoice.getState().leave();

    expect(leaveMock).toHaveBeenCalledTimes(1);
    expect(useVoice.getState().active).toBeNull();
    expect(useVoice.getState().selfDeafened).toBe(false);
    expect(participantKeys()).toEqual([]);
  });
});

describe('useVoice.toggleMute', () => {
  it('coupe puis rétablit le micro en restant dans le salon', async () => {
    seedActive();
    muteMock.mockResolvedValue({});

    await useVoice.getState().toggleMute();
    expect(muteMock).toHaveBeenLastCalledWith(true);
    expect(useVoice.getState().active?.muted).toBe(true);

    await useVoice.getState().toggleMute();
    expect(muteMock).toHaveBeenLastCalledWith(false);
    expect(useVoice.getState().active?.muted).toBe(false);
    expect(useVoice.getState().active?.groupId).toBe('g1');
  });

  it('ne fait rien hors salon vocal', async () => {
    await useVoice.getState().toggleMute();

    expect(muteMock).not.toHaveBeenCalled();
  });

  it('ne bascule pas localement quand le nœud refuse', async () => {
    seedActive();
    muteMock.mockRejectedValueOnce(new Error('hors ligne'));

    await expect(useVoice.getState().toggleMute()).rejects.toThrow();

    expect(useVoice.getState().active?.muted).toBe(false);
  });

  it('garde le micro coupé tant que le deafen est actif (état demandé mémorisé au nœud)', async () => {
    seedActive();
    useVoice.setState({
      active: { groupId: 'g1', channelId: 'g1', muted: true, isCall: false },
      selfDeafened: true,
    });
    muteMock.mockResolvedValue({});

    await useVoice.getState().setMuted(false);

    // La demande part au nœud (qui la mémorise), le micro reste coupé.
    expect(muteMock).toHaveBeenLastCalledWith(false);
    expect(useVoice.getState().active?.muted).toBe(true);
  });
});

describe('useVoice.setDeafened', () => {
  it('coupe la sortie et force le micro coupé (sémantique Discord)', async () => {
    seedActive();
    deafenMock.mockResolvedValue({});

    await useVoice.getState().setDeafened(true);

    expect(deafenMock).toHaveBeenLastCalledWith(true);
    expect(useVoice.getState().selfDeafened).toBe(true);
    expect(useVoice.getState().active?.muted).toBe(true);
  });

  it('rétablit via une resynchronisation (le nœud restaure le micro demandé)', async () => {
    seedActive();
    useVoice.setState({
      active: { groupId: 'g1', channelId: 'g1', muted: true, isCall: false },
      selfDeafened: true,
    });
    deafenMock.mockResolvedValue({});
    statusMock.mockResolvedValueOnce({
      master_volume: 100,
      active: {
        group_id: 'g1',
        channel_id: 'g1',
        muted: false,
        deafened: false,
        participants: [
          { pubkey: 'moi', speaking: false, muted: false, deafened: false, volume: 100 },
        ],
      },
    });

    await useVoice.getState().setDeafened(false);

    expect(deafenMock).toHaveBeenLastCalledWith(false);
    expect(useVoice.getState().selfDeafened).toBe(false);
    // Le micro restauré vient de voice.status, pas d'une supposition locale.
    expect(useVoice.getState().active?.muted).toBe(false);
  });

  it('est idempotent et sans effet hors salon', async () => {
    await useVoice.getState().setDeafened(true);
    expect(deafenMock).not.toHaveBeenCalled();

    seedActive();
    await useVoice.getState().setDeafened(false);
    expect(deafenMock).not.toHaveBeenCalled();
  });

  it('toggleDeafen alterne l’état', async () => {
    seedActive();
    deafenMock.mockResolvedValue({});
    statusMock.mockResolvedValue({ master_volume: 100, active: null });

    await useVoice.getState().toggleDeafen();
    expect(useVoice.getState().selfDeafened).toBe(true);
  });

  it('ne change rien localement quand le nœud refuse', async () => {
    seedActive();
    deafenMock.mockRejectedValueOnce(new Error('hors ligne'));

    await expect(useVoice.getState().setDeafened(true)).rejects.toThrow();

    expect(useVoice.getState().selfDeafened).toBe(false);
    expect(useVoice.getState().active?.muted).toBe(false);
  });
});

describe('useVoice.setVolume', () => {
  it('règle le volume principal (peer null) et le mémorise', async () => {
    setVolumeMock.mockResolvedValue({});

    await useVoice.getState().setVolume(null, 150);

    expect(setVolumeMock).toHaveBeenLastCalledWith(null, 150);
    expect(useVoice.getState().masterVolume).toBe(150);
  });

  it('règle le volume d’un participant connu', async () => {
    seedActive();
    setVolumeMock.mockResolvedValue({});

    await useVoice.getState().setVolume('alice', 40);

    expect(setVolumeMock).toHaveBeenLastCalledWith('alice', 40);
    expect(useVoice.getState().participants.get('alice')?.volume).toBe(40);
    expect(useVoice.getState().participants.get('moi')?.volume).toBe(100);
  });

  it('borne et arrondit le volume avant l’appel (0-200, entier)', async () => {
    setVolumeMock.mockResolvedValue({});

    await useVoice.getState().setVolume(null, 250);
    expect(setVolumeMock).toHaveBeenLastCalledWith(null, 200);

    await useVoice.getState().setVolume(null, -10);
    expect(setVolumeMock).toHaveBeenLastCalledWith(null, 0);

    await useVoice.getState().setVolume(null, 99.6);
    expect(setVolumeMock).toHaveBeenLastCalledWith(null, 100);
  });

  it('ne mémorise rien quand le nœud refuse', async () => {
    setVolumeMock.mockRejectedValueOnce(new Error('hors bornes'));

    await expect(useVoice.getState().setVolume(null, 150)).rejects.toThrow();

    expect(useVoice.getState().masterVolume).toBe(100);
  });
});

describe('clampVolume', () => {
  it('borne en entier 0-200 et retombe à 100 sur une valeur non finie', () => {
    expect(clampVolume(150)).toBe(150);
    expect(clampVolume(201)).toBe(200);
    expect(clampVolume(-1)).toBe(0);
    expect(clampVolume(49.5)).toBe(50);
    expect(clampVolume(Number.NaN)).toBe(100);
  });
});

describe('événements voice_*', () => {
  it('voice_joined ajoute un participant au salon actif', () => {
    seedActive();

    useVoice.getState().applyJoined({ group_id: 'g1', channel_id: 'g1', pubkey: 'bob' });

    expect(participantKeys()).toEqual(['moi', 'alice', 'bob']);
    expect(useVoice.getState().participants.get('bob')).toEqual(idle());
  });

  it('voice_joined ignore un autre salon et reste idempotent', () => {
    seedActive('g1', [['alice', { speaking: true }]]);

    useVoice.getState().applyJoined({ group_id: 'g2', channel_id: 'g2', pubkey: 'bob' });
    useVoice
      .getState()
      .applyJoined({ group_id: 'g1', channel_id: 'g1', pubkey: 'alice' });

    expect(participantKeys()).toEqual(['alice']);
    // Le doublon ne réinitialise pas l'état de parole existant.
    expect(useVoice.getState().participants.get('alice')).toEqual(
      idle({ speaking: true }),
    );
  });

  it('voice_joined est ignoré hors salon vocal', () => {
    useVoice.getState().applyJoined({ group_id: 'g1', channel_id: 'g1', pubkey: 'bob' });

    expect(participantKeys()).toEqual([]);
  });

  it('voice_left retire le participant du salon actif', () => {
    seedActive();

    useVoice.getState().applyLeft({ group_id: 'g1', channel_id: 'g1', pubkey: 'alice' });

    expect(participantKeys()).toEqual(['moi']);
  });

  it('voice_left ignore un autre salon et un inconnu', () => {
    seedActive();

    useVoice.getState().applyLeft({ group_id: 'g2', channel_id: 'g2', pubkey: 'moi' });
    useVoice
      .getState()
      .applyLeft({ group_id: 'g1', channel_id: 'g1', pubkey: 'fantôme' });

    expect(participantKeys()).toEqual(['moi', 'alice']);
  });

  it('voice_speaking met à jour l’état de parole sans toucher au reste', () => {
    seedActive('g1', [['alice', { muted: true, volume: 60 }]]);

    useVoice.getState().applySpeaking({ pubkey: 'alice', speaking: true });
    expect(useVoice.getState().participants.get('alice')).toEqual(
      idle({ speaking: true, muted: true, volume: 60 }),
    );

    useVoice.getState().applySpeaking({ pubkey: 'alice', speaking: false });
    expect(useVoice.getState().participants.get('alice')).toEqual(
      idle({ muted: true, volume: 60 }),
    );
  });

  it('voice_speaking ignore un participant inconnu', () => {
    seedActive('g1', [['moi', {}]]);

    useVoice.getState().applySpeaking({ pubkey: 'fantôme', speaking: true });

    expect(participantKeys()).toEqual(['moi']);
  });

  it('voice_mute applique micro/deafen d’un participant connu', () => {
    seedActive('g1', [['alice', { speaking: true, volume: 80 }]]);

    useVoice
      .getState()
      .applyMuteState({ pubkey: 'alice', muted: true, deafened: true });

    expect(useVoice.getState().participants.get('alice')).toEqual(
      idle({ speaking: true, muted: true, deafened: true, volume: 80 }),
    );
  });

  it('voice_mute ignore un participant inconnu et reste idempotent', () => {
    seedActive('g1', [['moi', {}]]);
    const before = useVoice.getState().participants;

    useVoice
      .getState()
      .applyMuteState({ pubkey: 'fantôme', muted: true, deafened: false });
    useVoice
      .getState()
      .applyMuteState({ pubkey: 'moi', muted: false, deafened: false });

    // Aucun changement : la référence de la table est conservée.
    expect(useVoice.getState().participants).toBe(before);
  });
});

describe('useVoice.sync', () => {
  it('restaure le salon actif, les participants et les volumes', async () => {
    statusMock.mockResolvedValueOnce({
      master_volume: 130,
      active: {
        group_id: 'g1',
        channel_id: 'g1',
        muted: true,
        deafened: true,
        participants: [
          { pubkey: 'moi', speaking: false, muted: true, deafened: true, volume: 100 },
          { pubkey: 'alice', speaking: true, muted: false, deafened: false, volume: 40 },
        ],
      },
    });

    await useVoice.getState().sync();

    expect(useVoice.getState().active).toEqual({
      groupId: 'g1',
      channelId: 'g1',
      muted: true,
      isCall: false,
    });
    expect(useVoice.getState().selfDeafened).toBe(true);
    expect(useVoice.getState().masterVolume).toBe(130);
    expect(useVoice.getState().participants.get('alice')).toEqual(
      idle({ speaking: true, volume: 40 }),
    );
    expect(useVoice.getState().participants.get('moi')).toEqual(
      idle({ muted: true, deafened: true }),
    );
  });

  it('vide l’état local quand aucun salon n’est actif côté nœud', async () => {
    seedActive();
    useVoice.setState({ selfDeafened: true });
    statusMock.mockResolvedValueOnce({ master_volume: 90, active: null });

    await useVoice.getState().sync();

    expect(useVoice.getState().active).toBeNull();
    expect(useVoice.getState().selfDeafened).toBe(false);
    expect(useVoice.getState().masterVolume).toBe(90);
    expect(participantKeys()).toEqual([]);
  });
});

describe('useVoice.loadMasterVolume', () => {
  it('recharge le volume principal sans toucher au salon actif', async () => {
    seedActive();
    statusMock.mockResolvedValueOnce({ master_volume: 175, active: null });

    await useVoice.getState().loadMasterVolume();

    expect(useVoice.getState().masterVolume).toBe(175);
    expect(useVoice.getState().active?.groupId).toBe('g1');
  });

  it('recharge aussi les réglages DSP persistés', async () => {
    statusMock.mockResolvedValueOnce({
      master_volume: 100,
      active: null,
      dsp: { noise_suppression: true, agc: true },
    });

    await useVoice.getState().loadMasterVolume();

    expect(useVoice.getState().dsp).toEqual({ noiseSuppression: true, agc: true });
  });

  it('retombe sur des DSP désactivés si le nœud ne les émet pas (tolérance)', async () => {
    statusMock.mockResolvedValueOnce({ master_volume: 100, active: null });

    await useVoice.getState().loadMasterVolume();

    expect(useVoice.getState().dsp).toEqual({ noiseSuppression: false, agc: false });
  });
});

describe('useVoice DSP (voice.set_noise_suppression / voice.set_agc)', () => {
  it('setNoiseSuppression appelle le nœud et mémorise localement', async () => {
    setNoiseSuppressionMock.mockResolvedValue({});

    await useVoice.getState().setNoiseSuppression(true);

    expect(setNoiseSuppressionMock).toHaveBeenLastCalledWith(true);
    expect(useVoice.getState().dsp.noiseSuppression).toBe(true);
  });

  it('setAgc appelle le nœud et mémorise localement', async () => {
    setAgcMock.mockResolvedValue({});

    await useVoice.getState().setAgc(true);

    expect(setAgcMock).toHaveBeenLastCalledWith(true);
    expect(useVoice.getState().dsp.agc).toBe(true);
  });

  it('ne mémorise rien quand le nœud refuse', async () => {
    setNoiseSuppressionMock.mockRejectedValueOnce(new Error('hors ligne'));

    await expect(useVoice.getState().setNoiseSuppression(true)).rejects.toThrow();

    expect(useVoice.getState().dsp.noiseSuppression).toBe(false);
  });
});

describe('useVoice.sync avec un appel 1-à-1 (sentinelle)', () => {
  const SENTINEL = '0'.repeat(32);

  it('reflète is_call et les champs de modération (toujours false en appel)', async () => {
    statusMock.mockResolvedValueOnce({
      master_volume: 100,
      active: {
        group_id: SENTINEL,
        channel_id: 'callid',
        muted: false,
        deafened: false,
        is_call: true,
        participants: [
          {
            pubkey: 'moi',
            speaking: false,
            muted: false,
            deafened: false,
            volume: 100,
            server_muted: false,
            server_deafened: false,
            priority_speaker: false,
          },
        ],
      },
    });

    await useVoice.getState().sync();

    expect(useVoice.getState().active).toEqual({
      groupId: SENTINEL,
      channelId: 'callid',
      muted: false,
      isCall: true,
    });
  });
});

describe('événement voice_moderate', () => {
  it('applique la modération sur un participant du salon de groupe actif', () => {
    seedActive('g1', [['alice', {}]]);

    useVoice.getState().applyVoiceModerate({
      group_id: 'g1',
      pubkey: 'alice',
      server_muted: true,
      server_deafened: false,
      priority_speaker: false,
    });

    expect(useVoice.getState().participants.get('alice')).toEqual(
      idle({ serverMuted: true }),
    );
  });

  it('ignore un autre salon et un participant inconnu, reste idempotent', () => {
    seedActive('g1', [['moi', {}]]);
    const before = useVoice.getState().participants;

    useVoice.getState().applyVoiceModerate({
      group_id: 'g2',
      pubkey: 'moi',
      server_muted: true,
      server_deafened: false,
      priority_speaker: false,
    });
    useVoice.getState().applyVoiceModerate({
      group_id: 'g1',
      pubkey: 'fantôme',
      server_muted: true,
      server_deafened: false,
      priority_speaker: false,
    });
    useVoice.getState().applyVoiceModerate({
      group_id: 'g1',
      pubkey: 'moi',
      server_muted: false,
      server_deafened: false,
      priority_speaker: false,
    });

    expect(useVoice.getState().participants).toBe(before);
  });

  it('est ignoré hors salon vocal (jamais émis pour une session d’appel)', () => {
    useVoice.getState().applyVoiceModerate({
      group_id: 'g1',
      pubkey: 'moi',
      server_muted: true,
      server_deafened: false,
      priority_speaker: false,
    });

    expect(useVoice.getState().participants.size).toBe(0);
  });
});
