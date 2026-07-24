/**
 * Mode streamer : le code ami ne doit pas rester lisible à l'écran quand le
 * réglage est actif — et doit rester parfaitement normal quand il ne l'est pas.
 */

import { beforeEach, describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useUi } from '../stores/ui';
import { SecretText } from './SecretText';

beforeEach(() => {
  useUi.setState({ lang: 'fr', streamerMode: false });
});

describe('SecretText', () => {
  it('affiche la valeur telle quelle hors mode streamer', () => {
    render(<SecretText value="cheval-lune-orage" />);
    expect(screen.getByText('cheval-lune-orage')).toBeInTheDocument();
    expect(screen.queryByRole('button')).toBeNull();
  });

  it('masque la valeur en mode streamer', () => {
    useUi.setState({ streamerMode: true });
    render(<SecretText value="cheval-lune-orage" />);
    expect(screen.queryByText('cheval-lune-orage')).toBeNull();
    expect(screen.getByRole('button', { name: 'Afficher' })).toBeInTheDocument();
  });

  it('ne laisse pas fuir la longueur de la valeur', () => {
    // Un masque proportionnel dirait déjà quelque chose du secret.
    useUi.setState({ streamerMode: true });
    const { rerender } = render(<SecretText value="court" />);
    const courtMasque = screen.getByRole('button').textContent;
    rerender(<SecretText value="beaucoup-beaucoup-plus-long-que-le-precedent" />);
    expect(screen.getByRole('button').textContent).toBe(courtMasque);
  });

  it('révèle la valeur à la demande', async () => {
    const user = userEvent.setup();
    useUi.setState({ streamerMode: true });
    render(<SecretText value="cheval-lune-orage" />);

    await user.click(screen.getByRole('button', { name: 'Afficher' }));
    expect(screen.getByText('cheval-lune-orage')).toBeInTheDocument();
  });
});
