import { useEffect, useMemo, useRef, useState } from 'react';
import { interpolate } from '../i18n';
import { bouclerTab } from '../lib/focus';
import { initials } from '../lib/format';
import {
  buildQuickSwitchItems,
  buildRecentItems,
  rankQuickSwitchItems,
  sectionQuickSwitchItems,
  type QuickSwitchCommandIcon,
  type QuickSwitchItem,
  type QuickSwitchSection,
  type QuickSwitchSectionId,
} from '../lib/quickSwitch';
import { useFriends } from '../stores/friends';
import { useGroups } from '../stores/groups';
import { useSession } from '../stores/session';
import { useT, useUi } from '../stores/ui';
import { Avatar } from './Avatar';
import {
  BellOffMenuIcon,
  CheckMenuIcon,
  CopyMenuIcon,
  EnvelopeMenuIcon,
  GearMenuIcon,
  LeaveMenuIcon,
  PlusMenuIcon,
  SearchIcon,
  VoiceDeafenMenuIcon,
} from './ContextMenu';
import { channelToRestore } from './ServerRail';
import { ChannelIcon } from './Sidebar';
import { useQuickSwitchCommands } from './useQuickSwitchCommands';

function StrokeIcon({ children }: { children: React.ReactNode }) {
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
      {children}
    </svg>
  );
}

function FriendsIcon() {
  return (
    <StrokeIcon>
      <path d="M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2" />
      <circle cx="9" cy="7" r="4" />
      <path d="M22 21v-2a4 4 0 0 0-3-3.87" />
      <path d="M16 3.13a4 4 0 0 1 0 7.75" />
    </StrokeIcon>
  );
}

function CommandIcon() {
  return (
    <StrokeIcon>
      <path d="M4 21v-7m0-4V3m8 18v-9m0-4V3m8 18v-5m0-4V3" />
      <path d="M2 14h4m4-6h4m4 8h4" />
    </StrokeIcon>
  );
}

function CalendarIcon() {
  return (
    <StrokeIcon>
      <rect width="18" height="18" x="3" y="4" rx="2" />
      <path d="M16 2v4M8 2v4M3 10h18" />
    </StrokeIcon>
  );
}

function PaletteIcon() {
  return (
    <StrokeIcon>
      <path d="M12 3a9 9 0 1 0 0 18h1.5a1.5 1.5 0 0 0 0-3H12a2 2 0 0 1 0-4h1a8 8 0 0 0 8-8c0-1.7-3.8-3-9-3Z" />
      <circle cx="7.5" cy="9" r=".8" fill="currentColor" stroke="none" />
      <circle cx="11" cy="6.5" r=".8" fill="currentColor" stroke="none" />
      <circle cx="15" cy="7" r=".8" fill="currentColor" stroke="none" />
    </StrokeIcon>
  );
}

function ShieldIcon() {
  return (
    <StrokeIcon>
      <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10Z" />
      <path d="m9 12 2 2 4-4" />
    </StrokeIcon>
  );
}

function HeadphonesIcon() {
  return (
    <StrokeIcon>
      <path d="M4 14v-2a8 8 0 0 1 16 0v2" />
      <path d="M18 19h1a1 1 0 0 0 1-1v-4h-4v4a1 1 0 0 0 1 1h1ZM6 19H5a1 1 0 0 1-1-1v-4h4v4a1 1 0 0 1-1 1H6Z" />
    </StrokeIcon>
  );
}

function BellIcon() {
  return (
    <StrokeIcon>
      <path d="M18 8a6 6 0 0 0-12 0c0 7-3 7-3 9h18c0-2-3-2-3-9" />
      <path d="M10 21h4" />
    </StrokeIcon>
  );
}

function MicrophoneIcon() {
  return (
    <StrokeIcon>
      <rect width="8" height="13" x="8" y="2" rx="4" />
      <path d="M5 10a7 7 0 0 0 14 0M12 19v3" />
    </StrokeIcon>
  );
}

