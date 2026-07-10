/**
 * Tests du store des conversations directes : chargement initial, pagination
 * `before_lamport`, fusion incrémentale des événements, détection de trou et
 * actions de message (édition, suppression, bascule de réaction).
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Mock } from 'vitest';

vi.mock('../lib/client', () => ({
  rpc: { call: vi.fn() },
  api: {
    dmSend: vi.fn(),
    dmEdit: vi.fn(),
    dmDelete: vi.fn(),
    dmReact: vi.fn(),
  },
}));

import { api, rpc } from '../lib/client';
import type { DmMessage } from '../lib/api';
import { PAGE_SIZE } from '../lib/history';
import { handleDmsNodeEvent, useDms } from './dms';

const callMock = rpc.call as unknown as Mock;
const sendMock = api.dmSend as unknown as Mock;
const editMock = api.dmEdit as unknown as Mock;
const deleteMock = api.dmDelete as unknown as Mock;
const reactMock = api.dmReact as unknown as Mock;

function dmMsg(id: string, lamport: number): DmMessage {
  return {
    msg_id: id,
    author: 'pair',
    lamport,
    sent_ms: lamport * 1000,
    acked: true,
    deleted: false,
    body: { type: 'text', text: `message ${id}`, reply_to: null, attachments: 0 },
    edited: null,
  };
}

/** Page pleine de lamports [start, start + PAGE_SIZE), du plus récent au plus ancien. */
function fullPage(start: number): DmMessage[] {
  return Array.from({ length: PAGE_SIZE }, (_, i) =>
    dmMsg(`m${start + i}`, start + i),
  ).reverse();
}

function conversation(peer: string): DmMessage[] {
  return useDms.getState().conversations[peer] ?? [];
}

beforeEach(() => {
  useDms.setState({ conversations: {}, hasMore: {}, loadingOlder: {}, peerRead: {} });
  callMock.mockReset();
  sendMock.mockReset();
  editMock.mockReset();
  deleteMock.mockReset();
  reactMock.mockReset();
});

describe('useDms.refresh', () => {
  it('charge la page récente en ordre croissant, sans page suivante si incomplète', async () => {
    callMock.mockResolvedValueOnce({ messages: [dmMsg('b', 2), dmMsg('a', 1)] });

    await useDms.getState().refresh('pair');

    expect(callMock).toHaveBeenCalledWith('dm.history', {
      pubkey: 'pair',
      limit: PAGE_SIZE,
    });
    expect(conversation('pair').map((m) => m.msg_id)).toEqual(['a', 'b']);
    expect(useDms.getState().hasMore['pair']).toBe(false);
  });

  it('marque hasMore quand la première page est pleine', async () => {
    callMock.mockResolvedValueOnce({ messages: fullPage(51) });

    await useDms.getState().refresh('pair');

    expect(conversation('pair')).toHaveLength(PAGE_SIZE);
    expect(useDms.getState().hasMore['pair']).toBe(true);
  });

  it('fusionne un nouvel événement sans recharger tout le fil', async () => {
    useDms.setState({
      conversations: { pair: [dmMsg('a', 1), dmMsg('b', 2)] },
      hasMore: { pair: false },
    });
    callMock.mockResolvedValueOnce({
      messages: [dmMsg('c', 3), dmMsg('b', 2), dmMsg('a', 1)],
    });

    await useDms.getState().refresh('pair');

    expect(conversation('pair').map((m) => m.msg_id)).toEqual(['a', 'b', 'c']);
  });

  it('remplace le fil par la page récente quand un trou est détecté', async () => {
    useDms.setState({
      conversations: { pair: [dmMsg('a', 1), dmMsg('b', 2)] },
      hasMore: { pair: false },
    });
    callMock.mockResolvedValueOnce({ messages: fullPage(1000) });

    await useDms.getState().refresh('pair');

    const messages = conversation('pair');
    expect(messages).toHaveLength(PAGE_SIZE);
    expect(messages[0]?.lamport).toBe(1000);
    expect(useDms.getState().hasMore['pair']).toBe(true);
  });
});

