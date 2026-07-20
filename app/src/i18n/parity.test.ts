/**
 * Garde-fou d'internationalisation : les dictionnaires FR et EN doivent avoir
 * exactement les mêmes clés, et chaque chaîne traduite doit porter les mêmes
 * marqueurs d'interpolation `{...}`. Le typage (`en: Dict`) attrape déjà une
 * clé manquante, mais pas un placeholder oublié — `{count}` présent en FR et
 * absent en EN casse silencieusement l'interpolation à l'exécution.
 */

import { describe, expect, it } from 'vitest';
import { fr } from './fr';
import { en } from './en';

type Leaf = string;
type Tree = { [key: string]: Leaf | Tree };

/** Aplati l'arbre de traductions en `chemin.pointé` → chaîne. */
function flatten(tree: Tree, prefix = ''): Record<string, string> {
  const out: Record<string, string> = {};
  for (const [key, value] of Object.entries(tree)) {
    const path = prefix === '' ? key : `${prefix}.${key}`;
    if (typeof value === 'string') out[path] = value;
    else Object.assign(out, flatten(value, path));
  }
  return out;
}

/** Ensemble trié des marqueurs `{nom}` d'une chaîne. */
function placeholders(text: string): string[] {
  return [...text.matchAll(/\{(\w+)\}/g)].map((m) => m[1] ?? '').sort();
}

const flatFr = flatten(fr as Tree);
const flatEn = flatten(en as Tree);

describe('parité i18n FR/EN', () => {
  it('a exactement le même ensemble de clés', () => {
    const keysFr = Object.keys(flatFr).sort();
    const keysEn = Object.keys(flatEn).sort();
    expect(keysEn).toEqual(keysFr);
  });

  it('n’a aucune valeur vide dans l’une ou l’autre langue', () => {
    const vides: string[] = [];
    for (const [key, value] of Object.entries(flatFr))
      if (value.trim() === '') vides.push(`fr.${key}`);
    for (const [key, value] of Object.entries(flatEn))
      if (value.trim() === '') vides.push(`en.${key}`);
    expect(vides).toEqual([]);
  });

  it('a les mêmes marqueurs d’interpolation pour chaque clé', () => {
    const divergences: string[] = [];
    for (const [key, textFr] of Object.entries(flatFr)) {
      const textEn = flatEn[key];
      if (textEn === undefined) continue;
      const a = placeholders(textFr);
      const b = placeholders(textEn);
      if (a.join(',') !== b.join(',')) {
        divergences.push(`${key} : fr={${a.join(',')}} en={${b.join(',')}}`);
      }
    }
    expect(divergences).toEqual([]);
  });
});
