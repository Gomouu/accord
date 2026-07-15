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
      networkAddPeer: vi.fn(),
      networkRemovePeer: vi.fn(),
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

beforeEach(() => {
  statusMock.mockReset();
  addPeerMock.mockReset();
  statusMock.mockResolvedValue(STATUS);
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
});
