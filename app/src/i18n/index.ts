/**
 * Internationalisation typée : le dictionnaire français est la référence de
 * forme, les autres langues doivent s'y conformer à la compilation. Accès par objet
 * (`t.friends.title`) — aucune clé magique en chaîne.
 *
 * Seul le français est dans le chargement initial ; les autres langues sont des
 * chunks séparés, chargés à la demande. Un dictionnaire pèse ~60 ko : les
 * embarquer tous ferait payer à chaque utilisateur les langues qu'il ne lira
 * jamais, et la facture croîtrait à chaque langue ajoutée.
 *
 * Chaque langue est elle-même en deux morceaux, pour la même raison à
 * l'intérieur d'une langue : le **noyau** (`<lang>.ts`, type [`Dict`]) et
 * l'**extension réglages** (`<lang>.settings.ts`, type [`SettingsDict`]). Le
 * noyau français est le seul module de traduction du chargement initial ; son
 * extension ne descend qu'à l'ouverture de la modale de réglages, qui est déjà
 * un chunk paresseux. Voir [`loadSettingsDict`].
 */

import { fr } from './fr';
// 🔒 Import de **type** uniquement : `typeof frSettings` sert de référence de
// forme sans mettre une seule chaîne de réglages dans le chargement initial.
// Le passer en import de valeur annulerait tout le découpage, sans rien casser
// de visible — seul scripts/check-bundle-budget.mjs s'en apercevrait.
import type { frSettings } from './fr.settings';

/**
 * Élargit récursivement les littéraux (figés par `as const` dans fr.ts) en
 * `string`, pour que la forme du dictionnaire serve de référence sans imposer
 * les textes français aux autres langues.
 */
type Widen<T> = { [K in keyof T]: T[K] extends string ? string : Widen<T[K]> };

export type Dict = Widen<typeof fr>;

/**
 * Vocabulaire propre au panneau de réglages (`settings`, `decorations`).
 *
 * 🔒 Type distinct de [`Dict`], et c'est là toute la garantie : un composant du
 * socle qui n'a que `t: Dict` ne peut pas écrire `t.settings.autoLockOff`, donc
 * ne peut pas réintroduire ces chaînes dans le chargement initial sans que la
 * compilation le refuse. Le découpage ne peut pas se défaire en silence.
 *
 * Les quelques libellés de réglages que le socle affiche lui-même (l'entrée de
 * menu « Paramètres », la déconnexion, l'import de sauvegarde) vivent dans le
 * noyau, sous `app` et `onboarding`.
 */
export type SettingsDict = Widen<typeof frSettings>;

/**
 * Clés d'une section de dictionnaire dont la valeur est un texte.
 *
 * Une section peut contenir des sous-objets (`settings.languageNames`,
 * `decorations.labels`) ; un index dynamique typé `keyof` rendrait alors
 * `string | objet`, et l'appelant qui attend un libellé ne compilerait plus.
 * Ce type restreint l'index aux seules clés utilisables comme libellé.
 */
export type TextKey<T> = { [K in keyof T]: T[K] extends string ? K : never }[keyof T];

export type Lang = 'fr' | 'en' | 'es' | 'pt' | 'de' | 'ru' | 'zh' | 'hi' | 'bn' | 'ar';

/** Langues proposées, dans l'ordre d'affichage. */
export const LANGS: readonly Lang[] = [
  'fr',
  'en',
  'es',
  'pt',
  'de',
  'ru',
  'zh',
  'hi',
  'bn',
  'ar',
];

/** Sens d'écriture d'une langue. */
export type Direction = 'ltr' | 'rtl';

/**
 * Langues écrites de droite à gauche.
 *
 * Liste des exceptions plutôt que table complète : une langue ajoutée s'écrit
 * de gauche à droite sauf mention contraire, ce qui est vrai de la quasi
 * totalité d'entre elles. Oublier d'inscrire une langue LTR ici ne casse donc
 * rien, alors qu'une table exhaustive laisserait passer un trou silencieux.
 */
const RTL_LANGS: readonly Lang[] = ['ar'];

/** Sens d'écriture d'une langue — `'ltr'` par défaut. */
export function direction(lang: Lang): Direction {
  return RTL_LANGS.includes(lang) ? 'rtl' : 'ltr';
}

export { fr };

type LazyLang = Exclude<Lang, 'fr'>;

