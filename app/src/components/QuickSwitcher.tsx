/**
 * Sélecteur rapide (Ctrl/Cmd+K) : palette centrée façon Discord pour sauter
 * vers la vue Amis, une conversation privée, un serveur ou un salon sans
 * quitter le clavier. Classement flou pur (`lib/quickSwitch.ts`) ; sans
 * texte, propose les dernières destinations visitées. Sélectionner un salon
 * vocal ne le rejoint jamais (rejoindre reste un clic explicite dans la
 * barre latérale) : on navigue vers le serveur, comme un clic sur son icône
 * dans le rail (`ServerRail.channelToRestore`).
 */

import { useEffect, useMemo, useRef, useState } from 'react';
import { initials } from '../lib/format';
import {
  buildQuickSwitchItems,
  buildRecentItems,
  rankQuickSwitchItems,
  type QuickSwitchItem,
} from '../lib/quickSwitch';
import { useFriends } from '../stores/friends';
import { useGroups } from '../stores/groups';
import { useSession } from '../stores/session';
import { useUi, useT } from '../stores/ui';
import { Avatar } from './Avatar';
import { SearchIcon } from './ContextMenu';
import { channelToRestore } from './ServerRail';
import { ChannelIcon } from './Sidebar';

/** Icône « Amis », identique à celle du bouton dédié de la barre latérale. */
function FriendsIcon() {
  return (
    <svg
      width="16"
      height="16"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
    >
      <path d="M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2" />
      <circle cx="9" cy="7" r="4" />
      <path d="M22 21v-2a4 4 0 0 0-3-3.87" />
      <path d="M16 3.13a4 4 0 0 1 0 7.75" />
    </svg>
  );
}

/** Identifiant DOM d'une option, dérivé de son id logique (voir `lib/quickSwitch`). */
function optionDomId(itemId: string): string {
  return `quick-switch-option-${itemId.replace(/[^a-zA-Z0-9_-]/g, '-')}`;
}

/** Pastille ronde d'initiales de serveur (voir « server initial » du contrat produit). */
function ServerInitialBadge({ name }: { name: string }) {
  return (
    <span
      aria-hidden
      className="flex h-3.5 w-3.5 shrink-0 items-center justify-center rounded-full bg-rail text-[8px] font-semibold leading-none text-faint"
    >
      {initials(name)}
    </span>
  );
}

function ItemIcon({ item }: { item: QuickSwitchItem }) {
  if (item.kind === 'friends') {
    return (
      <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-full bg-input text-muted">
        <FriendsIcon />
      </span>
    );
  }
  if (item.kind === 'dm') {
    return (
      <Avatar
        id={item.pubkey}
        name={item.label}
        size={32}
        avatarHash={item.avatarHash}
        hint={item.pubkey}
        decoration={item.avatarDecoration}
      />
    );
  }
  if (item.kind === 'server') {
    return (
      <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-full bg-input text-xs font-semibold text-faint">
        {initials(item.label)}
      </span>
    );
  }
  return (
    <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-full bg-input text-faint">
      <ChannelIcon kind={item.channelKind} />
    </span>
  );
}

function ResultRow({
  item,
  active,
  onSelect,
  onHover,
  registerRef,
}: {
  item: QuickSwitchItem;
  active: boolean;
  onSelect: (item: QuickSwitchItem) => void;
  onHover: () => void;
  registerRef: (el: HTMLDivElement | null) => void;
}) {
  const t = useT();
  return (
    <div
      ref={registerRef}
      id={optionDomId(item.id)}
      role="option"
      aria-selected={active}
      tabIndex={-1}
      onMouseEnter={onHover}
      onMouseDown={(e) => e.preventDefault()}
      onClick={() => onSelect(item)}
      className={`flex h-11 w-full cursor-pointer items-center gap-2.5 rounded-md px-2 transition-colors duration-fast ${
        active
          ? 'bg-blurple/15 text-header ring-1 ring-inset ring-blurple/20'
          : 'text-muted'
      }`}
    >
      <ItemIcon item={item} />
      <span className="min-w-0 flex-1">
        <span className="block truncate font-medium">{item.label}</span>
        {item.kind === 'server' && (
          <span className="block truncate text-xs text-faint">
            {t.quickSwitch.serverHint}
          </span>
        )}
        {item.kind === 'channel' && (
          <span className="flex items-center gap-1 truncate text-xs text-faint">
            <ServerInitialBadge name={item.subtitle} />
            <span className="truncate">{item.subtitle}</span>
            {item.channelKind === 'voice' && (
              <span> · {t.quickSwitch.voiceChannelHint}</span>
            )}
          </span>
        )}
      </span>
    </div>
  );
}

