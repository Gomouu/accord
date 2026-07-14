/**
 * Tests de l'onglet Mon compte : édition du pseudo et de la bio (validation,
 * toast de succès, échec), avatar (retrait), code ami copiable, clé publique
 * abrégée et rappel sur la phrase de récupération non ré-affichable.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type { SelfProfile } from '../../lib/api';
import { useSession } from '../../stores/session';
import { useUi } from '../../stores/ui';
import { AccountTab } from './AccountTab';

vi.mock('../../lib/files', () => ({
  lireFichier: vi.fn(() => new Promise(() => {})),
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
};

beforeEach(() => {
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

describe('AccountTab — couleurs de profil', () => {
  it('choisit un préréglage comme couleur d’accent et confirme par un toast', async () => {
    const setAccentColor = vi.fn(async () => {});
    useSession.setState({ setAccentColor });
    render(<AccountTab />);

    const group = screen.getByRole('group', { name: 'Couleur d’accent' });
    fireEvent.click(within(group).getByRole('button', { name: 'Couleur #5865f2' }));

    await waitFor(() => expect(setAccentColor).toHaveBeenCalledWith(0x5865f2));
    await waitFor(() => {
      expect(
        useUi.getState().toasts.some((t) => t.kind === 'info' && /accent/i.test(t.text)),
      ).toBe(true);
    });
  });

  it('efface la couleur de bannière via la pastille « Aucune couleur »', async () => {
    const setBannerColor = vi.fn(async () => {});
    useSession.setState({ self: { ...self, banner_color: 0x3ba55c }, setBannerColor });
    render(<AccountTab />);

    const group = screen.getByRole('group', { name: 'Couleur de bannière' });
    fireEvent.click(within(group).getByRole('button', { name: 'Aucune couleur' }));

    await waitFor(() => expect(setBannerColor).toHaveBeenCalledWith(null));
  });

  it('désactive la pastille « Aucune couleur » tant qu’aucune couleur n’est fixée', () => {
    render(<AccountTab />);

    const group = screen.getByRole('group', { name: 'Couleur de bannière' });
    expect(within(group).getByRole('button', { name: 'Aucune couleur' })).toBeDisabled();
  });

  it('garde la pastille blanche visible et suffisamment grande en thème clair', () => {
    render(<AccountTab />);

    const group = screen.getByRole('group', { name: 'Couleur d’accent' });
    expect(within(group).getByRole('button', { name: 'Couleur #ffffff' })).toHaveClass(
      'h-9',
      'w-9',
      'border',
      'border-input',
    );
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

    expect(screen.getByText('Orbite · Constellation')).toBeInTheDocument();
    const group = screen.getByRole('group', { name: 'Effet de profil' });
    expect(within(group).getByRole('button', { name: 'Constellation' })).toHaveAttribute(
      'aria-pressed',
      'true',
    );
    await user.click(within(group).getByRole('button', { name: 'Braises' }));

    expect(setProfileEffect).toHaveBeenCalledWith('floating_particles');
  });
});
