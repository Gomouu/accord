/**
 * Tests de l'aide pure `firstUnreadIndex` : position du séparateur « nouveaux
 * messages » — au-delà de la marque lue, jamais sur ses propres messages.
 */

import { describe, expect, it } from 'vitest';
import { firstUnreadIndex, type DisplayMessage } from './messageModel';

const SELF = 'moi';

function msg(id: string, author: string, lamport: number): DisplayMessage {
  return {
    msg_id: id,
    author,
    sent_ms: lamport * 1000,
    deleted: false,
    body: { type: 'text', text: id, reply_to: null, attachments: 0 },
    edited: null,
    lamport,
  };
}

describe('firstUnreadIndex', () => {
  it('rend -1 quand aucun message ne dépasse la marque lue', () => {
    const messages = [msg('a', 'pair', 1), msg('b', 'pair', 2)];
    expect(firstUnreadIndex(messages, 5, SELF)).toBe(-1);
  });

  it('rend l’index du premier message d’autrui au-delà de la marque', () => {
    const messages = [msg('a', 'pair', 3), msg('b', 'pair', 6), msg('c', 'pair', 7)];
    // Marque 5 : « a » (3) est lu, « b » (6) est le premier non lu.
    expect(firstUnreadIndex(messages, 5, SELF)).toBe(1);
  });

  it('ignore ses propres messages non lus (ne comptent pas comme nouveauté)', () => {
    const messages = [msg('a', SELF, 6), msg('b', 'pair', 7)];
    // « a » (le mien) au-delà de la marque est ignoré ; « b » ancre le séparateur.
    expect(firstUnreadIndex(messages, 5, SELF)).toBe(1);
  });

  it('rend -1 quand tous les non-lus sont de soi', () => {
    const messages = [msg('a', SELF, 6), msg('b', SELF, 7)];
    expect(firstUnreadIndex(messages, 5, SELF)).toBe(-1);
  });

  it('rend -1 pour une marque nulle, nulle valeur ou négative (jamais lu / nœud ancien)', () => {
    const messages = [msg('a', 'pair', 3)];
    expect(firstUnreadIndex(messages, null, SELF)).toBe(-1);
    expect(firstUnreadIndex(messages, 0, SELF)).toBe(-1);
  });

  it('ignore les messages sans horloge de Lamport', () => {
    const withoutLamport: DisplayMessage = {
      msg_id: 'a',
      author: 'pair',
      sent_ms: 0,
      deleted: false,
      body: { type: 'text', text: 'a', reply_to: null, attachments: 0 },
      edited: null,
    };
    expect(firstUnreadIndex([withoutLamport], 5, SELF)).toBe(-1);
  });
});
