/**
 * Tests du menu contextuel générique : rendu des items fournis par le
 * store, déclenchement de `onClick` (et fermeture), fermeture à Échap.
 */

import { afterEach, describe, expect, it, vi } from 'vitest';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { useContextMenu, type ContextMenuItem } from '../stores/contextMenu';
import { ContextMenu, placeContextMenu } from './ContextMenu';

function openWith(items: ContextMenuItem[]): void {
  useContextMenu.getState().openMenu(10, 20, items);
}

afterEach(() => {
  act(() => useContextMenu.setState({ menu: null }));
});

describe('ContextMenu', () => {
  it('place un menu ancré au-dessus avec un espace constant', () => {
    const position = placeContextMenu({
      x: 343,
      y: 646,
      width: 210,
      height: 86,
      viewportWidth: 1280,
      viewportHeight: 720,
      anchor: { left: 343, top: 646, right: 379, bottom: 682 },
      preferredSide: 'top',
      gap: 8,
    });

    expect(position).toEqual({
      left: 343,
      top: 552,
      side: 'top',
      maxHeight: null,
    });
    expect(position.top + 86).toBe(638);
  });

  it('bascule sous l’ancre quand le dessus manque de place', () => {
    expect(
      placeContextMenu({
        x: 20,
        y: 30,
        width: 210,
        height: 86,
        viewportWidth: 1280,
        viewportHeight: 720,
        anchor: { left: 20, top: 30, right: 56, bottom: 66 },
        preferredSide: 'top',
        gap: 8,
      }),
    ).toEqual({ left: 20, top: 74, side: 'bottom', maxHeight: null });
  });

  it('conserve le placement ponctuel borné pour les menus contextuels', () => {
    expect(
      placeContextMenu({
        x: 1200,
        y: 700,
        width: 210,
        height: 86,
        viewportWidth: 1280,
        viewportHeight: 720,
      }),
    ).toEqual({ left: 1062, top: 626, side: 'point', maxHeight: null });
  });

  it('limite la hauteur sans recouvrir l’ancre quand aucun côté ne suffit', () => {
    expect(
      placeContextMenu({
        x: 20,
        y: 80,
        width: 210,
        height: 120,
        viewportWidth: 1280,
        viewportHeight: 200,
        anchor: { left: 20, top: 80, right: 56, bottom: 116 },
        preferredSide: 'top',
        gap: 8,
      }),
    ).toEqual({ left: 20, top: 124, side: 'bottom', maxHeight: 68 });
  });

  it('adapte l’origine de l’animation au placement supérieur', async () => {
    useContextMenu
      .getState()
      .openMenu(343, 646, [{ label: 'Joindre', onClick: vi.fn() }], {
        anchor: { left: 343, top: 646, right: 379, bottom: 682 },
        preferredSide: 'top',
        gap: 8,
      });

    render(<ContextMenu />);

    expect(screen.getByRole('menu')).toHaveClass('origin-bottom-left');
    await waitFor(() =>
      expect(screen.getByRole('menuitem', { name: 'Joindre' })).toHaveFocus(),
    );
  });

  it('ne rend rien tant qu’aucun menu n’est ouvert', () => {
    render(<ContextMenu />);
    expect(screen.queryByRole('menu')).toBeNull();
  });

  it('rend les items fournis par le store, avec le style destructif sur `danger`', async () => {
    openWith([
      { label: 'Copier le texte', onClick: vi.fn() },
      { label: 'Supprimer', onClick: vi.fn(), danger: true },
    ]);
    render(<ContextMenu />);

    expect(screen.getByRole('menu')).toBeInTheDocument();
    expect(screen.getByRole('menuitem', { name: 'Copier le texte' })).toHaveClass(
      'focus-visible:ring-2',
      'focus-visible:ring-inset',
      'focus-visible:ring-blurple',
    );
    expect(screen.getByRole('menuitem', { name: 'Supprimer' })).toHaveClass('text-red');
    await waitFor(() =>
      expect(screen.getByRole('menuitem', { name: 'Copier le texte' })).toHaveFocus(),
    );
  });

  it('déclenche `onClick` de l’item cliqué puis referme le menu', () => {
    const onClick = vi.fn();
    openWith([
      { label: 'Copier', onClick },
      { label: 'Autre', onClick: vi.fn() },
    ]);
    render(<ContextMenu />);

    fireEvent.click(screen.getByRole('menuitem', { name: 'Copier' }));

    expect(onClick).toHaveBeenCalledTimes(1);
    expect(useContextMenu.getState().menu).toBeNull();
  });

  it('se ferme à Échap sans déclencher d’item', () => {
    const onClick = vi.fn();
    openWith([{ label: 'Copier', onClick }]);
    render(<ContextMenu />);
    expect(screen.getByRole('menu')).toBeInTheDocument();

    fireEvent.keyDown(screen.getByRole('menu'), { key: 'Escape' });

    expect(onClick).not.toHaveBeenCalled();
    expect(useContextMenu.getState().menu).toBeNull();
  });

  it('rend le focus au déclencheur après fermeture à Échap', async () => {
    render(
      <>
        <button type="button">Déclencheur</button>
        <ContextMenu />
      </>,
    );
    const declencheur = screen.getByRole('button', { name: 'Déclencheur' });
    declencheur.focus();

    act(() => openWith([{ label: 'Copier', onClick: vi.fn() }]));
    await waitFor(() =>
      expect(screen.getByRole('menuitem', { name: 'Copier' })).toHaveFocus(),
    );

    fireEvent.keyDown(screen.getByRole('menu'), { key: 'Escape' });

    expect(declencheur).toHaveFocus();
  });

  it('se ferme sur Tab (convention menu) au lieu de laisser fuir le focus', () => {
    openWith([{ label: 'Copier', onClick: vi.fn() }]);
    render(<ContextMenu />);

    fireEvent.keyDown(screen.getByRole('menu'), { key: 'Tab' });

    expect(useContextMenu.getState().menu).toBeNull();
  });

  it('ferme le menu quand le viewport change', () => {
    openWith([{ label: 'Copier', onClick: vi.fn() }]);
    render(<ContextMenu />);

    fireEvent(window, new Event('resize'));

    expect(useContextMenu.getState().menu).toBeNull();
  });

  it('focalise le premier item, navigue avec les flèches et l’active', async () => {
    const onClick = vi.fn();
    openWith([
      { label: 'Premier', onClick },
      { label: 'Deuxième', onClick: vi.fn() },
      { label: 'Dernier', onClick: vi.fn() },
    ]);
    render(<ContextMenu />);

    const first = screen.getByRole('menuitem', { name: 'Premier' });
    const last = screen.getByRole('menuitem', { name: 'Dernier' });
    await waitFor(() => expect(first).toHaveFocus());

    fireEvent.keyDown(first, { key: 'ArrowUp' });
    expect(last).toHaveFocus();

    fireEvent.keyDown(last, { key: 'ArrowDown' });
    expect(first).toHaveFocus();

    fireEvent.keyDown(first, { key: 'Enter' });
    expect(onClick).toHaveBeenCalledTimes(1);
    expect(useContextMenu.getState().menu).toBeNull();
  });
});
