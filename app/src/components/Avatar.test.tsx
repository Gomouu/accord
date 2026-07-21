/**
 * Tests de l'avatar : repli initiales + couleur sans hash, image chargée via
 * lireFichier quand un hash est fourni, repli pendant le chargement et en
 * cas d'échec (image indisponible).
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Mock } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { Avatar } from './Avatar';

vi.mock('../lib/files', () => ({ lireFichier: vi.fn() }));

import { lireFichier } from '../lib/files';

const lireMock = lireFichier as unknown as Mock;
const HASH = 'ab'.repeat(32);

beforeEach(() => {
  lireMock.mockReset();
});

describe('Avatar', () => {
  it('affiche les initiales sans hash, sans lire le magasin', () => {
    render(<Avatar id="aabbcc" name="Alice Bob" />);

    expect(screen.getByText('AB')).toBeInTheDocument();
    expect(lireMock).not.toHaveBeenCalled();
  });

  it('affiche l’image une fois le blob lu (hint transmis)', async () => {
    lireMock.mockResolvedValueOnce('blob:avatar');
    const { container } = render(
      <Avatar id="aabbcc" name="Alice" avatarHash={HASH} hint="alice-pk" />,
    );

    await waitFor(() => {
      expect(container.querySelector('img')).toHaveAttribute('src', 'blob:avatar');
    });
    expect(lireMock).toHaveBeenCalledWith(HASH, 'alice-pk');
    expect(screen.queryByText('A')).not.toBeInTheDocument();
  });

  it('garde les initiales pendant le chargement', () => {
    lireMock.mockReturnValueOnce(new Promise(() => {}));
    const { container } = render(<Avatar id="aabbcc" name="Alice" avatarHash={HASH} />);

    expect(screen.getByText('A')).toBeInTheDocument();
    expect(container.querySelector('img')).not.toBeInTheDocument();
  });

  it('retombe sur les initiales quand la lecture échoue', async () => {
    lireMock.mockRejectedValueOnce(new Error('introuvable'));
    const { container } = render(<Avatar id="aabbcc" name="Alice" avatarHash={HASH} />);

    await waitFor(() => expect(lireMock).toHaveBeenCalled());
    expect(screen.getByText('A')).toBeInTheDocument();
    expect(container.querySelector('img')).not.toBeInTheDocument();
  });

  it('recharge l’image quand le hash change', async () => {
    lireMock.mockResolvedValue('blob:v1');
    const { rerender } = render(<Avatar id="x" name="Alice" avatarHash={HASH} />);
    await waitFor(() => expect(lireMock).toHaveBeenCalledTimes(1));

    rerender(<Avatar id="x" name="Alice" avatarHash={'cd'.repeat(32)} />);

    await waitFor(() => expect(lireMock).toHaveBeenCalledTimes(2));
    expect(lireMock).toHaveBeenLastCalledWith('cd'.repeat(32), undefined);
  });

  it('rend une décoration connue dans un calque SVG dimensionné par le cadre', async () => {
    const { container } = render(
      <Avatar id="x" name="Alice" decoration="golden_laurel" />,
    );

    expect(await screen.findByTestId('avatar-decoration')).toBeInTheDocument();
    expect(container.querySelector('svg')).toHaveClass('avatar-decoration__svg');
  });

  it("ignore entièrement un id de pair inconnu sans l'injecter dans le DOM", () => {
    const hostile = '"><style>body{display:none}</style>';
    const { container } = render(<Avatar id="x" name="Alice" decoration={hostile} />);

    expect(screen.queryByTestId('avatar-decoration')).not.toBeInTheDocument();
    expect(container.innerHTML).not.toContain(hostile);
  });

  it("affiche l'aperçu local sans relire le hash persistant", () => {
    const { container } = render(
      <Avatar
        id="x"
        name="Alice"
        avatarHash={HASH}
        imageUrl="data:image/png;base64,AA=="
      />,
    );

    expect(container.querySelector('img')).toHaveAttribute(
      'src',
      'data:image/png;base64,AA==',
    );
    expect(lireMock).not.toHaveBeenCalled();
  });

  it("retombe sur les initiales si l'image ne peut pas être décodée", () => {
    const { container, rerender } = render(
      <Avatar id="x" name="Alice" imageUrl="blob:invalide" />,
    );

    fireEvent.error(container.querySelector('img') as HTMLImageElement);

    expect(screen.getByText('A')).toBeInTheDocument();
    expect(container.querySelector('img')).not.toBeInTheDocument();

    rerender(<Avatar id="x" name="Alice" imageUrl="blob:valide" />);

    expect(container.querySelector('img')).toHaveAttribute('src', 'blob:valide');
  });
});
