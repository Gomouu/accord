/**
 * Backup-schedule panel tests: renders the status, changes the cadence, and
 * runs a manual backup (recording the time, then invoking the host export).
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Mock } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

vi.mock('../lib/client', () => ({
  rpc: { onEvent: vi.fn(() => () => {}), onStatus: vi.fn(() => () => {}) },
  api: {
    backupStatus: vi.fn(),
    backupSchedule: vi.fn(async () => ({ ok: true })),
    backupRecordDone: vi.fn(async () => ({ ok: true })),
  },
}));
vi.mock('../lib/bridge', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../lib/bridge')>()),
  backupExport: vi.fn(async () => null),
}));

import { api } from '../lib/client';
import { backupExport } from '../lib/bridge';
import type { BackupStatus } from '../lib/api';
import { usePlanning } from '../stores/planning';
import { useUi } from '../stores/ui';
import { BackupSettingsPanel } from './BackupSettingsPanel';

const statusMock = api.backupStatus as unknown as Mock;
const scheduleMock = api.backupSchedule as unknown as Mock;
const recordMock = api.backupRecordDone as unknown as Mock;
const exportMock = backupExport as unknown as Mock;

function status(overrides?: Partial<BackupStatus>): BackupStatus {
  return {
    cadence: 'off',
    dir: null,
    last_backup_at: null,
    next_due_at: null,
    due: false,
    ...overrides,
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  useUi.setState({ lang: 'en' });
  usePlanning.setState({ scheduled: [], reminders: [], backup: null });
});

describe('BackupSettingsPanel', () => {
  it('renders the cadence options and folder state', async () => {
    statusMock.mockResolvedValue(status());
    render(<BackupSettingsPanel />);

    expect(await screen.findByRole('button', { name: 'Weekly' })).toBeInTheDocument();
    expect(screen.getByText('None (reminder only)')).toBeInTheDocument();
  });

  it('changes the cadence through the API', async () => {
    const user = userEvent.setup();
    statusMock.mockResolvedValue(status());
    render(<BackupSettingsPanel />);

    await user.click(await screen.findByRole('button', { name: 'Monthly' }));
    expect(scheduleMock).toHaveBeenCalledWith('monthly', null);
  });

  it('records the time then runs the host export on "back up now"', async () => {
    const user = userEvent.setup();
    statusMock.mockResolvedValue(status({ cadence: 'weekly' }));
    render(<BackupSettingsPanel />);

    await user.type(
      await screen.findByPlaceholderText('Profile passphrase'),
      'my-passphrase',
    );
    await user.click(screen.getByRole('button', { name: 'Back up now' }));

    expect(recordMock).toHaveBeenCalled();
    expect(exportMock).toHaveBeenCalledWith('my-passphrase');
  });
});
