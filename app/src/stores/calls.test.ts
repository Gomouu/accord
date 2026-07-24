/**
 * Tests du store d'appel 1-à-1 : actions optimistes (start/accept/decline/
 * hangup), resynchronisation (calls.status), application des événements
 * `event.call_*` (idempotence, appels croisés/superseded) et badge d'appel
 * manqué.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Mock } from 'vitest';

vi.mock('../lib/client', () => ({
  api: {
    callsStart: vi.fn(),
    callsAccept: vi.fn(),
    callsDecline: vi.fn(),
    callsHangup: vi.fn(),
    callsStatus: vi.fn(),
  },
}));

// Isole le store de la capture/du DOM WebCodecs (v5).
vi.mock('../lib/mediaController', () => ({
  startLocalStream: vi.fn(),
  stopLocalStream: vi.fn(),
  stopAllLocal: vi.fn(),
  resetRemote: vi.fn(),
  resetPeer: vi.fn(),
  resetAllRemote: vi.fn(),
}));

import { api } from '../lib/client';
import {
  resetAllRemote,
  resetPeer,
  resetRemote,
  startLocalStream,
  stopAllLocal,
  stopLocalStream,
} from '../lib/mediaController';
import { useCalls } from './calls';

const startMock = api.callsStart as unknown as Mock;
const acceptMock = api.callsAccept as unknown as Mock;
const declineMock = api.callsDecline as unknown as Mock;
const hangupMock = api.callsHangup as unknown as Mock;
const statusMock = api.callsStatus as unknown as Mock;
const startShareMock = startLocalStream as unknown as Mock;
const stopShareMock = stopLocalStream as unknown as Mock;
const stopAllMock = stopAllLocal as unknown as Mock;
const resetRemoteMock = resetRemote as unknown as Mock;
const resetPeerMock = resetPeer as unknown as Mock;
const resetAllMock = resetAllRemote as unknown as Mock;

beforeEach(() => {
  useCalls.setState({
    phase: 'idle',
    peer: null,
    callId: null,
    sincePhaseMs: null,
    missedPeers: new Set(),
    localSharing: false,
    localCamera: false,
    remoteVideo: {},
  });
  startMock.mockReset();
  acceptMock.mockReset();
  declineMock.mockReset();
  hangupMock.mockReset();
  statusMock.mockReset();
  startShareMock.mockReset();
  stopShareMock.mockReset();
  stopAllMock.mockReset();
  resetRemoteMock.mockReset();
  resetPeerMock.mockReset();
  resetAllMock.mockReset();
});

describe('useCalls.start', () => {
  it('démarre un appel et passe en sonnerie sortante', async () => {
    startMock.mockResolvedValueOnce({ call_id: 'c1' });

    await useCalls.getState().start('alice');

    expect(startMock).toHaveBeenCalledWith('alice');
    const s = useCalls.getState();
    expect(s.phase).toBe('outgoing_ringing');
    expect(s.peer).toBe('alice');
    expect(s.callId).toBe('c1');
    expect(s.sincePhaseMs).not.toBeNull();
  });

  it('ne change rien localement quand le nœud refuse (occupé)', async () => {
    startMock.mockRejectedValueOnce(new Error('occupé'));

    await expect(useCalls.getState().start('alice')).rejects.toThrow();

    expect(useCalls.getState().phase).toBe('idle');
  });
});

describe('useCalls.accept', () => {
  it('accepte l’appel entrant et passe actif', async () => {
    useCalls.setState({
      phase: 'incoming_ringing',
      peer: 'alice',
      callId: 'c1',
      sincePhaseMs: 1000,
    });
    acceptMock.mockResolvedValueOnce({ ok: true });

    await useCalls.getState().accept();

    expect(acceptMock).toHaveBeenCalledWith('c1');
    expect(useCalls.getState().phase).toBe('active');
    expect(useCalls.getState().peer).toBe('alice');
  });

  it('ne fait rien hors sonnerie entrante', async () => {
    await useCalls.getState().accept();

    expect(acceptMock).not.toHaveBeenCalled();
  });
});

describe('useCalls.decline', () => {
  it('refuse l’appel entrant et repasse idle', async () => {
    useCalls.setState({
      phase: 'incoming_ringing',
      peer: 'alice',
      callId: 'c1',
      sincePhaseMs: 1000,
    });
    declineMock.mockResolvedValueOnce({ ok: true });

    await useCalls.getState().decline();

    expect(declineMock).toHaveBeenCalledWith('c1');
    const s = useCalls.getState();
    expect(s.phase).toBe('idle');
    expect(s.peer).toBeNull();
    expect(s.callId).toBeNull();
  });

  it('ne fait rien hors sonnerie entrante', async () => {
    await useCalls.getState().decline();

    expect(declineMock).not.toHaveBeenCalled();
  });
});

describe('useCalls.hangup', () => {
  it('annule une sonnerie sortante', async () => {
    useCalls.setState({
      phase: 'outgoing_ringing',
      peer: 'alice',
      callId: 'c1',
      sincePhaseMs: 1000,
    });
    hangupMock.mockResolvedValueOnce({ ok: true });

    await useCalls.getState().hangup();

    expect(hangupMock).toHaveBeenCalledTimes(1);
    expect(useCalls.getState().phase).toBe('idle');
  });

  it('raccroche un appel actif', async () => {
    useCalls.setState({
      phase: 'active',
      peer: 'alice',
      callId: 'c1',
      sincePhaseMs: 1000,
    });
    hangupMock.mockResolvedValueOnce({ ok: true });

    await useCalls.getState().hangup();

    expect(useCalls.getState().phase).toBe('idle');
  });

  it('est un no-op au repos (idempotent)', async () => {
    await useCalls.getState().hangup();

    expect(hangupMock).not.toHaveBeenCalled();
  });
});

describe('useCalls.sync', () => {
  it('restaure une phase active depuis calls.status', async () => {
    statusMock.mockResolvedValueOnce({
      state: 'active',
      peer: 'alice',
      call_id: 'c1',
      since_ms: 500,
    });

    await useCalls.getState().sync();

    const s = useCalls.getState();
    expect(s.phase).toBe('active');
    expect(s.peer).toBe('alice');
    expect(s.callId).toBe('c1');
    expect(s.sincePhaseMs).not.toBeNull();
  });

  it('reste idle sans ancre de temps quand aucun appel n’est en cours', async () => {
    statusMock.mockResolvedValueOnce({
      state: 'idle',
      peer: null,
      call_id: null,
      since_ms: null,
    });

    await useCalls.getState().sync();

    const s = useCalls.getState();
    expect(s.phase).toBe('idle');
    expect(s.sincePhaseMs).toBeNull();
  });
});

describe('événements call_outgoing / call_incoming', () => {
  it('call_outgoing met en sonnerie sortante', () => {
    useCalls.getState().applyOutgoing({ peer: 'alice', call_id: 'c1' });

    const s = useCalls.getState();
    expect(s.phase).toBe('outgoing_ringing');
    expect(s.peer).toBe('alice');
    expect(s.callId).toBe('c1');
  });

  it('call_incoming met en sonnerie entrante (montre l’overlay)', () => {
    useCalls.getState().applyIncoming({ peer: 'bob', call_id: 'c2' });

    const s = useCalls.getState();
    expect(s.phase).toBe('incoming_ringing');
    expect(s.peer).toBe('bob');
    expect(s.callId).toBe('c2');
  });

  it('un doublon du même call_id ne réinitialise pas l’ancre de temps', () => {
    useCalls.getState().applyIncoming({ peer: 'bob', call_id: 'c2' });
    const firstAnchor = useCalls.getState().sincePhaseMs;

    useCalls.getState().applyIncoming({ peer: 'bob', call_id: 'c2' });

    expect(useCalls.getState().sincePhaseMs).toBe(firstAnchor);
  });
});

describe('événement call_accepted', () => {
  it('passe actif avec le pair/call_id de l’événement', () => {
    useCalls.setState({
      phase: 'outgoing_ringing',
      peer: 'alice',
      callId: 'c1',
      sincePhaseMs: 1000,
    });

    useCalls.getState().applyAccepted({ peer: 'alice', call_id: 'c1' });

    expect(useCalls.getState().phase).toBe('active');
  });

  it('appels croisés (superseded) : adopte le nouveau call_id, quel qu’il soit', () => {
    useCalls.setState({
      phase: 'outgoing_ringing',
      peer: 'alice',
      callId: 'c-losing',
      sincePhaseMs: 1000,
    });

    // Le call_ended{reason: superseded} de l'appel perdant a déjà été traité
    // ailleurs ; l'événement accepted qui suit porte le call_id retenu.
    useCalls.getState().applyAccepted({ peer: 'alice', call_id: 'c-winning' });

    const s = useCalls.getState();
    expect(s.phase).toBe('active');
    expect(s.callId).toBe('c-winning');
  });
});

describe('événement call_ended', () => {
  it('réinitialise l’état quand le call_id correspond et rend true', () => {
    useCalls.setState({
      phase: 'outgoing_ringing',
      peer: 'alice',
      callId: 'c1',
      sincePhaseMs: 1000,
    });

    const applied = useCalls
      .getState()
      .applyEnded({ peer: 'alice', call_id: 'c1', reason: 'timeout' });

    expect(applied).toBe(true);
    const s = useCalls.getState();
    expect(s.phase).toBe('idle');
    expect(s.peer).toBeNull();
    expect(s.callId).toBeNull();
  });

  it('ignore un événement dont le call_id ne correspond pas et rend false', () => {
    useCalls.setState({
      phase: 'active',
      peer: 'alice',
      callId: 'c-current',
      sincePhaseMs: 1000,
    });

    const applied = useCalls
      .getState()
      .applyEnded({ peer: 'alice', call_id: 'c-old', reason: 'hangup' });

    expect(applied).toBe(false);
    expect(useCalls.getState().phase).toBe('active');
  });

  it('un decline() local absorbe l’event.call_ended qui suit (pas de double effet)', async () => {
    useCalls.setState({
      phase: 'incoming_ringing',
      peer: 'alice',
      callId: 'c1',
      sincePhaseMs: 1000,
    });
    declineMock.mockResolvedValueOnce({ ok: true });

    await useCalls.getState().decline();
    const applied = useCalls
      .getState()
      .applyEnded({ peer: 'alice', call_id: 'c1', reason: 'declined' });

    // callId local déjà null : l'événement tardif ne correspond plus.
    expect(applied).toBe(false);
  });
});

describe('badge d’appel manqué', () => {
  it('markMissed puis clearMissed', () => {
    useCalls.getState().markMissed('alice');
    expect(useCalls.getState().missedPeers.has('alice')).toBe(true);

    useCalls.getState().clearMissed('alice');
    expect(useCalls.getState().missedPeers.has('alice')).toBe(false);
  });

  it('est idempotent (référence stable sans changement)', () => {
    const before = useCalls.getState().missedPeers;

    useCalls.getState().clearMissed('jamais-marqué');

    expect(useCalls.getState().missedPeers).toBe(before);
  });
});

describe('useCalls — partage d’écran (v5)', () => {
  it('startScreenShare marche hors appel (salon vocal de groupe, v6.1)', async () => {
    // Depuis la 6.1 le partage vaut aussi en salon de groupe : le store ne
    // filtre plus sur la phase d'appel, c'est le nœud qui ignore la demande
    // s'il n'y a aucune session média active.
    useCalls.setState({ phase: 'idle' });
    startShareMock.mockResolvedValueOnce(undefined);
    await useCalls.getState().startScreenShare();
    expect(startShareMock).toHaveBeenCalledWith('screen', expect.any(Function));
    expect(useCalls.getState().localSharing).toBe(true);
  });

  it('startScreenShare est idempotent quand on partage déjà', async () => {
    useCalls.setState({ phase: 'active', localSharing: true });
    await useCalls.getState().startScreenShare();
    expect(startShareMock).not.toHaveBeenCalled();
  });

  it('startScreenShare active le partage local pendant un appel actif', async () => {
    useCalls.setState({ phase: 'active' });
    startShareMock.mockResolvedValueOnce(undefined);
    await useCalls.getState().startScreenShare();
    expect(startShareMock).toHaveBeenCalledWith('screen', expect.any(Function));
    expect(useCalls.getState().localSharing).toBe(true);
  });

  it('stopScreenShare coupe le partage local', async () => {
    useCalls.setState({ phase: 'active', localSharing: true });
    stopShareMock.mockResolvedValueOnce(undefined);
    await useCalls.getState().stopScreenShare();
    expect(stopShareMock).toHaveBeenCalledWith('screen');
    expect(useCalls.getState().localSharing).toBe(false);
  });

  it('applyScreenState bascule le partage d’un pair et réinitialise à l’arrêt', () => {
    useCalls.getState().applyScreenState({ peer: 'p', sharing: true });
    expect(useCalls.getState().remoteVideo.p?.screen).toBe(true);
    expect(resetRemoteMock).not.toHaveBeenCalled();

    useCalls.getState().applyScreenState({ peer: 'p', sharing: false });
    // Plus aucun flux : le pair sort de la table plutôt que d'y laisser une
    // entrée qui afficherait une tuile vide.
    expect(useCalls.getState().remoteVideo.p).toBeUndefined();
    expect(resetRemoteMock).toHaveBeenCalledWith('p', 'screen');
  });

  it('suit plusieurs émetteurs simultanés sans les confondre', () => {
    // Le cas du salon de groupe : deux personnes diffusent en même temps.
    useCalls.getState().applyScreenState({ peer: 'alice', sharing: true });
    useCalls.getState().applyCameraState({ peer: 'bob', on: true });
    expect(useCalls.getState().remoteVideo).toEqual({
      alice: { screen: true, camera: false },
      bob: { screen: false, camera: true },
    });

    // L'arrêt de l'une ne touche pas l'autre.
    useCalls.getState().applyScreenState({ peer: 'alice', sharing: false });
    expect(useCalls.getState().remoteVideo.alice).toBeUndefined();
    expect(useCalls.getState().remoteVideo.bob?.camera).toBe(true);
  });

  it('une même personne peut diffuser sa caméra ET son écran', () => {
    useCalls.getState().applyCameraState({ peer: 'p', on: true });
    useCalls.getState().applyScreenState({ peer: 'p', sharing: true });
    expect(useCalls.getState().remoteVideo.p).toEqual({ screen: true, camera: true });
    // Couper l'écran laisse la caméra en place.
    useCalls.getState().applyScreenState({ peer: 'p', sharing: false });
    expect(useCalls.getState().remoteVideo.p).toEqual({ screen: false, camera: true });
  });

  it('noteRemoteFrame marque le flux actif quand l’annonce s’est perdue', () => {
    useCalls.getState().noteRemoteFrame('p', 'screen');
    expect(useCalls.getState().remoteVideo.p?.screen).toBe(true);
  });

  it('forgetPeerVideo retire les tuiles d’un participant parti sans annonce', () => {
    // Un départ brutal (fermeture de l'app, perte de réseau) n'envoie aucune
    // annonce d'arrêt : la tuile resterait figée sur la dernière image.
    useCalls.getState().applyCameraState({ peer: 'alice', on: true });
    useCalls.getState().applyScreenState({ peer: 'bob', sharing: true });
    useCalls.getState().forgetPeerVideo('alice');
    expect(useCalls.getState().remoteVideo.alice).toBeUndefined();
    expect(resetPeerMock).toHaveBeenCalledWith('alice');
    // Les autres participants ne sont pas touchés.
    expect(useCalls.getState().remoteVideo.bob?.screen).toBe(true);
  });

  it('forgetPeerVideo ne fait rien pour un pair sans flux', () => {
    useCalls.getState().forgetPeerVideo('inconnu');
    expect(resetPeerMock).not.toHaveBeenCalled();
  });

  it('noteRemoteFrame ne recrée pas l’état à chaque trame', () => {
    // 12 à 24 trames par seconde : recréer la table à chaque fois ferait
    // re-rendre toute la grille en continu.
    useCalls.getState().noteRemoteFrame('p', 'camera');
    const premier = useCalls.getState().remoteVideo;
    useCalls.getState().noteRemoteFrame('p', 'camera');
    expect(useCalls.getState().remoteVideo).toBe(premier);
  });

  it('applyEnded coupe local et distant et réinitialise la visionneuse', () => {
    useCalls.setState({
      phase: 'active',
      peer: 'p',
      callId: 'c1',
      localSharing: true,
      remoteVideo: { p: { screen: true, camera: false } },
    });
    const reset = useCalls.getState().applyEnded({
      peer: 'p',
      call_id: 'c1',
      reason: 'hangup',
    });
    expect(reset).toBe(true);
    expect(stopAllMock).toHaveBeenCalledTimes(1);
    expect(resetAllMock).toHaveBeenCalled();
    expect(useCalls.getState().localSharing).toBe(false);
    expect(useCalls.getState().remoteVideo).toEqual({});
  });

  it('hangup coupe un partage en cours avant de raccrocher', async () => {
    useCalls.setState({ phase: 'active', peer: 'p', callId: 'c1', localSharing: true });
    hangupMock.mockResolvedValueOnce({ ok: true });
    stopAllMock.mockResolvedValueOnce(undefined);
    await useCalls.getState().hangup();
    expect(stopAllMock).toHaveBeenCalledTimes(1);
    expect(hangupMock).toHaveBeenCalledTimes(1);
    expect(useCalls.getState().localSharing).toBe(false);
  });
});
