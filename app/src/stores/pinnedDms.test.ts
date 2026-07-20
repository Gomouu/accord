/** Tests de l'épinglage des MP : bascule, persistance, tri pinned-first. */

import { beforeEach, describe, expect, it } from 'vitest';
import { sortPinnedFirst, usePinnedDms } from './pinnedDms';

beforeEach(() => {
  window.localStorage.clear();
  usePinnedDms.setState({ pinned: [] });
});

describe('usePinnedDms', () => {
  it('épingle puis désépingle par bascule, et persiste', () => {
    usePinnedDms.getState().toggle('a');
    expect(usePinnedDms.getState().isPinned('a')).toBe(true);
    expect(JSON.parse(window.localStorage.getItem('accord.pinnedDms') ?? '[]')).toEqual([
      'a',
    ]);

    usePinnedDms.getState().toggle('a');
    expect(usePinnedDms.getState().isPinned('a')).toBe(false);
  });
});

describe('sortPinnedFirst', () => {
  const list = [{ pubkey: 'a' }, { pubkey: 'b' }, { pubkey: 'c' }];

  it('remonte les épinglés en tête en préservant l’ordre', () => {
    const out = sortPinnedFirst(list, new Set(['c']));
    expect(out.map((x) => x.pubkey)).toEqual(['c', 'a', 'b']);
  });

  it('laisse l’ordre inchangé sans épingle', () => {
    expect(sortPinnedFirst(list, new Set()).map((x) => x.pubkey)).toEqual([
      'a',
      'b',
      'c',
    ]);
  });
});
