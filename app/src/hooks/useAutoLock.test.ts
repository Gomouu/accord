/**
 * Verrouillage automatique : ce qu'il doit faire, et surtout ce qu'il ne doit
 * jamais faire. Un reverrouillage au mauvais moment — pendant un appel, avant
 * l'échéance — est pire que pas de verrouillage du tout : l'utilisateur perd
 * confiance dans le réglage et le désactive.
 */

import { describe, expect, it, vi } from 'vitest';
import { createIdleController } from './useAutoLock';

/** Contrôleur avec une horloge pilotée à la main. */
function harness(timeoutMs: number) {
  let now = 1_000_000;
  const lock = vi.fn();
  const controller = createIdleController(timeoutMs, { lock, now: () => now });
  return {
    controller,
    lock,
    advance: (ms: number) => {
      now += ms;
    },
  };
}

const MINUTE = 60_000;

describe('createIdleController', () => {
  it('verrouille une fois le délai écoulé sans activité', () => {
    const { controller, lock, advance } = harness(5 * MINUTE);
    advance(4 * MINUTE);
    expect(controller.tick(false)).toBe(false);
    expect(lock).not.toHaveBeenCalled();

    advance(2 * MINUTE);
    expect(controller.tick(false)).toBe(true);
    expect(lock).toHaveBeenCalledTimes(1);
  });

  it('remet le compteur à zéro à chaque activité', () => {
    const { controller, lock, advance } = harness(5 * MINUTE);
    for (let i = 0; i < 10; i += 1) {
      advance(4 * MINUTE);
      controller.activity();
      controller.tick(false);
    }
    // Quarante minutes se sont écoulées, mais jamais cinq d'affilée.
    expect(lock).not.toHaveBeenCalled();
  });

  it('ne verrouille jamais quand le réglage est désactivé', () => {
    const { controller, lock, advance } = harness(0);
    advance(24 * 60 * MINUTE);
    expect(controller.tick(false)).toBe(false);
    expect(lock).not.toHaveBeenCalled();
  });

  it('ne verrouille pas pendant un appel, et laisse le délai complet ensuite', () => {
    // Le cas hostile : l'utilisateur est présent, il écoute, il ne tape pas.
    const { controller, lock, advance } = harness(5 * MINUTE);
    advance(30 * MINUTE);
    expect(controller.tick(true)).toBe(false);
    expect(lock).not.toHaveBeenCalled();

    // L'appel se termine : le compte à rebours repart de zéro, il ne se fait
    // pas éjecter à la seconde où il raccroche.
    advance(4 * MINUTE);
    expect(controller.tick(false)).toBe(false);
    advance(2 * MINUTE);
    expect(controller.tick(false)).toBe(true);
  });

  it('ne verrouille qu’une fois par période d’inactivité', () => {
    const { controller, lock, advance } = harness(5 * MINUTE);
    advance(6 * MINUTE);
    expect(controller.tick(false)).toBe(true);
    // Ticks suivants sans nouvelle activité : pas de rafale de verrouillages.
    expect(controller.tick(false)).toBe(false);
    advance(MINUTE);
    expect(controller.tick(false)).toBe(false);
    expect(lock).toHaveBeenCalledTimes(1);
  });

  it('verrouille pile à l’échéance, pas avant', () => {
    const { controller, lock, advance } = harness(MINUTE);
    advance(MINUTE - 1);
    expect(controller.tick(false)).toBe(false);
    advance(1);
    expect(controller.tick(false)).toBe(true);
    expect(lock).toHaveBeenCalledTimes(1);
  });
});
