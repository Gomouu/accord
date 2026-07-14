/**
 * Onglet Système : lancement au démarrage, icône dans la barre des
 * menus/systray et fermeture réduite. Le premier reflète toujours l'état réel
 * du système (`@tauri-apps/plugin-autostart`), jamais une intention locale
 * seule — les deux autres sont persistés côté UI (`stores/ui.ts`) et
 * appliqués en direct côté hôte (création/destruction de l'icône,
 * interception de fermeture) sans redémarrage requis.
 */

import { useEffect, useState } from 'react';
import { autostartIsEnabled, autostartSetEnabled } from '../../lib/bridge';
import { api } from '../../lib/client';
import { requestNotificationPermission } from '../../lib/notifications';
import { useUi, useT } from '../../stores/ui';
import { SettingsSection, ToggleRow } from './controls';

export function SystemTab() {
  const t = useT();
  const toast = useUi((s) => s.toast);
  const keepInTray = useUi((s) => s.keepInTray);
  const setKeepInTray = useUi((s) => s.setKeepInTray);
  const closeToTray = useUi((s) => s.closeToTray);
  const setCloseToTray = useUi((s) => s.setCloseToTray);

  const [autostart, setAutostart] = useState(false);
  const [autostartBusy, setAutostartBusy] = useState(false);
  const [permsBusy, setPermsBusy] = useState(false);

  useEffect(() => {
    let cancelled = false;
    void autostartIsEnabled().then((enabled) => {
      if (!cancelled) setAutostart(enabled);
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

  // Re-sollicite les autorisations système (notifications + micro). L'OS ne
  // ré-affiche l'invite que si l'état est « indéterminé » ; après un refus
  // explicite, l'utilisateur doit passer par les Réglages système — d'où le
  // message d'indication.
  const reRequestPermissions = (): void => {
    setPermsBusy(true);
    void requestNotificationPermission()
      .then((notif) => {
        void api.voiceMicTest(true).catch(() => undefined);
        window.setTimeout(() => {
          void api.voiceMicTest(false).catch(() => undefined);
        }, 1500);
        toast(
          'info',
          notif === 'granted'
            ? t.settings.systemPermsRequested
            : t.settings.systemPermsHintDenied,
        );
      })
      .finally(() => setPermsBusy(false));
  };

  return (
    <div>
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
        <button
          type="button"
          disabled={permsBusy}
          onClick={reRequestPermissions}
          className="rounded-md bg-blurple px-4 py-2.5 text-sm font-medium text-white shadow-sm transition-[transform,background-color,box-shadow,opacity] duration-fast hover:-translate-y-px hover:bg-blurple-hover hover:shadow-md active:translate-y-0 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-modal disabled:opacity-60"
        >
          {t.settings.systemPermsButton}
        </button>
      </SettingsSection>
    </div>
  );
}