function ServerIcon() {
  return (
    <StrokeIcon>
      <rect width="18" height="7" x="3" y="3" rx="2" />
      <rect width="18" height="7" x="3" y="14" rx="2" />
      <path d="M7 6.5h.01M7 17.5h.01" />
    </StrokeIcon>
  );
}

function StatusIcon() {
  return (
    <StrokeIcon>
      <circle cx="12" cy="12" r="8" />
      <circle cx="12" cy="12" r="3" fill="currentColor" stroke="none" />
    </StrokeIcon>
  );
}

function UnreadIcon() {
  return (
    <StrokeIcon>
      <path d="M18 8a6 6 0 0 0-12 0c0 7-3 7-3 9h18c0-2-3-2-3-9" />
      <path d="M10 21h4" />
      <circle cx="19" cy="5" r="3" fill="currentColor" stroke="none" />
    </StrokeIcon>
  );
}

function CommandGlyph({ icon }: { icon: QuickSwitchCommandIcon | undefined }) {
  switch (icon) {
    case 'appearance':
    case 'theme':
      return <PaletteIcon />;
    case 'calendar':
      return <CalendarIcon />;
    case 'category':
    case 'channel':
      return <PlusMenuIcon />;
    case 'copy':
      return <CopyMenuIcon />;
    case 'deafen':
      return <VoiceDeafenMenuIcon />;
    case 'dm':
    case 'invite':
      return <EnvelopeMenuIcon />;
    case 'headphones':
    case 'voice':
      return <HeadphonesIcon />;
    case 'leave':
      return <LeaveMenuIcon />;
    case 'mark-read':
      return <CheckMenuIcon />;
    case 'microphone':
      return <MicrophoneIcon />;
    case 'muted-channels':
      return <BellOffMenuIcon />;
    case 'notifications':
      return <BellIcon />;
    case 'privacy':
      return <ShieldIcon />;
    case 'server':
      return <ServerIcon />;
    case 'settings':
      return <GearMenuIcon />;
    case 'status':
      return <StatusIcon />;
    case 'unread':
      return <UnreadIcon />;
    default:
      return <CommandIcon />;
  }
}

function optionDomId(itemId: string): string {
  return `quick-switch-option-${itemId.replace(/[^a-zA-Z0-9_-]/g, '-')}`;
}

function sectionDomId(sectionId: QuickSwitchSectionId): string {
  return `quick-switch-section-${sectionId}`;
}

function ServerInitialBadge({ name }: { name: string }) {
  return (
    <span
      aria-hidden
      className="flex h-4 w-4 shrink-0 items-center justify-center rounded-full bg-rail text-[8px] font-semibold leading-none text-faint"
    >
      {initials(name)}
    </span>
  );
}