describe('useDms.loadOlder', () => {
  it('demande la page précédant le plus ancien lamport connu et la fusionne', async () => {
    callMock.mockResolvedValueOnce({ messages: fullPage(51) });
    await useDms.getState().refresh('pair');

    callMock.mockResolvedValueOnce({
      messages: [dmMsg('m50', 50), dmMsg('m49', 49)],
    });
    await useDms.getState().loadOlder('pair');

    expect(callMock).toHaveBeenLastCalledWith('dm.history', {
      pubkey: 'pair',
      limit: PAGE_SIZE,
      before_lamport: 51,
    });
    const messages = conversation('pair');
    expect(messages).toHaveLength(PAGE_SIZE + 2);
    expect(messages[0]?.msg_id).toBe('m49');
    // Page ancienne incomplète : le début du fil est atteint.
    expect(useDms.getState().hasMore['pair']).toBe(false);
  });

  it('ne fait rien sans page suivante annoncée', async () => {
    callMock.mockResolvedValueOnce({ messages: [dmMsg('a', 1)] });
    await useDms.getState().refresh('pair');
    callMock.mockClear();

    await useDms.getState().loadOlder('pair');

    expect(callMock).not.toHaveBeenCalled();
  });

  it('ignore les déclenchements concurrents pendant un chargement', async () => {
    callMock.mockResolvedValueOnce({ messages: fullPage(51) });
    await useDms.getState().refresh('pair');
    callMock.mockClear();

    let release: (value: { messages: DmMessage[] }) => void = () => {};
    callMock.mockReturnValueOnce(
      new Promise((resolve) => {
        release = resolve;
      }),
    );

    const first = useDms.getState().loadOlder('pair');
    const second = useDms.getState().loadOlder('pair');
    release({ messages: [] });
    await Promise.all([first, second]);

    expect(callMock).toHaveBeenCalledTimes(1);
  });
});

describe('useDms.send', () => {
  it('envoie puis rafraîchit la page récente', async () => {
    sendMock.mockResolvedValueOnce({ msg_id: 'x' });
    callMock.mockResolvedValueOnce({ messages: [dmMsg('x', 1)] });

    await useDms.getState().send('pair', 'bonjour');

    expect(sendMock).toHaveBeenCalledWith('pair', 'bonjour', undefined, undefined);
    expect(conversation('pair').map((m) => m.msg_id)).toEqual(['x']);
  });

  it('transmet le message cité en réponse', async () => {
    sendMock.mockResolvedValueOnce({ msg_id: 'y' });
    callMock.mockResolvedValueOnce({ messages: [dmMsg('y', 2)] });

    await useDms.getState().send('pair', 'réponse', 'orig');

    expect(sendMock).toHaveBeenCalledWith('pair', 'réponse', 'orig', undefined);
  });

  it('transmet les pièces jointes publiées (texte vide admis)', async () => {
    sendMock.mockResolvedValueOnce({ msg_id: 'z' });
    callMock.mockResolvedValueOnce({ messages: [dmMsg('z', 3)] });
    const piece = {
      merkle_root: 'aa'.repeat(32),
      name: 'a.png',
      size: 3,
      mime: 'image/png',
    };

    await useDms.getState().send('pair', '', undefined, [piece]);

    expect(sendMock).toHaveBeenCalledWith('pair', '', undefined, [piece]);
  });
});

describe('useDms.edit', () => {
  it('modifie côté nœud puis reflète le nouveau texte localement', async () => {
    useDms.setState({ conversations: { pair: [dmMsg('a', 1), dmMsg('b', 2)] } });
    editMock.mockResolvedValueOnce({ ok: true });

    await useDms.getState().edit('pair', 'a', 'texte corrigé');

    expect(editMock).toHaveBeenCalledWith('pair', 'a', 'texte corrigé');
    expect(conversation('pair')[0]?.edited).toBe('texte corrigé');
    expect(conversation('pair')[1]?.edited).toBeNull();
  });

  it('ne modifie rien localement quand le nœud refuse', async () => {
    useDms.setState({ conversations: { pair: [dmMsg('a', 1)] } });
    editMock.mockRejectedValueOnce(new Error('auteur seul'));

    await expect(useDms.getState().edit('pair', 'a', 'refusé')).rejects.toThrow();

    expect(conversation('pair')[0]?.edited).toBeNull();
  });
});

