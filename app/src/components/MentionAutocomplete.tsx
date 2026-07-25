/**
 * Autocomplete popup for @mentions in the composer. Presentational: it renders
 * the filtered candidates and reports clicks/hovers. The composer owns the
 * caret, keyboard navigation and text insertion (see MessageInput).
 */

import { roleColorCss } from '../stores/groups';
import { useT } from '../stores/ui';
import type { MentionCandidate } from '../lib/mentions';

/** Short descriptor shown at the trailing edge of a suggestion. */
function describe(
  candidate: MentionCandidate,
  t: ReturnType<typeof useT>,
): string | null {
  switch (candidate.kind) {
    case 'everyone':
      return t.mentions.everyone;
    case 'here':
      return t.mentions.here;
    case 'role':
      return t.mentions.roleTag;
    case 'member':
      return null;
  }
}

/** Leading glyph: member initial, role colour dot, or a broadcast '@'. */
function MentionIcon({ candidate }: { candidate: MentionCandidate }) {
  if (candidate.kind === 'role') {
    const color =
      candidate.color !== undefined && candidate.color !== 0
        ? roleColorCss(candidate.color)
        : 'rgb(var(--color-faint))';
    return (
      <span
        aria-hidden
        className="h-3 w-3 shrink-0 rounded-full"
        style={{ backgroundColor: color }}
      />
    );
  }
  if (candidate.kind === 'member') {
    const initial = candidate.label.replace('@', '').trim().charAt(0) || '?';
    return (
      <span
        aria-hidden
        className="flex h-6 w-6 shrink-0 items-center justify-center rounded-full bg-rail text-xs font-semibold uppercase text-muted"
      >
        {initial}
      </span>
    );
  }
  return (
    <span
      aria-hidden
      className="flex h-6 w-6 shrink-0 items-center justify-center rounded-full bg-blurple/20 text-sm font-semibold text-blurple"
    >
      @
    </span>
  );
}

interface MentionAutocompleteProps {
  candidates: readonly MentionCandidate[];
  /** Index of the highlighted candidate (kept in range by the caller). */
  activeIndex: number;
  onSelect: (candidate: MentionCandidate) => void;
  onHover: (index: number) => void;
}

export function MentionAutocomplete({
  candidates,
  activeIndex,
  onSelect,
  onHover,
}: MentionAutocompleteProps) {
  const t = useT();
  if (candidates.length === 0) return null;
  return (
    <div
      role="listbox"
      aria-label={t.mentions.autocompleteLabel}
      className="popover-enter glass-strong absolute bottom-full start-0 z-20 mb-1 max-h-56 w-72 overflow-y-auto rounded-lg p-1"
    >
      {candidates.map((candidate, i) => {
        const selected = i === activeIndex;
        const desc = describe(candidate, t);
        return (
          <button
            key={candidate.id}
            type="button"
            role="option"
            aria-selected={selected}
            // `mousedown` (not `click`) so the textarea keeps focus and its
            // caret while the mention is inserted.
            onMouseDown={(e) => {
              e.preventDefault();
              onSelect(candidate);
            }}
            onMouseEnter={() => onHover(i)}
            className={`flex h-9 w-full items-center gap-2 rounded-md px-2 text-start transition-colors duration-fast focus-visible:bg-chat-hover focus-visible:outline-none ${
              selected ? 'bg-blurple/15 text-header' : 'text-muted'
            }`}
          >
            <MentionIcon candidate={candidate} />
            <span className="min-w-0 flex-1 truncate text-sm font-medium">
              {candidate.label}
            </span>
            {desc !== null && <span className="shrink-0 text-xs text-faint">{desc}</span>}
          </button>
        );
      })}
    </div>
  );
}
