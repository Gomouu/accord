/**
 * Pont entre le fil affiché (`DisplayMessage`, couplé à l'API) et le
 * formateur pur `transcript.ts` : résout le texte de chaque message (édition,
 * placeholders média) puis assemble la transcription Markdown. Isolé ici pour
 * garder `transcript.ts` sans dépendance aux types d'API — et testable seul.
 */

import type { DisplayMessage } from '../components/messageModel';
import { displayText } from '../components/messageModel';
import {
  buildTranscript,
  type TranscriptFormatters,
  type TranscriptMessage,
} from './transcript';

/** Libellés média + structure, tous déjà traduits par l'appelant. */
export interface ConversationExportLabels {
  readonly heading: string;
  readonly subtitle: string;
  readonly deleted: string;
  readonly attachment: string;
  readonly edited: string;
  readonly empty: string;
  readonly sticker: string;
  readonly poll: string;
}

/** Texte lisible d'un message, avec placeholder pour les corps non textuels. */
function texteMessage(m: DisplayMessage, labels: ConversationExportLabels): string | null {
  const txt = displayText(m);
  if (txt !== null) return txt;
  if (m.body.type === 'sticker') return `[${labels.sticker}] ${m.body.name}`;
  if (m.body.type === 'poll') return `[${labels.poll}] ${m.body.question}`;
  return null;
}

/** Aplati un fil affiché en messages de transcription (fonction pure). */
export function toTranscriptMessages(
  messages: readonly DisplayMessage[],
  labels: ConversationExportLabels,
): TranscriptMessage[] {
  return messages.map((m) => ({
    author: m.author,
    sentMs: m.sent_ms,
    deleted: m.deleted,
    text: texteMessage(m, labels),
    edited: m.edited !== null && !m.deleted,
    attachments: (m.attachments ?? []).map((a) => a.name),
  }));
}

/** Contexte complet d'un export de conversation. */
export interface ConversationExportContext {
  readonly messages: readonly DisplayMessage[];
  readonly labels: ConversationExportLabels;
  readonly formatters: TranscriptFormatters;
}

/** Transcription Markdown prête à copier/coller (fonction pure). */
export function buildConversationMarkdown(ctx: ConversationExportContext): string {
  return buildTranscript(
    toTranscriptMessages(ctx.messages, ctx.labels),
    {
      heading: ctx.labels.heading,
      subtitle: ctx.labels.subtitle,
      deleted: ctx.labels.deleted,
      attachment: ctx.labels.attachment,
      edited: ctx.labels.edited,
      empty: ctx.labels.empty,
    },
    ctx.formatters,
  );
}
