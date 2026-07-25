/**
 * Tous les dictionnaires, chargés d'un bloc.
 *
 * ⚠️ Réservé aux **tests** et aux outils. L'application ne doit jamais importer
 * ce module : il annulerait le découpage par langue de `index.ts`, qui ne met
 * que le français dans le chargement initial. Une langue supplémentaire ajoute
 * ~60 ko au socle si elle transite par ici.
 */

import { fr } from './fr';
import { en } from './en';
import { es } from './es';
import { pt } from './pt';
import { de } from './de';
import { ru } from './ru';
import type { Dict, Lang } from './index';

export const dictionaries: Record<Lang, Dict> = { fr, en, es, pt, de, ru };