export function QuickSwitcher() {
  const t = useT();
  const open = useUi((s) => s.quickSwitcherOpen);
  const close = useUi((s) => s.closeQuickSwitcher);
  const setView = useUi((s) => s.setView);
  const lastChannelByServer = useUi((s) => s.lastChannelByServer);
  const lastDmPeer = useUi((s) => s.lastDmPeer);
  const contacts = useFriends((s) => s.contacts);
  const groupIds = useGroups((s) => s.ids);
  const groupStates = useGroups((s) => s.states);
  const self = useSession((s) => s.self);

  const [query, setQuery] = useState('');
  const [activeIndex, setActiveIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const optionRefs = useRef<Array<HTMLDivElement | null>>([]);

  const items = useMemo(
    () =>
      buildQuickSwitchItems({
        friendsLabel: t.friends.title,
        contacts,
        groupIds,
        groupStates,
        selfPubkey: self?.pubkey ?? null,
      }),
    [t, contacts, groupIds, groupStates, self],
  );

  const trimmed = query.trim();
  const results = useMemo(
    () =>
      trimmed === ''
        ? buildRecentItems(items, groupIds, lastChannelByServer, lastDmPeer)
        : rankQuickSwitchItems(items, trimmed),
    [items, trimmed, groupIds, lastChannelByServer, lastDmPeer],
  );

  // Réinitialise la requête et le focus à chaque ouverture ; à la fermeture,
  // rend le focus à l'élément qui l'avait (déclencheur du raccourci).
  useEffect(() => {
    if (!open) return;
    const trigger =
      document.activeElement instanceof HTMLElement ? document.activeElement : null;
    setQuery('');
    setActiveIndex(0);
    inputRef.current?.focus();
    return () => trigger?.focus();
  }, [open]);

  // Le curseur virtuel revient en tête à chaque nouveau jeu de résultats.
  useEffect(() => {
    setActiveIndex(0);
  }, [trimmed, results.length]);

  useEffect(() => {
    // `scrollIntoView` est absent de jsdom (tests) : appel optionnel.
    optionRefs.current[activeIndex]?.scrollIntoView?.({ block: 'nearest' });
  }, [activeIndex]);

  if (!open) return null;

  const select = (item: QuickSwitchItem): void => {
    if (
      item.kind === 'server' ||
      (item.kind === 'channel' && item.channelKind === 'voice')
    ) {
      // Serveur : même destination qu'un clic sur son icône dans le rail
      // (dernier salon consulté). Salon vocal : ne jamais rejoindre la voix
      // depuis le sélecteur, on navigue vers le serveur de la même façon.
      setView({
        kind: 'group',
        groupId: item.groupId,
        channelId: channelToRestore(
          groupStates[item.groupId],
          lastChannelByServer[item.groupId],
        ),
      });
    } else {
      setView(item.view);
    }
    close();
  };

  const move = (delta: number): void => {
    if (results.length === 0) return;
    setActiveIndex((i) => (i + delta + results.length) % results.length);
  };

  const onKeyDown = (e: React.KeyboardEvent<HTMLInputElement>): void => {
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      move(1);
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      move(-1);
    } else if (e.key === 'Enter') {
      e.preventDefault();
      const item = results[activeIndex];
      if (item !== undefined) select(item);
    } else if (e.key === 'Escape') {
      e.preventDefault();
      close();
    } else if (e.key === 'Tab') {
      // Focus toujours gardé sur le champ (voir combobox ARIA ci-dessous).
      e.preventDefault();
    }
  };

  const activeItem = results[activeIndex];

  return (
    <div
      className="modal-overlay-enter fixed inset-0 z-50 flex items-start justify-center bg-black/60 pt-[22vh] backdrop-blur-sm"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) close();
      }}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label={t.quickSwitch.title}
        className="glass-strong modal-panel-enter flex max-h-[60vh] w-[560px] max-w-[92vw] flex-col overflow-hidden rounded-xl shadow-3"
      >
        <div className="flex items-center gap-2 border-b border-transparent px-3 focus-within:border-blurple/50">
          <span
            aria-hidden
            className="flex h-4 w-4 shrink-0 items-center justify-center text-faint"
          >
            <SearchIcon size={16} />
          </span>
          <input
            ref={inputRef}
            role="combobox"
            aria-expanded="true"
            aria-controls="quick-switch-listbox"
            aria-autocomplete="list"
            aria-activedescendant={
              activeItem !== undefined ? optionDomId(activeItem.id) : undefined
            }
            aria-label={t.quickSwitch.placeholder}
            placeholder={t.quickSwitch.placeholder}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={onKeyDown}
            className="min-w-0 flex-1 bg-transparent py-3 text-sm text-norm placeholder-faint outline-none"
          />
        </div>
        <div
          id="quick-switch-listbox"
          role="listbox"
          aria-label={t.quickSwitch.title}
          className="min-h-0 flex-1 overflow-y-auto p-2"
        >
          {trimmed === '' && results.length > 0 && (
            <div className="px-2 pb-1 pt-1 text-[11px] font-medium uppercase tracking-wide text-faint">
              {t.quickSwitch.recent}
            </div>
          )}
          {results.length === 0 && (
            <p className="px-2 py-6 text-center text-sm text-muted">
              {t.quickSwitch.noResults}
            </p>
          )}
          {results.map((item, i) => (
            <ResultRow
              key={item.id}
              item={item}
              active={i === activeIndex}
              onSelect={select}
              onHover={() => setActiveIndex(i)}
              registerRef={(el) => {
                optionRefs.current[i] = el;
              }}
            />
          ))}
        </div>
      </div>
    </div>
  );
}
