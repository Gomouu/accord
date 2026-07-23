/**
 * Planning settings tab (Lot F): hosts the scheduled-messages, reminders and
 * backup-schedule panels. Each panel manages its own local state; nothing here
 * touches the wire.
 */

import { BackupSettingsPanel } from '../BackupSettingsPanel';
import { RemindersPanel } from '../RemindersPanel';
import { ScheduledMessagesPanel } from '../ScheduledMessagesPanel';

export function PlanningTab() {
  return (
    <div>
      <ScheduledMessagesPanel />
      <RemindersPanel />
      <BackupSettingsPanel />
    </div>
  );
}