function ItemIcon({ item }: { item: QuickSwitchItem }) {
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
      <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-input text-xs font-semibold text-faint">
        {initials(item.label)}
      </span>
    );
  }
  const danger = item.kind === 'command' && item.danger === true;
  return (
    <span
      className={`flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-input/80 ${danger ? 'text-red' : 'text-muted'}`}
    >
      {item.kind === 'friends' ? (
        <FriendsIcon />
      ) : item.kind === 'command' ? (
        <CommandGlyph icon={item.icon} />
      ) : (
        <ChannelIcon kind={item.channelKind} />
      )}
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
  registerRef: (element: HTMLDivElement | null) => void;
}) {
  const t = useT();
  const danger = item.kind === 'command' && item.danger === true;
  return (
    <div
      ref={registerRef}
      id={optionDomId(item.id)}
      role="option"
      aria-selected={active}
      tabIndex={-1}
      onMouseEnter={onHover}
      onMouseDown={(event) => event.preventDefault()}
      onClick={() => onSelect(item)}
      className={`group flex min-h-12 w-full cursor-pointer items-center gap-3 rounded-lg px-3 py-2 outline-none transition-transform duration-fast active:scale-[0.99] ${
        active
          ? danger
            ? 'bg-red/15 text-red ring-1 ring-inset ring-red/30'
            : 'bg-blurple/15 text-header ring-1 ring-inset ring-blurple/25'
          : danger
            ? 'text-red'
            : 'text-muted'
      }`}
    >
      <ItemIcon item={item} />
      <span className="min-w-0 flex-1">
        <span className="block truncate text-sm font-medium">{item.label}</span>
        {item.kind === 'server' && (
          <span className="block truncate text-xs text-faint">
            {t.quickSwitch.serverHint}
          </span>
        )}
        {item.kind === 'command' && (
          <span
            className={`block truncate text-xs ${danger ? 'text-red' : 'text-faint'}`}
          >
            {item.subtitle}
          </span>
        )}
        {item.kind === 'channel' && (
          <span className="flex items-center gap-1.5 truncate text-xs text-faint">
            <ServerInitialBadge name={item.subtitle} />
            <span className="truncate">↳ {item.subtitle}</span>
            {item.channelKind === 'voice' && (
              <span> · {t.quickSwitch.voiceChannelHint}</span>
            )}
          </span>
        )}
      </span>
      {item.kind === 'command' && item.shortcut !== undefined && (
        <kbd className="shrink-0 rounded-md border border-rail bg-input px-1.5 py-0.5 font-mono text-[11px] font-medium text-faint">
          {item.shortcut}
        </kbd>
      )}
      {item.kind === 'command' &&
        item.customStatusMode === 'edit' &&
        item.shortcut === undefined && (
          <span aria-hidden className="shrink-0 text-lg leading-none text-faint">
            ›
          </span>
        )}
    </div>
  );
}

function KeyHint({ keys, label }: { keys: string; label: string }) {
  return (
    <span className="flex items-center gap-1.5 whitespace-nowrap">
      <kbd className="rounded-md border border-rail bg-input px-1.5 py-0.5 font-mono text-[10px] font-semibold text-faint">
        {keys}
      </kbd>
      <span className="max-sm:hidden">{label}</span>
    </span>
  );
}

