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

vi.mock('../../lib/client', () => ({
  api: {
    devicesList: () => devicesList(),
    devicesRename: (name: string) => devicesRename(name),
  },
}));

import { DevicesSection } from './DevicesSection';
import { fr } from '../../i18n/fr';
import { useUi } from '../../stores/ui';

// Libellés lus dans le dictionnaire plutôt que recopiés : le test suit une
// reformulation sans devenir faux, et ne dépend pas de la langue par défaut.
const L = fr.settings;

const APPAREIL = {
  pubkey: 'ab'.repeat(32),
  name: 'Portable',
  added_ms: 0,
  is_current: true,
};

beforeEach(() => {
  devicesList.mockReset();
  devicesRename.mockReset();
  useUi.setState({ lang: 'fr' });
});

describe('DevicesSection', () => {
  it('affiche l’appareil courant et le distingue des autres', async () => {
    devicesList.mockResolvedValue({ devices: [APPAREIL] });
    render(<DevicesSection />);

    expect(await screen.findByText('Portable')).toBeInTheDocument();
    expect(screen.getByText(L.deviceCurrent)).toBeInTheDocument();
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
