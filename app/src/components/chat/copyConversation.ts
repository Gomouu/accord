/**
 * Action « Copier la conversation » partagée par `DmView` et `GroupView` :
 * assemble une transcription Markdown du fil affiché (`conversationExport`) et
 * la place dans le presse-papiers, avec retour utilisateur par toast. Fonction
 * simple (non-hook) pour être appelable après un retour anticipé de composant ;
 * langue/heure/toast sont lus au clic via `useUi.getState()`.
 */

import type { DisplayMessage } from '../messageModel';
import { buildConversationMarkdown } from '../../lib/conversationExport';
import { copyToClipboard } from '../../lib/clipboard';
import { formatDay, formatEventDateTime, formatTimeOnly } from '../../lib/format';
import { interpolate, type Dict } from '../../i18n';
import { useUi } from '../../stores/ui';

/**
 * Copie le fil `messages` en Markdown. `title` est le nom lisible de la
 * conversation (pair ou salon), `nameOf` résout les auteurs, `t` le
 * dictionnaire courant (fourni par l'appelant, déjà abonné au rendu).
 */
export function copyConversation(
  messages: readonly DisplayMessage[],
  nameOf: (author: string) => string,
  title: string,
  t: Dict,
): void {
  const { lang, timeFormat, toast } = useUi.getState();
  const markdown = buildConversationMarkdown({
    messages,
    labels: {
      heading: interpolate(t.transcript.heading, { name: title }),
      subtitle: interpolate(t.transcript.subtitle, {
        date: formatEventDateTime(Date.now(), lang, timeFormat),
        count: String(messages.length),
      }),
      deleted: t.transcript.deleted,
      attachment: t.transcript.attachment,
      edited: t.transcript.edited,
      empty: t.transcript.empty,
      sticker: t.transcript.sticker,
      poll: t.transcript.poll,
    },
    formatters: {
      nameOf,
      dayOf: (ms) => formatDay(ms, lang),
      timeOf: (ms) => formatTimeOnly(ms, lang, timeFormat),
    },
  });
  copyToClipboard(
    markdown,
    () => toast('info', t.transcript.copied),
    () => toast('error', t.transcript.copyFailed),
  );
}
