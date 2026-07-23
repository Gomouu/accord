/**
 * Planning event-handler tests: a due reminder / backup nudge turns into a
 * native notification; unrelated events are ignored.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Mock } from 'vitest';

vi.mock('../lib/client', () => ({
  rpc: { onEvent: vi.fn(() => () => {}), onStatus: vi.fn(() => () => {}) },
  api: { remindersList: vi.fn(async () => ({ reminders: [] })) },
}));
vi.mock('../lib/notifications', () => ({
  sendNativeNotification: vi.fn(async () => true),
}));

import { handlePlanningNodeEvent } from './planning';
import { sendNativeNotification } from '../lib/notifications';
import { useUi } from './ui';

const notif = sendNativeNotification as unknown as Mock;

beforeEach(() => {
  vi.clearAllMocks();
  useUi.setState({ lang: 'en' });
});

describe('handlePlanningNodeEvent', () => {
  it('fires a notification carrying the note on event.reminder', () => {
    handlePlanningNodeEvent('event.reminder', { note: 'call the vet' });
    expect(notif).toHaveBeenCalledWith('Reminder', 'call the vet');
  });

  it('falls back to a default body when the note is empty', () => {
    handlePlanningNodeEvent('event.reminder', { note: '' });
    expect(notif).toHaveBeenCalledWith('Reminder', 'You have a reminder.');
  });

  it('fires a notification on event.backup_due', () => {
    handlePlanningNodeEvent('event.backup_due', { auto: false, dir: null });
    expect(notif).toHaveBeenCalledWith('Backup', 'Time to back up your profile.');
  });

  it('ignores unrelated events', () => {
    handlePlanningNodeEvent('event.dm', { peer: 'x' });
    expect(notif).not.toHaveBeenCalled();
  });
});
