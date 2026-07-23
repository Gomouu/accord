/**
 * Planning store (Lot F): scheduled messages, reminders and backup status,
 * plus the node-event handler that turns `event.reminder` / `event.backup_due`
 * into native notifications and refreshes the lists. All state is local.
 */

import { create } from 'zustand';
import type { BackupStatus, ReminderInfo, ScheduledMessageInfo } from '../lib/api';
import { api, rpc } from '../lib/client';
import { dictionaries } from '../i18n';
import { sendNativeNotification } from '../lib/notifications';
import { useUi } from './ui';

interface PlanningState {
  scheduled: ScheduledMessageInfo[];
  reminders: ReminderInfo[];
  backup: BackupStatus | null;
  loadScheduled: () => Promise<void>;
  loadReminders: () => Promise<void>;
  loadBackup: () => Promise<void>;
}

export const usePlanning = create<PlanningState>((set) => ({
  scheduled: [],
  reminders: [],
  backup: null,
  loadScheduled: async () => {
    const { scheduled } = await api.scheduleList();
    set({ scheduled });
  },
  loadReminders: async () => {
    const { reminders } = await api.remindersList();
    set({ reminders });
  },
  loadBackup: async () => {
    set({ backup: await api.backupStatus() });
  },
}));

/**
 * Node events owned by this domain. A reminder fires a native notification and
 * refreshes the list; a backup-due nudge fires a notification (the actual
 * export runs from the backup panel — it needs the passphrase and stops the
 * node). Exported for tests; wired at module load (the RPC client is a
 * singleton, no teardown needed).
 */
export function handlePlanningNodeEvent(method: string, params: unknown): void {
  const dict = dictionaries[useUi.getState().lang];
  if (method === 'event.reminder') {
    const p = params as { note: string };
    const body = p.note.trim() !== '' ? p.note : dict.planning.reminderNotifBody;
    void sendNativeNotification(dict.planning.reminderNotifTitle, body);
    void usePlanning
      .getState()
      .loadReminders()
      .catch(() => {
        // Best effort: the list reloads on the next open.
      });
    return;
  }
  if (method === 'event.backup_due') {
    void sendNativeNotification(
      dict.planning.backupNotifTitle,
      dict.planning.backupNotifBody,
    );
  }
}

// Environment guard: unit tests that stub `../lib/client` without `rpc.onEvent`
// must still be able to import this module.
try {
  rpc.onEvent(handlePlanningNodeEvent);
} catch {
  // No RPC client (test env without event wiring): nothing to subscribe.
}
