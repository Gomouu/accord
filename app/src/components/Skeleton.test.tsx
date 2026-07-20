/** Tests des squelettes de chargement : bloc pulsant et faux fil de messages. */

import { describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/react';
import { MessageListSkeleton, Skeleton } from './Skeleton';

describe('Skeleton', () => {
  it('rend un bloc pulsant décoratif (hors de l’arbre d’accessibilité)', () => {
    const { container } = render(<Skeleton className="h-4 w-10" />);
    const bloc = container.firstElementChild;
    expect(bloc).toHaveClass('animate-pulse');
    expect(bloc).toHaveAttribute('aria-hidden');
  });
});

describe('MessageListSkeleton', () => {
  it('expose une région de statut étiquetée pour le lecteur d’écran', () => {
    render(<MessageListSkeleton rows={3} label="Chargement…" />);
    const region = screen.getByRole('status');
    expect(region).toHaveAttribute('aria-label', 'Chargement…');
    expect(region).toHaveAttribute('aria-busy');
  });

  it('rend autant de fausses lignes que demandé', () => {
    const { container } = render(<MessageListSkeleton rows={4} />);
    const avatars = container.querySelectorAll('.rounded-full');
    expect(avatars).toHaveLength(4);
  });
});
