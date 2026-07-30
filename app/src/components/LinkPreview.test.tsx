/**
 * Tests de la carte d'aperçu. Le point qui compte n'est pas l'affichage : c'est
 * qu'aucune requête ne parte tant que l'utilisateur n'a rien activé.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';

const invokeMock = vi.fn();
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...a: unknown[]) => invokeMock(...a),
}));

import { useUi } from '../stores/ui';
import { LinkPreview, premierLien } from './LinkPreview';

const APERCU = {
  url: 'https://exemple.fr/page',
  titre: 'Un titre',
  description: 'Une description',
  image: null,
  hote: 'exemple.fr',
};

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockResolvedValue(APERCU);
  useUi.setState({ lang: 'fr', linkPreviews: false });
});

describe('LinkPreview', () => {
  it('🔒 ne va rien chercher tant que le réglage est éteint', async () => {
    const { container } = render(<LinkPreview texte="regarde https://exemple.fr/page" />);

    // Laisse tourner les microtâches : un appel différé serait quand même parti.
    await Promise.resolve();

    expect(invokeMock).not.toHaveBeenCalled();
    expect(container).toBeEmptyDOMElement();
  });

  it('va chercher l’aperçu une fois le réglage activé', async () => {
    useUi.setState({ linkPreviews: true });

    render(<LinkPreview texte="regarde https://exemple.fr/page" />);

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith('apercu_lien', {
        url: 'https://exemple.fr/page',
      }),
    );
    expect(await screen.findByText('Un titre')).toBeInTheDocument();
    expect(screen.getByText('exemple.fr')).toBeInTheDocument();
  });

  it('🔒 ne pose jamais dans un href une URL finale de schéma non http(s)', async () => {
    useUi.setState({ linkPreviews: true });
    // `apercu.url` est l'URL finale après redirections, recomposée depuis un
    // en-tête `Location` que le site visité écrit. L'hôte n'en retient qu'une
    // http(s) ; la carte ne s'en remet pas à lui pour autant.
    invokeMock.mockResolvedValue({ ...APERCU, url: 'javascript:alert(1)' });

    const { container } = render(<LinkPreview texte="regarde https://exemple.fr/page" />);

    await waitFor(() => expect(invokeMock).toHaveBeenCalled());
    expect(container.querySelector('a')).toBeNull();
    expect(screen.queryByText('Un titre')).not.toBeInTheDocument();
  });

  it('n’appelle rien sur un message sans lien', async () => {
    useUi.setState({ linkPreviews: true });

    render(<LinkPreview texte="pas de lien ici" />);
    await Promise.resolve();

    expect(invokeMock).not.toHaveBeenCalled();
  });

  it('reste muet quand la récupération échoue', async () => {
    useUi.setState({ linkPreviews: true });
    invokeMock.mockRejectedValue(new Error('refusé'));

    const { container } = render(<LinkPreview texte="https://exemple.fr/x" />);
    await waitFor(() => expect(invokeMock).toHaveBeenCalled());

    // Un aperçu manquant n'est pas une erreur pour l'utilisateur : le lien
    // lui-même reste cliquable au-dessus, rendu par MarkdownText.
    expect(container).toBeEmptyDOMElement();
  });
});

describe('premierLien', () => {
  it('ne retient que le premier lien', () => {
    // Un message truffé de liens déclencherait autant de requêtes — c'est le
    // levier qu'utiliserait quelqu'un cherchant à faire parler l'appareil.
    expect(premierLien('a https://un.fr/x puis https://deux.fr/y')).toBe(
      'https://un.fr/x',
    );
  });

  it('laisse la ponctuation finale à la phrase', () => {
    expect(premierLien('vu sur https://exemple.fr/page.')).toBe(
      'https://exemple.fr/page',
    );
    expect(premierLien('(https://exemple.fr/a)')).toBe('https://exemple.fr/a');
  });

  it('ignore ce qui n’est pas http(s)', () => {
    expect(premierLien('file:///etc/passwd')).toBeNull();
    expect(premierLien('javascript:alert(1)')).toBeNull();
    expect(premierLien('sans lien')).toBeNull();
  });
});
