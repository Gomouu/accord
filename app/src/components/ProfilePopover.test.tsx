/**
 * Tests de la carte de profil : contenu (pseudo, bio, présence, rôles en
 * contexte de serveur), actions (« Envoyer un message », « Bloquer ») et
 * fermeture (Échap). Les actions du store sont remplacées par des espions.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import type { Contact, GroupStateJson, SelfProfile } from '../lib/api';
import { useFriends } from '../stores/friends';
import { useGroups } from '../stores/groups';
import { useSession } from '../stores/session';
import { useUi, type AncrePopover, type ProfileSurface } from '../stores/ui';
import { ProfilePopover } from './ProfilePopover';

const ANCRE: AncrePopover = { top: 0, left: 0, bottom: 0, right: 0 };

const MOI: SelfProfile = {
  node_id: 'nm',
  pubkey: 'moi',
  friend_code: 'accord-moi',
  name: 'Moi',
  bio: 'ma bio',
  avatar: null,
  banner: null,
  pronouns: null,
  accent_color: null,
  banner_color: null,
  avatar_decoration: null,
  profile_effect: null,
  profile_frame: null,
};

const AMI: Contact = {
  node_id: 'na',
  pubkey: 'ami-pk',
  friend_code: 'accord-ami',
  display_name: 'Alice',
  bio: 'Salut, je suis Alice',
  avatar: null,
  banner: null,
  state: 'friend',
  last_seen_ms: 0,
  online: true,
};

function serverState(): GroupStateJson {
  return {
    group_id: 'g1',
    name: 'Guilde',
    icon: null,
    founder: 'f',
    members: [{ pubkey: 'ami-pk', roles: ['r1'] }],
    bans: [],
    channels: [],
    categories: [],
    roles: [
      { role_id: 'r1', name: 'Modo', color: 0xff0000, position: 5, permissions: 0 },
    ],
    invites: [],
    emojis: [],
    my_permissions: 0x1ff,
  };
}

function openFor(
  pubkey: string,
  groupId: string | null = null,
  surface: ProfileSurface = 'profile-card',
): void {
  useUi.setState({ profile: { pubkey, ancre: ANCRE, groupId, surface } });
}

beforeEach(() => {
  useUi.setState({ lang: 'fr', profile: null, view: { kind: 'friends' }, modal: null });
  useSession.setState({ self: MOI });
  useFriends.setState({ contacts: [AMI] });
  useGroups.setState({ states: {} });
});

describe('ProfilePopover — contenu', () => {
  it('affiche pseudo, bio et présence d’un ami', () => {
    openFor('ami-pk');
    render(<ProfilePopover />);

    expect(screen.getByText('Alice')).toBeInTheDocument();
    expect(screen.getByText('Salut, je suis Alice')).toBeInTheDocument();
    expect(screen.getByText('En ligne')).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: 'Envoyer un message' }),
    ).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Bloquer' })).toBeInTheDocument();
  });

  it("n'affiche pas le code ami d'un autre utilisateur (repartage impossible)", () => {
    openFor('ami-pk');
    render(<ProfilePopover />);

    expect(screen.queryByText('accord-ami')).not.toBeInTheDocument();
    expect(
      screen.queryByRole('button', { name: 'Copier le code ami' }),
    ).not.toBeInTheDocument();
  });

  it('affiche son propre code ami avec un bouton de copie', () => {
    openFor('moi');
    render(<ProfilePopover />);

    expect(screen.getByText('accord-moi')).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: 'Copier le code ami' }),
    ).toBeInTheDocument();
  });

  it('copie son propre code ami dans le presse-papiers au clic', () => {
    const writeText = vi.fn(() => Promise.resolve());
    Object.defineProperty(navigator, 'clipboard', {
      value: { writeText },
      configurable: true,
    });
    openFor('moi');
    render(<ProfilePopover />);

    fireEvent.click(screen.getByRole('button', { name: 'Copier le code ami' }));

    expect(writeText).toHaveBeenCalledWith('accord-moi');
  });

  it('affiche les rôles colorés en contexte de serveur', () => {
    useGroups.setState({ states: { g1: serverState() } });
    openFor('ami-pk', 'g1');
    render(<ProfilePopover />);

    expect(screen.getByText('Rôles')).toBeInTheDocument();
    expect(screen.getByText('Modo')).toBeInTheDocument();
  });

  it('n’offre ni message ni blocage sur son propre profil', () => {
    openFor('moi');
    render(<ProfilePopover />);

    expect(screen.getByText('Moi')).toBeInTheDocument();
    expect(
      screen.queryByRole('button', { name: 'Envoyer un message' }),
    ).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Bloquer' })).not.toBeInTheDocument();
  });

  it('ne rend rien sans cible', () => {
    const { container } = render(<ProfilePopover />);
    expect(container).toBeEmptyDOMElement();
  });
});

describe('ProfilePopover — actions', () => {
  it('« Envoyer un message » ouvre la conversation directe', () => {
    openFor('ami-pk');
    render(<ProfilePopover />);

    fireEvent.click(screen.getByRole('button', { name: 'Envoyer un message' }));

    expect(useUi.getState().view).toEqual({ kind: 'dm', peer: 'ami-pk' });
  });

  it('« Bloquer » appelle le store et ferme la carte', () => {
    const block = vi.fn(() => Promise.resolve());
    useFriends.setState({ block });
    openFor('ami-pk');
    render(<ProfilePopover />);

    fireEvent.click(screen.getByRole('button', { name: 'Bloquer' }));

    expect(block).toHaveBeenCalledWith('ami-pk');
    expect(useUi.getState().profile).toBeNull();
  });

  it('se ferme avec Échap', () => {
    openFor('ami-pk');
    render(<ProfilePopover />);

    fireEvent.keyDown(window, { key: 'Escape' });

    expect(useUi.getState().profile).toBeNull();
  });

  it('piège le focus et le rend au déclencheur', () => {
    const trigger = document.createElement('button');
    document.body.append(trigger);
    trigger.focus();
    openFor('moi');
    render(<ProfilePopover />);

    const dialog = screen.getByRole('dialog', { name: 'Profil' });
    expect(dialog).toHaveFocus();

    fireEvent.keyDown(dialog, { key: 'Tab', shiftKey: true });
    expect(screen.getByRole('button', { name: 'Modifier mon profil' })).toHaveFocus();

    fireEvent.keyDown(window, { key: 'Escape' });
    expect(trigger).toHaveFocus();
    trigger.remove();
  });
});

describe('ProfilePopover — pseudos de serveur', () => {
  function stateWith(members: GroupStateJson['members']): GroupStateJson {
    return { ...serverState(), members };
  }

  it('affiche le pseudo de serveur au lieu du pseudo global', () => {
    useGroups.setState({
      states: {
        g1: stateWith([{ pubkey: 'ami-pk', roles: ['r1'], nickname: 'Alicette' }]),
      },
    });
    openFor('ami-pk', 'g1');
    render(<ProfilePopover />);

    expect(screen.getByText('Alicette')).toBeInTheDocument();
    expect(screen.queryByText('Alice')).not.toBeInTheDocument();
  });

  it('affiche sa carte publique propre dans la liste des membres', () => {
    useGroups.setState({
      states: { g1: stateWith([{ pubkey: 'moi', roles: [], nickname: 'MoiServeur' }]) },
    });
    openFor('moi', 'g1');
    render(<ProfilePopover />);

    expect(screen.getByRole('dialog', { name: 'Profil' })).toBeInTheDocument();
    expect(screen.getByText('MoiServeur')).toBeInTheDocument();
    expect(screen.queryByLabelText('Pseudo de serveur')).not.toBeInTheDocument();
    expect(screen.queryByText('Changer de compte')).not.toBeInTheDocument();
    expect(screen.queryByText('Se déconnecter')).not.toBeInTheDocument();
    expect(screen.queryByText(/Définir le statut/)).not.toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: 'Modifier mon profil' }),
    ).toBeInTheDocument();
  });

  it('conserve le menu utilisateur sur son panneau local', () => {
    openFor('moi', null, 'user-menu');
    render(<ProfilePopover />);

    expect(screen.getByRole('dialog', { name: 'Menu utilisateur' })).toBeInTheDocument();
  });

  it('rend le cadre et l’effet dans le menu ouvert depuis le panneau local', async () => {
    useSession.setState({
      self: { ...MOI, profile_effect: 'starfield', profile_frame: 'lumen_bloom' },
    });
    openFor('moi', null, 'user-menu');
    render(<ProfilePopover />);

    expect(screen.getByRole('dialog', { name: 'Menu utilisateur' })).toBeInTheDocument();
    expect(await screen.findByTestId('profile-frame')).toBeInTheDocument();
    expect(await screen.findByTestId('profile-effect')).toBeInTheDocument();
  });

  it('n’affiche aucun cadre dans le menu local sans cadre choisi', () => {
    useSession.setState({ self: { ...MOI, profile_frame: null } });
    openFor('moi', null, 'user-menu');
    render(<ProfilePopover />);

    expect(screen.queryByTestId('profile-frame')).not.toBeInTheDocument();
  });

  it('ne montre pas le champ de pseudo hors contexte de serveur', () => {
    openFor('moi');
    render(<ProfilePopover />);

    expect(screen.queryByLabelText('Pseudo de serveur')).not.toBeInTheDocument();
  });
});

describe('ProfilePopover — pronoms et couleurs de profil', () => {
  it('affiche les pronoms sous le pseudo pour un ami', () => {
    useFriends.setState({ contacts: [{ ...AMI, pronouns: 'iel/iel' }] });
    openFor('ami-pk');
    render(<ProfilePopover />);

    expect(screen.getByText('iel/iel')).toBeInTheDocument();
  });

  it('affiche les pronoms sous son propre pseudo', () => {
    useSession.setState({ self: { ...MOI, pronouns: 'il/lui' } });
    openFor('moi');
    render(<ProfilePopover />);

    expect(screen.getByText('il/lui')).toBeInTheDocument();
  });

  it('n’affiche rien quand les pronoms sont absents', () => {
    openFor('ami-pk');
    render(<ProfilePopover />);

    expect(screen.queryByText('iel/iel')).not.toBeInTheDocument();
  });

  it('remplit la bannière avec la couleur de profil quand aucune image n’est définie', () => {
    useFriends.setState({ contacts: [{ ...AMI, banner_color: 0x5865f2 }] });
    openFor('ami-pk');
    render(<ProfilePopover />);

    expect(screen.getByTestId('profile-banner-fill')).toHaveStyle({
      backgroundColor: '#5865f2',
    });
  });

  it('l’image de bannière l’emporte toujours sur la couleur', () => {
    useFriends.setState({
      contacts: [{ ...AMI, banner: 'ab'.repeat(32), banner_color: 0x5865f2 }],
    });
    openFor('ami-pk');
    render(<ProfilePopover />);

    // Bannière encore en chargement (fichier non résolu en test) : fond
    // neutre, jamais la couleur, tant qu'une image est annoncée.
    expect(screen.getByTestId('profile-banner-fill')).not.toHaveStyle({
      backgroundColor: '#5865f2',
    });
  });

  it('teinte le pseudo avec la couleur d’accent quand elle est définie', () => {
    useSession.setState({ self: { ...MOI, accent_color: 0xed4245 } });
    openFor('moi');
    render(<ProfilePopover />);

    expect(screen.getByText('Moi')).toHaveStyle({ color: '#ed4245' });
  });
});

describe('ProfilePopover — carte thématisée', () => {
  it('teinte le fond de carte quand une couleur de bannière est définie', () => {
    useFriends.setState({ contacts: [{ ...AMI, banner_color: 0x5865f2 }] });
    openFor('ami-pk');
    render(<ProfilePopover />);

    const tint = document.querySelector('.profile-card-tint') as HTMLElement;
    expect(tint.style.backgroundImage).toContain('rgba(88, 101, 242');
  });

  it('garde le fond neutre sans couleur de profil', () => {
    openFor('ami-pk');
    render(<ProfilePopover />);

    expect(document.querySelector('.profile-card-tint')).toBeNull();
  });
});

describe('ProfilePopover — bio et liens', () => {
  it('rend un lien http(s) de la bio cliquable', () => {
    useFriends.setState({
      contacts: [{ ...AMI, bio: 'Visitez https://example.com pour en savoir plus' }],
    });
    openFor('ami-pk');
    render(<ProfilePopover />);

    const link = screen.getByRole('link', { name: 'https://example.com' });
    expect(link).toHaveAttribute('href', 'https://example.com');
    expect(link).toHaveAttribute('target', '_blank');
    expect(link).toHaveAttribute('rel', 'noopener noreferrer');
  });

  it('ne rend jamais un schéma javascript: cliquable dans la bio', () => {
    useFriends.setState({
      contacts: [{ ...AMI, bio: '[cliquez ici](javascript:alert(1))' }],
    });
    openFor('ami-pk');
    render(<ProfilePopover />);

    expect(screen.queryByRole('link')).not.toBeInTheDocument();
  });
});

describe('ProfilePopover — personnalisation', () => {
  it("rend séparément la décoration, l'effet et le cadre du profil local", async () => {
    useSession.setState({
      self: {
        ...MOI,
        avatar_decoration: 'aurora_ring',
        profile_effect: 'starfield',
        profile_frame: 'lumen_bloom',
      },
    });
    openFor('moi');
    render(<ProfilePopover />);

    expect(await screen.findByTestId('avatar-decoration')).toBeInTheDocument();
    expect(await screen.findByTestId('profile-effect')).toBeInTheDocument();
    expect(await screen.findByTestId('profile-frame')).toBeInTheDocument();
  });

  it("rend séparément la décoration, l'effet et le cadre annoncés par un ami", async () => {
    useFriends.setState({
      contacts: [
        {
          ...AMI,
          avatar_decoration: 'sakura_arc',
          profile_effect: 'falling_petals',
          profile_frame: 'crystal_crown',
        },
      ],
    });
    openFor('ami-pk');
    render(<ProfilePopover />);

    expect(await screen.findByTestId('avatar-decoration')).toBeInTheDocument();
    expect(await screen.findByTestId('profile-effect')).toBeInTheDocument();
    expect(await screen.findByTestId('profile-frame')).toBeInTheDocument();
  });

  it("ignore les ids inconnus d'un pair sans les refléter dans le DOM", () => {
    const hostile = '"><style>body{display:none}</style>';
    useFriends.setState({
      contacts: [
        {
          ...AMI,
          avatar_decoration: hostile,
          profile_effect: hostile,
          profile_frame: hostile,
        },
      ],
    });
    openFor('ami-pk');
    const { container } = render(<ProfilePopover />);

    expect(screen.queryByTestId('avatar-decoration')).not.toBeInTheDocument();
    expect(screen.queryByTestId('profile-effect')).not.toBeInTheDocument();
    expect(screen.queryByTestId('profile-frame')).not.toBeInTheDocument();
    expect(container.innerHTML).not.toContain(hostile);
  });
});
