/**
 * Tests du côté nouvel appareil de l'appairage.
 *
 * L'horloge est simulée : l'écran attend l'empreinte par sondage, et abandonne
 * au bout des cinq minutes de vie d'un code — deux durées qu'on n'attendrait
 * pas vraiment.
 */

import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const pairSubmit = vi.fn();
const pairStatus = vi.fn();
const pairConfirm = vi.fn();
const pairCancel = vi.fn();

vi.mock('../../lib/client', () => ({
  api: {
    devicesPairSubmit: (code: string) => pairSubmit(code),
    devicesPairStatus: () => pairStatus(),
    devicesPairConfirm: () => pairConfirm(),
    devicesPairCancel: () => pairCancel(),
  },
}));

import { isCodeComplete, JoinDeviceForm } from './JoinDeviceForm';
import { fr } from '../../i18n/fr';
import { useUi } from '../../stores/ui';

// Libellés lus dans le dictionnaire plutôt que recopiés : le test suit une
// reformulation sans devenir faux.
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
  pairSubmit.mockReset();
  pairStatus.mockReset();
  pairConfirm.mockReset();
  pairCancel.mockReset();
  pairSubmit.mockResolvedValue({});
  pairConfirm.mockResolvedValue({});
  pairCancel.mockResolvedValue({});
  // Par défaut, l'échange n'a pas encore abouti : l'écran reste en attente.
  pairStatus.mockResolvedValue({ fingerprint: null });
  useUi.setState({ lang: 'fr' });
});

afterEach(() => {
  vi.useRealTimers();
});

/** Saisit un code et l'envoie. */
function saisir(code: string): void {
  fireEvent.change(screen.getByLabelText(L.pairJoinLabel), { target: { value: code } });
  fireEvent.click(screen.getByRole('button', { name: L.pairJoinSubmit }));
}

/** Envoie un code accepté et amène l'écran jusqu'à l'étape de confirmation. */
async function jusquAlEmpreinte(): Promise<void> {
  pairStatus
    .mockResolvedValueOnce({ fingerprint: null })
    .mockResolvedValue({ fingerprint: EMPREINTE });
  render(<JoinDeviceForm />);
  saisir('abcd-efgh');
  await screen.findByText(L.pairJoinWaiting);
  await vi.advanceTimersByTimeAsync(3_000);
  await screen.findByText(EMPREINTE);
}

describe('isCodeComplete', () => {
  it('accepte un code recopié avec espaces, tirets ou minuscules', () => {
    // Le code se lit sur un autre écran : la ponctuation de la recopie ne dit
    // rien de sa validité, et c'est le nœud qui normalise.
    expect(isCodeComplete('ABCDEFGH')).toBe(true);
    expect(isCodeComplete('abcd-efgh')).toBe(true);
    expect(isCodeComplete('  ABCD EFGH  ')).toBe(true);
    expect(isCodeComplete('a-b-c-d-e-f-g-h')).toBe(true);
  });

  it('refuse une longueur qui n’est pas celle d’un code', () => {
    expect(isCodeComplete('')).toBe(false);
    expect(isCodeComplete('ABCDEFG')).toBe(false);
    expect(isCodeComplete('ABCDEFGHJ')).toBe(false);
    expect(isCodeComplete('----')).toBe(false);
  });

  it('laisse passer un caractère ambigu au lieu de le corriger', () => {
    // 🔒 `0`, `O`, `1`, `I` et `L` sont hors alphabet. Les remplacer d'office
    // enverrait un code que l'utilisateur croit avoir tapé alors qu'il en a
    // tapé un autre : c'est au nœud de refuser, et à l'écran de le dire.
    expect(isCodeComplete('ABCD-EF0H')).toBe(true);
    expect(isCodeComplete('IL01ABCD')).toBe(true);
  });
});

describe('JoinDeviceForm — saisie', () => {
  it('n’envoie rien tant que le code n’a pas la bonne longueur', () => {
    render(<JoinDeviceForm />);

    fireEvent.change(screen.getByLabelText(L.pairJoinLabel), {
      target: { value: 'ABCD' },
    });

    expect(screen.getByRole('button', { name: L.pairJoinSubmit })).toBeDisabled();
    expect(pairSubmit).not.toHaveBeenCalled();
  });

  it('envoie le code tel qu’il a été recopié, sans le retoucher', async () => {
    // 🔒 La normalisation appartient au nœud. Un écran qui nettoie de son côté
    // ferait diverger deux règles qui doivent rester une seule.
    render(<JoinDeviceForm />);

    saisir('  abcd-efgh ');

    await waitFor(() => expect(pairSubmit).toHaveBeenCalledWith('  abcd-efgh '));
  });

  it('passe en attente une fois le code accepté', async () => {
    render(<JoinDeviceForm />);

    saisir('ABCD-EFGH');

    expect(await screen.findByText(L.pairJoinWaiting)).toBeInTheDocument();
  });

  it('affiche le refus du nœud et reste sur la saisie', async () => {
    // Un caractère ambigu part se faire refuser : l'écran ne l'a pas corrigé,
    // et il rend le refus lisible plutôt que de rester muet.
    pairSubmit.mockRejectedValue(new Error('caractère hors alphabet'));
    render(<JoinDeviceForm />);

    saisir('ABCD-EF0H');

    expect(await screen.findByText(L.pairJoinRejected)).toBeInTheDocument();
    expect(screen.queryByText(L.pairJoinWaiting)).not.toBeInTheDocument();
    expect(screen.getByLabelText(L.pairJoinLabel)).toBeInTheDocument();
  });

  it('ne sonde pas le nœud après un envoi refusé', async () => {
    pairSubmit.mockRejectedValue(new Error('code refusé'));
    render(<JoinDeviceForm />);

    saisir('ABCD-EFGH');
    await screen.findByText(L.pairJoinRejected);
    await vi.advanceTimersByTimeAsync(10_000);

    expect(pairStatus).not.toHaveBeenCalled();
  });

  it('annuler pendant l’attente prévient le nœud', async () => {
    // 🔒 Sinon la tentative resterait ouverte côté nœud alors que plus
    // personne ne l'attend.
    render(<JoinDeviceForm />);
    saisir('ABCD-EFGH');
    await screen.findByText(L.pairJoinWaiting);

    fireEvent.click(screen.getByRole('button', { name: L.pairCancel }));

    await waitFor(() => expect(pairCancel).toHaveBeenCalledTimes(1));
    expect(await screen.findByLabelText(L.pairJoinLabel)).toBeInTheDocument();
  });
});

