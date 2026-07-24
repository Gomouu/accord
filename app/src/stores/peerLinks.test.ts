/**
 * Qualité de lien par ami : ce que l'indicateur a le droit d'affirmer.
 *
 * Le piège est d'annoncer « hors ligne » sur la foi d'un état incomplet — un
 * nœud trop ancien pour `network.peers`, ou un diagnostic pas encore chargé.
 * Ces cas doivent rester silencieux plutôt que mentir.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { PeerLink } from '../lib/api';
import { qualityOf, rttOf, usePeerLinks } from './peerLinks';

const api = vi.hoisted(() => ({ networkPeers: vi.fn() }));
vi.mock('../lib/client', () => ({ api }));

function link(over: Partial<PeerLink>): PeerLink {
  return {
    pubkey: 'aa',
    live: true,
    addr: '203.0.113.7:48016',
    transport: 'direct',
    ...over,
  };
}

describe('qualityOf', () => {
  it('distingue direct, relayé et hors ligne', () => {
    const links = {
      direct: link({ pubkey: 'direct' }),
      relayed: link({ pubkey: 'relayed', transport: 'relay' }),
      down: link({ pubkey: 'down', live: false }),
    };
    expect(qualityOf(links, 'direct')).toBe('direct');
    expect(qualityOf(links, 'relayed')).toBe('relay');
    expect(qualityOf(links, 'down')).toBe('offline');
  });

  it('traite un pair inconnu du diagnostic comme hors ligne', () => {
    // Rien ne garantit qu'un message parte : c'est bien ce qu'il faut dire.
    expect(qualityOf({}, 'jamais_vu')).toBe('offline');
  });

  it('ne suppose pas « direct » quand le nœud ne renseigne pas le transport', () => {
    // Champ additif : un nœud antérieur à la 4.0 l'omet. Vivant sans précision
    // vaut mieux affiché comme direct que comme relais, mais surtout jamais
    // comme hors ligne.
    const sans: Record<string, PeerLink> = {
      a: { pubkey: 'a', live: true, addr: null },
    };
    expect(qualityOf(sans, 'a')).toBe('direct');
  });
});

describe('rttOf', () => {
  it('rend la latence mesurée, ou null quand aucun cycle n’a abouti', () => {
    const links = {
      mesure: link({ pubkey: 'mesure', rtt_ms: 42 }),
      sans: link({ pubkey: 'sans' }),
    };
    expect(rttOf(links, 'mesure')).toBe(42);
    expect(rttOf(links, 'sans')).toBeNull();
    expect(rttOf(links, 'absent')).toBeNull();
  });
});

describe('usePeerLinks.refresh', () => {
  beforeEach(() => {
    api.networkPeers.mockReset();
    usePeerLinks.getState().reset();
  });

  it('indexe les liens par clé publique', async () => {
    api.networkPeers.mockResolvedValue([
      link({ pubkey: 'aa' }),
      link({ pubkey: 'bb', transport: 'relay' }),
    ]);
    await usePeerLinks.getState().refresh();
    expect(Object.keys(usePeerLinks.getState().links).sort()).toEqual(['aa', 'bb']);
  });

  it('reste silencieux si le nœud ne connaît pas la méthode', async () => {
    // Un nœud plus ancien fait échouer l'appel : l'indicateur disparaît, mais
    // aucune erreur n'est montrée — ce n'est pas une panne pour l'utilisateur.
    api.networkPeers.mockRejectedValue(new Error('unknown method'));
    await expect(usePeerLinks.getState().refresh()).resolves.toBeUndefined();
    expect(usePeerLinks.getState().links).toEqual({});
  });

  it('remplace l’état plutôt que de le fusionner', async () => {
    api.networkPeers.mockResolvedValue([link({ pubkey: 'aa' })]);
    await usePeerLinks.getState().refresh();
    api.networkPeers.mockResolvedValue([link({ pubkey: 'bb' })]);
    await usePeerLinks.getState().refresh();
    // Un ami retiré ne doit pas laisser une pastille fantôme derrière lui.
    expect(Object.keys(usePeerLinks.getState().links)).toEqual(['bb']);
  });
});
