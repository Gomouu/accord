/**
 * Corps textuel d'un message du fil : markdown (mentions, émojis custom,
 * couleurs de rôle), mention « modifié » après édition, et repli en italique
 * pour les messages supprimés ou au corps non pris en charge.
 */

import { memo } from 'react';
import { maskFiltered } from '../lib/automod';
import { useT } from '../stores/ui';
import { MarkdownText } from './MarkdownText';
import { displayText, type DisplayMessage } from './messageModel';

interface BodyTextProps {
  message: DisplayMessage;
  emojiMap?: ReadonlyMap<string, string> | undefined;
  knownMentions?: ReadonlySet<string> | undefined;
  roleColors?: ReadonlyMap<string, number> | undefined;
  /** Mots filtrés par l'AutoMod du serveur, masqués au rendu (absent en MP). */
  automodWords?: readonly string[] | undefined;
}

function BodyTextInner({
  message,
  emojiMap,
  knownMentions,
  roleColors,
  automodWords,
}: BodyTextProps) {
  const t = useT();
  if (message.deleted) {
    return <em className="text-faint">{t.dm.deletedMessage}</em>;
  }
  const text = displayText(message);
  if (text === null) {
    return <em className="text-faint">{t.dm.unsupported}</em>;
  }
  // AutoMod appliqué au rendu (modèle serverless) : les clients honnêtes
  // masquent, rien n'est supprimé du réseau.
  const masked =
    automodWords !== undefined && automodWords.length > 0
      ? maskFiltered(text, automodWords)
      : text;
  return (
    <div className="selectable whitespace-pre-wrap break-words">
      <MarkdownText
        text={masked}
        emojis={emojiMap}
        knownMentions={knownMentions}
        roleColors={roleColors}
        hint={message.author}
      />
      {message.edited !== null && (
        <span className="ml-1 text-[10px] text-faint">{t.dm.edited}</span>
      )}
    </div>
  );
}

/**
 * Mémoïsé : le corps d'un message ne se re-rend pas quand la vue parente se
 * re-rend sans changement de ce message (le `DisplayMessage` garde son
 * identité tant qu'il n'est ni édité ni réagi — voir la fusion incrémentale
 * des stores). Combiné à `MarkdownText`, coupe le re-parse du fil entier.
 */
export const BodyText = memo(BodyTextInner);
