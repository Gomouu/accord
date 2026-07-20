/**
 * Tests des utilitaires de focus (accessibilité) : énumération des focusables,
 * piège à focus Tab/Maj-Tab, ouverture de menu au clavier, ancrage et
 * navigation verticale aux flèches.
 */

import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  bouclerTab,
  deplacerFocusVertical,
  estOuvertureMenu,
  focusables,
  pointAncrageMenu,
} from './focus';

function monter(html: string): HTMLElement {
  const root = document.createElement('div');
  root.innerHTML = html;
  document.body.appendChild(root);
  return root;
}

afterEach(() => {
  document.body.innerHTML = '';
});

const evt = (key: string, shiftKey = false) => ({
  key,
  shiftKey,
  preventDefault: vi.fn(),
});

describe('focusables', () => {
  it('liste les éléments focusables et ignore les désactivés', () => {
    const root = monter(
      '<button>a</button><button disabled>b</button><input /><a href="#">l</a><span tabindex="-1">x</span>',
    );
    const noms = focusables(root).map((el) => el.tagName.toLowerCase());
    expect(noms).toEqual(['button', 'input', 'a']);
  });

  it('rend une liste vide pour une racine nulle', () => {
    expect(focusables(null)).toEqual([]);
  });
});

describe('estOuvertureMenu', () => {
  it('reconnaît la touche Menu et Maj+F10', () => {
    expect(estOuvertureMenu({ key: 'ContextMenu', shiftKey: false })).toBe(true);
    expect(estOuvertureMenu({ key: 'F10', shiftKey: true })).toBe(true);
  });

  it('rejette F10 sans Maj et les autres touches', () => {
    expect(estOuvertureMenu({ key: 'F10', shiftKey: false })).toBe(false);
    expect(estOuvertureMenu({ key: 'Enter', shiftKey: false })).toBe(false);
  });
});

describe('pointAncrageMenu', () => {
  it('rend le centre de l’élément', () => {
    const el = document.createElement('button');
    el.getBoundingClientRect = () =>
      ({ left: 10, top: 20, width: 40, height: 10 }) as DOMRect;
    expect(pointAncrageMenu(el)).toEqual({ x: 30, y: 25 });
  });
});

describe('bouclerTab', () => {
  it('reboucle du dernier au premier avec Tab', () => {
    const root = monter('<button id="a">a</button><button id="b">b</button>');
    (root.querySelector('#b') as HTMLElement).focus();
    const e = evt('Tab');
    bouclerTab(e, root);
    expect(e.preventDefault).toHaveBeenCalled();
    expect(document.activeElement?.id).toBe('a');
  });

  it('reboucle du premier au dernier avec Maj+Tab', () => {
    const root = monter('<button id="a">a</button><button id="b">b</button>');
    (root.querySelector('#a') as HTMLElement).focus();
    bouclerTab(evt('Tab', true), root);
    expect(document.activeElement?.id).toBe('b');
  });

  it('ignore une touche autre que Tab', () => {
    const root = monter('<button id="a">a</button>');
    const e = evt('Enter');
    bouclerTab(e, root);
    expect(e.preventDefault).not.toHaveBeenCalled();
  });
});

describe('deplacerFocusVertical', () => {
  it('descend puis remonte, borné aux extrémités', () => {
    const root = monter(
      '<button id="a">a</button><button id="b">b</button><button id="c">c</button>',
    );
    (root.querySelector('#a') as HTMLElement).focus();
    deplacerFocusVertical(evt('ArrowDown'), root);
    expect(document.activeElement?.id).toBe('b');
    deplacerFocusVertical(evt('ArrowUp'), root);
    expect(document.activeElement?.id).toBe('a');
    deplacerFocusVertical(evt('ArrowUp'), root);
    expect(document.activeElement?.id).toBe('a');
  });
});
