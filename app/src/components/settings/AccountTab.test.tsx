/**
 * Tests de l'onglet Mon compte : édition du pseudo et de la bio (validation,
 * toast de succès, échec), avatar (retrait), code ami copiable, clé publique
 * abrégée et rappel sur la phrase de récupération non ré-affichable.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type { SelfProfile } from '../../lib/api';
import { backupExport, backupImport } from '../../lib/bridge';
import { useSession } from '../../stores/session';
import { useUi } from '../../stores/ui';
import { AccountTab } from './AccountTab';

vi.mock('../../lib/files', () => ({
  lireFichier: vi.fn(() => new Promise(() => {})),
}));

// Sauvegarde : seuls export/import sont simulés (sélecteur natif + commande
// hôte indisponibles sous vitest), le reste du pont reste intact pour les
// autres imports du store de session.
vi.mock('../../lib/bridge', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../lib/bridge')>()),
  backupExport: vi.fn(async () => null),
  backupImport: vi.fn(async () => null),
}));

/** Bouton Enregistrer de la section nommée (pseudo et bio partagent le libellé). */
function saveButtonOf(sectionName: string): HTMLElement {
  const section = screen.getByRole('region', { name: sectionName });
  return within(section).getByRole('button', { name: 'Enregistrer' });
}

const PUBKEY = 'aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899';

const self: SelfProfile = {
  node_id: 'n-moi',
  pubkey: PUBKEY,
  friend_code: 'accord-moi-12345',
  name: 'Alex',
  bio: null,
  avatar: null,
  banner: null,
  pronouns: null,
  accent_color: null,
  banner_color: null,
  avatar_decoration: null,
  profile_effect: null,
  profile_frame: null,
};

beforeEach(() => {
  vi.mocked(backupExport).mockClear();
  vi.mocked(backupImport).mockClear();
  useUi.setState({ lang: 'fr', toasts: [] });
  useSession.setState({
    self,
    phase: 'ready',
    setName: vi.fn(async () => {}),
    setBio: vi.fn(async () => {}),
    setAvatar: vi.fn(async () => {}),
    setPronouns: vi.fn(async () => {}),
    setAccentColor: vi.fn(async () => {}),
    setBannerColor: vi.fn(async () => {}),
    setAvatarDecoration: vi.fn(async () => {}),
    setProfileEffect: vi.fn(async () => {}),
    setProfileFrame: vi.fn(async () => {}),
  });
});

describe('AccountTab — pseudo', () => {
  it('préremplit le champ avec le pseudo courant', () => {
    render(<AccountTab />);

    expect(screen.getByRole('textbox', { name: 'Pseudo' })).toHaveValue('Alex');
  });

  it('désactive Enregistrer tant que le pseudo est inchangé ou invalide', () => {
    render(<AccountTab />);
    const save = saveButtonOf('Pseudo');

    expect(save).toBeDisabled();

    fireEvent.change(screen.getByRole('textbox', { name: 'Pseudo' }), {
      target: { value: 'A' },
    });
    expect(save).toBeDisabled();
    expect(
      screen.getByText('Le pseudo doit faire entre 2 et 32 caractères'),
    ).toBeInTheDocument();
  });

  it('enregistre le nouveau pseudo et confirme par un toast', async () => {
    const setName = vi.fn(async () => {});
    useSession.setState({ setName });
    render(<AccountTab />);

    fireEvent.change(screen.getByRole('textbox', { name: 'Pseudo' }), {
      target: { value: '  Alexandra  ' },
    });
    fireEvent.click(saveButtonOf('Pseudo'));

    await waitFor(() => expect(setName).toHaveBeenCalledWith('Alexandra'));
    await waitFor(() => {
      expect(
        useUi.getState().toasts.some((t) => t.kind === 'info' && /Pseudo/.test(t.text)),
      ).toBe(true);
    });
  });

  it('signale l’échec de l’enregistrement par un toast d’erreur', async () => {
    useSession.setState({
      setName: vi.fn(async () => Promise.reject(new Error('hors ligne'))),
    });
    render(<AccountTab />);

    fireEvent.change(screen.getByRole('textbox', { name: 'Pseudo' }), {
      target: { value: 'Alexandra' },
    });
    fireEvent.click(saveButtonOf('Pseudo'));

    await waitFor(() => {
      expect(useUi.getState().toasts.some((t) => t.kind === 'error')).toBe(true);
    });
  });
});

