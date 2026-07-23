/**
 * Scheduled-messages panel (F1): lists locally scheduled messages with their
 * firing time and preview, and lets the user cancel or reschedule each one.
 * Purely local — sending happens on the node's maintenance loop when due.
 */

import { useCallback, useEffect } from 'react';
import { useT, useUi } from '../stores/ui';
import { usePlanning } from '../stores/planning';
import { api } from '../lib/client';
import { SettingsSection } from './settings/controls';

/** Wall-clock ms → locale date+time. */
function useFormatDateTime(): (ms: number) => string {
  const lang = useUi((s) => s.lang);
  return (ms: number) =>
    new Intl.DateTimeFormat(lang, { dateStyle: 'medium', timeStyle: 'short' }).format(
      new Date(ms),
    );
}

/** `<input type="datetime-local">` value for a timestamp (local time). */
function toLocalInput(ms: number): string {
  const d = new Date(ms);
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

export function ScheduledMessagesPanel() {
  const t = useT();
  const toast = useUi((s) => s.toast);
  const scheduled = usePlanning((s) => s.scheduled);
  const load = usePlanning((s) => s.loadScheduled);
  const fmt = useFormatDateTime();

  const reload = useCallback((): void => {
    void load().catch(() => toast('error', t.errors.loadFailed));
  }, [load, toast, t]);

  useEffect(() => {
    reload();
  }, [reload]);

  const cancel = (id: string): void => {
    api
      .scheduleCancel(id)
      .then(reload)
      .catch(() => toast('error', t.errors.actionFailed));
  };

  const reschedule = (id: string, value: string): void => {
    const ms = new Date(value).getTime();
    if (Number.isNaN(ms)) return;
    api
      .scheduleReschedule(id, ms)
      .then(reload)
      .catch(() => toast('error', t.errors.actionFailed));
  };

  return (
    <SettingsSection title={t.planning.scheduledTitle} hint={t.planning.scheduledHint}>
      {scheduled.length === 0 ? (
        <p className="rounded-lg bg-sidebar px-4 py-6 text-center text-sm text-muted">
          {t.planning.scheduledEmpty}
        </p>
      ) : (
        <ul className="flex flex-col gap-2">
          {scheduled.map((m) => (
            <li
              key={m.id}
              className="flex flex-col gap-2 rounded-lg bg-sidebar px-4 py-3 sm:flex-row sm:items-center sm:justify-between"
            >
              <div className="min-w-0">
                <p className="truncate text-sm text-norm">{m.preview}</p>
                <p className="mt-0.5 text-xs text-faint">
                  {m.scope === 'group' ? t.planning.scopeGroup : t.planning.scopeDm} ·{' '}
                  {fmt(m.fire_at)}
                </p>
              </div>
              <div className="flex shrink-0 items-center gap-2">
                <input
                  type="datetime-local"
                  aria-label={t.planning.reschedule}
                  defaultValue={toLocalInput(m.fire_at)}
                  onChange={(e) => reschedule(m.id, e.target.value)}
                  className="rounded-md bg-input px-2 py-1 text-xs text-norm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple"
                />
                <button
                  type="button"
                  onClick={() => cancel(m.id)}
                  className="rounded-md px-2 py-1 text-xs font-medium text-red transition-colors duration-fast hover:bg-red/10 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-red active:scale-95"
                >
                  {t.planning.cancel}
                </button>
              </div>
            </li>
          ))}
        </ul>
      )}
    </SettingsSection>
  );
}
