/**
 * Tests de la pastille de présence : couleur par statut, accessibilité
 * (libellé optionnel) et glyphes distincts (lune, barre, anneau).
 */

import { describe, expect, it } from 'vitest';
import { render } from '@testing-library/react';
import { PresenceDot } from './PresenceDot';

describe('PresenceDot', () => {
  it('colore chaque statut différemment', () => {
    const cases = [
      ['online', 'text-green'],
      ['idle', 'text-yellow'],
      ['dnd', 'text-red'],
      ['offline', 'text-faint'],
    ] as const;
    for (const [status, className] of cases) {
      const { container, unmount } = render(<PresenceDot status={status} />);
      const dot = container.querySelector(`[data-status="${status}"]`);
      expect(dot).not.toBeNull();
      expect(dot).toHaveClass(className);
      unmount();
    }
  });

  it('est décorative sans libellé, image nommée avec', () => {
    const decorative = render(<PresenceDot status="online" />);
    expect(
      decorative.container.querySelector('[aria-hidden="true"]'),
    ).not.toBeNull();
    decorative.unmount();

    const labelled = render(<PresenceDot status="dnd" label="Ne pas déranger" />);
    expect(labelled.getByRole('img', { name: 'Ne pas déranger' })).toBeInTheDocument();
  });

  it('découpe un glyphe (masque) pour inactif, occupé et hors ligne, pas en ligne', () => {
    for (const status of ['idle', 'dnd', 'offline'] as const) {
      const { container, unmount } = render(<PresenceDot status={status} />);
      expect(container.querySelector('mask')).not.toBeNull();
      unmount();
    }
    const { container } = render(<PresenceDot status="online" />);
    expect(container.querySelector('mask')).toBeNull();
  });
});
