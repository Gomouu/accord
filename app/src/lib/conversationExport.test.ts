import { describe, expect, it } from 'vitest';
import type { DisplayMessage } from '../components/messageModel';
import {
  buildConversationMarkdown,
  toTranscriptMessages,
  type ConversationExportLabels,
} from './conversationExport';

const labels: ConversationExportLabels = {
  heading: 'Conversation — Alice',
  subtitle: '2 messages',
  deleted: 'message supprimé',
  attachment: 'Pièce jointe',
  edited: 'modifié',
  empty: 'Aucun message',
  sticker: 'autocollant',
  poll: 'sondage',
};

function dm(over: Partial<DisplayMessage>): DisplayMessage {
  return {
    msg_id: 'm1',
    author: 'k_alice',
    sent_ms: 1000,
    deleted: false,
    body: { type: 'text', text: 'salut', reply_to: null, attachments: 0 },
    edited: null,
    ...over,
  };
}

describe('toTranscriptMessages', () => {
  it('utilise le texte d’un message texte', () => {
    const [m] = toTranscriptMessages([dm({})], labels);
    expect(m?.text).toBe('salut');
    expect(m?.edited).toBe(false);
  });

  it('préfère le texte édité et marque l’édition', () => {
    const [m] = toTranscriptMessages([dm({ edited: 'corrigé' })], labels);
    expect(m?.text).toBe('corrigé');
    expect(m?.edited).toBe(true);
  });

  it('n’affiche jamais l’édition d’un message supprimé', () => {
    const [m] = toTranscriptMessages([dm({ deleted: true, edited: 'x' })], labels);
    expect(m?.edited).toBe(false);
    expect(m?.deleted).toBe(true);
  });

  it('rend un placeholder pour un autocollant', () => {
    const [m] = toTranscriptMessages(
      [dm({ edited: null, body: { type: 'sticker', name: 'cat', merkle_root: 'r' } })],
      labels,
    );
    expect(m?.text).toBe('[autocollant] cat');
  });

  it('rend un placeholder pour un sondage', () => {
    const [m] = toTranscriptMessages(
      [
        dm({
          edited: null,
          body: {
            type: 'poll',
            poll_id: 'p',
            question: 'Pizza ?',
            options: ['oui', 'non'],
          },
        }),
      ],
      labels,
    );
    expect(m?.text).toBe('[sondage] Pizza ?');
  });

  it('mappe les noms des pièces jointes', () => {
    const [m] = toTranscriptMessages(
      [
        dm({
          attachments: [
            { merkle_root: 'r', name: 'photo.png', size: 10, mime: 'image/png' },
          ],
        }),
      ],
      labels,
    );
    expect(m?.attachments).toEqual(['photo.png']);
  });
});

describe('buildConversationMarkdown', () => {
  it('produit un Markdown complet copiable', () => {
    const md = buildConversationMarkdown({
      messages: [dm({ author: 'k_alice', sent_ms: 1000 })],
      labels,
      formatters: {
        nameOf: () => 'Alice',
        dayOf: () => 'lundi',
        timeOf: () => '10:00',
      },
    });
    expect(md).toContain('# Conversation — Alice');
    expect(md).toContain('## lundi');
    expect(md).toContain('**Alice** · 10:00');
    expect(md).toContain('salut');
  });
});
