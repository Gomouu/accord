/**
 * Tests du côté nouvel appareil de l'appairage, jusqu'à l'adoption du compte.
 *
 * L'horloge est simulée : l'écran attend l'empreinte puis la racine du compte
 * par sondage, et abandonne au bout de durées qu'on n'attendrait pas vraiment.
 */

import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const pairSubmit = vi.fn();
const pairStatus = vi.fn();
const pairConfirm = vi.fn();
const pairCancel = vi.fn();

// Seule `api` est bouchonnée : le store de session s'abonne au vrai `rpc` dès
// sa création, et un module entièrement remplacé le priverait de cet export.
vi.mock('../../lib/client', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../lib/client')>()),
  api: {
    devicesPairSubmit: (code: string) => pairSubmit(code),
    devicesPairStatus: () => pairStatus(),
    devicesPairConfirm: () => pairConfirm(),
    devicesPairCancel: () => pairCancel(),
  },
}));

import { isCodeComplete, JoinDeviceForm } from './JoinDeviceForm';
import { frSettings } from '../../i18n/fr.settings';
import { fr } from '../../i18n/fr';
import { useSession } from '../../stores/session';
import { useUi } from '../../stores/ui';

// Libellés lus dans le dictionnaire plutôt que recopiés : le test suit une
// reformulation sans devenir faux.
const L = frSettings.settings;
const O = fr.onboarding;
const T0 = 1_700_000_000_000;
/** Cinq minutes, la durée de vie d'un code (`CODE_TTL_MS` côté nœud). */
const TTL = 5 * 60 * 1000;
/** Une minute, l'attente maximale de la racine après confirmation. */
const ADOPT_TTL = 60 * 1000;
/** L'empreinte que le nœud rend une fois l'échange abouti : six chiffres. */
const EMPREINTE = '481902';
/** Une phrase de passe locale valable (12 caractères au moins). */
const PHRASE_DE_PASSE = 'phrase-de-passe-locale';

/** L'adoption elle-même : c'est elle qui bascule sur le compte reçu. */
const adopter = vi.fn();

/** Réponse de `devices.pair_status`, dont les trois champs comptent. */
function statut(over: { fingerprint?: string | null; adopted?: boolean } = {}): {
  fingerprint: string | null;
  role: 'joiner';
  adopted: boolean;
} {
  return { fingerprint: null, adopted: false, role: 'joiner', ...over };
}

