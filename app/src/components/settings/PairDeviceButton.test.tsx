/**
 * Tests de « Ajouter un appareil ».
 *
 * L'horloge est simulée : un compte à rebours qu'on attendrait vraiment ferait
 * un test de cinq minutes.
 */

import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const pairStart = vi.fn();
const pairCancel = vi.fn();

vi.mock('../../lib/client', () => ({
  api: {
    devicesPairStart: () => pairStart(),
    devicesPairCancel: () => pairCancel(),
  },
}));

import { formatRemaining, PairDeviceButton } from './PairDeviceButton';
import { fr } from '../../i18n/fr';
import { useUi } from '../../stores/ui';

const L = fr.settings;
const T0 = 1_700_000_000_000;
/** Cinq minutes, la durée de vie d'un code (`CODE_TTL_MS` côté nœud). */
const TTL = 5 * 60 * 1000;

beforeEach(() => {
  // `shouldAdvanceTime` est indispensable : sans lui, les utilitaires
  // asynchrones de testing-library attendent une horloge gelée et expirent.
  vi.useFakeTimers({ shouldAdvanceTime: true });
  vi.setSystemTime(T0);
  pairStart.mockReset();
  pairCancel.mockReset();
  pairCancel.mockResolvedValue({});
  useUi.setState({ lang: 'fr' });
});

afterEach(() => {
  vi.useRealTimers();
});

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
