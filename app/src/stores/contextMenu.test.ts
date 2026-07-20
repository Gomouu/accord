/**
 * Tests du store de menu contextuel : ouverture/fermeture et détection des
 * cibles éditables (les raccourcis clavier globaux doivent s'effacer quand le
 * focus est dans un champ de saisie).
 */

import { afterEach, describe, expect, it } from 'vitest';
import { isEditableTarget, useContextMenu } from './contextMenu';

afterEach(() => {
  document.body.innerHTML = '';
  useContextMenu.setState({ menu: null });
});

describe('useContextMenu', () => {
  it('ouvre le menu aux coordonnées données puis le ferme', () => {
    useContextMenu.getState().openMenu(10, 20, [{ label: 'a', onClick: () => {} }]);
    const menu = useContextMenu.getState().menu;
    expect(menu?.x).toBe(10);
    expect(menu?.y).toBe(20);
    expect(menu?.items).toHaveLength(1);

    useContextMenu.getState().closeMenu();
    expect(useContextMenu.getState().menu).toBeNull();
  });
});

describe('isEditableTarget', () => {
  it('reconnaît un champ de saisie (input/textarea/contenteditable)', () => {
    document.body.innerHTML =
      '<input id="i" /><textarea id="t"></textarea><div contenteditable="true" id="c"><span id="s">x</span></div>';
    expect(isEditableTarget(document.getElementById('i'))).toBe(true);
    expect(isEditableTarget(document.getElementById('t'))).toBe(true);
    // Un descendant d'une zone éditable compte aussi (closest remonte l'arbre).
    expect(isEditableTarget(document.getElementById('s'))).toBe(true);
  });

  it('rejette un élément non éditable et une cible nulle', () => {
    document.body.innerHTML = '<button id="b">x</button>';
    expect(isEditableTarget(document.getElementById('b'))).toBe(false);
    expect(isEditableTarget(null)).toBe(false);
  });
});