describe('AccountTab — bio', () => {
  it('préremplit la zone avec la bio courante et affiche le compteur', () => {
    useSession.setState({ self: { ...self, bio: 'Salut !' } });
    render(<AccountTab />);

    expect(screen.getByRole('textbox', { name: 'À propos de moi' })).toHaveValue(
      'Salut !',
    );
    expect(screen.getByText('7/2048')).toBeInTheDocument();
  });

  it('enregistre la bio épurée et confirme par un toast', async () => {
    const setBio = vi.fn(async () => {});
    useSession.setState({ setBio });
    render(<AccountTab />);

    fireEvent.change(screen.getByRole('textbox', { name: 'À propos de moi' }), {
      target: { value: '  fan de fromage  ' },
    });
    fireEvent.click(saveButtonOf('À propos de moi'));

    await waitFor(() => expect(setBio).toHaveBeenCalledWith('fan de fromage'));
    await waitFor(() => {
      expect(
        useUi.getState().toasts.some((t) => t.kind === 'info' && /Bio/.test(t.text)),
      ).toBe(true);
    });
  });

  it('désactive Enregistrer tant que la bio est inchangée', () => {
    render(<AccountTab />);

    expect(saveButtonOf('À propos de moi')).toBeDisabled();
  });

  it('permet d’effacer la bio (chaîne vide)', async () => {
    const setBio = vi.fn(async () => {});
    useSession.setState({ self: { ...self, bio: 'ancienne' }, setBio });
    render(<AccountTab />);

    fireEvent.change(screen.getByRole('textbox', { name: 'À propos de moi' }), {
      target: { value: '' },
    });
    fireEvent.click(saveButtonOf('À propos de moi'));

    await waitFor(() => expect(setBio).toHaveBeenCalledWith(''));
  });
});

describe('AccountTab — avatar', () => {
  it('propose le choix d’une image, sans retrait tant qu’aucun avatar', () => {
    render(<AccountTab />);

    expect(screen.getByRole('button', { name: 'Choisir une image' })).toBeEnabled();
    expect(
      screen.queryByRole('button', { name: 'Retirer l’avatar' }),
    ).not.toBeInTheDocument();
  });

  it('retire l’avatar courant via profile.set_avatar(null)', async () => {
    const setAvatar = vi.fn(async () => {});
    useSession.setState({ self: { ...self, avatar: 'ab'.repeat(32) }, setAvatar });
    render(<AccountTab />);

    fireEvent.click(screen.getByRole('button', { name: 'Retirer l’avatar' }));

    await waitFor(() => expect(setAvatar).toHaveBeenCalledWith(null));
    await waitFor(() => {
      expect(useUi.getState().toasts.some((t) => t.kind === 'info')).toBe(true);
    });
  });
});

