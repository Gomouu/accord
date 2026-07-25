/**
 * Internationalisation typée : le dictionnaire français est la référence de
 * forme, les autres langues doivent s'y conformer à la compilation. Accès par objet
 * (`t.friends.title`) — aucune clé magique en chaîne.
 *
 * Seul le français est dans le chargement initial ; les autres langues sont des
 * chunks séparés, chargés à la demande. Un dictionnaire pèse ~60 ko : les
 * embarquer tous ferait payer à chaque utilisateur les langues qu'il ne lira
 * jamais, et la facture croîtrait à chaque langue ajoutée.
 */

import { fr } from './fr';

/**
 * Élargit récursivement les littéraux (figés par `as const` dans fr.ts) en
 * `string`, pour que la forme du dictionnaire serve de référence sans imposer
 * les textes français aux autres langues.
 */
type Widen<T> = { [K in keyof T]: T[K] extends string ? string : Widen<T[K]> };

export type Dict = Widen<typeof fr>;
export type Lang = 'fr' | 'en' | 'es' | 'pt' | 'de';

/** Langues proposées, dans l'ordre d'affichage. */
export const LANGS: readonly Lang[] = ['fr', 'en', 'es', 'pt', 'de'];

export { fr };

type LazyLang = Exclude<Lang, 'fr'>;

const LOADERS: Record<LazyLang, () => Promise<Record<string, unknown>>> = {
  en: () => import('./en'),
  es: () => import('./es'),
  pt: () => import('./pt'),
  de: () => import('./de'),
};

/** Dictionnaires déjà résolus. Le français y est d'emblée. */
const resolved = new Map<Lang, Dict>([['fr', fr as Dict]]);

/**
 * Dictionnaire d'une langue **si déjà chargé**, sinon le français.
 *
 * Repli délibéré plutôt qu'attente : un appelant synchrone — un toast émis
 * depuis un store, par exemple — doit rendre un texte tout de suite. En
 * pratique la langue active est chargée avant le premier rendu, voir
 * [`loadDictionary`] et l'amorçage de `main.tsx`.
 */
export function dictionary(lang: Lang): Dict {
  return resolved.get(lang) ?? (fr as Dict);
}

/** Vrai si le dictionnaire de cette langue est déjà en mémoire. */
export function dictionaryLoaded(lang: Lang): boolean {
  return resolved.has(lang);
}

/** Charge (une seule fois) le dictionnaire d'une langue et le rend. */
export async function loadDictionary(lang: Lang): Promise<Dict> {
  const known = resolved.get(lang);
  if (known !== undefined) return known;
  const loaded = await LOADERS[lang as LazyLang]();
  const dict = loaded[lang] as Dict;
  registerDictionary(lang, dict);
  return dict;
}

/**
 * Déclare un dictionnaire déjà en mémoire comme résolu. Le chargement normal
 * passe par [`loadDictionary`] ; ce point d'entrée sert aux tests, qui montent
 * toutes les langues d'un bloc pour que basculer `lang` suffise.
 */
export function registerDictionary(lang: Lang, dict: Dict): void {
  resolved.set(lang, dict);
}

/** Interpole `{name}`-style placeholders dans un libellé. */
export function interpolate(label: string, vars: Record<string, string>): string {
  return label.replace(/\{(\w+)\}/g, (_, key: string) => vars[key] ?? `{${key}}`);
}

/**
 * Libellé traduit d'une décoration de profil, indexé par son identifiant de
 * registre. Se replie sur l'identifiant si la traduction manque : une
 * décoration ajoutée sans son libellé reste choisissable, et le test de parité
 * signale l'oubli.
 */
export function decorationLabel(dict: Dict, id: string): string {
  const labels: Record<string, string> = dict.decorations.labels;
  return labels[id] ?? id;
}
