/**
 * Onglet Système : lancement au démarrage, icône dans la barre des
 * menus/systray et fermeture réduite. Le premier reflète toujours l'état réel
 * du système (`@tauri-apps/plugin-autostart`), jamais une intention locale
 * seule — les deux autres sont persistés côté UI (`stores/ui.ts`) et
 * appliqués en direct côté hôte (création/destruction de l'icône,
 * interception de fermeture) sans redémarrage requis.
 */

import { useEffect, useState } from 'react';
import { interpolate } from '../../i18n';
import {
  autostartIsEnabled,
  autostartSetEnabled,
  isTauri,
  micPermissionRequest,
  micPermissionState,
  openSystemSettings,
  type MicPermissionState,
  type SystemSettingsSection,
} from '../../lib/bridge';
import { APP_VERSION } from '../../lib/meta';
import { requestNotificationPermission } from '../../lib/notifications';
import { RELEASES_URL } from '../../lib/updater';
import { useUpdater } from '../../stores/updater';
import { useUi, useT } from '../../stores/ui';
import { SettingsSection, ToggleRow } from './controls';

/** Pastille d'état d'une autorisation (accordée / refusée / à demander). */
function PermissionBadge({ state }: { state: 'granted' | 'denied' | 'ask' }) {
  const t = useT();
  const style =
    state === 'granted'
      ? 'bg-green/15 text-green'
      : state === 'denied'
        ? 'bg-red/15 text-red'
        : 'bg-input text-muted';
  const label =
    state === 'granted'
      ? t.settings.systemPermsStateGranted
      : state === 'denied'
        ? t.settings.systemPermsStateDenied
        : t.settings.systemPermsStateAsk;
  return (
    <span className={`rounded-full px-2 py-0.5 text-[11px] font-medium ${style}`}>
      {label}
    </span>
  );
}

/**
 * Ligne d'autorisation : intitulé + explication, état facultatif, action de
 * demande facultative (l'OS n'affiche son invite qu'à l'état « indéterminé »)
 * et raccourci vers le panneau des réglages système — seul recours après un
 * refus.
 */
function PermissionRow({
  title,
  hint,
  badge,
  action,
  section,
}: {
  title: string;
  hint: string;
  badge?: 'granted' | 'denied' | 'ask' | undefined;
  action?: { label: string; busy: boolean; onClick: () => void } | undefined;
  section: SystemSettingsSection;
}) {
  const t = useT();
  const toast = useUi((s) => s.toast);
  const openSettings = (): void => {
    openSystemSettings(section).catch(() => {
      toast('info', t.settings.systemPermsSettingsUnavailable);
    });
  };
  return (
    <div className="flex items-start justify-between gap-4 rounded-lg bg-sidebar px-4 py-3">
      <div className="min-w-0">
        <div className="flex items-center gap-2 text-sm font-medium text-header">
          {title}
          {badge !== undefined && <PermissionBadge state={badge} />}
        </div>
        <p className="mt-0.5 text-xs leading-relaxed text-faint">{hint}</p>
      </div>
      <div className="flex shrink-0 items-center gap-2">
        {action !== undefined && (
          <button
            type="button"
            disabled={action.busy}
            onClick={action.onClick}
            className="rounded-md bg-blurple px-3 py-1.5 text-sm font-medium text-white transition-colors duration-fast hover:bg-blurple-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-modal disabled:opacity-60"
          >
            {action.label}
          </button>
        )}
        <button
          type="button"
          onClick={openSettings}
          className="rounded-md bg-rail px-3 py-1.5 text-sm font-medium text-norm transition-colors duration-fast hover:bg-input hover:text-header focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-modal"
        >
          {t.settings.systemPermsOpenSettings}
        </button>
      </div>
    </div>
  );
}

/**
 * Section « Mises à jour » (D-049) : version installée, vérification
 * manuelle et cycle installer → redémarrer. La vérification automatique
 * (démarrage + périodique) vit dans `App` ; ici on partage le même store,
 * donc l'état affiché est toujours celui du cycle réel.
 */
