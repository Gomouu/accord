/**
 * Tests de « Ajouter un appareil ».
 *
 * L'horloge est simulée : un compte à rebours qu'on attendrait vraiment ferait
 * un test de cinq minutes, et le sondage de l'offre n'apporterait jamais son
 * empreinte à temps.
 */

import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const pairStart = vi.fn();
const pairCancel = vi.fn();
const pairStatus = vi.fn();
const pairConfirm = vi.fn();

vi.mock('../../lib/client', () => ({
  api: {
    devicesPairStart: () => pairStart(),
    devicesPairCancel: () => pairCancel(),
    devicesPairStatus: () => pairStatus(),
    devicesPairConfirm: () => pairConfirm(),
  },
}));

import { formatRemaining, PairDeviceButton } from './PairDeviceButton';
import { fr } from '../../i18n/fr';
import { useUi } from '../../stores/ui';

const L = fr.settings;
const T0 = 1_700_000_000_000;
/** Cinq minutes, la durée de vie d'un code (`CODE_TTL_MS` côté nœud). */
const TTL = 5 * 60 * 1000;
/** L'empreinte que le nœud rend une fois l'échange abouti : six chiffres. */
const EMPREINTE = '481902';

beforeEach(() => {
  // `shouldAdvanceTime` est indispensable : sans lui, les utilitaires
  // asynchrones de testing-library attendent une horloge gelée et expirent.
  vi.useFakeTimers({ shouldAdvanceTime: true });
  vi.setSystemTime(T0);
  pairStart.mockReset();
  pairCancel.mockReset();
  pairStatus.mockReset();
  pairConfirm.mockReset();
  pairCancel.mockResolvedValue({});
  pairConfirm.mockResolvedValue({});
  // Par défaut, aucun échange n'a abouti : l'écran reste sur le code.
  pairStatus.mockResolvedValue({ fingerprint: null });
  useUi.setState({ lang: 'fr' });
});

afterEach(() => {
  vi.useRealTimers();
});

/** Ouvre une offre et amène l'écran jusqu'à l'étape de confirmation. */
async function jusquAlEmpreinte(): Promise<void> {
  pairStart.mockResolvedValue({ code: 'ABCD-EFGH', expires_ms: T0 + TTL });
  pairStatus
    .mockResolvedValueOnce({ fingerprint: null })
    .mockResolvedValue({ fingerprint: EMPREINTE });
  render(<PairDeviceButton />);
  fireEvent.click(screen.getByRole('button', { name: L.pairAdd }));
  await screen.findByText('ABCD-EFGH');
  await vi.advanceTimersByTimeAsync(3_000);
  await screen.findByText(EMPREINTE);
}

describe('formatRemaining', () => {
  it('rend un reste en m:ss, sans jamais passer sous zéro', () => {
    expect(formatRemaining(5 * 60 * 1000)).toBe('5:00');
    expect(formatRemaining(61_000)).toBe('1:01');
    expect(formatRemaining(1_000)).toBe('0:01');
    // Un reste négatif ne doit pas afficher « -0:01 » : l'écran dira « expiré ».
    expect(formatRemaining(-5_000)).toBe('0:00');
  });
});

describe('PairDeviceButton', () => {
  it('affiche le code et son compte à rebours après ouverture', async () => {
    pairStart.mockResolvedValue({ code: 'ABCD-EFGH', expires_ms: T0 + TTL });
    render(<PairDeviceButton />);

    fireEvent.click(screen.getByRole('button', { name: L.pairAdd }));

    expect(await screen.findByText('ABCD-EFGH')).toBeInTheDocument();
    expect(screen.getByText('Expire dans 5:00')).toBeInTheDocument();
  });

  it('décompte au fil des secondes', async () => {
    pairStart.mockResolvedValue({ code: 'ABCD-EFGH', expires_ms: T0 + TTL });
    render(<PairDeviceButton />);
    fireEvent.click(screen.getByRole('button', { name: L.pairAdd }));
    await screen.findByText('ABCD-EFGH');

    await vi.advanceTimersByTimeAsync(65_000);

    expect(screen.getByText('Expire dans 3:55')).toBeInTheDocument();
  });

  it('annonce l’expiration une fois l’échéance passée', async () => {
    // 🔒 Un code affiché sans mention d'expiration serait un code qu'on croit
    // encore valable — et une saisie qui échoue sans explication.
    pairStart.mockResolvedValue({ code: 'ABCD-EFGH', expires_ms: T0 + TTL });
    render(<PairDeviceButton />);
    fireEvent.click(screen.getByRole('button', { name: L.pairAdd }));
    await screen.findByText('ABCD-EFGH');

    await vi.advanceTimersByTimeAsync(TTL + 1_000);

    expect(screen.getByText(L.pairExpired)).toBeInTheDocument();
  });

  it('annuler prévient le nœud, pas seulement l’écran', async () => {
    // 🔒 Sinon le code resterait acceptable côté nœud alors que plus personne
    // ne regarde l'écran qui l'affichait.
    pairStart.mockResolvedValue({ code: 'ABCD-EFGH', expires_ms: T0 + TTL });
    render(<PairDeviceButton />);
    fireEvent.click(screen.getByRole('button', { name: L.pairAdd }));
    await screen.findByText('ABCD-EFGH');

    fireEvent.click(screen.getByRole('button', { name: L.pairCancel }));

    await waitFor(() => expect(pairCancel).toHaveBeenCalledTimes(1));
    expect(screen.queryByText('ABCD-EFGH')).not.toBeInTheDocument();
  });

  it('demander un nouveau code remplace l’affichage', async () => {
    pairStart
      .mockResolvedValueOnce({ code: 'ABCD-EFGH', expires_ms: T0 + TTL })
      .mockResolvedValueOnce({ code: 'JKMN-PQRS', expires_ms: T0 + TTL });
    render(<PairDeviceButton />);
    fireEvent.click(screen.getByRole('button', { name: L.pairAdd }));
    await screen.findByText('ABCD-EFGH');

    fireEvent.click(screen.getByRole('button', { name: L.pairNewCode }));

    expect(await screen.findByText('JKMN-PQRS')).toBeInTheDocument();
    expect(screen.queryByText('ABCD-EFGH')).not.toBeInTheDocument();
  });
});

