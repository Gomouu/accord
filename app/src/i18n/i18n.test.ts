/** Tests i18n : interpolation et parité des clés fr/en/es. */

import { describe, expect, it } from 'vitest';
import { LANGS, direction, interpolate } from './index';
import { dictionaries } from './all';
import { fr } from './fr';
import { en } from './en';
import { es } from './es';

describe('interpolate', () => {
  it('remplace les variables {nom} par leur valeur', () => {
    expect(interpolate('Écrire à @{name}', { name: 'Alice' })).toBe('Écrire à @Alice');
  });

  it('remplace plusieurs occurrences et variables', () => {
    expect(interpolate('{a} et {b} et {a}', { a: '1', b: '2' })).toBe('1 et 2 et 1');
  });

  it('laisse le marqueur visible quand la variable manque', () => {
    expect(interpolate('Bonjour {name}', {})).toBe('Bonjour {name}');
  });

  it('rend le libellé inchangé sans marqueur', () => {
    expect(interpolate('Bonjour', { name: 'x' })).toBe('Bonjour');
  });
});

/** Chemins feuilles (`section.cle`) d'un dictionnaire, récursivement. */
function keyPaths(node: Record<string, unknown>, prefix = ''): string[] {
  return Object.entries(node).flatMap(([key, value]) =>
    typeof value === 'string'
      ? [`${prefix}${key}`]
      : keyPaths(value as Record<string, unknown>, `${prefix}${key}.`),
  );
}

describe('parité des dictionnaires', () => {
  const frPaths = keyPaths(fr).sort();
  const enPaths = keyPaths(en).sort();
  const esPaths = keyPaths(es).sort();

  it('expose exactement les langues déclarées dans LANGS', () => {
    // Le test nommait « les trois langues » en dur : chaque langue ajoutée le
    // faisait échouer pour une raison sans intérêt. Il compare maintenant les
    // deux sources qui doivent rester d'accord — la liste déclarée et les
    // dictionnaires réellement fournis.
    expect(Object.keys(dictionaries).sort()).toEqual([...LANGS].sort());
  });

  it('en.ts couvre exactement les clés de fr.ts (référence)', () => {
    // Échoue en nommant les clés manquantes ou en trop.
    expect(enPaths).toEqual(frPaths);
  });

  it('es.ts couvre exactement les clés de fr.ts (référence)', () => {
    expect(esPaths).toEqual(frPaths);
  });

  it('aucune traduction n’est vide', () => {
    for (const dict of [fr, en, es]) {
      for (const path of keyPaths(dict)) {
        const leaf = path
          .split('.')
          .reduce<unknown>((node, key) => (node as Record<string, unknown>)[key], dict);
        expect(leaf, `clé vide : ${path}`).not.toBe('');
      }
    }
  });
});

describe('direction', () => {
  it('rend « rtl » pour l’arabe et « ltr » pour toutes les autres', () => {
    expect(direction('ar')).toBe('rtl');
    for (const lang of LANGS.filter((l) => l !== 'ar')) {
      expect(direction(lang)).toBe('ltr');
    }
  });
});
