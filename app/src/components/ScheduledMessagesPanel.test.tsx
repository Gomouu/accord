/**
 * Scheduled-messages panel tests: renders the scheduled list from the store
 * and cancels an item through the API.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Mock } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

vi.mock('../lib/client', () => ({
  rpc: { onEvent: vi.fn(() => () => {}), onStatus: vi.fn(() => () => {}) },
  api: {
    scheduleList: vi.fn(),
    scheduleCancel: vi.fn(async () => ({ ok: true })),
    scheduleReschedule: vi.fn(async () => ({ ok: true })),
  },
}));

import { api } from '../lib/client';
import type { ScheduledMessageInfo } from '../lib/api';
import { usePlanning } from '../stores/planning';
import { useUi } from '../stores/ui';
import { ScheduledMessagesPanel } from './ScheduledMessagesPanel';

const listMock = api.scheduleList as unknown as Mock;
const cancelMock = api.scheduleCancel as unknown as Mock;

function item(overrides?: Partial<ScheduledMessageInfo>): ScheduledMessageInfo {
  return {
    id: 'abcd',
    scope: 'dm',
    scope_id: '09'.repeat(32),
    channel_id: null,
    fire_at: 1_900_000_000_000,
    created_at: 1,
    preview: 'send this later',
    ...overrides,
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  useUi.setState({ lang: 'en' });
  usePlanning.setState({ scheduled: [], reminders: [], backup: null });
});

describe('ScheduledMessagesPanel', () => {
  it('renders a scheduled message with its preview and scope', async () => {
    listMock.mockResolvedValue({ scheduled: [item()] });
    render(<ScheduledMessagesPanel />);

    expect(await screen.findByText('send this later')).toBeInTheDocument();
    expect(screen.getByText(/Direct message/)).toBeInTheDocument();
  });

  it('cancels a scheduled message by id', async () => {
    const user = userEvent.setup();
    listMock.mockResolvedValue({ scheduled: [item({ id: 'ff00' })] });
    render(<ScheduledMessagesPanel />);

    await user.click(await screen.findByRole('button', { name: 'Cancel' }));
    expect(cancelMock).toHaveBeenCalledWith('ff00');
  });

  it('shows the empty state when there is nothing scheduled', async () => {
    listMock.mockResolvedValue({ scheduled: [] });
    render(<ScheduledMessagesPanel />);

    expect(await screen.findByText('No scheduled messages.')).toBeInTheDocument();
  });
});
