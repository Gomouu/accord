/**
 * Tests du transfert d'historique : l'abonnement rend bien son désabonnement,
 * il ignore les autres événements et comble les champs manquants ; et
 * `conclure` sépare les trois seules choses qu'un transfert terminé permet de
 * dire — dont l'ambiguïté « rien de plus ancien OU version trop ancienne »,
 * que le nœud ne sait pas trancher et que l'interface ne doit pas masquer.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';

type EventHandler = (method: string, params: unknown) => void;
const handlers = new Set<EventHandler>();

vi.mock('./client', () => ({
  rpc: {
    onEvent: vi.fn((handler: EventHandler) => {
      handlers.add(handler);
      return () => handlers.delete(handler);
    }),
  },
  api: {},
}));

import {
  conclure,
  observerTransfertHistorique,
  type AvancementHistorique,
} from './historyTransfer';

/** Simule une notification du nœud. */
function pousser(method: string, params: unknown): void {
  for (const handler of [...handlers]) handler(method, params);
}

beforeEach(() => {
  handlers.clear();
});

describe('observerTransfertHistorique', () => {
  it('relaie chaque event.history_transfer', () => {
    const vus: AvancementHistorique[] = [];
    observerTransfertHistorique((a) => vus.push(a));

    pousser('event.history_transfer', {
      done: 2,
      total: 5,
      messages: 7,
      complete: false,
    });
    pousser('event.history_transfer', {
      done: 5,
      total: 5,
      messages: 9,
      complete: true,
    });

    expect(vus).toEqual([
      { done: 2, total: 5, messages: 7, complete: false },
      { done: 5, total: 5, messages: 9, complete: true },
    ]);
  });

  it('ignore les autres événements du nœud', () => {
    const onProgress = vi.fn();
    observerTransfertHistorique(onProgress);

    // Même famille de champs : c'est justement celui qu'on ne doit pas confondre.
    pousser('event.file_progress', {
      merkle_root: 'ab',
      done: 1,
      total: 4,
      complete: true,
    });

    expect(onProgress).not.toHaveBeenCalled();
  });

  it('comble les champs absents plutôt que de rendre des undefined', () => {
    const vus: AvancementHistorique[] = [];
    observerTransfertHistorique((a) => vus.push(a));

    pousser('event.history_transfer', {});

    expect(vus).toEqual([{ done: 0, total: 0, messages: 0, complete: false }]);
  });

  it('rend un désabonnement qui coupe vraiment le flux', () => {
    const onProgress = vi.fn();
    const off = observerTransfertHistorique(onProgress);

    off();
    pousser('event.history_transfer', { done: 1, total: 1, messages: 1, complete: true });

    expect(onProgress).not.toHaveBeenCalled();
  });
});

describe('conclure', () => {
  it('des pages reçues : rien à interpréter', () => {
    expect(conclure(3, 12)).toBe('recu');
    // Un carnet annoncé vide qui rend pourtant des pages reste un succès :
    // c'est l'arrivée de messages qui fait foi, pas le décompte.
    expect(conclure(0, 1)).toBe('recu');
  });

  it('carnet vide : il n’y avait rien à demander, aucun doute à fabriquer', () => {
    expect(conclure(0, 0)).toBe('carnet-vide');
  });

  it('zéro page avec un carnet non vide : ambigu, jamais « terminé »', () => {
    // 🔴 Le cas qui vaut ce module. Le nœud ne distingue pas « l'appareil d'en
    // face n'a rien de plus ancien » de « il tourne une version qui ignore la
    // demande » : les deux finissent ici, et l'interface doit nommer les deux.
    expect(conclure(1, 0)).toBe('ambigu');
    expect(conclure(42, 0)).toBe('ambigu');
  });
});