function UpdatesSection() {
  const t = useT();
  const status = useUpdater((s) => s.status);
  const version = useUpdater((s) => s.version);
  const notes = useUpdater((s) => s.notes);
  const progress = useUpdater((s) => s.progress);
  const error = useUpdater((s) => s.error);
  const check = useUpdater((s) => s.check);
  const install = useUpdater((s) => s.install);
  const restart = useUpdater((s) => s.restart);

  if (!isTauri()) {
    return (
      <SettingsSection title={t.updates.title}>
        <div className="rounded-lg bg-sidebar p-4">
          <p className="text-sm text-muted">{t.updates.unavailable}</p>
        </div>
      </SettingsSection>
    );
  }

  const percent = progress !== null ? Math.round(progress * 100) : null;
  const statusText =
    status === 'checking'
      ? t.updates.checking
      : status === 'upToDate'
        ? t.updates.upToDate
        : status === 'available'
          ? interpolate(t.updates.available, { version: version ?? '' })
          : status === 'downloading'
            ? percent !== null
              ? interpolate(t.updates.downloading, { percent: String(percent) })
              : t.updates.downloadingIndeterminate
            : status === 'ready'
              ? t.updates.ready
              : status === 'error'
                ? interpolate(t.updates.error, { error: error ?? '' })
                : null;

  const action =
    status === 'idle' || status === 'upToDate'
      ? { label: t.updates.check, busy: false, run: () => void check(true) }
      : status === 'checking'
        ? { label: t.updates.checking, busy: true, run: () => undefined }
        : status === 'available'
          ? { label: t.updates.install, busy: false, run: () => void install() }
          : status === 'downloading'
            ? { label: t.updates.install, busy: true, run: () => undefined }
            : status === 'ready'
              ? { label: t.updates.restart, busy: false, run: () => void restart() }
              : {
                  label: t.updates.retry,
                  busy: false,
                  run: () => void (version !== null ? install() : check(true)),
                };

  return (
    <SettingsSection title={t.updates.title} hint={t.updates.hint}>
      <div className="rounded-lg bg-sidebar p-4">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div className="min-w-0">
            <p className="text-sm text-norm">
              {interpolate(t.updates.current, { version: APP_VERSION })}
            </p>
            {statusText !== null && (
              <p role="status" className="mt-0.5 text-xs text-faint">
                {statusText}
              </p>
            )}
          </div>
          <button
            type="button"
            disabled={action.busy}
            onClick={action.run}
            className="rounded-md bg-blurple px-3 py-1.5 text-sm font-medium text-white transition-colors duration-fast hover:bg-blurple-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-modal disabled:opacity-60"
          >
            {action.label}
          </button>
        </div>
        {status === 'downloading' && (
          <div className="mt-3 h-1.5 w-full overflow-hidden rounded-full bg-input">
            <div
              className={`h-full rounded-full bg-blurple ${percent === null ? 'w-full animate-pulse' : ''}`}
              style={percent !== null ? { width: `${percent}%` } : undefined}
            />
          </div>
        )}
        {status === 'available' && notes !== null && (
          <div className="mt-3">
            <div className="text-xs font-medium uppercase text-faint">
              {t.updates.notes}
            </div>
            <p className="mt-1 max-h-40 overflow-y-auto whitespace-pre-wrap text-xs leading-relaxed text-muted">
              {notes}
            </p>
          </div>
        )}
        {status === 'error' && (
          <p className="selectable mt-2 break-all text-xs leading-relaxed text-faint">
            {interpolate(t.updates.errorFallback, { url: RELEASES_URL })}
          </p>
        )}
      </div>
    </SettingsSection>
  );
}

