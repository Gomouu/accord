/**
 * Aperçu du message cité, affiché au-dessus d'une réponse : nom de l'auteur
 * et extrait tronqué, repli en italique si le message cité est supprimé ou
 * introuvable. Cliquable (saut vers le message d'origine) quand `onJump` est
 * fourni.
 */

import { maskFiltered } from '../lib/automod';
import { useT } from '../stores/ui';
import { displayText, type DisplayMessage } from './messageModel';

interface MessageQuoteProps {
  quoted: DisplayMessage | undefined;
  nameOf: (author: string) => string;
  onJump?: (() => void) | undefined;
  /**
   * Mots filtrés par l'AutoMod du serveur (absent en MP).
   *
   * 🔒 L'aperçu doit masquer comme le message lui-même : répondre à un message
   * filtré le recopiait ici en clair, une ligne au-dessus des `█`. Le mot
   * filtré se lisait donc toujours, il suffisait d'y répondre.
   */
  automodWords?: readonly string[] | undefined;
}

export function MessageQuote({
  quoted,
  nameOf,
  onJump,
  automodWords,
}: MessageQuoteProps) {
  const t = useT();
  // Les replis (« message supprimé », « indisponible ») sont des libellés de
  // l'interface, pas du texte de l'auteur : rien à y masquer.
  const texte = quoted === undefined ? null : displayText(quoted);
  const snippet =
    quoted === undefined
      ? t.dm.quoteUnavailable
      : quoted.deleted
        ? t.dm.deletedMessage
        : texte === null
          ? t.dm.unsupported
          : automodWords !== undefined && automodWords.length > 0
            ? maskFiltered(texte, automodWords)
            : texte;

  const inner = (
    <>
      <span
        aria-hidden
        className="ms-1 h-2 w-6 shrink-0 rounded-tl-md border-s-2 border-t-2 border-input"
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

  // La ligne de citation est cliquable (saut vers le message d'origine) et le
  // texte `text-xs` la laissait à 16 px de haut, sous le plancher de WCAG 2.2
  // SC 2.5.8. `min-h-6` porte la cible à 24 px ; les marges négatives rendent
  // les 6 px gagnés au flux (24 − 4 − 2 = 18 px, l'encombrement d'avant), si
  // bien que la zone saisissable déborde sur la gouttière au-dessus — qui ne
  // porte rien — au lieu de repousser tout le message vers le bas.
  //
  // Posée sur la classe partagée pour que la version non cliquable garde
  // exactement la même géométrie.
  const className =
    '-mt-1 -mb-0.5 flex min-h-6 w-full min-w-0 max-w-full items-center gap-1.5 overflow-hidden text-xs text-muted';
  if (onJump === undefined) return <div className={className}>{inner}</div>;
  return (
    <button
      type="button"
      onClick={onJump}
      className={`${className} rounded-sm text-start hover:text-norm focus-visible:outline-none focus-visible:text-norm focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-chat`}
    >
      {inner}
    </button>
  );
}
