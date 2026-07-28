/**
 * Tests de la section « Mes appareils ».
 *
 * L'API est bouchonnée : ce qui est vérifié ici, c'est ce que l'écran fait des
 * réponses — pas la résolution des appareils, couverte côté Rust.
 */

import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const devicesList = vi.fn();
const devicesRename = vi.fn();

// Seule `api` est bouchonnée : la section embarque désormais les deux bouts de
// l'appairage, et celui qui rejoint atteint le store de session — lequel
// s'abonne au vrai `rpc` à sa création. Un module entièrement remplacé le
// priverait de cet export et ferait échouer l'import, pas le test.
vi.mock('../../lib/client', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../lib/client')>()),
  api: {
    devicesList: () => devicesList(),
    devicesRename: (name: string) => devicesRename(name),
  },
}));

import { DevicesSection } from './DevicesSection';
import { frSettings } from '../../i18n/fr.settings';
import { formatEventDateTime } from '../../lib/format';
import { useUi } from '../../stores/ui';

// Libellés lus dans le dictionnaire plutôt que recopiés : le test suit une
// reformulation sans devenir faux, et ne dépend pas de la langue par défaut.
const L = frSettings.settings;

const APPAREIL = {
  pubkey: 'ab'.repeat(32),
  name: 'Portable',
  added_ms: 0,
  is_current: true,
  last_seen_ms: null,
  last_seen_route: null,
};

/** Une machine sœur du compte : celle dont « vu la dernière fois » a un sens. */
const SOEUR = {
  pubkey: 'cd'.repeat(32),
  name: 'Fixe',
  added_ms: Date.UTC(2026, 0, 12, 9, 30),
  is_current: false,
  last_seen_ms: Date.UTC(2026, 6, 26, 14, 5),
  last_seen_route: 'relay' as const,
};

beforeEach(() => {
  devicesList.mockReset();
  devicesRename.mockReset();
  useUi.setState({ lang: 'fr' });
});

/**
 * Texte complet de la ligne d'un appareil (nom, clé tronquée, historique) :
 * l'historique tient sur une seule phrase, on l'interroge donc en bloc plutôt
 * que par morceaux collés.
 */
function ligne(nom: string): string {
  return screen.getByText(nom).parentElement?.textContent ?? '';
}

/**
 * Début d'un libellé à trou, sans son `{date}` : le test suit une
 * reformulation de la traduction sans devenir faux.
 */
function debut(libelle: string): string {
  return (libelle.split('{date}')[0] ?? '').trim();
}

