/**
 * Épinglage de conversations privées — préférence LOCALE (localStorage) qui
 * remonte les MP choisis en tête de la liste d'accueil. Purement cosmétique et
 * local ; n'affecte ni le réseau ni l'autre pair.
 */

import { create } from 'zustand';

const STORAGE_KEY = 'accord.pinnedDms';

function charger(): string[] {
  try {
    const brut = window.localStorage.getItem(STORAGE_KEY);
    if (brut === null) return [];
    const parsed: unknown = JSON.parse(brut);
    if (!Array.isArray(parsed)) return [];
    return parsed.filter((x): x is string => typeof x === 'string');
  } catch {
    return [];
  }
}

function persister(items: string[]): void {
  try {
    window.localStorage.setItem(STORAGE_KEY, JSON.stringify(items));
  } catch {
    // Stockage indisponible : l'épinglage reste en mémoire pour la session.
  }
}

/**
 * Remonte les éléments épinglés en tête, en préservant l'ordre d'origine dans
 * chaque groupe (épinglés puis le reste). Pur et testable.
 */
export function sortPinnedFirst<T extends { pubkey: string }>(
  items: readonly T[],
  pinned: ReadonlySet<string>,
): T[] {
  const tete: T[] = [];
  const reste: T[] = [];
  for (const item of items) (pinned.has(item.pubkey) ? tete : reste).push(item);
  return [...tete, ...reste];
}

interface PinnedDmsState {
  pinned: string[];
  isPinned: (peer: string) => boolean;
  toggle: (peer: string) => void;
}

export const usePinnedDms = create<PinnedDmsState>((set, get) => ({
  pinned: charger(),
  isPinned: (peer) => get().pinned.includes(peer),
  toggle: (peer) =>
    set((s) => {
      const pinned = s.pinned.includes(peer)
        ? s.pinned.filter((p) => p !== peer)
        : [...s.pinned, peer];
      persister(pinned);
      return { pinned };
    }),
}));
