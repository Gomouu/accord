/**
 * Tests de l'écran des paramètres : navigation par catégories, bascule de
 * thème appliquée à la racine (et persistée), densité, liste des bloqués
 * avec déblocage, langue et fermeture par Échap.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import type { Contact, SelfProfile } from '../../lib/api';
import { APP_VERSION } from '../../lib/meta';
import { useFriends } from '../../stores/friends';
import { useSession } from '../../stores/session';
import { THEME_IDS, useUi } from '../../stores/ui';
import { SettingsModal } from './SettingsModal';

const self: SelfProfile = {
  node_id: 'n-moi',
  pubkey: 'moi',
  friend_code: 'accord-moi-12345',
  name: null,
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

function blockedContact(pubkey: string, name: string): Contact {
  return {
    node_id: `n-${pubkey}`,
    pubkey,
    friend_code: `accord-${pubkey}`,
    display_name: name,
    bio: null,
    avatar: null,
    banner: null,
    state: 'blocked',
    last_seen_ms: 0,
  };
}

beforeEach(() => {
  window.localStorage.clear();
  useUi.setState({ lang: 'fr', modal: { kind: 'settings' }, toasts: [] });
  useUi.getState().setTheme('dark');
  useUi.getState().setDensity('comfortable');
  useUi.getState().setFontScale(100);
  useUi.getState().setReducedMotion('system');
  useUi.getState().setSaturation(100);
  window.localStorage.clear();
  useSession.setState({ self, phase: 'ready' });
  useFriends.setState({
    contacts: [],
    loaded: true,
    load: vi.fn(async () => {}),
    unblock: vi.fn(async () => {}),
  });
});

function openTab(label: string): void {
  fireEvent.click(screen.getByRole('button', { name: label }));
}

describe('SettingsModal — structure', () => {
  it('présente les catégories et ouvre Mon compte par défaut', () => {
    render(<SettingsModal />);

    expect(screen.getByRole('dialog', { name: 'Paramètres' })).toBeInTheDocument();
    for (const label of [
      'Mon compte',
      'Confidentialité',
      'Apparence',
      'Accessibilité',
      'Texte & médias',
      'Langue et heure',
      'Voix',
      'Notifications',
      'Avancé',
    ]) {
      expect(screen.getByRole('button', { name: label })).toBeInTheDocument();
    }
    // L'onglet par défaut (Mon compte) expose l'édition du pseudo.
    expect(screen.getByRole('textbox', { name: 'Pseudo' })).toBeInTheDocument();
  });

  it('se ferme avec Échap', () => {
    render(<SettingsModal />);

    fireEvent.keyDown(window, { key: 'Escape' });

    expect(useUi.getState().modal).toBeNull();
  });

  it('se ferme avec le bouton dédié', () => {
    render(<SettingsModal />);

    fireEvent.click(screen.getByRole('button', { name: 'Fermer' }));

    expect(useUi.getState().modal).toBeNull();
  });

  it('donne le focus initial à la catégorie active et navigue aux flèches', () => {
    render(<SettingsModal />);

    // Focus initial sur la catégorie courante (aria-current="page").
    const compte = screen.getByRole('button', { name: 'Mon compte' });
    expect(compte).toHaveFocus();
    expect(compte).toHaveAttribute('aria-current', 'page');

    // Flèche bas : la catégorie suivante prend le focus, flèche haut revient.
    fireEvent.keyDown(compte, { key: 'ArrowDown' });
    expect(screen.getByRole('button', { name: 'Confidentialité' })).toHaveFocus();
    fireEvent.keyDown(screen.getByRole('button', { name: 'Confidentialité' }), {
      key: 'ArrowUp',
    });
    expect(compte).toHaveFocus();
  });

  it('piège Tab dans la modale (bouclage au dernier focusable)', () => {
    render(<SettingsModal />);

    // Maj+Tab depuis le premier focusable boucle vers le dernier.
    const premier = screen.getByRole('button', { name: 'Mon compte' });
    premier.focus();
    fireEvent.keyDown(screen.getByRole('dialog', { name: 'Paramètres' }), {
      key: 'Tab',
      shiftKey: true,
    });
    expect(premier).not.toHaveFocus();
    expect(
      screen.getByRole('dialog', { name: 'Paramètres' }).contains(document.activeElement),
    ).toBe(true);
  });
});

describe('SettingsModal — apparence', () => {
  it('bascule le thème clair, l’applique à la racine et le persiste', () => {
    render(<SettingsModal />);
    openTab('Apparence');

    const light = screen.getByRole('radio', { name: 'Clair' });
    expect(light).toHaveAttribute('aria-checked', 'false');
    fireEvent.click(light);

    expect(document.documentElement.dataset.theme).toBe('light');
    expect(window.localStorage.getItem('accord.theme')).toBe('light');
    expect(light).toHaveAttribute('aria-checked', 'true');
  });

  it('affiche la galerie de thèmes comme un radiogroup accessible', () => {
    render(<SettingsModal />);
    openTab('Apparence');

    const group = screen.getByRole('radiogroup', { name: 'Thème' });
    expect(group).toBeInTheDocument();
    expect(screen.getAllByRole('radio')).toHaveLength(THEME_IDS.length);
    expect(screen.getByRole('radio', { name: 'Sombre' })).toHaveAttribute(
      'aria-checked',
      'true',
    );
  });

  it('applique un thème immersif et expose sa scène dans la vignette', () => {
    render(<SettingsModal />);
    openTab('Apparence');

    const signal = screen.getByRole('radio', { name: 'Signal fantôme' });
    expect(signal.querySelector('[data-theme="signal"]')).toBeInTheDocument();
    fireEvent.click(signal);

    expect(document.documentElement.dataset.theme).toBe('signal');
    expect(window.localStorage.getItem('accord.theme')).toBe('signal');
    expect(signal).toHaveAttribute('aria-checked', 'true');
  });

  it('parcourt les thèmes dans l’ordre avec les flèches', () => {
    render(<SettingsModal />);
    openTab('Apparence');

    fireEvent.keyDown(screen.getByRole('radiogroup', { name: 'Thème' }), {
      key: 'ArrowDown',
    });

    const light = screen.getByRole('radio', { name: 'Clair' });
    expect(light).toHaveAttribute('aria-checked', 'true');
    expect(light).toHaveFocus();
  });

  it('bascule la densité compacte, l’applique à la racine et la persiste', () => {
    render(<SettingsModal />);
    openTab('Apparence');

    const compact = screen.getByRole('button', { name: 'Compacte' });
    expect(compact).toHaveClass('inline-flex', 'min-h-9');
    expect(compact.parentElement).toHaveClass('flex-wrap');
    fireEvent.click(compact);

    expect(document.documentElement.dataset.density).toBe('compact');
    expect(window.localStorage.getItem('accord.density')).toBe('compact');
  });
});

describe('SettingsModal — accessibilité', () => {
  it('change la taille de police sur la racine', () => {
    render(<SettingsModal />);
    openTab('Accessibilité');

    fireEvent.click(screen.getByRole('button', { name: '150 %' }));

    expect(document.documentElement.style.fontSize).toBe('150%');
    expect(window.localStorage.getItem('accord.fontScale')).toBe('150');
  });

  it('force la réduction d’animations sur la racine et la persiste', () => {
    render(<SettingsModal />);
    openTab('Accessibilité');

    fireEvent.click(screen.getByRole('button', { name: 'Activé' }));

    expect(document.documentElement.dataset.motion).toBe('reduce');
    expect(window.localStorage.getItem('accord.a11y.reducedMotion')).toBe('on');
  });

  it('ajuste la saturation globale et la persiste', () => {
    render(<SettingsModal />);
    openTab('Accessibilité');

    fireEvent.change(screen.getByLabelText('Régler la saturation des couleurs'), {
      target: { value: '50' },
    });

    expect(document.documentElement.style.getPropertyValue('--saturation')).toBe('50%');
    expect(window.localStorage.getItem('accord.a11y.saturation')).toBe('50');
  });
});

describe('SettingsModal — langue et heure', () => {
  it('bascule l’interface en anglais et persiste le choix', () => {
    render(<SettingsModal />);
    openTab('Langue et heure');

    fireEvent.click(screen.getByRole('button', { name: 'English' }));

    expect(useUi.getState().lang).toBe('en');
    expect(window.localStorage.getItem('accord.lang')).toBe('en');
    // L'interface a changé de langue immédiatement.
    expect(screen.getByRole('dialog', { name: 'Settings' })).toBeInTheDocument();
  });
});

describe('SettingsModal — confidentialité', () => {
  it('liste les utilisateurs bloqués et permet le déblocage', () => {
    const unblock = vi.fn(async () => {});
    useFriends.setState({
      contacts: [blockedContact('aaa', 'Casse-pieds'), blockedContact('bbb', 'Spammeur')],
      unblock,
    });
    render(<SettingsModal />);
    openTab('Confidentialité');

    expect(screen.getByText('Casse-pieds')).toBeInTheDocument();
    expect(screen.getByText('Spammeur')).toBeInTheDocument();

    const buttons = screen.getAllByRole('button', { name: 'Débloquer' });
    expect(buttons).toHaveLength(2);
    fireEvent.click(buttons[0]!);

    expect(unblock).toHaveBeenCalledWith('aaa');
  });

  it('affiche l’état vide et l’explication anti-spam', () => {
    render(<SettingsModal />);
    openTab('Confidentialité');

    expect(screen.getByText('Personne n’est bloqué.')).toBeInTheDocument();
    expect(screen.getByText(/une seule demande d’ami en attente/)).toBeInTheDocument();
  });
});

describe('SettingsModal — avancé', () => {
  it('affiche la version, la licence et le code ami copiable', () => {
    render(<SettingsModal />);
    openTab('Avancé');

    expect(screen.getByText(APP_VERSION)).toBeInTheDocument();
    expect(screen.getByText(/licence MIT/)).toBeInTheDocument();
    expect(screen.getByText(/THIRD_PARTY\.md/)).toBeInTheDocument();
    expect(screen.getByText(self.friend_code)).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: 'Copier mon code ami' }),
    ).toBeInTheDocument();
  });
});
