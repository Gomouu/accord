/**
 * Tests de l'émoji custom : repli sur le jeton texte `:name:` tant que l'image
 * n'est pas chargée (ou en cas d'échec), puis rendu de l'image une fois prête.
 */

import { describe, expect, it, vi } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import { CustomEmoji } from './CustomEmoji';

vi.mock('../lib/files', () => ({
  lireFichier: vi.fn(),
}));

import { lireFichier } from '../lib/files';
import type { Mock } from 'vitest';

const lireMock = lireFichier as unknown as Mock;

describe('CustomEmoji', () => {
  it('affiche le jeton texte `:name:` tant que l’image n’est pas chargée', () => {
    lireMock.mockReturnValue(new Promise(() => {}));
    render(<CustomEmoji name="party" merkleRoot="root" />);
    expect(screen.getByText(':party:')).toBeInTheDocument();
    expect(screen.queryByRole('img')).toBeNull();
  });

  it('rend l’image une fois le blob résolu, avec un alt accessible', async () => {
    lireMock.mockResolvedValue('blob:x');
    render(<CustomEmoji name="party" merkleRoot="root" />);
    const img = await screen.findByRole('img');
    expect(img).toHaveAttribute('src', 'blob:x');
    expect(img).toHaveAttribute('alt', ':party:');
  });

  it('reste sur le jeton texte si le chargement échoue', async () => {
    lireMock.mockRejectedValue(new Error('injoignable'));
    render(<CustomEmoji name="sad" merkleRoot="root" />);
    await waitFor(() => expect(screen.getByText(':sad:')).toBeInTheDocument());
    expect(screen.queryByRole('img')).toBeNull();
  });
});