export function SystemTab() {
  const t = useT();
  const toast = useUi((s) => s.toast);
  const keepInTray = useUi((s) => s.keepInTray);
  const setKeepInTray = useUi((s) => s.setKeepInTray);
  const closeToTray = useUi((s) => s.closeToTray);
  const setCloseToTray = useUi((s) => s.setCloseToTray);

  const [autostart, setAutostart] = useState(false);
  const [autostartBusy, setAutostartBusy] = useState(false);
  const [notifBusy, setNotifBusy] = useState(false);
  const [micBusy, setMicBusy] = useState(false);
  const [micState, setMicState] = useState<MicPermissionState>('unsupported');

  useEffect(() => {
    let cancelled = false;
    void autostartIsEnabled().then((enabled) => {
      if (!cancelled) setAutostart(enabled);
    });
    void micPermissionState().then((state) => {
      if (!cancelled) setMicState(state);
    });
    return () => {
      cancelled = true;
    };
  }, []);

  const toggleAutostart = (next: boolean): void => {
    setAutostartBusy(true);
    void autostartSetEnabled(next)
      .then(() => autostartIsEnabled())
      .then(setAutostart)
      .finally(() => setAutostartBusy(false));
  };

  // Demandes SÉPARÉES par autorisation : deux invites système empilées d'un
  // coup (l'ancien bouton combiné) se lisaient comme une boucle de dialogues.
  // L'OS ne ré-affiche l'invite que si l'état est « indéterminé » ; après un
  // refus explicite, seul le panneau système (bouton dédié) permet de revenir.
  const requestNotifications = (): void => {
    setNotifBusy(true);
    void requestNotificationPermission()
      .then((notif) => {
        toast(
          'info',
          notif === 'granted'
            ? t.settings.systemPermsNotifGranted
            : t.settings.systemPermsNotifDenied,
        );
      })
      .finally(() => setNotifBusy(false));
  };

  // Demande NATIVE (AVFoundation), sans ouvrir de capture : l'invite système
  // n'apparaît qu'à l'état « indéterminé » ; à tout autre état l'OS répond
  // immédiatement — impossible de redemander en boucle. L'état affiché est
  // rafraîchi avec la réponse.
  const requestMicrophone = (): void => {
    setMicBusy(true);
    void micPermissionRequest()
      .then((granted) => {
        toast(
          granted ? 'info' : 'error',
          granted
            ? t.settings.systemPermsStateGranted
            : t.settings.systemPermsMicDeniedToast,
        );
      })
      .catch(() => {
        toast('info', t.settings.systemPermsSettingsUnavailable);
      })
      .finally(() => {
        void micPermissionState().then(setMicState);
        setMicBusy(false);
      });
  };

  return (
    <div>
      <UpdatesSection />

      <SettingsSection
        title={t.settings.systemStartupTitle}
        hint={t.settings.systemStartupHint}
      >
        <ToggleRow
          label={t.settings.systemAutostart}
          hint={t.settings.systemAutostartHint}
          checked={autostart}
          disabled={autostartBusy}
          onChange={toggleAutostart}
        />
      </SettingsSection>

      <SettingsSection title={t.settings.systemTrayTitle}>
        <ToggleRow
          label={t.settings.systemKeepInTray}
          hint={t.settings.systemKeepInTrayHint}
          checked={keepInTray}
          onChange={setKeepInTray}
        />
        <ToggleRow
          label={t.settings.systemCloseToTray}
          hint={
            keepInTray
              ? t.settings.systemCloseToTrayHint
              : t.settings.systemCloseToTrayDisabledHint
          }
          checked={closeToTray}
          disabled={!keepInTray}
          onChange={setCloseToTray}
        />
      </SettingsSection>

      <SettingsSection
        title={t.settings.systemPermsTitle}
        hint={t.settings.systemPermsHint}
      >
        <div className="space-y-2">
          <PermissionRow
            title={t.settings.systemPermsNotifTitle}
            hint={t.settings.systemPermsNotifHint}
            action={{
              label: t.settings.systemPermsNotifButton,
              busy: notifBusy,
              onClick: requestNotifications,
            }}
            section="notifications"
          />
          <PermissionRow
            title={t.settings.systemPermsMicTitle}
            hint={t.settings.systemPermsMicHint}
            badge={
              micState === 'granted'
                ? 'granted'
                : micState === 'denied' || micState === 'restricted'
                  ? 'denied'
                  : micState === 'undetermined'
                    ? 'ask'
                    : undefined
            }
            action={
              micState === 'undetermined'
                ? {
                    label: t.settings.systemPermsMicButton,
                    busy: micBusy,
                    onClick: requestMicrophone,
                  }
                : undefined
            }
            section="microphone"
          />
          <PermissionRow
            title={t.settings.systemPermsNetTitle}
            hint={t.settings.systemPermsNetHint}
            section="firewall"
          />
        </div>
      </SettingsSection>
    </div>
  );
}
