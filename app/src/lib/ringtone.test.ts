/**
 * Tests de la sonnerie d'appel entrant : boucle toutes les ~2 s tant que
 * `stopRingtone` n'est pas appelé, jamais d'exception sans support Web Audio,
 * gardée par la préférence de son ET l'absence de Ne pas déranger. Chaque
 * test importe le module à neuf (`vi.resetModules`) pour isoler le contexte
 * audio partagé (singleton module) et le minuteur de boucle d'un test à
 * l'autre — même schéma que `notificationSound.test.ts`.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

class FakeGain {
  gain = {
    setValueAtTime: vi.fn(),
    linearRampToValueAtTime: vi.fn(),
    exponentialRampToValueAtTime: vi.fn(),
  };
  connect = vi.fn();
}

class FakeOscillator {
  type = 'sine';
  frequency = { value: 0 };
  connect = vi.fn();
  start = vi.fn();
  stop = vi.fn();
}

class FakeAudioContext {
  static instances: FakeAudioContext[] = [];
  state: 'running' | 'suspended' = 'running';
  currentTime = 0;
  destination = {};
  createGain = vi.fn(() => new FakeGain());
  createOscillator = vi.fn(() => new FakeOscillator());
  resume = vi.fn().mockResolvedValue(undefined);

  constructor() {
    FakeAudioContext.instances.push(this);
  }
}

interface WindowWithAudio {
  AudioContext?: typeof AudioContext | undefined;
  webkitAudioContext?: typeof AudioContext | undefined;
}

describe('startRingtone / stopRingtone', () => {
  const w = window as unknown as WindowWithAudio;
  let originalAudioContext: typeof AudioContext | undefined;

  beforeEach(() => {
    originalAudioContext = w.AudioContext;
    FakeAudioContext.instances = [];
    vi.resetModules();
    vi.useFakeTimers();
  });

  afterEach(() => {
    w.AudioContext = originalAudioContext;
    vi.useRealTimers();
  });

  it('ne lève jamais sans support Web Audio (API absente)', async () => {
    delete w.AudioContext;
    delete w.webkitAudioContext;
    const { startRingtone, stopRingtone } = await import('./ringtone');

    expect(() => startRingtone()).not.toThrow();
    expect(() => stopRingtone()).not.toThrow();
  });

  it('joue immédiatement au démarrage puis toutes les ~2 s', async () => {
    w.AudioContext = FakeAudioContext as unknown as typeof AudioContext;
    const { startRingtone } = await import('./ringtone');

    startRingtone();

    // Premier cycle immédiat : deux notes (deux oscillateurs).
    expect(FakeAudioContext.instances).toHaveLength(1);
    expect(FakeAudioContext.instances[0]?.createOscillator).toHaveBeenCalledTimes(2);

    await vi.advanceTimersByTimeAsync(2000);
    expect(FakeAudioContext.instances[0]?.createOscillator).toHaveBeenCalledTimes(4);

    await vi.advanceTimersByTimeAsync(2000);
    expect(FakeAudioContext.instances[0]?.createOscillator).toHaveBeenCalledTimes(6);
  });

  it('stopRingtone arrête immédiatement la boucle', async () => {
    w.AudioContext = FakeAudioContext as unknown as typeof AudioContext;
    const { startRingtone, stopRingtone } = await import('./ringtone');

    startRingtone();
    expect(FakeAudioContext.instances[0]?.createOscillator).toHaveBeenCalledTimes(2);

    stopRingtone();
    await vi.advanceTimersByTimeAsync(10_000);

    // Aucun cycle supplémentaire après l'arrêt.
    expect(FakeAudioContext.instances[0]?.createOscillator).toHaveBeenCalledTimes(2);
  });

  it('est idempotent : un second démarrage ne cumule pas les minuteurs', async () => {
    w.AudioContext = FakeAudioContext as unknown as typeof AudioContext;
    const { startRingtone } = await import('./ringtone');

    startRingtone();
    startRingtone();
    await vi.advanceTimersByTimeAsync(2000);

    // Un seul minuteur : un cycle immédiat + un cycle après 2 s = 2 au total.
    expect(FakeAudioContext.instances[0]?.createOscillator).toHaveBeenCalledTimes(4);
  });

  it('stopRingtone est un no-op sans sonnerie en cours', async () => {
    const { stopRingtone } = await import('./ringtone');

    expect(() => stopRingtone()).not.toThrow();
  });

  it('ne démarre pas quand le son de notification est désactivé', async () => {
    w.AudioContext = FakeAudioContext as unknown as typeof AudioContext;
    const { startRingtone } = await import('./ringtone');
    const { useUi } = await import('../stores/ui');

    useUi.getState().setNotifySoundEnabled(false);
    startRingtone();

    expect(FakeAudioContext.instances).toHaveLength(0);
  });

  it('ne démarre pas en Ne pas déranger (l’overlay reste affichable sans son)', async () => {
    w.AudioContext = FakeAudioContext as unknown as typeof AudioContext;
    const { startRingtone } = await import('./ringtone');
    const { useFriends } = await import('../stores/friends');

    useFriends.setState({ ownStatus: 'dnd' });
    startRingtone();

    expect(FakeAudioContext.instances).toHaveLength(0);
  });
});
