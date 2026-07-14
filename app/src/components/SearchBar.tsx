/**
 * Recherche locale (mode accueil) : interroge `search.query` (index HMAC
 * aveugle côté nœud, grammaire `from:`/`in:`/`has:`/`before:`/`after:`) et
 * affiche les résultats par leurs métadonnées (conversation, auteur, date).
 * Un extrait est ajouté quand la conversation est déjà chargée localement.
 * Cliquer un résultat saute au message (fenêtre `history_around` au besoin).
 */

import { useRef, useState } from 'react';
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
import { CloseIcon, SearchIcon } from './ContextMenu';

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
    <div aria-label={t.search.filters} className="mt-1.5 flex flex-wrap gap-1 px-1">
      {chips.map((chip, i) => (
        <span
          key={`${chip.type}:${chip.value}:${i}`}
          className="flex max-w-full items-center rounded-xs bg-rail px-1.5 py-0.5 text-[11px] text-muted"
          title={`${chip.type}:${chip.value}`}
        >
          <span className="shrink-0 font-medium text-blurple">{chip.type}:</span>
          <span className="min-w-0 truncate">{chip.value}</span>
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
  const timeFormat = useUi((s) => s.timeFormat);
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
      className="block w-full rounded-md px-2 py-1.5 text-left transition-colors duration-fast hover:bg-chat-hover focus-visible:bg-chat-hover focus-visible:outline-none"
    >
      <div className="flex items-baseline justify-between gap-2">
        <span className="truncate text-xs font-medium text-muted">{label}</span>
        <span className="shrink-0 text-[10px] text-faint">
          {formatTimestamp(hit.timestamp, lang, undefined, timeFormat)}
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
  /** Identifie la seule requête encore autorisée à modifier l'interface. */
  const requestIdRef = useRef(0);

  const chips = parseSearchChips(query);

  const clear = (): void => {
    requestIdRef.current += 1;
    setQuery('');
    setBusy(false);
    setRows(null);
  };

  /** Toute modification invalide la réponse d'une recherche déjà partie. */
  const changeQuery = (value: string): void => {
    requestIdRef.current += 1;
    setQuery(value);
    setBusy(false);
    setRows(null);
  };

  const submit = async (): Promise<void> => {
    const trimmed = query.trim();
    if (trimmed === '' || busy) return;
    const requestId = requestIdRef.current + 1;
    requestIdRef.current = requestId;
    setBusy(true);
    try {
      const { hits } = await api.searchQuery(trimmed);
      if (requestIdRef.current !== requestId) return;
      const index = indexMessageText(
        useDms.getState().conversations,
        useGroups.getState().messages,
      );
      setRows(buildHitRows(hits, index));
    } catch {
      if (requestIdRef.current === requestId) toast('error', t.errors.loadFailed);
    } finally {
      if (requestIdRef.current === requestId) setBusy(false);
    }
  };

  const open = (hit: SearchQueryHit): void => {
    requestJump(hitView(hit), hit.msg_id);
    clear();
  };

  return (
    <div className="relative border-b border-input/50 p-2.5">
      <div className="flex items-center gap-1.5 rounded-xl border border-transparent bg-input px-2.5 transition-colors duration-fast focus-within:border-blurple/50">
        <span
          aria-hidden
          className="flex h-4 w-4 shrink-0 items-center justify-center text-faint"
        >
          <SearchIcon size={14} />
        </span>
        <input
          aria-label={t.search.placeholder}
          placeholder={t.search.placeholder}
          value={query}
          onChange={(e) => changeQuery(e.target.value)}
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
            className="flex h-8 w-8 shrink-0 items-center justify-center rounded-full text-faint transition-colors duration-fast hover:bg-chat-hover hover:text-norm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar active:scale-90"
          >
            <CloseIcon size={14} />
          </button>
        )}
      </div>
      {chips.length > 0 && <ChipRow chips={chips} />}
      {busy && <p className="px-1 pt-2 text-xs text-faint">{t.app.loading}</p>}
      {rows !== null && !busy && (
        <div
          className="glass-strong popover-enter absolute inset-x-2 top-full z-10 mt-1 flex flex-col overflow-hidden rounded-lg"
          style={{ maxHeight: 'min(24rem, calc(100vh - 7rem))' }}
        >
          <div className="min-h-0 overflow-y-auto p-2">
            <div className="px-2 pb-1 text-xs font-medium uppercase tracking-wide text-faint">
              {t.search.results} — {rows.length}
            </div>
            {rows.length === 0 && (
              <p className="px-2 py-2 text-sm text-muted">{t.search.noResults}</p>
            )}
            {rows.map((row) => (
              <HitRow key={row.hit.msg_id} row={row} onOpen={open} />
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
