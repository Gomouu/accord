/**
 * Tests de l'aperçu de carte de profil des paramètres : reprise fidèle des
 * couches de la carte (cadre, effet, décoration d'avatar) pilotées par la
 * session, et absence de couche quand rien n'est choisi.
 */

import { beforeEach, describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/react';
import type { SelfProfile } from '../../lib/api';
import { useFriends } from '../../stores/friends';
import { useSession } from '../../stores/session';
import { ProfileCardPreview } from './ProfileCardPreview';

const MOI: SelfProfile = {
  node_id: 'nm',
  pubkey: 'moi',
  friend_code: 'accord-moi',
  name: 'Moi',
  bio: 'ma bio',
  avatar: null,
  banner: null,
  pronouns: 'iel',
  accent_color: null,
  banner_color: null,
  avatar_decoration: null,
  profile_effect: null,
  profile_frame: null,
};

describe('ProfileCardPreview', () => {
  beforeEach(() => {
    useSession.setState({ self: MOI });
    useFriends.setState({ ownStatus: 'online', ownStatusText: null });
  });

  it('rend la carte avec cadre, effet et décoration choisis', () => {
    useSession.setState({
      self: {
        ...MOI,
        avatar_decoration: 'phoenix_plume',
        profile_effect: 'code_rain',
        profile_frame: 'wild_ivy',
      },
    });
    render(<ProfileCardPreview />);

    expect(screen.getByTestId('profile-card-preview')).toBeInTheDocument();
    expect(screen.getByText('Moi')).toBeInTheDocument();
    expect(screen.getByText('iel')).toBeInTheDocument();
    expect(screen.getByTestId('profile-frame')).toBeInTheDocument();
    expect(screen.getByTestId('profile-effect')).toBeInTheDocument();
    expect(screen.getByTestId('avatar-decoration')).toBeInTheDocument();
  });

  it("n'affiche aucune couche décorative sans sélection", () => {
    render(<ProfileCardPreview />);

    expect(screen.getByTestId('profile-card-preview')).toBeInTheDocument();
    expect(screen.queryByTestId('profile-frame')).not.toBeInTheDocument();
    expect(screen.queryByTestId('profile-effect')).not.toBeInTheDocument();
    expect(screen.queryByTestId('avatar-decoration')).not.toBeInTheDocument();
  });

  it('ne rend rien sans session', () => {
    useSession.setState({ self: null });
    const { container } = render(<ProfileCardPreview />);

    expect(container).toBeEmptyDOMElement();
  });
});