describe('JoinDeviceForm — confirmation d’empreinte', () => {
  it('reste en attente tant que le nœud ne rend aucune empreinte', async () => {
    render(<JoinDeviceForm />);
    saisir('ABCD-EFGH');
    await screen.findByText(L.pairJoinWaiting);

    await vi.advanceTimersByTimeAsync(10_000);

    expect(pairStatus).toHaveBeenCalled();
    expect(screen.getByText(L.pairJoinWaiting)).toBeInTheDocument();
    expect(screen.queryByText(L.pairFingerprintHint)).not.toBeInTheDocument();
  });

  it('bascule sur l’empreinte dès que le nœud la rend', async () => {
    await jusquAlEmpreinte();

    expect(screen.getByLabelText(L.pairFingerprintLabel)).toHaveTextContent(EMPREINTE);
    expect(screen.getByText(L.pairFingerprintHint)).toBeInTheDocument();
    expect(screen.queryByText(L.pairJoinWaiting)).not.toBeInTheDocument();
  });

  it('n’offre aucun moyen de poursuivre sans confirmer', async () => {
    // 🔒 Le cœur de l'étape, des deux côtés à l'identique : les nombres
    // concordent et on confirme, ou ils diffèrent et on annule. Un bouton
    // « continuer quand même » rendrait la comparaison décorative — c'est elle
    // qui transforme une fuite de code en tentative échouée.
    await jusquAlEmpreinte();

    const libelles = screen.getAllByRole('button').map((b) => b.textContent);
    expect(libelles).toEqual([L.pairConfirm, L.pairCancel]);
    expect(screen.getByText(L.pairFingerprintMismatch)).toBeInTheDocument();
  });

  it('confirmer valide l’empreinte auprès du nœud et rend la saisie vierge', async () => {
    await jusquAlEmpreinte();

    fireEvent.click(screen.getByRole('button', { name: L.pairConfirm }));

    await waitFor(() => expect(pairConfirm).toHaveBeenCalledTimes(1));
    expect(await screen.findByLabelText(L.pairJoinLabel)).toHaveValue('');
  });

  it('annuler à l’étape d’empreinte prévient le nœud et oublie le code', async () => {
    // 🔒 Un code abandonné parce que les empreintes divergeaient ne doit pas
    // rester dans le champ, à portée d'un second envoi.
    await jusquAlEmpreinte();

    fireEvent.click(screen.getByRole('button', { name: L.pairCancel }));

    await waitFor(() => expect(pairCancel).toHaveBeenCalledTimes(1));
    expect(pairConfirm).not.toHaveBeenCalled();
    expect(screen.queryByText(EMPREINTE)).not.toBeInTheDocument();
    expect(await screen.findByLabelText(L.pairJoinLabel)).toHaveValue('');
  });

  it('cesse de sonder une fois l’empreinte obtenue', async () => {
    // L'écran attend maintenant une décision humaine : continuer d'interroger
    // le nœud réveillerait l'application pour rien.
    await jusquAlEmpreinte();
    const appels = pairStatus.mock.calls.length;

    await vi.advanceTimersByTimeAsync(30_000);

    expect(pairStatus.mock.calls.length).toBe(appels);
  });

  it('abandonne l’attente passé la durée de vie d’un code', async () => {
    // Un code ne vit que cinq minutes : au-delà, plus rien ne viendra, et le
    // nœud doit oublier la tentative comme l'écran l'oublie.
    render(<JoinDeviceForm />);
    saisir('ABCD-EFGH');
    await screen.findByText(L.pairJoinWaiting);

    await vi.advanceTimersByTimeAsync(TTL + 2_000);

    expect(await screen.findByText(L.pairExpired)).toBeInTheDocument();
    await waitFor(() => expect(pairCancel).toHaveBeenCalledTimes(1));
    const appels = pairStatus.mock.calls.length;
    await vi.advanceTimersByTimeAsync(30_000);
    expect(pairStatus.mock.calls.length).toBe(appels);
  });
});
