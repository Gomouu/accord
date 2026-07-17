/** Tests de la pagination : enveloppes `before_lamport` et fusion des pages. */

import { describe, expect, it, vi } from 'vitest';
import {
  PAGE_SIZE,
  fetchDmPage,
  fetchGroupPage,
  mergeOlderPage,
  mergeRecentPage,
  sortAscending,
  type Sequenced,
} from './history';

interface FakeMsg extends Sequenced {
  edited?: string | null;
}

function msg(id: string, lamport: number, extra: Partial<FakeMsg> = {}): FakeMsg {
  return { msg_id: id, lamport, ...extra };
}

function ids(messages: readonly Sequenced[]): string[] {
  return messages.map((m) => m.msg_id);
}

describe('fetchDmPage / fetchGroupPage', () => {
  it('passe before_lamport au nœud uniquement quand il est fourni', async () => {
    const call = vi.fn().mockResolvedValue({ messages: [] });
    const rpc = { call };

    await fetchDmPage(rpc, 'clef', 42, 10);
    expect(call).toHaveBeenLastCalledWith('dm.history', {
      pubkey: 'clef',
      limit: 10,
      before_lamport: 42,
    });

    await fetchDmPage(rpc, 'clef');
    expect(call).toHaveBeenLastCalledWith('dm.history', {
      pubkey: 'clef',
      limit: PAGE_SIZE,
    });
  });

  it('adresse groups.history avec le groupe et le salon', async () => {
    const call = vi.fn().mockResolvedValue({ messages: [] });
    await fetchGroupPage({ call }, 'g1', 'c1', 7);
    expect(call).toHaveBeenCalledWith('groups.history', {
      group_id: 'g1',
      channel_id: 'c1',
      limit: PAGE_SIZE,
      before_lamport: 7,
    });
  });
});

describe('sortAscending', () => {
  it('trie par lamport croissant puis par msg_id, sans muter l’entrée', () => {
    const input = [msg('b', 2), msg('c', 1), msg('a', 2)];
    const sorted = sortAscending(input);
    expect(ids(sorted)).toEqual(['c', 'a', 'b']);
    expect(ids(input)).toEqual(['b', 'c', 'a']);
  });
});

describe('mergeRecentPage', () => {
  it('adopte la page telle quelle quand rien n’est encore chargé', () => {
    const { messages, gapDetected } = mergeRecentPage(
      [],
      [msg('b', 2), msg('a', 1)],
      false,
    );
    expect(ids(messages)).toEqual(['a', 'b']);
    expect(gapDetected).toBe(false);
  });

  it('fusionne sans doublon quand la page recouvre l’existant', () => {
    const existing = [msg('a', 1), msg('b', 2)];
    const page = [msg('c', 3), msg('b', 2)];
    const { messages, gapDetected } = mergeRecentPage(existing, page, true);
    expect(ids(messages)).toEqual(['a', 'b', 'c']);
    expect(gapDetected).toBe(false);
  });

  it('la copie fraîche remplace l’ancienne (édition rafraîchie)', () => {
    const existing = [msg('a', 1, { edited: null })];
    const page = [msg('a', 1, { edited: 'nouveau texte' })];
    const { messages } = mergeRecentPage(existing, page, false);
    expect(messages[0]?.edited).toBe('nouveau texte');
  });

  it('préserve l’identité de l’objet d’un message inchangé (memo efficace)', () => {
    const kept = msg('a', 1, { edited: null });
    const existing = [kept, msg('b', 2)];
    // Page fraîche : même contenu pour « a » (nouvel objet), « b » modifié.
    const page = [msg('a', 1, { edited: null }), msg('b', 2, { edited: 'x' })];
    const { messages } = mergeRecentPage(existing, page, false);
    const merged = new Map(messages.map((m) => [m.msg_id, m]));
    expect(merged.get('a')).toBe(kept);
    expect(merged.get('b')?.edited).toBe('x');
  });

  it('détecte un trou : page pleine, disjointe et plus récente → remplacement', () => {
    const existing = [msg('a', 1), msg('b', 2)];
    const page = [msg('z', 100), msg('y', 99)];
    const { messages, gapDetected } = mergeRecentPage(existing, page, true);
    expect(ids(messages)).toEqual(['y', 'z']);
    expect(gapDetected).toBe(true);
  });

  it('ne signale pas de trou si la page n’est pas pleine (fil complet)', () => {
    const existing = [msg('a', 1)];
    const page = [msg('z', 100)];
    const { messages, gapDetected } = mergeRecentPage(existing, page, false);
    expect(ids(messages)).toEqual(['a', 'z']);
    expect(gapDetected).toBe(false);
  });
});

describe('mergeOlderPage', () => {
  it('insère la page ancienne en tête, dédupliquée et ordonnée', () => {
    const existing = [msg('c', 3), msg('d', 4)];
    const page = [msg('c', 3), msg('b', 2), msg('a', 1)];
    expect(ids(mergeOlderPage(existing, page))).toEqual(['a', 'b', 'c', 'd']);
  });

  it('reste stable quand la page est vide', () => {
    const existing = [msg('a', 1)];
    expect(ids(mergeOlderPage(existing, []))).toEqual(['a']);
  });
});
