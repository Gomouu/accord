/**
 * Tests de la poignée de redimensionnement : ajustement au clavier (flèches,
 * Origine/Fin), réinitialisation au double-clic, et exposition ARIA (rôle
 * séparateur, orientation, bornes). Le glissé pointeur (setPointerCapture)
 * n'est pas simulable de façon fiable sous jsdom — voir `ResizeHandle.tsx`
 * qui appelle ces API en chaînage optionnel pour cette raison ; il est donc
 * couvert indirectement (bornage partagé avec le clavier) plutôt que testé
 * en direct ici.
 */

import { describe, expect, it, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { ResizeHandle } from './ResizeHandle';

const LABEL = 'Redimensionner la barre latérale';

/** Rend la poignée sous un sens d'écriture donné, puis le rétablit. */
function avecSens(dir: 'ltr' | 'rtl', corps: () => void): void {
  const precedent = document.documentElement.dir;
  document.documentElement.dir = dir;
  try {
    corps();
  } finally {
    document.documentElement.dir = precedent;
  }
}

describe('ResizeHandle', () => {
  it('expose un séparateur vertical avec les bornes ARIA courantes', () => {
    render(
      <ResizeHandle
        value={260}
        min={200}
        max={420}
        defaultValue={240}
        onChange={vi.fn()}
        ariaLabel={LABEL}
        panelSide="start"
      />,
    );

    const handle = screen.getByRole('separator', { name: LABEL });
    expect(handle).toHaveAttribute('aria-orientation', 'vertical');
    expect(handle).toHaveAttribute('aria-valuenow', '260');
    expect(handle).toHaveAttribute('aria-valuemin', '200');
    expect(handle).toHaveAttribute('aria-valuemax', '420');
    expect(handle).toHaveAttribute('tabindex', '0');
  });

  it('ArrowRight agrandit un panneau à gauche (+8px)', () => {
    const onChange = vi.fn();
    render(
      <ResizeHandle
        value={260}
        min={200}
        max={420}
        defaultValue={240}
        onChange={onChange}
        ariaLabel={LABEL}
        panelSide="start"
      />,
    );

    fireEvent.keyDown(screen.getByRole('separator', { name: LABEL }), {
      key: 'ArrowRight',
    });

    expect(onChange).toHaveBeenCalledWith(268);
  });

  it('ArrowLeft réduit un panneau à gauche (-8px)', () => {
    const onChange = vi.fn();
    render(
      <ResizeHandle
        value={260}
        min={200}
        max={420}
        defaultValue={240}
        onChange={onChange}
        ariaLabel={LABEL}
        panelSide="start"
      />,
    );

    fireEvent.keyDown(screen.getByRole('separator', { name: LABEL }), {
      key: 'ArrowLeft',
    });

    expect(onChange).toHaveBeenCalledWith(252);
  });

  it('inverse le sens pour un panneau à droite (ArrowRight le réduit)', () => {
    const onChange = vi.fn();
    render(
      <ResizeHandle
        value={260}
        min={180}
        max={380}
        defaultValue={240}
        onChange={onChange}
        ariaLabel="Redimensionner la liste des membres"
        panelSide="end"
      />,
    );

    fireEvent.keyDown(
      screen.getByRole('separator', { name: 'Redimensionner la liste des membres' }),
      { key: 'ArrowRight' },
    );

    expect(onChange).toHaveBeenCalledWith(252);
  });

  it('borne au clavier sans dépasser le minimum ou le maximum', () => {
    const onChangeAtMin = vi.fn();
    const { rerender } = render(
      <ResizeHandle
        value={202}
        min={200}
        max={420}
        defaultValue={240}
        onChange={onChangeAtMin}
        ariaLabel={LABEL}
        panelSide="start"
      />,
    );
    fireEvent.keyDown(screen.getByRole('separator', { name: LABEL }), {
      key: 'ArrowLeft',
    });
    expect(onChangeAtMin).toHaveBeenCalledWith(200);

    const onChangeAtMax = vi.fn();
    rerender(
      <ResizeHandle
        value={415}
        min={200}
        max={420}
        defaultValue={240}
        onChange={onChangeAtMax}
        ariaLabel={LABEL}
        panelSide="start"
      />,
    );
    fireEvent.keyDown(screen.getByRole('separator', { name: LABEL }), {
      key: 'ArrowRight',
    });
    expect(onChangeAtMax).toHaveBeenCalledWith(420);
  });

  it('Home et End sautent directement aux bornes', () => {
    const onChange = vi.fn();
    render(
      <ResizeHandle
        value={300}
        min={200}
        max={420}
        defaultValue={240}
        onChange={onChange}
        ariaLabel={LABEL}
        panelSide="start"
      />,
    );
    const handle = screen.getByRole('separator', { name: LABEL });

    fireEvent.keyDown(handle, { key: 'Home' });
    expect(onChange).toHaveBeenLastCalledWith(200);

    fireEvent.keyDown(handle, { key: 'End' });
    expect(onChange).toHaveBeenLastCalledWith(420);
  });

  it('le double-clic restaure la largeur par défaut', () => {
    const onChange = vi.fn();
    render(
      <ResizeHandle
        value={340}
        min={200}
        max={420}
        defaultValue={240}
        onChange={onChange}
        ariaLabel={LABEL}
        panelSide="start"
      />,
    );

    fireEvent.doubleClick(screen.getByRole('separator', { name: LABEL }));

    expect(onChange).toHaveBeenCalledWith(240);
  });
  // Les déplacements arrivent en pixels *écran* — flèches comme glissé — et
  // aucune propriété CSS ne peut les refléter : c'est au composant de tenir
  // compte du sens de lecture. En RTL la barre latérale est passée à droite,
  // donc pousser la poignée vers la droite entre dans le panneau et le réduit.
  it('inverse le sens du redimensionnement en écriture de droite à gauche', () => {
    const onChange = vi.fn();
    avecSens('rtl', () => {
      render(
        <ResizeHandle
          value={260}
          min={200}
          max={420}
          defaultValue={240}
          onChange={onChange}
          ariaLabel={LABEL}
          panelSide="start"
        />,
      );

      fireEvent.keyDown(screen.getByRole('separator', { name: LABEL }), {
        key: 'ArrowRight',
      });
    });

    expect(onChange).toHaveBeenCalledWith(252);
  });
});
