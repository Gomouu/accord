/**
 * Tous les dictionnaires, noyaux et extensions de réglages, chargés d'un bloc.
 *
 * ⚠️ Réservé aux **tests** et aux outils. L'application ne doit jamais importer
 * ce module : il annulerait le découpage par langue de `index.ts`, qui ne met
 * que le noyau français dans le chargement initial. Une langue supplémentaire
 * ajoute ~60 ko au socle si elle transite par ici.
 *
 * 🔒 C'est aussi le seul point où la forme des dix langues est vérifiée à la
 * compilation : les deux `Record<Lang, …>` ci-dessous refusent un dictionnaire
 * auquel il manque une clé de la référence française. Le test de parité
 * (`parity.test.ts`) prend le relais pour les clés en trop et les marqueurs
 * d'interpolation, que le typage ne voit pas.
 */

import { fr } from './fr';
import { en } from './en';
import { es } from './es';
import { pt } from './pt';
import { de } from './de';
import { ru } from './ru';
import { zh } from './zh';
import { hi } from './hi';
import { bn } from './bn';
import { ar } from './ar';
import { frSettings } from './fr.settings';
import { enSettings } from './en.settings';
import { esSettings } from './es.settings';
import { ptSettings } from './pt.settings';
import { deSettings } from './de.settings';
import { ruSettings } from './ru.settings';
import { zhSettings } from './zh.settings';
import { hiSettings } from './hi.settings';
import { bnSettings } from './bn.settings';
import { arSettings } from './ar.settings';
import type { Dict, Lang, SettingsDict } from './index';

export const dictionaries: Record<Lang, Dict> = {
  fr,
  en,
  es,
  pt,
  de,
  ru,
  zh,
  hi,
  bn,
  ar,
};

export const settingsDictionaries: Record<Lang, SettingsDict> = {
  fr: frSettings,
  en: enSettings,
  es: esSettings,
  pt: ptSettings,
  de: deSettings,
  ru: ruSettings,
  zh: zhSettings,
  hi: hiSettings,
  bn: bnSettings,
  ar: arSettings,
};
