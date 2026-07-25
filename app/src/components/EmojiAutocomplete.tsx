/**
 * Popup d'autocomplétion émoji du composeur (`:requete`). Présentationnel,
 * miroir de `MentionAutocomplete` : rend les suggestions et remonte
 * clics/survols — le composeur garde le caret, le clavier et l'insertion
 * (voir MessageInput).
 */

import type { EmojiSuggestion } from '../lib/emojiSuggest';
import { jetonEmojiTexte } from '../lib/emoji';
import { useT } from '../stores/ui';
import { CustomEmoji } from './CustomEmoji';

interface EmojiAutocompleteProps {
  suggestions: readonly EmojiSuggestion[];
  /** Index de la suggestion surlignée (borné par l'appelant). */
  activeIndex: number;
  onSelect: (suggestion: EmojiSuggestion) => void;
  onHover: (index: number) => void;
}

/** Clé stable d'une suggestion (les deux espaces de noms sont disjoints). */
function cle(s: EmojiSuggestion): string {
  return s.kind === 'unicode' ? `u:${s.char}` : `c:${s.name}`;
}

export function EmojiAutocomplete({
  suggestions,
  activeIndex,
  onSelect,
  onHover,
}: EmojiAutocompleteProps) {
  const t = useT();
  if (suggestions.length === 0) return null;
  return (
    <div
      role="listbox"
      aria-label={t.emoji.autocompleteLabel}
      className="popover-enter glass-strong absolute bottom-full start-0 z-20 mb-1 max-h-56 w-72 overflow-y-auto rounded-lg p-1"
    >
      {suggestions.map((suggestion, i) => {
        const selected = i === activeIndex;
        return (
          <button
            key={cle(suggestion)}
            type="button"
            role="option"
            aria-selected={selected}
            // `mousedown` (pas `click`) : le textarea garde focus et caret
            // pendant l'insertion, comme pour les mentions.
            onMouseDown={(e) => {
              e.preventDefault();
              onSelect(suggestion);
            }}
            onMouseEnter={() => onHover(i)}
            className={`flex h-9 w-full items-center gap-2 rounded-md px-2 text-start transition-colors duration-fast focus-visible:bg-chat-hover focus-visible:outline-none ${
              selected ? 'bg-blurple/15 text-header' : 'text-muted'
            }`}
          >
            <span
              aria-hidden
              className="flex h-6 w-6 shrink-0 items-center justify-center text-lg leading-none"
            >
              {suggestion.kind === 'unicode' ? (
                suggestion.char
              ) : (
                <CustomEmoji
                  name={suggestion.name}
                  merkleRoot={suggestion.merkleRoot}
                  size={22}
                />
              )}
            </span>
            <span className="min-w-0 flex-1 truncate text-sm font-medium">
              {jetonEmojiTexte(suggestion.name)}
            </span>
          </button>
        );
      })}
    </div>
  );
}