const LOADERS: Record<LazyLang, () => Promise<Record<string, unknown>>> = {
  en: () => import('./en'),
  es: () => import('./es'),
  pt: () => import('./pt'),
  de: () => import('./de'),
  ru: () => import('./ru'),
  zh: () => import('./zh'),
  hi: () => import('./hi'),
  bn: () => import('./bn'),
  ar: () => import('./ar'),
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

const SETTINGS_LOADERS: Record<Lang, () => Promise<Record<string, unknown>>> = {
  fr: () => import('./fr.settings'),
  en: () => import('./en.settings'),
  es: () => import('./es.settings'),
  pt: () => import('./pt.settings'),
  de: () => import('./de.settings'),
  ru: () => import('./ru.settings'),
  zh: () => import('./zh.settings'),
  hi: () => import('./hi.settings'),
  bn: () => import('./bn.settings'),
  ar: () => import('./ar.settings'),
};

/** Extensions de réglages déjà résolues. Le français n'y est pas d'emblée. */
const resolvedSettings = new Map<Lang, SettingsDict>();

/**
 * Chargements en cours, indexés par langue.
 *
 * 🔒 Une entrée par langue, et la *même* promesse rendue à chaque appel : c'est
 * ce qui permet à [`useSettingsT`] de la jeter à React sans relancer un
 * téléchargement à chaque rendu suspendu.
 */
const pendingSettings = new Map<Lang, Promise<SettingsDict>>();

/**
 * Dernier échec de chargement, par langue.
 *
 * 🔒 Sans lui, un chunk introuvable tourne en boucle et sans bruit. Jeter une
 * promesse à React n'est qu'un *signal de suspension* : le rejet réveille le
 * rendu exactement comme la résolution, sans jamais devenir une erreur qu'une
 * frontière puisse attraper. Le rendu suivant retrouvait donc l'extension
 * absente et relançait un téléchargement — mesuré à ~15 000 tentatives par
 * seconde — sous un repli de Suspense vide, donc invisible. [`useSettingsT`]
 * lit cette carte pour convertir l'échec en erreur, une bonne fois.
 */
const failedSettings = new Map<Lang, unknown>();

/**
 * Extension de réglages d'une langue **si déjà chargée**, sinon `null`.
 *
 * Pas de repli sur le français, contrairement à [`dictionary`] : le français
 * n'est pas là non plus au démarrage, il n'y a donc rien sur quoi se replier.
 * L'appelant attend (voir [`useSettingsT`]).
 */
export function settingsDictionary(lang: Lang): SettingsDict | null {
  return resolvedSettings.get(lang) ?? null;
}

/**
 * Échec retenu pour cette langue, sinon `undefined`.
 *
 * Lecture pure : l'échec n'est effacé que par une nouvelle tentative
 * ([`loadSettingsDict`]), jamais par le fait de le lire. Un rendu React peut
 * être rejoué ou abandonné ; il ne doit pas être le seul à avoir vu l'erreur.
 */
export function settingsFailure(lang: Lang): unknown {
  return failedSettings.get(lang);
}

/** Charge (une seule fois) l'extension de réglages d'une langue. */
export function loadSettingsDict(lang: Lang): Promise<SettingsDict> {
  const known = resolvedSettings.get(lang);
  if (known !== undefined) return Promise.resolve(known);
  const enCours = pendingSettings.get(lang);
  if (enCours !== undefined) return enCours;

  // Toute nouvelle tentative repart d'une ardoise propre : un échec retenu
  // décrit la tentative précédente, pas celle-ci.
  failedSettings.delete(lang);
  const chargement = SETTINGS_LOADERS[lang]()
    .then((module) => {
      const dict = module[`${lang}Settings`] as SettingsDict;
      registerSettingsDict(lang, dict);
      return dict;
    })
    .catch((erreur: unknown) => {
      // Retenu plutôt que ravalé : c'est ce que `useSettingsT` jettera au
      // rendu suivant, et ce qui arrête la boucle. La promesse en cours est
      // oubliée pour qu'un appel délibéré (changement de langue) puisse
      // retenter le téléchargement.
      failedSettings.set(lang, erreur);
      pendingSettings.delete(lang);
      throw erreur;
    });
  pendingSettings.set(lang, chargement);
  return chargement;
}

/**
 * Déclare une extension de réglages déjà en mémoire comme résolue. Pendant
 * normal de [`registerDictionary`] : les tests montent toutes les langues d'un
 * bloc pour qu'aucun rendu ne suspende.
 */
export function registerSettingsDict(lang: Lang, dict: SettingsDict): void {
  resolvedSettings.set(lang, dict);
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
export function decorationLabel(dict: SettingsDict, id: string): string {
  const labels: Record<string, string> = dict.decorations.labels;
  return labels[id] ?? id;
}
