/**
 * Tests du panneau utilisateur : menu de statut (clic sur l'avatar),
 * ouverture de la carte de profil (clic sur le pseudo), et bandeau « Vocal
 * connecté » — n'apparaît qu'en vocal, nomme le groupe, coupe le micro
 * (icône barrée, aria-pressed) et raccroche.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import type { GroupStateJson, SelfProfile } from '../lib/api';
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
  useUi.setState({ lang: 'fr', toasts: [], profile: null });
  useSession.setState({ self, phase: 'ready' });
  useGroups.setState({ states: { g1: groupState } });
  useVoice.setState({ active: null, participants: new Map() });
  useFriends.setState({
    ownStatus: 'online',
    ownStatusText: null,
    loadOwnStatus: vi.fn(async () => {}),
  });
});

describe('UserPanel — carte de profil et menu de statut', () => {
  it('ouvre le menu de statut au clic sur l’avatar (pas la carte de profil)', () => {
    render(<UserPanel />);

    fireEvent.click(screen.getByRole('button', { name: 'Définir le statut' }));

    expect(screen.getByRole('menu', { name: 'Définir le statut' })).toBeInTheDocument();
    expect(useUi.getState().profile).toBeNull();
  });

  it('applique le statut choisi puis ferme le menu', () => {
    const setOwnStatus = vi.fn(async () => {});
    useFriends.setState({ setOwnStatus });
    render(<UserPanel />);

    fireEvent.click(screen.getByRole('button', { name: 'Définir le statut' }));
    fireEvent.click(screen.getByRole('menuitemradio', { name: 'Ne pas déranger' }));

    expect(setOwnStatus).toHaveBeenCalledWith('dnd', undefined);
    expect(screen.queryByRole('menu')).not.toBeInTheDocument();
  });

  it('enregistre le texte de statut personnalisé avec Entrée', () => {
    const setOwnStatus = vi.fn(async () => {});
    useFriends.setState({ ownStatus: 'idle', setOwnStatus });
    render(<UserPanel />);

    fireEvent.click(screen.getByRole('button', { name: 'Définir le statut' }));
    const input = screen.getByRole('textbox', { name: 'Statut personnalisé' });
    fireEvent.change(input, { target: { value: 'en pause' } });
    fireEvent.keyDown(input, { key: 'Enter' });

    expect(setOwnStatus).toHaveBeenCalledWith('idle', 'en pause');
  });

  it('ouvre la carte de profil au clic sur le pseudo', () => {
    render(<UserPanel />);

    fireEvent.click(screen.getByRole('button', { name: 'Profil' }));

    expect(useUi.getState().profile?.pubkey).toBe('moi');
  });
});

describe('UserPanel — bandeau vocal', () => {
  it('reste absent hors salon vocal', () => {
    render(<UserPanel />);

    expect(screen.queryByText('Vocal connecté')).not.toBeInTheDocument();
  });

  it('affiche l’état connecté et le nom du groupe', () => {
    useVoice.setState({ active: { groupId: 'g1', channelId: 'g1', muted: false } });
    render(<UserPanel />);

    expect(screen.getByText('Vocal connecté')).toBeInTheDocument();
    expect(screen.getByText('Les copains')).toBeInTheDocument();
  });

  it('coupe le micro au clic et reflète l’état muet', () => {
    const toggleMute = vi.fn(async () => {});
    useVoice.setState({
      active: { groupId: 'g1', channelId: 'g1', muted: false },
      toggleMute,
    });
    render(<UserPanel />);

    const muteButton = screen.getByRole('button', { name: 'Couper le micro' });
    expect(muteButton).toHaveAttribute('aria-pressed', 'false');
    fireEvent.click(muteButton);

    expect(toggleMute).toHaveBeenCalledTimes(1);
  });

  it('présente le bouton de rétablissement quand le micro est coupé', () => {
    useVoice.setState({ active: { groupId: 'g1', channelId: 'g1', muted: true } });
    render(<UserPanel />);

    const muteButton = screen.getByRole('button', { name: 'Rétablir le micro' });
    expect(muteButton).toHaveAttribute('aria-pressed', 'true');
  });

  it('raccroche via le bouton rouge', () => {
    const leave = vi.fn(async () => {});
    useVoice.setState({
      active: { groupId: 'g1', channelId: 'g1', muted: false },
      leave,
    });
    render(<UserPanel />);

    fireEvent.click(screen.getByRole('button', { name: 'Raccrocher' }));

    expect(leave).toHaveBeenCalledTimes(1);
  });
});
