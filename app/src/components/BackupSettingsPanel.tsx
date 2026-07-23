/**
 * Backup-schedule panel (F3): pick a cadence (off / weekly / monthly) and an
 * optional destination folder, see the last/next backup, and run a backup now.
 *
 * The node only tracks the schedule and nudges when due; the archive itself is
 * written by the host `backup_export` flow (it stops the node and re-verifies
 * the passphrase). `last_backup_at` is therefore recorded optimistically here,
 * before the export — the export locks the session, so it cannot be recorded
 * afterwards. A cancelled save dialog leaves the timestamp slightly early,
 * which at worst skips one reminder.
 */

import { useCallback, useEffect, useState } from 'react';
import { backupExport } from '../lib/bridge';
import { api } from '../lib/client';
import { usePlanning } from '../stores/planning';
import { useT, useUi } from '../stores/ui';
import { OptionPill, SettingsSection } from './settings/controls';

const CADENCES: readonly string[] = ['off', 'weekly', 'monthly'];

function useFormatDateTime(): (ms: number) => string {
  const lang = useUi((s) => s.lang);
  return (ms: number) =>
    new Intl.DateTimeFormat(lang, { dateStyle: 'medium', timeStyle: 'short' }).format(
      new Date(ms),
    );
}

export function BackupSettingsPanel() {
  const t = useT();
  const toast = useUi((s) => s.toast);
  const status = usePlanning((s) => s.backup);
  const load = usePlanning((s) => s.loadBackup);
  const fmt = useFormatDateTime();
  const [passphrase, setPassphrase] = useState('');
  const [busy, setBusy] = useState(false);

  const reload = useCallback((): void => {
    void load().catch(() => toast('error', t.errors.loadFailed));
  }, [load, toast, t]);

  useEffect(() => {
    reload();
  }, [reload]);

  const cadenceLabel = (c: string): string =>
    c === 'weekly'
      ? t.planning.cadenceWeekly
      : c === 'monthly'
        ? t.planning.cadenceMonthly
        : t.planning.cadenceOff;

  const setCadence = (cadence: string): void => {
    api
      .backupSchedule(cadence, status?.dir ?? null)
      .then(reload)
      .catch(() => toast('error', t.errors.actionFailed));
  };

  const chooseFolder = async (): Promise<void> => {
    try {
      const { open } = await import('@tauri-apps/plugin-dialog');
      const dir = await open({ directory: true });
      if (typeof dir === 'string') {
        await api.backupSchedule(status?.cadence ?? 'off', dir);
        reload();
      }
    } catch {
      toast('error', t.errors.actionFailed);
    }
  };

  const clearFolder = (): void => {
    api
      .backupSchedule(status?.cadence ?? 'off', null)
      .then(reload)
      .catch(() => toast('error', t.errors.actionFailed));
  };

  const runBackup = async (): Promise<void> => {
    if (passphrase === '' || busy) return;
    setBusy(true);
    try {
      await api.backupRecordDone();
      const result = await backupExport(passphrase);
      if (result === null) {
        // User cancelled the save dialog.
        reload();
      } else {
        toast('info', t.planning.backupDone);
      }
    } catch {
      toast('error', t.planning.backupFailed);
    } finally {
      setBusy(false);
      setPassphrase('');
    }
  };

  return (
    <SettingsSection title={t.planning.backupTitle} hint={t.planning.backupHint}>
      <div className="mb-4 flex flex-wrap gap-2">
        {CADENCES.map((c) => (
          <OptionPill
            key={c}
            selected={(status?.cadence ?? 'off') === c}
            onSelect={() => setCadence(c)}
          >
            {cadenceLabel(c)}
          </OptionPill>
        ))}
      </div>

      <div className="divide-y divide-input rounded-lg bg-sidebar">
        <div className="flex items-center justify-between gap-4 px-4 py-2.5">
          <span className="text-sm text-norm">{t.planning.backupFolder}</span>
          <div className="flex items-center gap-2">
            <span className="max-w-[16rem] truncate text-sm text-faint">
              {status?.dir ?? t.planning.backupFolderNone}
            </span>
            <button
              type="button"
              onClick={() => void chooseFolder()}
              className="rounded-md bg-rail px-2 py-1 text-xs font-medium text-norm transition-colors duration-fast hover:bg-input focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple active:scale-95"
            >
              {t.planning.choose}
            </button>
            {status?.dir != null && (
              <button
                type="button"
                onClick={clearFolder}
                className="rounded-md px-2 py-1 text-xs font-medium text-muted transition-colors duration-fast hover:text-norm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple active:scale-95"
              >
                {t.planning.clear}
              </button>
            )}
          </div>
        </div>
        <div className="flex items-center justify-between gap-4 px-4 py-2.5">
          <span className="text-sm text-norm">{t.planning.backupLast}</span>
          <span className="text-sm text-faint">
            {status?.last_backup_at != null ? fmt(status.last_backup_at) : '—'}
          </span>
        </div>
        <div className="flex items-center justify-between gap-4 px-4 py-2.5">
          <span className="text-sm text-norm">{t.planning.backupNext}</span>
          <span className={`text-sm ${status?.due === true ? 'text-red' : 'text-faint'}`}>
            {status?.next_due_at != null ? fmt(status.next_due_at) : '—'}
          </span>
        </div>
      </div>

      <div className="mt-4 flex flex-col gap-2 sm:flex-row sm:items-center">
        <input
          type="password"
          value={passphrase}
          onChange={(e) => setPassphrase(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') void runBackup();
          }}
          placeholder={t.planning.backupPassphrase}
          className="flex-1 rounded-md bg-input px-3 py-2 text-sm text-norm placeholder:text-faint focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple"
        />
        <button
          type="button"
          disabled={busy || passphrase === ''}
          onClick={() => void runBackup()}
          className="rounded-md bg-blurple px-3 py-2 text-sm font-medium text-white transition-colors duration-fast hover:bg-blurple-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple active:scale-[0.98] disabled:opacity-50"
        >
          {t.planning.backupNow}
        </button>
      </div>
    </SettingsSection>
  );
}