beforeEach(() => {
  // `shouldAdvanceTime` est indispensable : sans lui, les utilitaires
  // asynchrones de testing-library attendent une horloge gelée et expirent.
  vi.useFakeTimers({ shouldAdvanceTime: true });
  vi.setSystemTime(T0);
  pairSubmit.mockReset();
  pairStatus.mockReset();
  pairConfirm.mockReset();
  pairCancel.mockReset();
  adopter.mockReset();
  pairSubmit.mockResolvedValue({});
  pairConfirm.mockResolvedValue({});
  pairCancel.mockResolvedValue({});
  adopter.mockResolvedValue(undefined);
  // Par défaut, l'échange n'a pas encore abouti : l'écran reste en attente.
  pairStatus.mockResolvedValue(statut());
  useUi.setState({ lang: 'fr', toasts: [] });
  useSession.setState({ adoptPairedAccount: adopter });
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
    .mockResolvedValueOnce(statut())
    .mockResolvedValue(statut({ fingerprint: EMPREINTE }));
  render(<JoinDeviceForm />);
  saisir('abcd-efgh');
  await screen.findByText(L.pairJoinWaiting);
  await vi.advanceTimersByTimeAsync(3_000);
  await screen.findByText(EMPREINTE);
}

/** Confirme l'empreinte, puis laisse le nœud annoncer la racine reçue. */
async function jusquALaPhraseDePasse(): Promise<void> {
  await jusquAlEmpreinte();
  pairStatus.mockResolvedValue(statut({ fingerprint: EMPREINTE, adopted: true }));
  fireEvent.click(screen.getByRole('button', { name: L.pairConfirm }));
  await screen.findByText(L.pairAdoptWaiting);
  await vi.advanceTimersByTimeAsync(3_000);
  await screen.findByLabelText(O.passphrase);
}

/** Renseigne la phrase de passe locale et lance l'adoption. */
function adopterAvec(phrase: string): void {
  fireEvent.change(screen.getByLabelText(O.passphrase), { target: { value: phrase } });
  fireEvent.click(screen.getByRole('button', { name: L.pairAdoptSubmit }));
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

  it('confirmer valide l’empreinte auprès du nœud et passe à la réception', async () => {
    await jusquAlEmpreinte();

    fireEvent.click(screen.getByRole('button', { name: L.pairConfirm }));

    await waitFor(() => expect(pairConfirm).toHaveBeenCalledTimes(1));
    expect(await screen.findByText(L.pairAdoptWaiting)).toBeInTheDocument();
  });

  it('n’annonce AUCUN succès en confirmant : rien n’a encore été adopté', async () => {
    // 🔒 Le piège de cet écran, et la raison de tout ce qui suit. Confirmer
    // termine l'appairage sur l'appareil autorisant, mais ici il ne fait que
    // l'ouvrir : cette machine reste son propre compte tant que la racine
    // n'est pas adoptée. Un « appareil appairé ! » ici annoncerait ce qui n'a
    // pas eu lieu — et c'est très exactement ce que ce test interdit.
    await jusquAlEmpreinte();

    fireEvent.click(screen.getByRole('button', { name: L.pairConfirm }));
    await screen.findByText(L.pairAdoptWaiting);
    await vi.advanceTimersByTimeAsync(5_000);

    expect(useUi.getState().toasts).toEqual([]);
    expect(screen.queryByText(L.pairConfirmed)).not.toBeInTheDocument();
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

describe('JoinDeviceForm — adoption du compte reçu', () => {
  it('attend la racine sans prétendre que quelque chose est fait', async () => {
    // L'attente a son propre état : retomber sur le formulaire de saisie
    // laisserait croire que rien n'est en cours, et afficher un succès
    // laisserait croire que tout l'est.
    await jusquAlEmpreinte();

    fireEvent.click(screen.getByRole('button', { name: L.pairConfirm }));

    expect(await screen.findByText(L.pairAdoptWaiting)).toBeInTheDocument();
    expect(screen.queryByLabelText(L.pairJoinLabel)).not.toBeInTheDocument();
    expect(screen.queryByLabelText(O.passphrase)).not.toBeInTheDocument();
  });

  it('demande la phrase de passe dès que le nœud annonce la racine', async () => {
    await jusquALaPhraseDePasse();

    expect(screen.getByText(L.pairAdoptHint)).toBeInTheDocument();
    expect(screen.getByLabelText(O.passphrase)).toBeInTheDocument();
    expect(screen.queryByText(L.pairAdoptWaiting)).not.toBeInTheDocument();
  });

  it('cesse de sonder une fois la racine annoncée', async () => {
    // 🔒 Le sondage n'a plus rien à apprendre : `adopted` restera vrai, et
    // l'écran attend une saisie humaine. Continuer réveillerait l'application
    // une fois par seconde pour relire la même réponse.
    await jusquALaPhraseDePasse();
    const appels = pairStatus.mock.calls.length;

    await vi.advanceTimersByTimeAsync(30_000);

    expect(pairStatus.mock.calls.length).toBe(appels);
  });

  it('prévient que le profil d’appairage restera dans la liste des comptes', async () => {
    // Deux entrées vont apparaître : celle qui a servi à appairer et celle
    // qu'on adopte. Le dire avant la bascule, pas après — rien ne supprime un
    // compte, et deux lignes surgies sans explication ressembleraient à un bug.
    await jusquALaPhraseDePasse();

    expect(screen.getByText(L.pairAdoptLeftover)).toBeInTheDocument();
  });

  it('refuse d’adopter sous une phrase de passe trop courte', async () => {
    // 🔒 Une adoption ratée est définitive : la racine reçue est consommée par
    // la tentative. Ce qui peut être refusé ici doit l'être ici.
    await jusquALaPhraseDePasse();

    fireEvent.change(screen.getByLabelText(O.passphrase), { target: { value: 'court' } });

    expect(screen.getByRole('button', { name: L.pairAdoptSubmit })).toBeDisabled();
    expect(screen.getByText(O.passphraseTooShort)).toBeInTheDocument();
    expect(adopter).not.toHaveBeenCalled();
  });

  it('adopte sous la phrase de passe saisie, et annonce le succès à ce moment-là', async () => {
    await jusquALaPhraseDePasse();

    adopterAvec(PHRASE_DE_PASSE);

    await waitFor(() => expect(adopter).toHaveBeenCalledWith(PHRASE_DE_PASSE));
    await waitFor(() =>
      expect(useUi.getState().toasts).toEqual([
        expect.objectContaining({ kind: 'success', text: L.pairAdopted }),
      ]),
    );
  });

  it('n’annonce aucun succès quand l’adoption échoue, et dit qu’il faut recommencer', async () => {
    // 🔒 L'hôte consomme la racine avant tout ce qui peut échouer : après un
    // refus il n'y a plus rien à adopter. L'écran ne doit donc ni prétendre
    // que ça a marché, ni offrir un « réessayer » qui échouerait autrement.
    adopter.mockRejectedValue(new Error('scellement impossible'));
    await jusquALaPhraseDePasse();

    adopterAvec(PHRASE_DE_PASSE);

    expect(await screen.findByText(L.pairAdoptFailed)).toBeInTheDocument();
    expect(useUi.getState().toasts.filter((t) => t.kind === 'success')).toEqual([]);
    expect(screen.queryByText(L.pairAdopted)).not.toBeInTheDocument();
    // Retour à la saisie d'un code, seul chemin qui puisse encore aboutir.
    expect(screen.getByLabelText(L.pairJoinLabel)).toHaveValue('');
    expect(
      screen.queryByRole('button', { name: L.pairAdoptSubmit }),
    ).not.toBeInTheDocument();
  });

  it('n’oublie pas la phrase de passe dans le composant après un échec', async () => {
    // 🔒 Elle scelle un compte : elle n'a aucune raison de survivre à l'écran
    // qui l'a demandée, et surtout pas d'être réutilisée par une tentative
    // suivante sans que l'utilisateur la retape.
    adopter.mockRejectedValue(new Error('scellement impossible'));
    await jusquALaPhraseDePasse();
    adopterAvec(PHRASE_DE_PASSE);
    await screen.findByText(L.pairAdoptFailed);

    // Un nouvel appairage repart d'un champ vierge.
    pairStatus.mockResolvedValue(statut({ fingerprint: EMPREINTE, adopted: true }));
    saisir('ABCD-EFGH');
    await screen.findByText(L.pairJoinWaiting);
    await vi.advanceTimersByTimeAsync(3_000);
    fireEvent.click(await screen.findByRole('button', { name: L.pairConfirm }));
    await vi.advanceTimersByTimeAsync(3_000);

    expect(await screen.findByLabelText(O.passphrase)).toHaveValue('');
  });

  it('abandonne si la racine n’arrive jamais, sans annuler l’appairage', async () => {
    // 🔒 L'appairage a bien eu lieu côté autorisant : il n'y a rien à annuler,
    // seulement une racine qui n'est pas venue. L'écran le dit au lieu de
    // tourner sans fin — et n'appelle pas `pair_cancel`, qui retirerait une
    // offre qui n'existe plus.
    await jusquAlEmpreinte();
    fireEvent.click(screen.getByRole('button', { name: L.pairConfirm }));
    await screen.findByText(L.pairAdoptWaiting);
    const annulations = pairCancel.mock.calls.length;

    await vi.advanceTimersByTimeAsync(ADOPT_TTL + 2_000);

    expect(await screen.findByText(L.pairAdoptNeverArrived)).toBeInTheDocument();
    expect(pairCancel.mock.calls.length).toBe(annulations);
    const appels = pairStatus.mock.calls.length;
    await vi.advanceTimersByTimeAsync(30_000);
    expect(pairStatus.mock.calls.length).toBe(appels);
  });
});
