/**
 * Tests des pastilles de non-lus : la variante simple (compte rouge) et la
 * variante « mention » (« @ » + compte), toutes deux masquées sans compteur.
 */

import { beforeEach, describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/react';
import { useUi } from '../stores/ui';
import { MentionBadge, UnreadBadge } from './UnreadBadge';

beforeEach(() => {
  useUi.setState({ lang: 'fr' });
});

describe('UnreadBadge', () => {
  it('affiche le compte de non-lus', () => {
    render(<UnreadBadge count={4} />);
    expect(screen.getByText('4')).toBeInTheDocument();
  });

  it('ne rend rien sans non-lu', () => {
    const { container } = render(<UnreadBadge count={0} />);
    expect(container).toBeEmptyDOMElement();
  });

  it('plafonne l’affichage à « 99+ » mais garde le compte exact en accessibilité', () => {
    render(<UnreadBadge count={150} />);
    expect(screen.getByText('99+')).toBeInTheDocument();
    expect(screen.getByLabelText('150 message(s) non lu(s)')).toBeInTheDocument();
  });
});

describe('MentionBadge', () => {
  it('affiche « @ » et le compte de mentions', () => {
    render(<MentionBadge count={2} />);
    const badge = screen.getByLabelText('2 mention(s) non lue(s)');
    expect(badge).toHaveTextContent('@2');
  });

  it('ne rend rien sans mention', () => {
    const { container } = render(<MentionBadge count={0} />);
    expect(container).toBeEmptyDOMElement();
  });
});