describe('AccountTab — logout (danger zone)', () => {
  it('shows the logout button without any inline confirmation at first', () => {
    render(<AccountTab />);

    expect(screen.getByRole('button', { name: 'Se déconnecter' })).toBeEnabled();
    expect(
      screen.queryByRole('button', { name: 'Oui, me déconnecter' }),
    ).not.toBeInTheDocument();
  });

  it('asks for an inline confirmation and cancels without locking', () => {
    const lock = vi.fn(async () => {});
    useSession.setState({ lock });
    render(<AccountTab />);

    fireEvent.click(screen.getByRole('button', { name: 'Se déconnecter' }));
    expect(
      screen.getByText('Votre phrase de passe sera nécessaire pour vous reconnecter.'),
    ).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'Annuler' }));

    expect(lock).not.toHaveBeenCalled();
    expect(
      screen.queryByRole('button', { name: 'Oui, me déconnecter' }),
    ).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Se déconnecter' })).toBeEnabled();
  });

  it('locks the vault and closes the settings modal on confirmation', async () => {
    const lock = vi.fn(async () => {});
    useSession.setState({ lock });
    useUi.setState({ modal: { kind: 'settings' } });
    render(<AccountTab />);

    fireEvent.click(screen.getByRole('button', { name: 'Se déconnecter' }));
    fireEvent.click(screen.getByRole('button', { name: 'Oui, me déconnecter' }));

    await waitFor(() => expect(lock).toHaveBeenCalledTimes(1));
    expect(useUi.getState().modal).toBeNull();
  });
});