export function QuickSwitcher() {
  const t = useT();
  const open = useUi((state) => state.quickSwitcherOpen);
  const close = useUi((state) => state.closeQuickSwitcher);
  const setView = useUi((state) => state.setView);
  const toast = useUi((state) => state.toast);
  const modal = useUi((state) => state.modal);
  const lastChannelByServer = useUi((state) => state.lastChannelByServer);
  const lastDmPeer = useUi((state) => state.lastDmPeer);
  const contacts = useFriends((state) => state.contacts);
  const ownStatus = useFriends((state) => state.ownStatus);
  const ownStatusText = useFriends((state) => state.ownStatusText);
  const setOwnStatus = useFriends((state) => state.setOwnStatus);
  const groupIds = useGroups((state) => state.ids);
  const groupStates = useGroups((state) => state.states);
  const self = useSession((state) => state.self);
  const [query, setQuery] = useState('');
  const [customStatusMode, setCustomStatusMode] = useState(false);
  const [customStatusDraft, setCustomStatusDraft] = useState('');
  const [activeIndex, setActiveIndex] = useState(0);
  const [rendered, setRendered] = useState(open);
  const [exiting, setExiting] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const dialogRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLElement | null>(null);
  const optionRefs = useRef<Array<HTMLDivElement | null>>([]);

  const navItems = useMemo(
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
  const commandItems = useQuickSwitchCommands(navItems);
  const allItems = useMemo(
    () => [...navItems, ...commandItems],
    [navItems, commandItems],
  );
  const recentItems = useMemo(
    () => buildRecentItems(navItems, groupIds, lastChannelByServer, lastDmPeer),
    [navItems, groupIds, lastChannelByServer, lastDmPeer],
  );
  const recentIds = useMemo(
    () => new Set(recentItems.map((item) => item.id)),
    [recentItems],
  );
  const trimmed = query.trim();
  const sections = useMemo<QuickSwitchSection[]>(() => {
    if (trimmed === '') {
      const recent = sectionQuickSwitchItems(recentItems, true);
      const featured = commandItems.filter((item) => item.featured === true);
      return [...recent, ...sectionQuickSwitchItems(featured)];
    }
    const ranked = rankQuickSwitchItems(allItems, trimmed);
    const matchedRecent = ranked.filter((item) => recentIds.has(item.id));
    const remaining = ranked.filter((item) => !recentIds.has(item.id));
    return [
      ...sectionQuickSwitchItems(matchedRecent, true),
      ...sectionQuickSwitchItems(remaining),
    ];
  }, [trimmed, recentItems, commandItems, allItems, recentIds]);
  const results = useMemo(() => sections.flatMap((section) => section.items), [sections]);
  const resultIndexes = useMemo(
    () => new Map(results.map((item, index) => [item.id, index] as const)),
    [results],
  );
  const sectionStarts = useMemo(() => {
    let offset = 0;
    return sections.map((section) => {
      const start = offset;
      offset += section.items.length;
      return start;
    });
  }, [sections]);
  const resultSignature = results.map((item) => item.id).join('\u0000');

  useEffect(() => {
    if (open) {
      triggerRef.current =
        document.activeElement instanceof HTMLElement ? document.activeElement : null;
      setRendered(true);
      setExiting(false);
      setQuery('');
      setCustomStatusMode(false);
      setActiveIndex(0);
      return;
    }
    if (!rendered) return;
    setExiting(true);
    const reduced =
      document.documentElement.dataset.motion === 'reduce' ||
      window.matchMedia?.('(prefers-reduced-motion: reduce)').matches === true;
    const timeout = window.setTimeout(
      () => {
        setRendered(false);
        setExiting(false);
      },
      reduced ? 0 : 170,
    );
    return () => window.clearTimeout(timeout);
  }, [open, rendered]);

  useEffect(() => {
    if (open || rendered) return;
    if (
      useUi.getState().modal === null &&
      triggerRef.current !== null &&
      triggerRef.current.isConnected &&
      document.activeElement === document.body
    ) {
      triggerRef.current.focus();
    }
  }, [open, rendered]);

  useEffect(() => {
    if (open && rendered) inputRef.current?.focus();
  }, [open, rendered, customStatusMode]);

  useEffect(() => {
    setActiveIndex(0);
  }, [resultSignature]);

  useEffect(() => {
    optionRefs.current[activeIndex]?.scrollIntoView?.({ block: 'nearest' });
  }, [activeIndex]);

  if (!rendered) return null;

  const select = (item: QuickSwitchItem): void => {
    if (item.kind === 'command') {
      if (item.customStatusMode === 'edit') {
        setCustomStatusDraft(ownStatusText ?? '');
        setCustomStatusMode(true);
        return;
      }
      item.run();
      close();
      return;
    }
    if (
      item.kind === 'server' ||
      (item.kind === 'channel' && item.channelKind === 'voice')
    ) {
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
    setActiveIndex((index) => (index + delta + results.length) % results.length);
  };

  const jumpSection = (delta: number): void => {
    if (sectionStarts.length === 0) return;
    let currentSection = 0;
    for (let index = 0; index < sectionStarts.length; index += 1) {
      const start = sectionStarts[index];
      if (start !== undefined && start <= activeIndex) currentSection = index;
    }
    const next = (currentSection + delta + sectionStarts.length) % sectionStarts.length;
    const nextIndex = sectionStarts[next];
    if (nextIndex !== undefined) setActiveIndex(nextIndex);
  };

  const submitCustomStatus = (): void => {
    void setOwnStatus(ownStatus, customStatusDraft.trim()).catch(() =>
      toast('error', t.errors.actionFailed),
    );
    close();
  };

  const onKeyDown = (event: React.KeyboardEvent<HTMLInputElement>): void => {
    if (customStatusMode) {
      if (event.key === 'Enter') {
        event.preventDefault();
        submitCustomStatus();
      }
      return;
    }
    if (event.key === 'ArrowDown') {
      event.preventDefault();
      move(1);
    } else if (event.key === 'ArrowUp') {
      event.preventDefault();
      move(-1);
    } else if (event.key === 'Home') {
      event.preventDefault();
      setActiveIndex(0);
    } else if (event.key === 'End') {
      event.preventDefault();
      setActiveIndex(Math.max(0, results.length - 1));
    } else if (event.key === 'Tab') {
      event.preventDefault();
      jumpSection(event.shiftKey ? -1 : 1);
    } else if (event.key === 'Enter') {
      event.preventDefault();
      const item = results[activeIndex];
      if (item !== undefined) select(item);
    }
  };

  const onDialogKeyDown = (event: React.KeyboardEvent<HTMLDivElement>): void => {
    if (event.key === 'Escape') {
      event.preventDefault();
      if (customStatusMode) setCustomStatusMode(false);
      else close();
    } else if (customStatusMode && event.key === 'Tab') {
      bouclerTab(event, dialogRef.current);
    }
  };

  const sectionLabels: Record<QuickSwitchSectionId, string> = {
    recent: t.quickSwitch.recent,
    channels: t.quickSwitch.sectionChannels,
    dms: t.quickSwitch.sectionDms,
    servers: t.quickSwitch.sectionServers,
    commands: t.quickSwitch.sectionCommands,
  };
  const activeItem = results[activeIndex];
  const inputValue = customStatusMode ? customStatusDraft : query;
  const yieldingToModal = exiting && modal !== null;

  return (
    <div
      aria-hidden={yieldingToModal || undefined}
      className={`${exiting ? 'modal-overlay-exit pointer-events-none' : 'modal-overlay-enter'} fixed inset-0 z-50 flex items-start justify-center bg-black/60 px-4 pt-[14vh] backdrop-blur-sm`}
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) close();
      }}
    >
      <div
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-label={t.quickSwitch.title}
        onKeyDown={onDialogKeyDown}
        className={`glass-strong ${exiting ? 'modal-panel-exit' : 'modal-panel-enter'} flex max-h-[72vh] w-[640px] max-w-full flex-col overflow-hidden rounded-2xl shadow-3`}
      >
        <div className="flex min-h-14 items-center gap-3 border-b border-input/70 px-4">
          <span
            aria-hidden
            className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-input/70 text-faint"
          >
            {customStatusMode ? <StatusIcon /> : <SearchIcon size={17} />}
          </span>
          <input
            ref={inputRef}
            role={customStatusMode ? undefined : 'combobox'}
            aria-expanded={customStatusMode ? undefined : true}
            aria-controls={customStatusMode ? undefined : 'quick-switch-listbox'}
            aria-autocomplete={customStatusMode ? undefined : 'list'}
            aria-activedescendant={
              !customStatusMode && activeItem !== undefined
                ? optionDomId(activeItem.id)
                : undefined
            }
            aria-label={
              customStatusMode
                ? t.quickSwitch.customStatusPlaceholder
                : t.quickSwitch.placeholder
            }
            placeholder={
              customStatusMode
                ? t.quickSwitch.customStatusPlaceholder
                : t.quickSwitch.placeholder
            }
            value={inputValue}
            maxLength={customStatusMode ? 128 : undefined}
            onChange={(event) => {
              if (customStatusMode) setCustomStatusDraft(event.target.value);
              else setQuery(event.target.value);
            }}
            onKeyDown={onKeyDown}
            className="min-w-0 flex-1 bg-transparent py-4 text-base text-norm placeholder-faint outline-none"
          />
          {!customStatusMode && (
            <kbd className="shrink-0 rounded-md border border-rail bg-input px-1.5 py-0.5 font-mono text-[10px] font-semibold uppercase text-faint">
              Esc
            </kbd>
          )}
        </div>
        <div role="status" aria-live="polite" className="sr-only">
          {results.length === 0
            ? t.quickSwitch.noResults
            : interpolate(t.quickSwitch.resultCount, {
                count: String(results.length),
              })}
        </div>
        {customStatusMode ? (
          <div className="min-h-0 flex-1 p-3">
            <button
              type="button"
              onClick={submitCustomStatus}
              className="flex min-h-14 w-full items-center gap-3 rounded-xl bg-blurple/15 px-3 text-left text-header ring-1 ring-inset ring-blurple/25 outline-none transition-transform duration-fast active:scale-[0.99] focus-visible:ring-2 focus-visible:ring-blurple"
            >
              <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-input text-blurple">
                <StatusIcon />
              </span>
              <span className="min-w-0 flex-1">
                <span className="block truncate text-sm font-medium">
                  {t.quickSwitch.commandSetCustomStatus}
                </span>
                <span className="block truncate text-xs text-faint">
                  {customStatusDraft.trim() || t.quickSwitch.customStatusPlaceholder}
                </span>
              </span>
              <kbd className="rounded-md border border-rail bg-input px-1.5 py-0.5 font-mono text-[10px] font-semibold text-faint">
                ↵
              </kbd>
            </button>
          </div>
        ) : (
          <div
            id="quick-switch-listbox"
            role="listbox"
            aria-label={t.quickSwitch.title}
            className="min-h-0 flex-1 overflow-y-auto overscroll-contain p-2"
          >
            {results.length === 0 && (
              <div className="flex flex-col items-center px-6 py-12 text-center">
                <span className="mb-3 flex h-11 w-11 items-center justify-center rounded-xl bg-input text-faint">
                  <SearchIcon size={20} />
                </span>
                <p className="text-sm font-medium text-norm">{t.quickSwitch.noResults}</p>
                <p className="mt-1 text-xs text-faint">{t.quickSwitch.noResultsHint}</p>
              </div>
            )}
            {sections.map((section, sectionIndex) => (
              <div
                key={section.id}
                role="group"
                aria-labelledby={sectionDomId(section.id)}
                className={sectionIndex === 0 ? '' : 'mt-2'}
              >
                <div
                  id={sectionDomId(section.id)}
                  className="sticky top-0 z-10 bg-modal/95 px-3 pb-1.5 pt-2 text-[11px] font-semibold uppercase tracking-[0.08em] text-faint"
                >
                  {sectionLabels[section.id]}
                </div>
                <div className="space-y-0.5">
                  {section.items.map((item) => {
                    const index = resultIndexes.get(item.id) ?? 0;
                    return (
                      <ResultRow
                        key={item.id}
                        item={item}
                        active={index === activeIndex}
                        onSelect={select}
                        onHover={() => setActiveIndex(index)}
                        registerRef={(element) => {
                          optionRefs.current[index] = element;
                        }}
                      />
                    );
                  })}
                </div>
              </div>
            ))}
          </div>
        )}
        <div className="flex min-h-10 flex-wrap items-center justify-end gap-x-4 gap-y-1 border-t border-input/70 px-4 py-1 text-[11px] text-faint">
          {!customStatusMode && (
            <KeyHint keys="↑↓" label={t.quickSwitch.keyboardNavigate} />
          )}
          <KeyHint keys="↵" label={t.quickSwitch.keyboardRun} />
          <KeyHint
            keys="Esc"
            label={
              customStatusMode ? t.quickSwitch.keyboardBack : t.quickSwitch.keyboardClose
            }
          />
        </div>
      </div>
    </div>
  );
}
