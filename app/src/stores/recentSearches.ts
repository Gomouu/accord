/**
 * Recherches récentes — mémoire LOCALE (localStorage) des dernières requêtes
 * lancées, proposées quand le champ de recherche est vide. Purement local,
 * borné et dédupliqué (la même requête relancée remonte en tête).
 */

import { create } from 'zustand';

const STORAGE_KEY = 'accord.recentSearches';
/** Nombre de requêtes récentes conservées (au-delà, la plus ancienne tombe). */
const MAX_RECENTS = 8;

function charger(): string[] {
  try {
    const brut = window.localStorage.getItem(STORAGE_KEY);
    if (brut === null) return [];
    const parsed: unknown = JSON.parse(brut);
    if (!Array.isArray(parsed)) return [];
    return parsed.filter((x): x is string => typeof x === 'string').slice(0, MAX_RECENTS);
  } catch {
    return [];
  }
}

function persister(items: string[]): void {
  try {
    window.localStorage.setItem(STORAGE_KEY, JSON.stringify(items));
  } catch {
    // Stockage indisponible : la liste reste en mémoire pour la session.
  }
}

interface RecentSearchesState {
  items: string[];
  /** Enregistre une requête (rognée) en tête, dédupliquée et bornée. */
  record: (query: string) => void;
  remove: (query: string) => void;
  clear: () => void;
}

export const useRecentSearches = create<RecentSearchesState>((set) => ({
  items: charger(),
  record: (query) =>
    set((s) => {
      const q = query.trim();
      if (q === '') return s;
      const items = [q, ...s.items.filter((x) => x !== q)].slice(0, MAX_RECENTS);
      persister(items);
      return { items };
    }),
  remove: (query) =>
    set((s) => {
      const items = s.items.filter((x) => x !== query);
      persister(items);
      return { items };
    }),
  clear: () => {
    persister([]);
    return set({ items: [] });
  },
}));