describe('useDms.deleteMessage', () => {
  it('supprime côté nœud puis pose le tombstone localement', async () => {
    useDms.setState({ conversations: { pair: [dmMsg('a', 1), dmMsg('b', 2)] } });
    deleteMock.mockResolvedValueOnce({ ok: true });

    await useDms.getState().deleteMessage('pair', 'b');

    expect(deleteMock).toHaveBeenCalledWith('pair', 'b');
    expect(conversation('pair')[1]?.deleted).toBe(true);
    expect(conversation('pair')[0]?.deleted).toBe(false);
  });
});

describe('useDms.toggleReaction', () => {
  it('ajoute sa réaction quand elle est absente', async () => {
    useDms.setState({
      conversations: {
        pair: [{ ...dmMsg('a', 1), reactions: [{ emoji: '👍', author: 'pair' }] }],
      },
    });
    reactMock.mockResolvedValueOnce({ ok: true });

    await useDms.getState().toggleReaction('pair', 'a', '👍', 'moi');

    expect(reactMock).toHaveBeenCalledWith('pair', 'a', '👍', false);
    expect(conversation('pair')[0]?.reactions).toEqual([
      { emoji: '👍', author: 'pair' },
      { emoji: '👍', author: 'moi' },
    ]);
  });

  it('retire sa réaction quand elle existe déjà (bascule)', async () => {
    useDms.setState({
      conversations: {
        pair: [
          {
            ...dmMsg('a', 1),
            reactions: [
              { emoji: '👍', author: 'moi' },
              { emoji: '👍', author: 'pair' },
            ],
          },
        ],
      },
    });
    reactMock.mockResolvedValueOnce({ ok: true });

    await useDms.getState().toggleReaction('pair', 'a', '👍', 'moi');

    expect(reactMock).toHaveBeenCalledWith('pair', 'a', '👍', true);
    expect(conversation('pair')[0]?.reactions).toEqual([{ emoji: '👍', author: 'pair' }]);
  });

  it('ignore un message inconnu localement', async () => {
    await useDms.getState().toggleReaction('pair', 'fantôme', '👍', 'moi');

    expect(reactMock).not.toHaveBeenCalled();
  });
});

describe('accusés de lecture (peerRead)', () => {
  it('capture peer_read_lamport de la réponse dm.history', async () => {
    callMock.mockResolvedValueOnce({
      messages: [dmMsg('a', 1)],
      peer_read_lamport: 4,
    });

    await useDms.getState().refresh('pair');

    expect(useDms.getState().peerRead['pair']).toBe(4);
  });

  it('ignore un peer_read_lamport absent ou nul', async () => {
    callMock.mockResolvedValueOnce({ messages: [], peer_read_lamport: null });

    await useDms.getState().refresh('pair');

    expect(useDms.getState().peerRead['pair']).toBeUndefined();
  });

  it('avance sans jamais reculer (applyPeerRead monotone)', () => {
    useDms.getState().applyPeerRead('pair', 7);
    useDms.getState().applyPeerRead('pair', 3);

    expect(useDms.getState().peerRead['pair']).toBe(7);

    useDms.getState().applyPeerRead('pair', 9);
    expect(useDms.getState().peerRead['pair']).toBe(9);
  });

  it('reflète event.dm_read et ignore les autres événements', () => {
    handleDmsNodeEvent('event.dm_read', { peer: 'pair', lamport: 12 });
    expect(useDms.getState().peerRead['pair']).toBe(12);

    handleDmsNodeEvent('event.dm', { peer: 'pair', msg_id: 'x' });
    handleDmsNodeEvent('event.dm_read', { peer: 'pair' });
    expect(useDms.getState().peerRead['pair']).toBe(12);
  });
});
