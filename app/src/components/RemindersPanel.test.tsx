/**
 * Reminders panel tests: renders reminders from the store, shows the fired
 * badge, and dismisses through the API.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Mock } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

vi.mock('../lib/client', () => ({
  rpc: { onEvent: vi.fn(() => () => {}), onStatus: vi.fn(() => () => {}) },
  api: {
    remindersList: vi.fn(),
    remindersDismiss: vi.fn(async () => ({ ok: true })),
  },
}));

import { api } from '../lib/client';
import type { ReminderInfo } from '../lib/api';
import { usePlanning } from '../stores/planning';
import { useUi } from '../stores/ui';
import { RemindersPanel } from './RemindersPanel';

const listMock = api.remindersList as unknown as Mock;
const dismissMock = api.remindersDismiss as unknown as Mock;

function reminder(overrides?: Partial<ReminderInfo>): ReminderInfo {
  return {
    id: 'r1',
    scope: 'dm',
    scope_id: '09'.repeat(32),
    msg_ref: '03'.repeat(16),
    note: 'call the bank',
    fire_at: 1_900_000_000_000,
    fired: false,
    created_at: 1,
    ...overrides,
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  useUi.setState({ lang: 'en' });
  usePlanning.setState({ scheduled: [], reminders: [], backup: null });
});

describe('RemindersPanel', () => {
  it('renders a reminder with its note', async () => {
    listMock.mockResolvedValue({ reminders: [reminder()] });
    render(<RemindersPanel />);

    expect(await screen.findByText('call the bank')).toBeInTheDocument();
  });

  it('shows the fired badge on a fired reminder', async () => {
    listMock.mockResolvedValue({ reminders: [reminder({ fired: true })] });
    render(<RemindersPanel />);

    expect(await screen.findByText('fired')).toBeInTheDocument();
  });

  it('dismisses a reminder by id', async () => {
    const user = userEvent.setup();
    listMock.mockResolvedValue({ reminders: [reminder({ id: 'r7' })] });
    render(<RemindersPanel />);

    await user.click(await screen.findByRole('button', { name: 'Dismiss' }));
    expect(dismissMock).toHaveBeenCalledWith('r7');
  });
});
