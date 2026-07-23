/**
 * Reminders panel (F2): lists local reminders (target scope, note, due time,
 * fired state) and lets the user dismiss them. The reminder itself fires as a
 * native notification at its due time (see stores/planning). Purely local.
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

export function RemindersPanel() {
  const t = useT();
  const toast = useUi((s) => s.toast);
  const reminders = usePlanning((s) => s.reminders);
  const load = usePlanning((s) => s.loadReminders);
  const fmt = useFormatDateTime();

  const reload = useCallback((): void => {
    void load().catch(() => toast('error', t.errors.loadFailed));
  }, [load, toast, t]);

  useEffect(() => {
    reload();
  }, [reload]);

  const dismiss = (id: string): void => {
    api
      .remindersDismiss(id)
      .then(reload)
      .catch(() => toast('error', t.errors.actionFailed));
  };

  return (
    <SettingsSection title={t.planning.remindersTitle} hint={t.planning.remindersHint}>
      {reminders.length === 0 ? (
        <p className="rounded-lg bg-sidebar px-4 py-6 text-center text-sm text-muted">
          {t.planning.remindersEmpty}
        </p>
      ) : (
        <ul className="flex flex-col gap-2">
          {reminders.map((r) => (
            <li
              key={r.id}
              className="flex items-center justify-between gap-3 rounded-lg bg-sidebar px-4 py-3"
            >
              <div className="min-w-0">
                <p className="truncate text-sm text-norm">
                  {r.note.trim() !== '' ? r.note : t.planning.reminderNoNote}
                </p>
                <p className="mt-0.5 flex items-center gap-1.5 text-xs text-faint">
                  <span>
                    {r.scope === 'group' ? t.planning.scopeGroup : t.planning.scopeDm}
                  </span>
                  <span aria-hidden>·</span>
                  <span>{fmt(r.fire_at)}</span>
                  {r.fired && (
                    <span className="rounded-full bg-green/15 px-1.5 py-0.5 text-[10px] font-medium uppercase tracking-wide text-green">
                      {t.planning.reminderFired}
                    </span>
                  )}
                </p>
              </div>
              <button
                type="button"
                onClick={() => dismiss(r.id)}
                className="shrink-0 rounded-md px-2 py-1 text-xs font-medium text-muted transition-colors duration-fast hover:bg-input hover:text-norm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple active:scale-95"
              >
                {t.planning.dismiss}
              </button>
            </li>
          ))}
        </ul>
      )}
    </SettingsSection>
  );
}
