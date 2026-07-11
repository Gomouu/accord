/**
 * Recherche locale (mode accueil) : interroge `search.query` (index HMAC
 * aveugle côté nœud, grammaire `from:`/`in:`/`has:`/`before:`/`after:`) et
 * affiche les résultats par leurs métadonnées (conversation, auteur, date).
 * Un extrait est ajouté quand la conversation est déjà chargée localement.
 * Cliquer un résultat saute au message (fenêtre `history_around` au besoin).
 */

import { useState } from 'react';
import type { SearchQueryHit } from '../lib/api';
import { api } from '../lib/client';
import { formatTimestamp, shortId } from '../lib/format';
import {
  buildHitRows,
  indexMessageText,
  parseSearchChips,
  type SearchChip,
  type SearchHitRow,
} from '../lib/search';
import { useDms } from '../stores/dms';
import { useFriends, displayNameOf } from '../stores/friends';
import { useGroups } from '../stores/groups';
import { useSession } from '../stores/session';
import { useUi, useT, type View } from '../stores/ui';

/** Vue cible (conversation) d'un résultat, pour le saut au message. */
function hitView(hit: SearchQueryHit): View {
  return hit.conversation.type === 'dm'
    ? { kind: 'dm', peer: hit.conversation.peer }
    : {
        kind: 'group',
        groupId: hit.conversation.group_id,
        channelId: hit.conversation.channel_id,
      };
}

function ChipRow({ chips }: { chips: SearchChip[] }) {
  const t = useT();
  return (
    <div
      aria-label={t.search.filters}
      className="mt-1.5 flex flex-wrap gap-1 px-1"
    >
      {chips.map((chip, i) => (
        <span
          key={`${chip.type}:${chip.value}:${i}`}
          className="rounded bg-rail px-1.5 py-0.5 text-[11px] text-muted"
        >
          <span className="font-semibold text-blurple">{chip.type}:</span>
          {chip.value}
        </span>
      ))}
    </div>
  );
}

function HitRow({
  row,
  onOpen,
}: {
  row: SearchHitRow;
  onOpen: (hit: SearchQueryHit) => void;
}) {
  const t = useT();
  const lang = useUi((s) => s.lang);
  const contacts = useFriends((s) => s.contacts);
  const groupStates = useGroups((s) => s.states);
  const self = useSession((s) => s.self);
  const { hit, text } = row;

  let label: string;
  if (hit.conversation.type === 'dm') {
    label = `@${displayNameOf(contacts, hit.conversation.peer)}`;
  } else {
    const state = groupStates[hit.conversation.group_id];
    const channelId = hit.conversation.channel_id;
    const channel = state?.channels.find((c) => c.channel_id === channelId);
    label = `${state?.name ?? shortId(hit.conversation.group_id)} · #${channel?.name ?? shortId(channelId)}`;
  }

  const author =
    self !== null && hit.author === self.pubkey
      ? t.app.you
      : displayNameOf(contacts, hit.author);

  return (
    <button
      type="button"
      onClick={() => onOpen(hit)}
      className="block w-full rounded px-2 py-1.5 text-left transition-colors duration-fast hover:bg-chat-hover"
    >
      <div className="flex items-baseline justify-between gap-2">
        <span className="truncate text-xs font-semibold text-muted">{label}</span>
        <span className="shrink-0 text-[10px] text-faint">
          {formatTimestamp(hit.timestamp, lang)}
        </span>
      </div>
      <div className="truncate text-sm text-norm">
        <span className="text-faint">{author} : </span>
        {text ?? <span className="italic text-faint">{t.search.jumpHint}</span>}
      </div>
    </button>
  );
}

export function SearchBar() {
  const t = useT();
  const requestJump = useUi((s) => s.requestJump);
  const toast = useUi((s) => s.toast);
  const [query, setQuery] = useState('');
  const [busy, setBusy] = useState(false);
  const [rows, setRows] = useState<SearchHitRow[] | null>(null);

  const chips = parseSearchChips(query);

  const clear = (): void => {
    setQuery('');
    setRows(null);
  };

  const submit = async (): Promise<void> => {
    const trimmed = query.trim();
    if (trimmed === '' || busy) return;
    setBusy(true);
    try {
      const { hits } = await api.searchQuery(trimmed);
      const index = indexMessageText(
        useDms.getState().conversations,
        useGroups.getState().messages,
      );
      setRows(buildHitRows(hits, index));
    } catch {
      toast('error', t.errors.loadFailed);
    } finally {
      setBusy(false);
    }
  };

  const open = (hit: SearchQueryHit): void => {
    requestJump(hitView(hit), hit.msg_id);
    clear();
  };

  return (
    <div className="relative border-b border-rail p-2.5">
      <div className="flex items-center gap-1.5 rounded bg-rail px-2">
        <svg
          width="14"
          height="14"
          viewBox="0 0 24 24"
          fill="currentColor"
          aria-hidden
          className="shrink-0 text-faint"
        >
          <path d="M10.5 3a7.5 7.5 0 1 0 4.55 13.46l4.24 4.25a1 1 0 0 0 1.42-1.42l-4.25-4.24A7.5 7.5 0 0 0 10.5 3Zm-5.5 7.5a5.5 5.5 0 1 1 11 0 5.5 5.5 0 0 1-11 0Z" />
        </svg>
        <input
          aria-label={t.search.placeholder}
          placeholder={t.search.placeholder}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') void submit();
            if (e.key === 'Escape') clear();
          }}
          className="min-w-0 flex-1 bg-transparent py-1.5 text-sm text-norm placeholder-faint outline-none"
        />
        {(query !== '' || rows !== null) && (
          <button
            type="button"
            aria-label={t.search.clear}
            title={t.search.clear}
            onClick={clear}
            className="shrink-0 text-faint transition-colors duration-fast hover:text-norm active:scale-90"
          >
            <svg
              width="14"
              height="14"
              viewBox="0 0 24 24"
              fill="currentColor"
              aria-hidden
            >
              <path d="M18.7 6.7a1 1 0 0 0-1.4-1.4L12 10.6 6.7 5.3a1 1 0 0 0-1.4 1.4l5.3 5.3-5.3 5.3a1 1 0 1 0 1.4 1.4l5.3-5.3 5.3 5.3a1 1 0 0 0 1.4-1.4L13.4 12l5.3-5.3Z" />
            </svg>
          </button>
        )}
      </div>
      {chips.length > 0 && <ChipRow chips={chips} />}
      {busy && <p className="px-1 pt-2 text-xs text-faint">{t.app.loading}</p>}
      {rows !== null && !busy && (
        <div className="popover-enter absolute inset-x-2 top-full z-10 mt-1 max-h-96 overflow-y-auto rounded-lg bg-tooltip p-2 shadow-elevation">
          <div className="px-2 pb-1 text-xs font-semibold uppercase tracking-wide text-faint">
            {t.search.results} — {rows.length}
          </div>
          {rows.length === 0 && (
            <p className="px-2 py-2 text-sm text-muted">{t.search.noResults}</p>
          )}
          {rows.map((row) => (
            <HitRow key={row.hit.msg_id} row={row} onOpen={open} />
          ))}
        </div>
      )}
    </div>
  );
}
