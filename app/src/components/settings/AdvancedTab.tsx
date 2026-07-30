/**
 * Onglet Avancé : version de l'application, licence (MIT + dépendances
 * tierces) et identité technique (code ami copiable, identifiant de nœud).
 */

import { useState } from 'react';
import { interpolate } from '../../i18n';
import { COPY_FEEDBACK_MS, copyToClipboard } from '../../lib/clipboard';
import { APP_LICENSE, APP_VERSION, THIRD_PARTY_FILE } from '../../lib/meta';
import { useSession } from '../../stores/session';
import { useSettingsT, useT, useUi } from '../../stores/ui';
import { SettingsSection } from './controls';

export function AdvancedTab() {
  const t = useT();
  const ts = useSettingsT();
  const toast = useUi((s) => s.toast);
  const self = useSession((s) => s.self);
  const [copied, setCopied] = useState(false);

  const copyCode = (): void => {
    if (!self) return;
    copyToClipboard(
      self.friend_code,
      () => {
        setCopied(true);
        setTimeout(() => setCopied(false), COPY_FEEDBACK_MS);
      },
      () => toast('error', t.errors.actionFailed),
    );
  };

  return (
    <div>
      <SettingsSection title={ts.settings.version}>
        <div className="rounded-lg bg-sidebar p-4">
          <p className="text-sm text-norm">
            {t.app.name}{' '}
            <span className="selectable font-mono text-muted">{APP_VERSION}</span>
          </p>
        </div>
      </SettingsSection>

      <SettingsSection title={ts.settings.license}>
        <div className="rounded-lg bg-sidebar p-4">
          <p className="text-sm leading-relaxed text-muted">
            {interpolate(ts.settings.licenseText, { file: THIRD_PARTY_FILE })}{' '}
            <span className="font-mono text-xs text-faint">({APP_LICENSE})</span>
          </p>
        </div>
      </SettingsSection>

      {self && (
        <SettingsSection title={ts.settings.identity}>
          <div className="rounded-lg bg-sidebar p-4">
            <div className="flex flex-wrap items-center justify-between gap-3">
              <div className="min-w-0">
                <div className="text-xs font-medium uppercase text-faint">
                  {t.friends.myCode}
                </div>
                <div className="selectable truncate font-mono text-norm">
                  {self.friend_code}
                </div>
              </div>
              <button
                type="button"
                onClick={copyCode}
                className="rounded-lg bg-blurple px-4 py-2 text-sm font-medium text-white transition-colors duration-fast hover:bg-blurple-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar"
              >
                {copied ? t.app.copied : ts.settings.copyFriendCode}
              </button>
            </div>
            <div className="mt-3 text-xs font-medium uppercase text-faint">
              {ts.settings.nodeId}
            </div>
            <div className="selectable break-all font-mono text-xs text-muted">
              {self.node_id}
            </div>
          </div>
        </SettingsSection>
      )}
    </div>
  );
}
