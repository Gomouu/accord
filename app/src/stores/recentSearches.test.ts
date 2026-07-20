/** Tests du store des recherches récentes : ordre, déduplication, bornage. */

import { beforeEach, describe, expect, it } from 'vitest';
import { useRecentSearches } from './recentSearches';

beforeEach(() => {
  window.localStorage.clear();
  useRecentSearches.setState({ items: [] });
});

describe('useRecentSearches', () => {
  it('enregistre en tête et ignore le vide', () => {
    useRecentSearches.getState().record('alice');
    useRecentSearches.getState().record('   ');
    useRecentSearches.getState().record('bob');
    expect(useRecentSearches.getState().items).toEqual(['bob', 'alice']);
  });

  it('déduplique en remontant la requête relancée', () => {
    useRecentSearches.getState().record('alice');
    useRecentSearches.getState().record('bob');
    useRecentSearches.getState().record('alice');
    expect(useRecentSearches.getState().items).toEqual(['alice', 'bob']);
  });

  it('borne à 8 entrées', () => {
    for (let i = 0; i < 12; i += 1) useRecentSearches.getState().record(`q${i}`);
    expect(useRecentSearches.getState().items).toHaveLength(8);
    expect(useRecentSearches.getState().items[0]).toBe('q11');
  });

  it('retire une entrée et vide tout', () => {
    useRecentSearches.getState().record('alice');
    useRecentSearches.getState().record('bob');
    useRecentSearches.getState().remove('alice');
    expect(useRecentSearches.getState().items).toEqual(['bob']);
    useRecentSearches.getState().clear();
    expect(useRecentSearches.getState().items).toEqual([]);
  });

  it('persiste et rogne les requêtes', () => {
    useRecentSearches.getState().record('  spaced  ');
    expect(useRecentSearches.getState().items).toEqual(['spaced']);
    expect(
      JSON.parse(window.localStorage.getItem('accord.recentSearches') ?? '[]'),
    ).toEqual(['spaced']);
  });
});