describe('DevicesSection', () => {
  it('affiche l’appareil courant et le distingue des autres', async () => {
    devicesList.mockResolvedValue({ devices: [APPAREIL] });
    render(<DevicesSection />);

    expect(await screen.findByText('Portable')).toBeInTheDocument();
    expect(screen.getByText(L.deviceCurrent)).toBeInTheDocument();
  });

  it('propose les deux bouts de l’appairage, sans porte fermée', async () => {
    // Ils sont restés cachés derrière un drapeau tant que l'adoption du compte
    // n'existait pas : les montrer plus tôt aurait annoncé un appairage qui
    // laissait la machine sur son propre compte. Ce test est ce qui empêche la
    // porte de se refermer sans qu'on s'en aperçoive.
    devicesList.mockResolvedValue({ devices: [APPAREIL] });
    render(<DevicesSection />);

    expect(await screen.findByRole('button', { name: L.pairAdd })).toBeInTheDocument();
    expect(screen.getByLabelText(L.pairJoinLabel)).toBeInTheDocument();
  });

  it('dit quand une machine sœur a été ajoutée, quand elle a été vue, et par quel chemin', async () => {
    // Feuille de route §17.4. `added_ms` était déjà récupéré et n'était affiché
    // nulle part : une liste d'appareils qui ne dit ni « depuis quand » ni
    // « toujours en service ? » ne permet pas de décider d'une révocation.
    devicesList.mockResolvedValue({ devices: [APPAREIL, SOEUR] });
    render(<DevicesSection />);

    await screen.findByText('Fixe');
    const texte = ligne('Fixe');
    expect(texte).toContain(debut(L.deviceAdded));
    expect(texte).toContain(formatEventDateTime(SOEUR.added_ms, 'fr'));
    expect(texte).toContain(debut(L.deviceLastSeen));
    expect(texte).toContain(formatEventDateTime(SOEUR.last_seen_ms, 'fr'));
    expect(texte).toContain(L.deviceRouteRelay);
  });

  it('distingue un dernier contact direct d’un contact relayé', async () => {
    // 🔒 Tout ce que « d'où » veut dire ici : le CHEMIN, jamais un lieu. Une
    // machine qu'on ne joint plus qu'en relais est le symptôme qui intéresse.
    devicesList.mockResolvedValue({
      devices: [APPAREIL, { ...SOEUR, last_seen_route: 'direct' }],
    });
    render(<DevicesSection />);

    await screen.findByText('Fixe');
    expect(ligne('Fixe')).toContain(L.deviceRouteDirect);
    expect(ligne('Fixe')).not.toContain(L.deviceRouteRelay);
  });

  it('dit « jamais joint » plutôt que d’inventer une date', async () => {
    devicesList.mockResolvedValue({
      devices: [APPAREIL, { ...SOEUR, last_seen_ms: null, last_seen_route: null }],
    });
    render(<DevicesSection />);

    await screen.findByText('Fixe');
    expect(ligne('Fixe')).toContain(L.deviceLastSeenNever);
    expect(ligne('Fixe')).not.toContain(debut(L.deviceLastSeen));
  });

  it('ne raconte pas à l’appareil courant qu’il vient de se voir lui-même', async () => {
    // « Vu il y a deux secondes » sur la machine qu'on tient dans les mains
    // ferait passer une évidence pour un fait de réseau. Et sa date d'ajout à
    // zéro (appareil issu de la migration) se dit, elle ne s'invente pas.
    devicesList.mockResolvedValue({ devices: [APPAREIL] });
    render(<DevicesSection />);

    await screen.findByText('Portable');
    const texte = ligne('Portable');
    expect(texte).toContain(L.deviceLastSeenHere);
    expect(texte).toContain(L.deviceAddedUnknown);
    expect(texte).not.toContain(debut(L.deviceLastSeen));
    expect(texte).not.toContain(L.deviceLastSeenNever);
  });

  it('n’offre la récupération d’historique que sur les machines sœurs', async () => {
    // §17.4. Se demander son propre historique n'irait chercher nulle part :
    // l'action n'a de sens que pointée vers un AUTRE appareil du compte.
    devicesList.mockResolvedValue({ devices: [APPAREIL, SOEUR] });
    render(<DevicesSection />);

    await screen.findByText('Fixe');
    expect(screen.getAllByRole('button', { name: L.historyTransferAction })).toHaveLength(
      1,
    );
  });

  it('n’affiche jamais la clé publique en entier', async () => {
    // 🔒 Une clé tronquée suffit à reconnaître un appareil dans une liste.
    // L'afficher entière n'aide personne et encombre une capture d'écran.
    devicesList.mockResolvedValue({ devices: [APPAREIL] });
    render(<DevicesSection />);

    await screen.findByText('Portable');
    expect(screen.queryByText(APPAREIL.pubkey)).not.toBeInTheDocument();
  });

  it('renomme l’appareil et reflète le nom rendu par le nœud', async () => {
    devicesList.mockResolvedValue({ devices: [APPAREIL] });
    // Le nœud rogne les espaces : c'est SA réponse qui fait foi, pas la saisie.
    devicesRename.mockResolvedValue({ name: 'Bureau' });
    render(<DevicesSection />);

    const champ = await screen.findByLabelText(L.deviceNameLabel);
    fireEvent.change(champ, { target: { value: '  Bureau  ' } });
    fireEvent.click(screen.getByRole('button', { name: L.pseudonymSave }));

    await waitFor(() => expect(devicesRename).toHaveBeenCalledWith('Bureau'));
    expect(await screen.findByText('Bureau')).toBeInTheDocument();
  });

  it('n’enregistre pas un nom inchangé ni un nom vide', async () => {
    devicesList.mockResolvedValue({ devices: [APPAREIL] });
    render(<DevicesSection />);

    const bouton = await screen.findByRole('button', { name: L.pseudonymSave });
    // Inchangé au chargement.
    expect(bouton).toBeDisabled();

    fireEvent.change(screen.getByLabelText(L.deviceNameLabel), {
      target: { value: '   ' },
    });
    expect(bouton).toBeDisabled();
    expect(devicesRename).not.toHaveBeenCalled();
  });

  it('affiche un état vide plutôt qu’une erreur quand l’appel échoue', async () => {
    // Un profil ouvert hors du démarrage normal n'a pas encore d'appareil.
    devicesList.mockRejectedValue(new Error('pas d’appareil'));
    render(<DevicesSection />);

    expect(await screen.findByText(L.devicesEmpty)).toBeInTheDocument();
  });

  it('refuse un nom qui dépasse la borne en OCTETS, pas en caractères', async () => {
    // 🔒 Le piège attrapé côté nœud, gardé aussi ici : 32 caractères
    // accentués font 64 octets. Les accepter afficherait un enregistrement
    // qui échoue ensuite sans que l'utilisateur comprenne pourquoi.
    devicesList.mockResolvedValue({ devices: [APPAREIL] });
    render(<DevicesSection />);

    const champ = await screen.findByLabelText(L.deviceNameLabel);
    fireEvent.change(champ, { target: { value: 'é'.repeat(32) } });

    expect(screen.getByRole('button', { name: L.pseudonymSave })).toBeDisabled();

    // 16 caractères accentués = 32 octets : accepté.
    fireEvent.change(champ, { target: { value: 'é'.repeat(16) } });
    expect(screen.getByRole('button', { name: L.pseudonymSave })).toBeEnabled();
  });
});
