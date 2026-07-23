/**
 * Reminder dialog (F2 hook): pick when to be reminded about a message, with an
 * optional note. Purely local — `reminders.add` stores it; the node emits a
 * notification when due. Self-hosted on `ui.reminderTarget` (opened from the
 * message context menu), same modal conventions as the rest of the repo.
 */

import { useEffect, useRef, useState } from 'react';
import { api } from '../lib/client';
import { bouclerTab } from '../lib/focus';
import { usePlanning } from '../stores/planning';
import { useT, useUi } from '../stores/ui';
import { CloseIcon } from './ContextMenu';

const HOUR_MS = 60 * 60 * 1000;

/** Tomorrow at 09:00 local time. */
function tomorrowMorning(): number {
  const d = new Date();
  d.setDate(d.getDate() + 1);
  d.setHours(9, 0, 0, 0);
  return d.getTime();
}

export function ReminderDialog() {
  const t = useT();
  const target = useUi((s) => s.reminderTarget);
  const close = useUi((s) => s.closeReminder);
  const toast = useUi((s) => s.toast);
  const reloadReminders = usePlanning((s) => s.loadReminders);
  const ref = useRef<HTMLDivElement>(null);
  const [note, setNote] = useState('');
  const [custom, setCustom] = useState('');
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    setNote('');
    setCustom('');
  }, [target]);

  useEffect(() => {
    if (target === null) return undefined;
    const previous =
      document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') close();
      else if (e.key === 'Tab') bouclerTab(e, ref.current);
    };
    window.addEventListener('keydown', onKey);
    ref.current?.focus();
    return () => {
      window.removeEventListener('keydown', onKey);
      if (previous !== null && previous.isConnected) previous.focus();
    };
  }, [target, close]);

  if (target === null) return null;

  const add = (fireAt: number): void => {
    if (busy || Number.isNaN(fireAt)) return;
    setBusy(true);
    api
      .remindersAdd(target.scope, target.scopeId, note, fireAt, target.msgRef)
      .then(() => {
        void reloadReminders().catch(() => {
          // Best effort: the list reloads when the panel opens.
        });
        close();
      })
      .catch(() => toast('error', t.errors.actionFailed))
      .finally(() => setBusy(false));
  };

  const presets: { label: string; at: () => number }[] = [
    { label: t.planning.reminderIn20m, at: () => Date.now() + 20 * 60 * 1000 },
    { label: t.planning.reminderIn1h, at: () => Date.now() + HOUR_MS },
    { label: t.planning.reminderIn3h, at: () => Date.now() + 3 * HOUR_MS },
    { label: t.planning.reminderTomorrow, at: tomorrowMorning },
  ];

  return (
    <div
      className="modal-overlay-enter fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) close();
      }}
    >
      <div
        ref={ref}
        role="dialog"
        aria-modal="true"
        aria-label={t.planning.reminderDialogTitle}
        tabIndex={-1}
        className="glass modal-panel-enter flex w-[24rem] max-w-[92vw] flex-col overflow-hidden rounded-xl shadow-3 focus:outline-none"
      >
        <div className="flex items-center justify-between px-5 pt-5">
          <h2 className="text-lg font-semibold text-header">
            {t.planning.reminderDialogTitle}
          </h2>
          <button
            type="button"
            aria-label={t.app.close}
            onClick={close}
            className="rounded-sm p-1 text-faint transition-colors duration-fast hover:text-norm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-modal active:scale-95"
          >
            <CloseIcon size={20} />
          </button>
        </div>
        <div className="flex flex-col gap-3 px-5 pb-5 pt-3">
          <input
            value={note}
            onChange={(e) => setNote(e.target.value)}
            placeholder={t.planning.reminderNotePlaceholder}
            className="rounded-lg bg-input px-3 py-2 text-sm text-norm placeholder:text-faint focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple"
          />
          <div className="grid grid-cols-2 gap-2">
            {presets.map((p) => (
              <button
                key={p.label}
                type="button"
                disabled={busy}
                onClick={() => add(p.at())}
                className="rounded-lg bg-rail px-3 py-2 text-sm font-medium text-norm transition-colors duration-fast hover:bg-input hover:text-header focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple active:scale-[0.98] disabled:opacity-50"
              >
                {p.label}
              </button>
            ))}
          </div>
          <label className="flex flex-col gap-1 text-sm text-muted">
            {t.planning.reminderCustom}
            <div className="flex items-center gap-2">
              <input
                type="datetime-local"
                value={custom}
                onChange={(e) => setCustom(e.target.value)}
                className="flex-1 rounded-lg bg-input px-3 py-2 text-sm text-norm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple"
              />
              <button
                type="button"
                disabled={busy || custom === ''}
                onClick={() => add(new Date(custom).getTime())}
                className="rounded-lg bg-blurple px-3 py-2 text-sm font-medium text-white transition-colors duration-fast hover:bg-blurple-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple active:scale-[0.98] disabled:opacity-50"
              >
                {t.planning.reminderAdd}
              </button>
            </div>
          </label>
        </div>
      </div>
    </div>
  );
}
