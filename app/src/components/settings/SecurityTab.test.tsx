/**
 * Tests de l'onglet Sécurité (jalon 2, lots 2.C et 2.D) : état par contact,
 * compteur local, bascule de l'exigence — et le garde-fou de formulation, qui
 * est la seule partie de cet écran dont une régression serait silencieuse.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Mock } from 'vitest';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';

vi.mock('../../lib/client', () => {
  const handlers = new Set<(method: string, params: unknown) => void>();
  return {
    api: {
      securityState: vi.fn(),
      securitySetRequireHybrid: vi.fn(),
      networkPeers: vi.fn(),
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

import { api, rpc } from '../../lib/client';
import { SecurityTab, etatChiffrement, part } from './SecurityTab';

const stateMock = api.securityState as unknown as Mock;
const setRequireMock = api.securitySetRequireHybrid as unknown as Mock;
const peersMock = api.networkPeers as unknown as Mock;
const fakeRpc = rpc as unknown as {
  emitEvent: (method: string, params: unknown) => void;
};

const ETAT = {
  hybrid_supported: true,
  require_hybrid: false,
  hybrid_sessions: 3,
  classic_sessions: 1,
};

async function renderTab(): Promise<void> {
  render(<SecurityTab />);
  await act(async () => {});
}

beforeEach(() => {
  stateMock.mockReset();
  setRequireMock.mockReset();
  peersMock.mockReset();
  stateMock.mockResolvedValue(ETAT);
  peersMock.mockResolvedValue([]);
});

describe('etatChiffrement', () => {
  const libelles = { hybride: 'HYB', classique: 'CLA', aucune: 'AUCUNE' };

  it('distingue session hybride, classique et absente', () => {
    expect(
      etatChiffrement(
        { pubkey: 'a', live: true, addr: null, post_quantum: true },
        libelles,
      ),
    ).toBe('HYB');
    expect(
      etatChiffrement(
        { pubkey: 'a', live: true, addr: null, post_quantum: false },
        libelles,
      ),
    ).toBe('CLA');
    expect(
      etatChiffrement(
        { pubkey: 'a', live: false, addr: null, post_quantum: true },
        libelles,
      ),
    ).toBe('AUCUNE');
  });

  it('reste muet quand le nœud ne renseigne pas le champ', () => {
    // ⚠️ « On ne sait pas » n'est pas « standard » : rabattre l'un sur l'autre
    // afficherait une affirmation que rien n'appuie.
    expect(etatChiffrement({ pubkey: 'a', live: true, addr: null }, libelles)).toBe('');
  });
});

describe('part', () => {
  it('rend un pourcentage entier, et rien quand le total est nul', () => {
    expect(part(3, 4)).toBe(75);
    expect(part(0, 4)).toBe(0);
    expect(part(0, 0)).toBeNull();
  });
});

describe('SecurityTab', () => {
  it('affiche le compteur local et sa part', async () => {
    await renderTab();
    await waitFor(() => expect(screen.getByText('3')).toBeInTheDocument());
    expect(screen.getByText('75 %')).toBeInTheDocument();
    expect(screen.getByText('25 %')).toBeInTheDocument();
  });

  it('dit explicitement que le compteur ne sort pas de l’appareil', async () => {
    await renderTab();
    expect(
      await screen.findByText(/counted on this device and never leave it/i),
    ).toBeInTheDocument();
  });

  it('nomme l’état de chiffrement de chaque contact connecté', async () => {
    peersMock.mockResolvedValue([
      { pubkey: 'aa'.repeat(32), live: true, addr: null, post_quantum: true },
      { pubkey: 'bb'.repeat(32), live: true, addr: null, post_quantum: false },
    ]);
    await renderTab();
    expect(
      await screen.findByText(/Reinforced encryption \(post-quantum\)/i),
    ).toBeInTheDocument();
    expect(screen.getByText(/^Standard encryption$/i)).toBeInTheDocument();
  });

  it('bascule l’exigence d’hybride et reflète l’état rendu par le nœud', async () => {
    setRequireMock.mockResolvedValue({ ...ETAT, require_hybrid: true });
    await renderTab();
    const bascule = await screen.findByRole('switch', {
      name: /Require reinforced encryption/i,
    });
    expect(bascule).toHaveAttribute('aria-checked', 'false');
    fireEvent.click(bascule);
    await waitFor(() => expect(setRequireMock).toHaveBeenCalledWith(true));
    await waitFor(() => expect(bascule).toHaveAttribute('aria-checked', 'true'));
    expect(screen.getByText(/standard sessions are refused/i)).toBeInTheDocument();
  });

  it('ne fait pas croire que l’exigence est posée si le nœud refuse', async () => {
    // Le pire échec possible ici serait un réglage qui paraît actif sans l'être.
    setRequireMock.mockRejectedValue(new Error('non'));
    await renderTab();
    const bascule = await screen.findByRole('switch', {
      name: /Require reinforced encryption/i,
    });
    fireEvent.click(bascule);
    await waitFor(() => expect(setRequireMock).toHaveBeenCalled());
    expect(bascule).toHaveAttribute('aria-checked', 'false');
  });

  it('relit l’état sur event.network', async () => {
    await renderTab();
    expect(stateMock).toHaveBeenCalledTimes(1);
    await act(async () => {
      fakeRpc.emitEvent('event.network', {});
    });
    await waitFor(() => expect(stateMock).toHaveBeenCalledTimes(2));
  });

  it('reste lisible quand le nœud ne sait pas répondre', async () => {
    stateMock.mockRejectedValue(new Error('méthode inconnue'));
    await renderTab();
    expect(
      await screen.findByText(/Encryption state not available on this node/i),
    ).toBeInTheDocument();
  });

  it('ne promet rien : aucun superlatif dans l’explication de l’hybride', async () => {
    // 🔒 Garde-fou de formulation, côté explication longue. C'est la chaîne la
    // plus tentante à « renforcer » un jour ; le test rend le glissement rouge.
    await renderTab();
    const explication = await screen.findByText(/A message intercepted today/i);
    const texte = explication.textContent ?? '';
    for (const proscrit of [
      /unbreakable/i,
      /forever/i,
      /impenetrable/i,
      /unhackable/i,
      /absolutely secure/i,
    ]) {
      expect(texte).not.toMatch(proscrit);
    }
    expect(texte).toMatch(/known to date/i);
  });
});
