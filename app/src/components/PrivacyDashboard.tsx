/**
 * Privacy dashboard (E3): renders the read-only `privacy.report` — what this
 * device stores (all of it local, encrypted at rest) and the only endpoint
 * kinds the node talks to. The headline fact: central servers contacted = 0,
 * by construction. Hooked as a section of the Privacy settings tab.
 */

import { useCallback, useEffect, useState } from 'react';
import type { PrivacyReport } from '../lib/api';
import { api } from '../lib/client';
import { useSettingsT, useT, useUi } from '../stores/ui';
import { SettingsSection } from './settings/controls';

/** Human-readable byte size (KiB/MiB — coarse is enough here). */
export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} o`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} Kio`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} Mio`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} Gio`;
}

function StatRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-center justify-between gap-4 px-4 py-2.5">
      <span className="min-w-0 text-sm text-norm">{label}</span>
      <span className="shrink-0 text-sm font-medium tabular-nums text-header">
        {value}
      </span>
    </div>
  );
}

export function PrivacyDashboard() {
  const t = useT();
  const ts = useSettingsT();
  const toast = useUi((s) => s.toast);
  const [report, setReport] = useState<PrivacyReport | null>(null);

  const load = useCallback((): void => {
    api
      .privacyReport()
      .then(setReport)
      .catch(() => toast('error', t.errors.loadFailed));
  }, [toast, t]);

  useEffect(() => {
    load();
  }, [load]);

  if (report === null) {
    return (
      <SettingsSection
        title={ts.settings.privacyDashboardTitle}
        hint={ts.settings.privacyDashboardHint}
      >
        <div aria-hidden className="h-40 animate-pulse rounded-lg bg-sidebar" />
      </SettingsSection>
    );
  }

  const { counts, storage, egress } = report;

  return (
    <SettingsSection
      title={ts.settings.privacyDashboardTitle}
      hint={ts.settings.privacyDashboardHint}
    >
      <h4 className="mb-1 mt-2 text-xs font-medium uppercase tracking-wide text-faint">
        {ts.settings.privacyCountsTitle}
      </h4>
      <div className="divide-y divide-input rounded-lg bg-sidebar">
        <StatRow label={ts.settings.privacyCountFriends} value={String(counts.friends)} />
        <StatRow label={ts.settings.privacyCountDms} value={String(counts.dm_messages)} />
        <StatRow label={ts.settings.privacyCountGroups} value={String(counts.groups)} />
        <StatRow
          label={ts.settings.privacyCountGroupMessages}
          value={String(counts.group_messages)}
        />
        <StatRow label={ts.settings.privacyCountFiles} value={String(counts.files)} />
        <StatRow label={ts.settings.privacyCountPins} value={String(counts.pins)} />
      </div>

      <h4 className="mb-1 mt-4 text-xs font-medium uppercase tracking-wide text-faint">
        {ts.settings.privacyStorageTitle}
      </h4>
      <div className="divide-y divide-input rounded-lg bg-sidebar">
        <StatRow
          label={ts.settings.privacyDbSize}
          value={storage.db_bytes === null ? '—' : formatBytes(storage.db_bytes)}
        />
        <StatRow
          label={ts.settings.privacyFilesSize}
          value={formatBytes(storage.file_bytes)}
        />
        <StatRow
          label={ts.settings.privacyEncrypted}
          value={storage.db_encrypted_at_rest ? ts.settings.privacyEncryptedYes : '—'}
        />
      </div>

      <h4 className="mb-1 mt-4 text-xs font-medium uppercase tracking-wide text-faint">
        {ts.settings.privacyEgressTitle}
      </h4>
      <p className="mb-1 text-xs text-muted">{ts.settings.privacyEgressHint}</p>
      {egress.available ? (
        <div className="divide-y divide-input rounded-lg bg-sidebar">
          <StatRow
            label={ts.settings.privacyEgressBootstrap}
            value={String(egress.bootstrap_peers)}
          />
          <StatRow
            label={ts.settings.privacyEgressDht}
            value={String(egress.dht_nodes)}
          />
          <StatRow
            label={ts.settings.privacyEgressPeers}
            value={String(egress.connected_peers)}
          />
          <StatRow
            label={ts.settings.privacyEgressRelays}
            value={String(egress.relay_circuits)}
          />
          <StatRow
            label={ts.settings.privacyEgressCentral}
            value={String(egress.central_servers)}
          />
        </div>
      ) : (
        <p className="rounded-lg bg-sidebar px-4 py-4 text-center text-sm text-muted">
          {ts.settings.privacyEgressUnavailable}
        </p>
      )}
    </SettingsSection>
  );
}
