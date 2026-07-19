import { describe, expect, it } from 'vitest';
import {
  buildTranscript,
  type TranscriptFormatters,
  type TranscriptLabels,
  type TranscriptMessage,
} from './transcript';

const labels: TranscriptLabels = {
  heading: 'Conversation — Alice',
  subtitle: 'Exporté le 19/07/2026 · 3 messages',
  deleted: 'message supprimé',
  attachment: 'Pièce jointe',
  edited: 'modifié',
  empty: 'Aucun message',
};

const fmt: TranscriptFormatters = {
  nameOf: (a) => (a === 'k_alice' ? 'Alice' : a === 'k_bob' ? 'Bob' : a),
  dayOf: (ms) => (ms < 2_000 ? 'lundi 1 janvier 1970' : 'mardi 2 janvier 1970'),
  timeOf: (ms) => `T+${ms}`,
};

function msg(over: Partial<TranscriptMessage>): TranscriptMessage {
  return {
    author: 'k_alice',
    sentMs: 1000,
    deleted: false,
    text: 'salut',
    edited: false,
    ...over,
  };
}

describe('buildTranscript', () => {
  it('rend un titre et le sous-titre', () => {
    const out = buildTranscript([msg({})], labels, fmt);
    expect(out.startsWith('# Conversation — Alice\n')).toBe(true);
    expect(out).toContain('_Exporté le 19/07/2026 · 3 messages_');
  });

  it('émet un en-tête de jour une seule fois par jour', () => {
    const out = buildTranscript(
      [msg({ sentMs: 1000 }), msg({ sentMs: 1500 }), msg({ sentMs: 5000 })],
      labels,
      fmt,
    );
    expect(out.match(/## lundi 1 janvier 1970/g)?.length).toBe(1);
    expect(out.match(/## mardi 2 janvier 1970/g)?.length).toBe(1);
  });

  it('préfixe chaque message du nom et de l’heure', () => {
    const out = buildTranscript([msg({ author: 'k_bob', sentMs: 1000 })], labels, fmt);
    expect(out).toContain('**Bob** · T+1000');
    expect(out).toContain('salut');
  });

  it('marque un message supprimé sans divulguer son contenu', () => {
    const out = buildTranscript([msg({ deleted: true, text: 'secret' })], labels, fmt);
    expect(out).toContain('_message supprimé_');
    expect(out).not.toContain('secret');
  });

  it('liste les pièces jointes par nom', () => {
    const out = buildTranscript(
      [msg({ text: null, attachments: ['photo.png', 'clip.mp4'] })],
      labels,
      fmt,
    );
    expect(out).toContain('📎 Pièce jointe : photo.png');
    expect(out).toContain('📎 Pièce jointe : clip.mp4');
  });

  it('accole le marqueur « modifié » à un message édité', () => {
    const out = buildTranscript([msg({ text: 'corrigé', edited: true })], labels, fmt);
    expect(out).toContain('corrigé _(modifié)_');
  });

  it('remplace un corps vide non supprimé par le libellé « aucun message »', () => {
    const out = buildTranscript([msg({ text: null, attachments: [] })], labels, fmt);
    expect(out).toContain('_Aucun message_');
  });

  it('gère une conversation vide', () => {
    const out = buildTranscript([], labels, fmt);
    expect(out).toContain('# Conversation — Alice');
    expect(out.trimEnd().endsWith('_Aucun message_')).toBe(true);
  });

  it('termine par un unique saut de ligne', () => {
    const out = buildTranscript([msg({})], labels, fmt);
    expect(out.endsWith('\n')).toBe(true);
    expect(out.endsWith('\n\n')).toBe(false);
  });

  it('combine texte et pièce jointe dans le même message', () => {
    const out = buildTranscript(
      [msg({ text: 'regarde', attachments: ['doc.pdf'] })],
      labels,
      fmt,
    );
    expect(out).toContain('regarde');
    expect(out).toContain('📎 Pièce jointe : doc.pdf');
  });
});
