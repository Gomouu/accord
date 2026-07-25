/**
 * Garde-fou d'internationalisation : chaque dictionnaire traduit doit avoir
 * exactement les mêmes clés que la référence française, et chaque chaîne doit
 * porter les mêmes marqueurs d'interpolation `{...}`. Le typage (`en: Dict`)
 * attrape déjà une clé manquante, mais pas un placeholder oublié — `{count}`
 * présent en FR et absent ailleurs casse silencieusement l'interpolation à
 * l'exécution.
 *
 * Les deux moitiés de chaque langue sont confrontées séparément (noyau et
 * extension de réglages) : une clé qui se serait égarée d'une moitié à l'autre
 * dans une seule langue passerait inaperçue si on les fusionnait avant de
 * comparer.
 */

import { describe, expect, it } from 'vitest';
import { type Lang } from './index';
import { dictionaries, settingsDictionaries } from './all';
import { fr } from './fr';
import { frSettings } from './fr.settings';

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

/** Toutes les langues sauf la référence : ce sont elles qu'on confronte au FR. */
const traductions = (Object.keys(dictionaries) as Lang[]).filter((lang) => lang !== 'fr');

/** Les deux moitiés d'une langue, chacune avec sa référence française. */
const moities = [
  {
    nom: 'noyau',
    reference: flatten(fr as Tree),
    de: (lang: Lang) => dictionaries[lang],
  },
  {
    nom: 'réglages',
    reference: flatten(frSettings as Tree),
    de: (lang: Lang) => settingsDictionaries[lang],
  },
] as const;

describe.each(
  moities.flatMap((moitie) => traductions.map((lang) => ({ ...moitie, lang }))),
)('parité i18n FR/$lang ($nom)', ({ reference: flatFr, de, lang }) => {
  const flatTrad = flatten(de(lang) as unknown as Tree);

  it('a exactement le même ensemble de clés', () => {
    expect(Object.keys(flatTrad).sort()).toEqual(Object.keys(flatFr).sort());
  });

  it('n’a aucune valeur vide dans l’une ou l’autre langue', () => {
    const vides: string[] = [];
    for (const [key, value] of Object.entries(flatFr))
      if (value.trim() === '') vides.push(`fr.${key}`);
    for (const [key, value] of Object.entries(flatTrad))
      if (value.trim() === '') vides.push(`${lang}.${key}`);
    expect(vides).toEqual([]);
  });

  it('a les mêmes marqueurs d’interpolation pour chaque clé', () => {
    const divergences: string[] = [];
    for (const [key, textFr] of Object.entries(flatFr)) {
      const traduit = flatTrad[key];
      if (traduit === undefined) continue;
      const a = placeholders(textFr);
      const b = placeholders(traduit);
      if (a.join(',') !== b.join(',')) {
        divergences.push(`${key} : fr={${a.join(',')}} ${lang}={${b.join(',')}}`);
      }
    }
    expect(divergences).toEqual([]);
  });
});
