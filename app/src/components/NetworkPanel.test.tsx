/**
 * Tests du panneau réseau (désormais dans l'onglet « Ajouter un ami ») :
 * classement adresse publique / locale, ajout d'un pair par adresse, et
 * rafraîchissement sur event.network.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Mock } from 'vitest';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';

vi.mock('../lib/client', () => {
  const handlers = new Set<(method: string, params: unknown) => void>();
  return {
    api: {
      networkStatus: vi.fn(),
      networkPeers: vi.fn(),
      networkAddPeer: vi.fn(),
      networkRemovePeer: vi.fn(),
      diagnosticsCounters: vi.fn(),
      diagnosticsSelftest: vi.fn(),
      diagnosticsReport: vi.fn(),
    },
    rpc: {
      onEvent: (handler: (method: string, params: unknown) => void) => {
        handlers.add(handler);
        return () => handlers.delete(handler);
      },
      emitEvent: (method: string, params: unknown) => {
        for (const handler of handlers) handler(method, params);
      },
    },
  };
});

import { api, rpc } from '../lib/client';
import { NetworkPanel, isLocalAddr } from './NetworkPanel';

const statusMock = api.networkStatus as unknown as Mock;
const addPeerMock = api.networkAddPeer as unknown as Mock;
const fakeRpc = rpc as unknown as {
  emitEvent: (method: string, params: unknown) => void;
};

const STATUS = {
  p2p_port: 48016,
  local_addrs: ['203.0.113.4:48016', '192.168.1.11:48016'],
  bootstrap: [],
  connected_peers: 0,
  dht_nodes: 0,
  external_addr: null,
  port_mapping: 'aucun',
  lan_peers: 0,
};

async function renderPanel(): Promise<void> {
  render(<NetworkPanel />);
  await act(async () => {});
}

const peersMock = api.networkPeers as unknown as Mock;
const countersMock = api.diagnosticsCounters as unknown as Mock;
const selftestMock = api.diagnosticsSelftest as unknown as Mock;
const reportMock = api.diagnosticsReport as unknown as Mock;

const COUNTERS = {
  punch: { requested: 4, received: 3, ok: 2, fail: 1 },
  relay: { open_ok: 1, open_fail: 0 },
  mailbox: { deposits: 5, pickups: 3 },
  outbox: { enqueued: 2, flushed: 2 },
  reconnect: { attempts: 3, ok: 3 },
};

beforeEach(() => {
  statusMock.mockReset();
  addPeerMock.mockReset();
  peersMock.mockReset();
  countersMock.mockReset();
  selftestMock.mockReset();
  reportMock.mockReset();
  statusMock.mockResolvedValue(STATUS);
  peersMock.mockResolvedValue([]);
  countersMock.mockResolvedValue(COUNTERS);
  selftestMock.mockResolvedValue(null);
});

describe('isLocalAddr', () => {
  it('classe les adresses locales et publiques', () => {
    // Locales (réseau seulement).
    for (const a of [
      '192.168.1.11:48016',
      '10.230.134.190:48016',
      '172.16.0.5:48016',
      '169.254.1.2:48016',
      '127.0.0.1:48016',
      '100.100.0.1:48016',
      '[fe80::1]:48016',
      '[fd12::1]:48016',
      '[::1]:48016',
    ]) {
      expect(isLocalAddr(a), a).toBe(true);
    }
    // Publiques (joignables depuis Internet), dont IPv6 globale.
    for (const a of [
      '203.0.113.4:48016',
      '[2001:861:324c:40b0::1]:48016',
      '[2a01:e0a:157:b7a0::1]:48016',
    ]) {
      expect(isLocalAddr(a), a).toBe(false);
    }
  });
});

describe('NetworkPanel', () => {
  it('sépare l’adresse publique de l’adresse locale', async () => {
    await renderPanel();
    expect(await screen.findByText('203.0.113.4:48016')).toBeInTheDocument();
    expect(screen.getByText('192.168.1.11:48016')).toBeInTheDocument();
    // Les deux en-têtes sont présents (locale de test : anglais).
    expect(screen.getByText(/reachable from the internet/i)).toBeInTheDocument();
    expect(screen.getByText(/same Wi-Fi network/i)).toBeInTheDocument();
  });

  it('invite à connecter le réseau quand aucune adresse publique n’existe', async () => {
    statusMock.mockResolvedValue({ ...STATUS, local_addrs: ['192.168.1.11:48016'] });
    await renderPanel();
    expect(await screen.findByText(/No public address known/i)).toBeInTheDocument();
  });

  it('ajoute un pair par son adresse via network.add_peer', async () => {
    addPeerMock.mockResolvedValue({ ...STATUS, bootstrap: ['198.51.100.7:48016'] });
    await renderPanel();

    const input = screen.getByPlaceholderText(/ip:port/i);
    fireEvent.change(input, { target: { value: '198.51.100.7:48016' } });
    fireEvent.keyDown(input, { key: 'Enter' });

    await waitFor(() => expect(addPeerMock).toHaveBeenCalledWith('198.51.100.7:48016'));
    expect(await screen.findByText('198.51.100.7:48016')).toBeInTheDocument();
  });

  it('rafraîchit l’état sur event.network', async () => {
    await renderPanel();
    expect(statusMock).toHaveBeenCalledTimes(1);

    statusMock.mockResolvedValue({ ...STATUS, connected_peers: 2 });
    await act(async () => {
      fakeRpc.emitEvent('event.network', { connected_peers: 2, dht_nodes: 5 });
    });

    await waitFor(() => expect(statusMock).toHaveBeenCalledTimes(2));
  });

  it('affiche l’état de connexion par ami (connecté/hors ligne + adresse)', async () => {
    peersMock.mockResolvedValue([
      { pubkey: 'aa'.repeat(32), live: true, addr: '203.0.113.9:48016' },
      { pubkey: 'bb'.repeat(32), live: false, addr: null },
    ]);
    await renderPanel();
    expect(await screen.findByText('Connected')).toBeInTheDocument();
    expect(screen.getByText('Offline')).toBeInTheDocument();
    expect(screen.getByText(/203\.0\.113\.9:48016/)).toBeInTheDocument();
    expect(screen.getByText(/Address never learned/i)).toBeInTheDocument();
  });
});

describe('NetworkPanel — mention du chiffrement (jalon 2)', () => {
  it('nomme le chiffrement renforcé sur une session hybride, neutre sinon', async () => {
    peersMock.mockResolvedValue([
      {
        pubkey: 'aa'.repeat(32),
        live: true,
        addr: '203.0.113.9:48016',
        post_quantum: true,
      },
      {
        pubkey: 'bb'.repeat(32),
        live: true,
        addr: '203.0.113.8:48016',
        post_quantum: false,
      },
    ]);
    await renderPanel();
    expect(
      await screen.findByText(/Reinforced encryption \(post-quantum\)/i),
    ).toBeInTheDocument();
    expect(screen.getByText(/^Standard encryption$/i)).toBeInTheDocument();
  });

  it('ne promet rien à l’utilisateur : aucun superlatif dans les libellés', async () => {
    // 🔒 Garde-fou de formulation. Un jour quelqu'un « améliorera » ces
    // chaînes ; ce test rend le glissement visible au lieu de le laisser
    // partir en production dans dix langues.
    peersMock.mockResolvedValue([
      { pubkey: 'aa'.repeat(32), live: true, addr: null, post_quantum: true },
    ]);
    await renderPanel();
    const mention = await screen.findByText(/Reinforced encryption/i);
    const texte = `${mention.textContent ?? ''} ${mention.getAttribute('title') ?? ''}`;
    for (const proscrit of [/unbreakable/i, /forever/i, /impenetrable/i, /unhackable/i]) {
      expect(texte).not.toMatch(proscrit);
    }
    expect(texte).toMatch(/known to date/i);
  });

  it('n’affirme rien quand le nœud ne renseigne pas le champ', async () => {
    // Un nœud antérieur au jalon 2 omet `post_quantum` : « standard » serait
    // une affirmation sans fondement.
    peersMock.mockResolvedValue([
      { pubkey: 'aa'.repeat(32), live: true, addr: '203.0.113.9:48016' },
    ]);
    await renderPanel();
    expect(await screen.findByText('Connected')).toBeInTheDocument();
    expect(screen.queryByText(/encryption/i)).not.toBeInTheDocument();
  });
});

describe('NetworkPanel — diagnostic (4.0)', () => {
  it('affiche le type de NAT et les compteurs de diagnostic', async () => {
    statusMock.mockResolvedValue({ ...STATUS, nat_kind: 'cone' });
    await renderPanel();
    await waitFor(() => expect(screen.getByText(/cone/i)).toBeInTheDocument());
    // Poinçonnage : ok / requested = 2 / 4 (voir COUNTERS).
    expect(screen.getByText('2 / 4')).toBeInTheDocument();
  });

  it('montre le lien relayé et la latence d’un ami connecté', async () => {
    peersMock.mockResolvedValue([
      {
        pubkey: 'alice',
        live: true,
        addr: null,
        transport: 'relay',
        relay: '9.9.9.9:48016',
        rtt_ms: 42,
        last_recv_age_ms: 100,
        last_delivery_ms: null,
      },
    ]);
    await renderPanel();
    await waitFor(() => expect(screen.getByText('Relay')).toBeInTheDocument());
    expect(screen.getByText(/42 ms/)).toBeInTheDocument();
    expect(screen.getByText(/9\.9\.9\.9:48016/)).toBeInTheDocument();
  });

  it('lance l’auto-test et affiche le verdict de joignabilité', async () => {
    selftestMock.mockResolvedValue({
      p2p_port: 48016,
      nat_kind: 'symmetric',
      port_mapping: 'aucun',
      external_addr: null,
      observed_consensus: null,
      dht_nodes: 5,
      connected_peers: 1,
      relay_eligible: true,
      bootstrap: [{ addr: '1.1.1.1:48016', ok: true }],
      relay_probe: { addr: '2.2.2.2:48016', ok: true },
      reachability: 'relay',
    });
    await renderPanel();
    fireEvent.click(screen.getByRole('button', { name: /self-test/i }));
    await waitFor(() => expect(screen.getByText('Via relay')).toBeInTheDocument());
    expect(screen.getByText(/1\.1\.1\.1:48016/)).toBeInTheDocument();
  });
});

describe('rapport de diagnostic', () => {
  const RAPPORT = {
    version: '7.1.0',
    platform: 'macos/aarch64',
    counters: COUNTERS,
    selftest: {
      p2p_port: 48016,
      nat_kind: 'cone',
      port_mapping: 'upnp',
      external_addr: 'masqué:48016',
      observed_consensus: null,
      dht_nodes: 5,
      connected_peers: 1,
      relay_eligible: false,
      bootstrap: [],
      relay_probe: null,
      reachability: 'punch',
    },
    links: [
      {
        peer: 1,
        live: true,
        transport: 'direct',
        relay: null,
        last_recv_age_ms: 800,
        rtt_ms: 42,
        capabilities: 0,
      },
    ],
  };

  /**
   * Presse-papiers de test. Il accumule ce qui lui est écrit plutôt que de
   * laisser le test aller le chercher dans `mock.calls[0][0]` : sous
   * `noUncheckedIndexedAccess`, cet accès n'est pas typable sans assertion,
   * et une assertion sur un tableau vide masquerait justement le cas « rien
   * n'a été copié » que le dernier test vérifie.
   */
  function presse_papiers(): { writeText: Mock; copie: () => string } {
    const ecrits: string[] = [];
    const writeText = vi.fn((valeur: string) => {
      ecrits.push(valeur);
      return Promise.resolve();
    });
    Object.assign(navigator, { clipboard: { writeText } });
    return { writeText, copie: () => ecrits.join('') };
  }

  it('copie le rapport rendu par le nœud et le confirme', async () => {
    const { writeText, copie } = presse_papiers();
    reportMock.mockResolvedValue(RAPPORT);
    await renderPanel();

    fireEvent.click(screen.getByRole('button', { name: /diagnostic report/i }));

    await waitFor(() => expect(writeText).toHaveBeenCalledTimes(1));
    expect(JSON.parse(copie())).toEqual(RAPPORT);
    await waitFor(() =>
      expect(screen.getByText(/Diagnostic report copied/)).toBeInTheDocument(),
    );
  });

  it('ne recompose jamais le rapport à partir de network.peers', async () => {
    // 🔒 Le panneau AFFICHE la clé et l'adresse de chaque ami — c'est son
    // travail, l'utilisateur les connaît déjà. Le rapport, lui, part chez un
    // inconnu. Ce test épingle que le presse-papiers reçoit ce que le nœud a
    // caviardé, et rien qui vienne de la liste affichée à l'écran.
    const { writeText, copie } = presse_papiers();
    peersMock.mockResolvedValue([
      {
        pubkey: 'bb'.repeat(32),
        live: true,
        addr: '198.51.100.42:48016',
        transport: 'direct',
        relay: null,
        last_recv_age_ms: 800,
        rtt_ms: 42,
        last_delivery_ms: null,
        capabilities: 0,
      },
    ]);
    reportMock.mockResolvedValue(RAPPORT);
    await renderPanel();

    fireEvent.click(screen.getByRole('button', { name: /diagnostic report/i }));
    await waitFor(() => expect(writeText).toHaveBeenCalledTimes(1));

    expect(copie()).not.toContain('bb'.repeat(32));
    expect(copie()).not.toContain('198.51.100.42');
  });

  it('signale l’échec sans rien copier', async () => {
    const { writeText } = presse_papiers();
    reportMock.mockRejectedValue(new Error('nœud trop ancien'));
    await renderPanel();

    fireEvent.click(screen.getByRole('button', { name: /diagnostic report/i }));

    await waitFor(() =>
      expect(screen.getByText(/Could not produce the report/)).toBeInTheDocument(),
    );
    expect(writeText).not.toHaveBeenCalled();
  });
});
