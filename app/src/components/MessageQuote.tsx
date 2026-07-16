/**
 * Aperçu du message cité, affiché au-dessus d'une réponse : nom de l'auteur
 * et extrait tronqué, repli en italique si le message cité est supprimé ou
 * introuvable. Cliquable (saut vers le message d'origine) quand `onJump` est
 * fourni.
 */

import { useT } from '../stores/ui';
import { displayText, type DisplayMessage } from './messageModel';

interface MessageQuoteProps {
  quoted: DisplayMessage | undefined;
  nameOf: (author: string) => string;
  onJump?: (() => void) | undefined;
}

export function MessageQuote({ quoted, nameOf, onJump }: MessageQuoteProps) {
  const t = useT();
  const snippet =
    quoted === undefined
      ? t.dm.quoteUnavailable
      : quoted.deleted
        ? t.dm.deletedMessage
        : (displayText(quoted) ?? t.dm.unsupported);

  const inner = (
    <>
      <span
        aria-hidden
        className="ml-1 h-2 w-6 shrink-0 rounded-tl-md border-l-2 border-t-2 border-input"
      />
      {quoted !== undefined && (
        <span className="min-w-0 max-w-[35%] truncate font-medium text-header">
          {nameOf(quoted.author)}
        </span>
      )}
      <span
        className={`min-w-0 flex-1 truncate ${quoted === undefined ? 'italic text-faint' : ''}`}
      >
        {snippet}
      </span>
    </>
  );

  const className =
    'mb-0.5 flex w-full min-w-0 max-w-full items-center gap-1.5 overflow-hidden text-xs text-muted';
  if (onJump === undefined) return <div className={className}>{inner}</div>;
  return (
    <button
      type="button"
      onClick={onJump}
      className={`${className} rounded-sm text-left hover:text-norm focus-visible:outline-none focus-visible:text-norm focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-chat`}
    >
      {inner}
    </button>
  );
}