describe('PairDeviceButton — confirmation d’empreinte', () => {
  it('reste sur le code tant que le nœud ne rend aucune empreinte', async () => {
    pairStart.mockResolvedValue({ code: 'ABCD-EFGH', expires_ms: T0 + TTL });
    render(<PairDeviceButton />);
    fireEvent.click(screen.getByRole('button', { name: L.pairAdd }));
    await screen.findByText('ABCD-EFGH');

    await vi.advanceTimersByTimeAsync(10_000);

    expect(pairStatus).toHaveBeenCalled();
    expect(screen.getByText('ABCD-EFGH')).toBeInTheDocument();
    expect(screen.queryByText(L.pairFingerprintHint)).not.toBeInTheDocument();
  });

  it('bascule sur l’empreinte dès que le nœud la rend', async () => {
    await jusquAlEmpreinte();

    expect(screen.getByText(EMPREINTE)).toBeInTheDocument();
    expect(screen.getByLabelText(L.pairFingerprintLabel)).toHaveTextContent(EMPREINTE);
    expect(screen.getByText(L.pairFingerprintHint)).toBeInTheDocument();
    // Le code n'a plus de rôle une fois l'échange abouti.
    expect(screen.queryByText('ABCD-EFGH')).not.toBeInTheDocument();
  });

  it('confirmer valide l’empreinte auprès du nœud', async () => {
    await jusquAlEmpreinte();

    fireEvent.click(screen.getByRole('button', { name: L.pairConfirm }));

    await waitFor(() => expect(pairConfirm).toHaveBeenCalledTimes(1));
    expect(await screen.findByRole('button', { name: L.pairAdd })).toBeInTheDocument();
  });

  it('annuler à l’étape d’empreinte prévient le nœud', async () => {
    await jusquAlEmpreinte();

    fireEvent.click(screen.getByRole('button', { name: L.pairCancel }));

    await waitFor(() => expect(pairCancel).toHaveBeenCalledTimes(1));
    expect(pairConfirm).not.toHaveBeenCalled();
    expect(screen.queryByText(EMPREINTE)).not.toBeInTheDocument();
  });

  it('n’offre aucun moyen de poursuivre sans confirmer', async () => {
    // 🔒 Le cœur de l'étape. Deux issues, et deux seulement : les nombres
    // concordent et on confirme, ou ils diffèrent et on annule. Un bouton
    // « continuer quand même » rendrait la comparaison décorative — c'est elle
    // qui transforme une fuite de code en tentative échouée.
    await jusquAlEmpreinte();

    const libelles = screen.getAllByRole('button').map((b) => b.textContent);
    expect(libelles).toEqual([L.pairConfirm, L.pairCancel]);
    // Et le libellé dit lui-même quoi faire en cas de divergence.
    expect(screen.getByText(L.pairFingerprintMismatch)).toBeInTheDocument();
    expect(L.pairFingerprintMismatch).toContain('annule');
  });

  it('cesse de sonder une fois l’empreinte obtenue', async () => {
    // Un intervalle qui continue d'interroger le nœud alors que l'écran attend
    // une décision humaine réveille l'application pour rien.
    await jusquAlEmpreinte();
    const appels = pairStatus.mock.calls.length;

    await vi.advanceTimersByTimeAsync(30_000);

    expect(pairStatus.mock.calls.length).toBe(appels);
  });

  it('cesse de sonder quand l’offre est annulée', async () => {
    pairStart.mockResolvedValue({ code: 'ABCD-EFGH', expires_ms: T0 + TTL });
    render(<PairDeviceButton />);
    fireEvent.click(screen.getByRole('button', { name: L.pairAdd }));
    await screen.findByText('ABCD-EFGH');
    await vi.advanceTimersByTimeAsync(3_000);

    fireEvent.click(screen.getByRole('button', { name: L.pairCancel }));
    await waitFor(() => expect(pairCancel).toHaveBeenCalledTimes(1));
    const appels = pairStatus.mock.calls.length;

    await vi.advanceTimersByTimeAsync(30_000);

    expect(pairStatus.mock.calls.length).toBe(appels);
  });

  it('cesse de sonder quand le code a expiré', async () => {
    pairStart.mockResolvedValue({ code: 'ABCD-EFGH', expires_ms: T0 + TTL });
    render(<PairDeviceButton />);
    fireEvent.click(screen.getByRole('button', { name: L.pairAdd }));
    await screen.findByText('ABCD-EFGH');

    await vi.advanceTimersByTimeAsync(TTL + 2_000);
    expect(screen.getByText(L.pairExpired)).toBeInTheDocument();
    const appels = pairStatus.mock.calls.length;

    await vi.advanceTimersByTimeAsync(30_000);

    expect(pairStatus.mock.calls.length).toBe(appels);
  });
});
