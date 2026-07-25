/**
 * Disappearing-message timer picker (E2). Local-only setting: messages older
 * than the chosen duration are deleted from THIS device's encrypted store —
 * no wire negotiation, the other side keeps its own copy.
 *
 * Two variants: `header` (icon button + small popover, for the DM header)
 * and `section` (labeled pill row, for server settings).
 */

import { useEffect, useRef, useState } from 'react';
import { api } from '../lib/client';
import { useT, useUi } from '../stores/ui';

/** Conversation the timer applies to. */
export type EphemeralScope =
  { kind: 'dm'; peer: string } | { kind: 'group'; groupId: string };

/** Offered durations (seconds); `null` disables the timer. */
const CHOICES: (number | null)[] = [
  null,
  3600,
  8 * 3600,
  86_400,
  7 * 86_400,
  90 * 86_400,
];

function labelOf(t: ReturnType<typeof useT>, ttl: number | null): string {
  switch (ttl) {
    case 3600:
      return t.dm.ephemeral1h;
    case 8 * 3600:
      return t.dm.ephemeral8h;
    case 86_400:
      return t.dm.ephemeral1d;
    case 7 * 86_400:
      return t.dm.ephemeral7d;
    case 90 * 86_400:
      return t.dm.ephemeral90d;
    default:
      return t.dm.ephemeralOff;
  }
}

function fetchTtl(scope: EphemeralScope): Promise<{ ttl_secs: number | null }> {
  return scope.kind === 'dm'
    ? api.dmGetEphemeral(scope.peer)
    : api.groupsGetEphemeral(scope.groupId);
}

function storeTtl(scope: EphemeralScope, ttl: number | null): Promise<{ ok: true }> {
  return scope.kind === 'dm'
    ? api.dmSetEphemeral(scope.peer, ttl)
    : api.groupsSetEphemeral(scope.groupId, ttl);
}

/** Hourglass glyph (stroke follows the current color). */
function TimerIcon({ size = 18 }: { size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
    >
      <path d="M5 22h14" />
      <path d="M5 2h14" />
      <path d="M17 22v-4.172a2 2 0 0 0-.586-1.414L12 12l-4.414 4.414A2 2 0 0 0 7 17.828V22" />
      <path d="M7 2v4.172a2 2 0 0 0 .586 1.414L12 12l4.414-4.414A2 2 0 0 0 17 6.172V2" />
    </svg>
  );
}

export function EphemeralPicker({
  scope,
  variant,
}: {
  scope: EphemeralScope;
  variant: 'header' | 'section';
}) {
  const t = useT();
  const toast = useUi((s) => s.toast);
  /** Current timer; `undefined` while loading, `null` = disabled. */
  const [ttl, setTtl] = useState<number | null | undefined>(undefined);
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);
  const scopeKey = scope.kind === 'dm' ? scope.peer : scope.groupId;

  useEffect(() => {
    setTtl(undefined);
    setOpen(false);
    let alive = true;
    fetchTtl(scope)
      .then(({ ttl_secs }) => {
        if (alive) setTtl(ttl_secs);
      })
      .catch(() => {
        if (alive) setTtl(null);
      });
    return () => {
      alive = false;
    };
    // Reload when the conversation changes (key), not on object identity.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [scopeKey]);

  useEffect(() => {
    if (!open) return undefined;
    const onDown = (e: MouseEvent): void => {
      if (rootRef.current !== null && rootRef.current.contains(e.target as Node)) return;
      setOpen(false);
    };
    const onKey = (e: KeyboardEvent): void => {
      if (e.key === 'Escape') setOpen(false);
    };
    document.addEventListener('mousedown', onDown);
    window.addEventListener('keydown', onKey);
    return () => {
      document.removeEventListener('mousedown', onDown);
      window.removeEventListener('keydown', onKey);
    };
  }, [open]);

  const choose = (value: number | null): void => {
    const previous = ttl;
    // Optimistic: reflect immediately, roll back if the node refuses.
    setTtl(value);
    setOpen(false);
    storeTtl(scope, value).catch(() => {
      setTtl(previous);
      toast('error', t.errors.actionFailed);
    });
  };

  const armed = typeof ttl === 'number';

  if (variant === 'section') {
    return (
      <div>
        <p className="mb-3 text-sm text-muted">{t.dm.ephemeralHint}</p>
        <div
          className="flex flex-wrap gap-2"
          role="group"
          aria-label={t.dm.ephemeralTitle}
        >
          {CHOICES.map((choice) => (
            <button
              key={choice ?? 'off'}
              type="button"
              aria-pressed={ttl === choice}
              onClick={() => choose(choice)}
              className={`inline-flex min-h-9 items-center rounded-full px-3 py-1.5 text-sm font-medium transition-colors duration-150 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar ${
                ttl === choice
                  ? 'bg-blurple text-white'
                  : 'bg-rail text-norm hover:bg-input hover:text-header'
              }`}
            >
              {labelOf(t, choice)}
            </button>
          ))}
        </div>
      </div>
    );
  }

  return (
    <div ref={rootRef} className="relative">
      <button
        type="button"
        title={t.dm.ephemeralTitle}
        aria-label={t.dm.ephemeralTitle}
        aria-expanded={open}
        aria-haspopup="menu"
        onClick={() => setOpen((v) => !v)}
        className={`flex h-8 w-8 items-center justify-center rounded-md transition-colors duration-fast hover:bg-chat-hover hover:text-norm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-chat active:scale-95 ${
          armed ? 'text-green' : 'text-faint'
        }`}
      >
        <TimerIcon />
      </button>
      {open && (
        <div
          role="menu"
          aria-label={t.dm.ephemeralTitle}
          className="glass-strong popover-enter absolute end-0 top-9 z-50 w-64 rounded-lg p-2 shadow-3"
        >
          <p className="px-2 pb-2 pt-1 text-xs text-muted">{t.dm.ephemeralHint}</p>
          {CHOICES.map((choice) => (
            <button
              key={choice ?? 'off'}
              type="button"
              role="menuitemradio"
              aria-checked={ttl === choice}
              onClick={() => choose(choice)}
              className={`flex w-full items-center justify-between rounded-md px-2 py-1.5 text-start text-sm transition-colors duration-fast hover:bg-chat-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple ${
                ttl === choice ? 'font-medium text-header' : 'text-norm'
              }`}
            >
              {labelOf(t, choice)}
              {ttl === choice && (
                <span aria-hidden className="text-blurple">
                  ●
                </span>
              )}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
