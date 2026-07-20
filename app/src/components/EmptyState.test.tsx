/** Tests de l'état vide réutilisable : libellé, description, action facultative. */

import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { EmptyState } from './EmptyState';

describe('EmptyState', () => {
  it('affiche le libellé et rien de plus par défaut', () => {
    render(<EmptyState icon={<i data-testid="ic" />} label="Aucun ami" />);
    expect(screen.getByText('Aucun ami')).toBeInTheDocument();
    expect(screen.queryByRole('button')).toBeNull();
  });

  it('affiche la description quand elle est fournie', () => {
    render(
      <EmptyState
        icon={<i />}
        label="Aucun résultat"
        description="Essaie un autre terme"
      />,
    );
    expect(screen.getByText('Essaie un autre terme')).toBeInTheDocument();
  });

  it('rend un bouton d’action qui déclenche le rappel', async () => {
    const onClick = vi.fn();
    render(
      <EmptyState
        icon={<i />}
        label="Aucun ami"
        action={{ label: 'Ajouter', onClick }}
      />,
    );
    await userEvent.click(screen.getByRole('button', { name: 'Ajouter' }));
    expect(onClick).toHaveBeenCalledOnce();
  });
});