describe('AccountTab — sauvegarde', () => {
  it('affiche les deux boutons et l’avertissement de verrouillage', () => {
    // Arrange / Act
    render(<AccountTab />);

    // Assert : export, import et l'avertissement (l'export verrouille la
    // session) sont visibles en permanence dans la section Sauvegarde.
    const section = screen.getByRole('region', { name: 'Sauvegarde' });
    expect(
      within(section).getByRole('button', { name: 'Exporter une sauvegarde…' }),
    ).toBeEnabled();
    expect(
      within(section).getByRole('button', { name: 'Importer une sauvegarde…' }),
    ).toBeEnabled();
    expect(within(section).getByText(/verrouille la session/)).toBeInTheDocument();
  });

  it('exporte puis verrouille : toast, modale fermée, lock() appelé', async () => {
    // Arrange : l'hôte confirme l'export (coffre désormais verrouillé).
    vi.mocked(backupExport).mockResolvedValueOnce('locked');
    const lock = vi.fn(async () => {});
    useSession.setState({ lock });
    useUi.setState({ modal: { kind: 'settings' } });
    render(<AccountTab />);

    // Act : ouvre la saisie, confirme la phrase de passe puis exporte.
    fireEvent.click(screen.getByRole('button', { name: 'Exporter une sauvegarde…' }));
    fireEvent.change(screen.getByLabelText('Confirmez votre phrase de passe'), {
      target: { value: 'ma-phrase' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Confirmer' }));

    // Assert : toast de succès, modale fermée (l'écran de déverrouillage ne
    // doit jamais rester sous une modale), état de session aligné via lock().
    await waitFor(() => expect(lock).toHaveBeenCalledTimes(1));
    expect(backupExport).toHaveBeenCalledWith('ma-phrase');
    expect(useUi.getState().modal).toBeNull();
    expect(
      useUi.getState().toasts.some((t) => t.kind === 'info' && /export/i.test(t.text)),
    ).toBe(true);
  });

  it('ne verrouille rien quand le sélecteur d’export est annulé', async () => {
    // Arrange : sélecteur natif annulé — le pont rend null sans invoquer l'hôte.
    vi.mocked(backupExport).mockResolvedValueOnce(null);
    const lock = vi.fn(async () => {});
    useSession.setState({ lock });
    render(<AccountTab />);

    // Act
    fireEvent.click(screen.getByRole('button', { name: 'Exporter une sauvegarde…' }));
    fireEvent.change(screen.getByLabelText('Confirmez votre phrase de passe'), {
      target: { value: 'ma-phrase' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Confirmer' }));

    // Assert : aucun toast, aucun verrouillage, bouton de confirmation de nouveau actif.
    await waitFor(() =>
      expect(screen.getByRole('button', { name: 'Confirmer' })).toBeEnabled(),
    );
    expect(lock).not.toHaveBeenCalled();
    expect(useUi.getState().toasts).toHaveLength(0);
  });

  it('confirme l’import d’une archive par un toast (compte neuf)', async () => {
    // Arrange : l'hôte rend les métadonnées du compte fraîchement importé.
    vi.mocked(backupImport).mockResolvedValueOnce({
      id: 'importe-1',
      name: 'Compte importé',
      created_ms: 3,
      last_used_ms: 0,
      is_legacy: false,
      pubkey_short: null,
    });
    render(<AccountTab />);

    // Act : ouvre la saisie (phrase facultative à l'import) et confirme.
    fireEvent.click(screen.getByRole('button', { name: 'Importer une sauvegarde…' }));
    fireEvent.change(screen.getByLabelText('Phrase de passe de la sauvegarde'), {
      target: { value: 'phrase-sauvegarde' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Confirmer' }));

    // Assert : le toast renvoie vers le sélecteur de comptes, la session
    // active n'est pas touchée (aucune modale fermée, aucun verrouillage).
    await waitFor(() => {
      expect(
        useUi
          .getState()
          .toasts.some((t) => t.kind === 'info' && /sélecteur de comptes/.test(t.text)),
      ).toBe(true);
    });
  });

  it('signale l’échec de l’export par un toast d’erreur', async () => {
    // Arrange
    vi.mocked(backupExport).mockRejectedValueOnce(new Error('disque plein'));
    render(<AccountTab />);

    // Act
    fireEvent.click(screen.getByRole('button', { name: 'Exporter une sauvegarde…' }));
    fireEvent.change(screen.getByLabelText('Confirmez votre phrase de passe'), {
      target: { value: 'ma-phrase' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Confirmer' }));

    // Assert
    await waitFor(() => {
      expect(useUi.getState().toasts.some((t) => t.kind === 'error')).toBe(true);
    });
  });

  it('refuse d’exporter sans phrase de passe (garde-fou)', async () => {
    render(<AccountTab />);

    fireEvent.click(screen.getByRole('button', { name: 'Exporter une sauvegarde…' }));
    fireEvent.click(screen.getByRole('button', { name: 'Confirmer' }));

    await waitFor(() => {
      expect(useUi.getState().toasts.some((t) => t.kind === 'error')).toBe(true);
    });
    // La commande n'est jamais invoquée sans phrase.
    expect(backupExport).not.toHaveBeenCalled();
  });

  it('traduit l’erreur « mauvais secret » de l’hôte en message dédié', async () => {
    // L'hôte remonte l'erreur crypto (phrase erronée re-vérifiée avant export).
    vi.mocked(backupExport).mockRejectedValueOnce(
      new Error('cryptographie : secret de déverrouillage incorrect'),
    );
    render(<AccountTab />);

    fireEvent.click(screen.getByRole('button', { name: 'Exporter une sauvegarde…' }));
    fireEvent.change(screen.getByLabelText('Confirmez votre phrase de passe'), {
      target: { value: 'mauvaise' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Confirmer' }));

    await waitFor(() => {
      expect(
        useUi
          .getState()
          .toasts.some((t) => t.kind === 'error' && /incorrecte/.test(t.text)),
      ).toBe(true);
    });
  });
});

describe('AccountTab — identité et phrase de récupération', () => {
  it('affiche le code ami, son bouton de copie et la clé publique abrégée', () => {
    render(<AccountTab />);

    expect(screen.getByText('accord-moi-12345')).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: 'Copier mon code ami' }),
    ).toBeInTheDocument();
    // Clé abrégée : début, ellipse, fin — jamais le mur d'hexadécimal complet.
    expect(screen.getByText('aabbccddeeff…66778899')).toBeInTheDocument();
  });

  it('rappelle que la phrase de récupération ne sera jamais ré-affichée', () => {
    render(<AccountTab />);

    expect(screen.getByText(/affichée une seule fois/)).toBeInTheDocument();
    expect(screen.getByText(/ne peut pas être ré-affichée/)).toBeInTheDocument();
  });
});

describe('AccountTab — pronoms', () => {
  it('préremplit le champ avec les pronoms courants', () => {
    useSession.setState({ self: { ...self, pronouns: 'iel/iel' } });
    render(<AccountTab />);

    expect(screen.getByRole('textbox', { name: 'Pronoms' })).toHaveValue('iel/iel');
  });

  it('désactive Enregistrer tant que les pronoms sont inchangés', () => {
    render(<AccountTab />);

    expect(saveButtonOf('Pronoms')).toBeDisabled();
  });

  it('enregistre les pronoms épurés et confirme par un toast', async () => {
    const setPronouns = vi.fn(async () => {});
    useSession.setState({ setPronouns });
    render(<AccountTab />);

    fireEvent.change(screen.getByRole('textbox', { name: 'Pronoms' }), {
      target: { value: '  il/lui  ' },
    });
    fireEvent.click(saveButtonOf('Pronoms'));

    await waitFor(() => expect(setPronouns).toHaveBeenCalledWith('il/lui'));
    await waitFor(() => {
      expect(
        useUi.getState().toasts.some((t) => t.kind === 'info' && /Pronoms/.test(t.text)),
      ).toBe(true);
    });
  });

  it('permet d’effacer les pronoms (chaîne vide)', async () => {
    const setPronouns = vi.fn(async () => {});
    useSession.setState({ self: { ...self, pronouns: 'iel/iel' }, setPronouns });
    render(<AccountTab />);

    fireEvent.change(screen.getByRole('textbox', { name: 'Pronoms' }), {
      target: { value: '' },
    });
    fireEvent.click(saveButtonOf('Pronoms'));

    await waitFor(() => expect(setPronouns).toHaveBeenCalledWith(''));
  });
});

describe('AccountTab — personnalisation', () => {
  it('enregistre une décoration choisie et expose son état sélectionné', async () => {
    const user = userEvent.setup();
    const setAvatarDecoration = vi.fn(async () => {});
    useSession.setState({ setAvatarDecoration });
    render(<AccountTab />);

    const group = screen.getByRole('group', { name: "Décoration d'avatar" });
    await user.click(within(group).getByRole('button', { name: 'Éclipse' }));

    expect(setAvatarDecoration).toHaveBeenCalledWith('neon_ring');
  });

  it('enregistre un effet et réaffiche les choix persistés dans l’aperçu', async () => {
    const user = userEvent.setup();
    const setProfileEffect = vi.fn(async () => {});
    useSession.setState({
      self: {
        ...self,
        avatar_decoration: 'aurora_ring',
        profile_effect: 'starfield',
      },
      setProfileEffect,
    });
    render(<AccountTab />);

    expect(screen.getByText('Orbite · Constellation · Aucune')).toBeInTheDocument();
    const group = screen.getByRole('group', { name: 'Effet de profil' });
    expect(within(group).getByRole('button', { name: 'Constellation' })).toHaveAttribute(
      'aria-pressed',
      'true',
    );
    await user.click(within(group).getByRole('button', { name: 'Braises' }));

    expect(setProfileEffect).toHaveBeenCalledWith('floating_particles');
  });

  it('enregistre un cadre indépendamment de l’effet', async () => {
    const user = userEvent.setup();
    const setProfileFrame = vi.fn(async () => {});
    const setProfileEffect = vi.fn(async () => {});
    useSession.setState({
      self: {
        ...self,
        profile_effect: 'starfield',
        profile_frame: 'lumen_bloom',
      },
      setProfileFrame,
      setProfileEffect,
    });
    render(<AccountTab />);

    expect(
      screen.getByText('Aucune · Constellation · Jardin de lumière'),
    ).toBeInTheDocument();
    const group = screen.getByRole('group', { name: 'Cadre de profil' });
    expect(
      within(group).getByRole('button', { name: 'Jardin de lumière' }),
    ).toHaveAttribute('aria-pressed', 'true');
    await user.click(within(group).getByRole('button', { name: 'Aucune' }));

    expect(setProfileFrame).toHaveBeenCalledWith(null);
    expect(setProfileEffect).not.toHaveBeenCalled();
  });
});
