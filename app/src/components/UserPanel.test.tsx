/**
 * Tests du panneau utilisateur : menu utilisateur rapide (clic sur
 * l'avatar/pseudo — statut, copie d'ID, déconnexion), et bandeau « Vocal
 * connecté » — n'apparaît qu'en vocal, nomme le groupe, coupe le micro
 * (icône barrée, aria-pressed) et raccroche.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import type { Contact, GroupStateJson, SelfProfile } from '../lib/api';
import { useCalls } from '../stores/calls';
import { useFriends } from '../stores/friends';
import { useGroups } from '../stores/groups';
import { useSession } from '../stores/session';
import { useUi } from '../stores/ui';
import { useVoice } from '../stores/voice';
import { UserPanel } from './UserPanel';

const self: SelfProfile = {
  node_id: 'n-moi',
  pubkey: 'moi',
  friend_code: 'accord-moi',
  name: null,
  bio: null,
  avatar: null,
  banner: null,
  pronouns: null,
  accent_color: null,
  banner_color: null,
  avatar_decoration: null,
  profile_effect: null,
};

const alice: Contact = {
  node_id: 'n-alice',
  pubkey: 'alice',
  friend_code: 'accord-alice',
  display_name: 'Alice',
  bio: null,
  avatar: null,
  banner: null,
  state: 'friend',
  last_seen_ms: 0,
};

const groupState: GroupStateJson = {
  group_id: 'g1',
  name: 'Les copains',
  icon: null,
  founder: null,
  members: [{ pubkey: 'moi', roles: [] }],
  bans: [],
  channels: [],
  categories: [],
  roles: [],
  invites: [],
  my_permissions: 0x3,
};

beforeEach(() => {
  useUi.setState({ lang: 'fr', toasts: [], profile: null, modal: null });
  useSession.setState({ self, phase: 'ready' });
  useGroups.setState({ states: { g1: groupState } });
  useVoice.setState({ active: null, participants: new Map() });
  useCalls.setState({ phase: 'idle', peer: null, callId: null, sincePhaseMs: null });
  useFriends.setState({
    contacts: [alice],
    ownStatus: 'online',
    ownStatusText: null,
    loadOwnStatus: vi.fn(async () => {}),
  });
});

describe('UserPanel — menu utilisateur rapide', () => {
  it('rend la décoration de son propre avatar', () => {
    useSession.setState({ self: { ...self, avatar_decoration: 'neon_ring' } });

    render(<UserPanel />);

    expect(screen.getByTestId('avatar-decoration')).toBeInTheDocument();
  });

  it('ouvre le menu utilisateur au clic sur l’avatar/pseudo', () => {
    render(<UserPanel />);

    const trigger = screen.getByRole('button', { name: 'Menu utilisateur' });
    expect(trigger).toHaveAttribute('aria-haspopup', 'dialog');
    expect(trigger).toHaveAttribute('aria-expanded', 'false');

    fireEvent.click(trigger);

    expect(screen.getByRole('dialog', { name: 'Menu utilisateur' })).toBeInTheDocument();
    expect(trigger).toHaveAttribute('aria-expanded', 'true');
    expect(useUi.getState().profile).toBeNull();
  });

  it('referme le panneau au second clic sur son déclencheur', () => {
    render(<UserPanel />);

    const trigger = screen.getByRole('button', { name: 'Menu utilisateur' });
    fireEvent.click(trigger);
    fireEvent.mouseDown(trigger);
    fireEvent.click(trigger);

    expect(screen.queryByRole('dialog', { name: 'Menu utilisateur' })).toBeNull();
    expect(trigger).toHaveAttribute('aria-expanded', 'false');
  });

  it('applique le statut choisi puis ferme le menu', () => {
    const setOwnStatus = vi.fn(async () => {});
    useFriends.setState({ setOwnStatus });
    render(<UserPanel />);

    fireEvent.click(screen.getByRole('button', { name: 'Menu utilisateur' }));
    fireEvent.click(screen.getByRole('button', { name: 'Définir le statut — En ligne' }));
    fireEvent.click(screen.getByRole('radio', { name: 'Ne pas déranger' }));

    expect(setOwnStatus).toHaveBeenCalledWith('dnd', undefined);
    expect(
      screen.queryByRole('dialog', { name: 'Menu utilisateur' }),
    ).not.toBeInTheDocument();
  });

  it('enregistre le texte de statut personnalisé avec Entrée', () => {
    const setOwnStatus = vi.fn(async () => {});
    useFriends.setState({ ownStatus: 'idle', setOwnStatus });
    render(<UserPanel />);

    fireEvent.click(screen.getByRole('button', { name: 'Menu utilisateur' }));
    fireEvent.click(screen.getByRole('button', { name: 'Définir le statut — Inactif' }));
    const input = screen.getByRole('textbox', { name: 'Statut personnalisé' });
    fireEvent.change(input, { target: { value: 'en pause' } });
    fireEvent.keyDown(input, { key: 'Enter' });

    expect(setOwnStatus).toHaveBeenCalledWith('idle', 'en pause');
  });

  it('copie son propre ID dans le presse-papiers', () => {
    const writeText = vi.fn(() => Promise.resolve());
    Object.defineProperty(navigator, 'clipboard', {
      value: { writeText },
      configurable: true,
    });
    render(<UserPanel />);

    fireEvent.click(screen.getByRole('button', { name: 'Menu utilisateur' }));
    fireEvent.click(
      screen.getByRole('button', { name: 'Copier l’identifiant utilisateur' }),
    );

    expect(writeText).toHaveBeenCalledWith('moi');
    expect(
      screen.queryByRole('dialog', { name: 'Menu utilisateur' }),
    ).not.toBeInTheDocument();
  });

  it('ouvre directement les paramètres de profil', () => {
    render(<UserPanel />);

    fireEvent.click(screen.getByRole('button', { name: 'Menu utilisateur' }));
    fireEvent.click(screen.getByRole('button', { name: 'Modifier mon profil' }));

    expect(useUi.getState().modal).toEqual({ kind: 'settings' });
    expect(
      screen.queryByRole('dialog', { name: 'Menu utilisateur' }),
    ).not.toBeInTheDocument();
  });

  it('déconnexion rapide : demande confirmation puis appelle lock()', () => {
    const lock = vi.fn(async () => {});
    useSession.setState({ lock });
    render(<UserPanel />);

    fireEvent.click(screen.getByRole('button', { name: 'Menu utilisateur' }));
    fireEvent.click(screen.getByRole('button', { name: 'Se déconnecter' }));

    // Premier clic : confirmation inline, pas encore déconnecté.
    expect(lock).not.toHaveBeenCalled();
    expect(
      screen.getByText('Votre phrase de passe sera nécessaire pour vous reconnecter.'),
    ).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'Oui, me déconnecter' }));

    expect(lock).toHaveBeenCalledTimes(1);
  });

  it('ferme le menu à Échap', () => {
    render(<UserPanel />);

    fireEvent.click(screen.getByRole('button', { name: 'Menu utilisateur' }));
    expect(screen.getByRole('dialog', { name: 'Menu utilisateur' })).toBeInTheDocument();

    fireEvent.keyDown(window, { key: 'Escape' });

    expect(
      screen.queryByRole('dialog', { name: 'Menu utilisateur' }),
    ).not.toBeInTheDocument();
  });
});

describe('UserPanel — bandeau vocal', () => {
  it('reste absent hors salon vocal', () => {
    render(<UserPanel />);

    expect(screen.queryByText('Vocal connecté')).not.toBeInTheDocument();
  });

  it('affiche l’état connecté et le nom du groupe', () => {
    useVoice.setState({
      active: { groupId: 'g1', channelId: 'g1', muted: false, isCall: false },
    });
    render(<UserPanel />);

    expect(screen.getByText('Vocal connecté')).toBeInTheDocument();
    expect(screen.getByText('Les copains')).toBeInTheDocument();
  });

  it('coupe le micro au clic et reflète l’état muet', () => {
    const toggleMute = vi.fn(async () => {});
    useVoice.setState({
      active: { groupId: 'g1', channelId: 'g1', muted: false, isCall: false },
      toggleMute,
    });
    render(<UserPanel />);

    const muteButton = screen.getByRole('button', { name: 'Couper le micro' });
    expect(muteButton).toHaveAttribute('aria-pressed', 'false');
    fireEvent.click(muteButton);

    expect(toggleMute).toHaveBeenCalledTimes(1);
  });

  it('présente le bouton de rétablissement quand le micro est coupé', () => {
    useVoice.setState({
      active: { groupId: 'g1', channelId: 'g1', muted: true, isCall: false },
    });
    render(<UserPanel />);

    const muteButton = screen.getByRole('button', { name: 'Rétablir le micro' });
    expect(muteButton).toHaveAttribute('aria-pressed', 'true');
  });

  it('raccroche via le bouton rouge', () => {
    const leave = vi.fn(async () => {});
    useVoice.setState({
      active: { groupId: 'g1', channelId: 'g1', muted: false, isCall: false },
      leave,
    });
    render(<UserPanel />);

    fireEvent.click(screen.getByRole('button', { name: 'Raccrocher' }));

    expect(leave).toHaveBeenCalledTimes(1);
  });
});

describe('UserPanel — bandeau d’appel 1-à-1', () => {
  it('prime sur le bandeau de salon vocal de groupe (jamais les deux)', () => {
    useVoice.setState({
      active: { groupId: 'g1', channelId: 'g1', muted: false, isCall: false },
    });
    useCalls.setState({
      phase: 'active',
      peer: 'alice',
      callId: 'c1',
      sincePhaseMs: Date.now(),
    });
    render(<UserPanel />);

    expect(screen.getByText('Alice')).toBeInTheDocument();
    expect(screen.queryByText('Les copains')).not.toBeInTheDocument();
  });

  it('affiche « Sonnerie… » en sonnerie sortante, sans contrôles mute/deafen', () => {
    useCalls.setState({
      phase: 'outgoing_ringing',
      peer: 'alice',
      callId: 'c1',
      sincePhaseMs: Date.now(),
    });
    render(<UserPanel />);

    expect(screen.getByText('Alice')).toBeInTheDocument();
    expect(screen.getByText('Sonnerie…')).toBeInTheDocument();
    expect(
      screen.queryByRole('button', { name: 'Couper le micro' }),
    ).not.toBeInTheDocument();
  });

  it('annule via le bouton rouge en sonnerie sortante', () => {
    const hangup = vi.fn(async () => {});
    useCalls.setState({
      phase: 'outgoing_ringing',
      peer: 'alice',
      callId: 'c1',
      sincePhaseMs: Date.now(),
      hangup,
    });
    render(<UserPanel />);

    fireEvent.click(screen.getByRole('button', { name: 'Annuler l’appel' }));

    expect(hangup).toHaveBeenCalledTimes(1);
  });

  it('affiche les contrôles mute/deafen une fois la session vocale de l’appel synchronisée', () => {
    useCalls.setState({
      phase: 'active',
      peer: 'alice',
      callId: 'c1',
      sincePhaseMs: Date.now(),
    });
    useVoice.setState({
      active: { groupId: '0'.repeat(32), channelId: 'c1', muted: false, isCall: true },
    });
    render(<UserPanel />);

    expect(screen.getByRole('button', { name: 'Couper le micro' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Raccrocher' })).toBeInTheDocument();
  });

  it('reste absent hors appel', () => {
    render(<UserPanel />);

    expect(screen.queryByText('Sonnerie…')).not.toBeInTheDocument();
    expect(
      screen.queryByRole('button', { name: 'Annuler l’appel' }),
    ).not.toBeInTheDocument();
  });
});
