/**
 * Onglet Notifications : autorisation système (plugin Tauri, repli explicite
 * hors application) et réglages persistés — messages privés, messages de
 * groupe, « seulement en arrière-plan », sons de notification, notifications
 * natives (interrupteurs maîtres) et filtrage du son par nature de message
 * (tous / mentions seulement / aucun). Le câblage de l'envoi vit dans
 * AppShell et lib/notifications.ts ; ici on ne gère que l'intention de
 * l'utilisateur.
 */

import { useEffect, useState } from 'react';
import {
  queryNotificationPermission,
  requestNotificationPermission,
  type NotificationPermission,
} from '../../lib/notifications';
import { useUi, useT, type NotifySoundMode } from '../../stores/ui';
import { OptionPill, SettingsSection, ToggleRow } from './controls';

/** Sélecteur d'heure locale (00 h – 23 h) pour la plage Ne pas déranger. */
function HourSelect({
  value,
  onChange,
}: {
  value: number;
  onChange: (h: number) => void;
}) {
  return (
    <select
      value={value}
      onChange={(e) => onChange(Number(e.target.value))}
      className="rounded-md border border-input bg-sidebar px-2 py-1 text-sm text-norm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple"
    >
      {Array.from({ length: 24 }, (_, h) => (
        <option key={h} value={h}>
          {String(h).padStart(2, '0')}:00
        </option>
      ))}
    </select>
  );
}

export function NotificationsTab() {
  const t = useT();
  const notifyDms = useUi((s) => s.notifyDms);
  const notifyGroups = useUi((s) => s.notifyGroups);
  const notifyOnlyUnfocused = useUi((s) => s.notifyOnlyUnfocused);
  const setNotifyDms = useUi((s) => s.setNotifyDms);
  const setNotifyGroups = useUi((s) => s.setNotifyGroups);
  const setNotifyOnlyUnfocused = useUi((s) => s.setNotifyOnlyUnfocused);
  const notifySoundEnabled = useUi((s) => s.notifySoundEnabled);
  const setNotifySoundEnabled = useUi((s) => s.setNotifySoundEnabled);
  const notifyNative = useUi((s) => s.notifyNative);
  const setNotifyNative = useUi((s) => s.setNotifyNative);
  const notifySoundMode = useUi((s) => s.notifySoundMode);
  const setNotifySoundMode = useUi((s) => s.setNotifySoundMode);
  const quietHours = useUi((s) => s.quietHours);
  const setQuietHours = useUi((s) => s.setQuietHours);

  const soundModes: { id: NotifySoundMode; label: string }[] = [
    { id: 'all', label: t.settings.notifSoundModeAll },
    { id: 'mentionsOnly', label: t.settings.notifSoundModeMentions },
    { id: 'none', label: t.settings.notifSoundModeNone },
  ];

  const [permission, setPermission] = useState<NotificationPermission | null>(null);

  useEffect(() => {
    let cancelled = false;
    void queryNotificationPermission().then((state) => {
      if (!cancelled) setPermission(state);
    });
    return () => {
      cancelled = true;
    };
  }, []);

  const askPermission = (): void => {
    void requestNotificationPermission().then(setPermission);
  };

  const permissionLabel =
    permission === 'granted'
      ? t.settings.notifPermissionGranted
      : permission === 'unavailable'
        ? t.settings.notifPermissionUnavailable
        : t.settings.notifPermissionDenied;

  return (
    <div>
      <SettingsSection title={t.settings.notifPermissionTitle}>
        <div className="flex flex-wrap items-center justify-between gap-3 rounded-lg bg-sidebar px-4 py-3">
          <span
            className={`text-sm ${permission === 'granted' ? 'text-green' : 'text-muted'}`}
          >
            {permissionLabel}
          </span>
          {permission === 'denied' && (
            <button
              type="button"
              onClick={askPermission}
              className="rounded-lg bg-blurple px-4 py-2 text-sm font-medium text-white transition-colors duration-fast hover:bg-blurple-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar"
            >
              {t.settings.notifAllow}
            </button>
          )}
        </div>
      </SettingsSection>

      <SettingsSection
        title={t.settings.notifPrefsTitle}
        hint={t.settings.notifPrivacyHint}
      >
        <ToggleRow
          label={t.settings.notifDms}
          checked={notifyDms}
          onChange={setNotifyDms}
        />
        <ToggleRow
          label={t.settings.notifGroups}
          checked={notifyGroups}
          onChange={setNotifyGroups}
        />
        <ToggleRow
          label={t.settings.notifOnlyUnfocused}
          checked={notifyOnlyUnfocused}
          onChange={setNotifyOnlyUnfocused}
        />
      </SettingsSection>

      <SettingsSection title={t.settings.quietTitle} hint={t.settings.quietHint}>
        <ToggleRow
          label={t.settings.quietEnable}
          checked={quietHours.enabled}
          onChange={(enabled) => setQuietHours({ ...quietHours, enabled })}
        />
        <div
          className={`mt-2 flex items-center gap-2 text-sm ${
            quietHours.enabled ? 'text-norm' : 'pointer-events-none opacity-50'
          }`}
        >
          <span>{t.settings.quietFrom}</span>
          <HourSelect
            value={quietHours.start}
            onChange={(start) => setQuietHours({ ...quietHours, start })}
          />
          <span>{t.settings.quietTo}</span>
          <HourSelect
            value={quietHours.end}
            onChange={(end) => setQuietHours({ ...quietHours, end })}
          />
        </div>
      </SettingsSection>

      <SettingsSection title={t.settings.notifMasterTitle}>
        <ToggleRow
          label={t.settings.notifSoundEnabled}
          hint={t.settings.notifSoundEnabledHint}
          checked={notifySoundEnabled}
          onChange={setNotifySoundEnabled}
        />
        <ToggleRow
          label={t.settings.notifNativeEnabled}
          hint={t.settings.notifNativeEnabledHint}
          checked={notifyNative}
          onChange={setNotifyNative}
        />
      </SettingsSection>

      <SettingsSection title={t.settings.notifSoundModeTitle}>
        <div className="flex flex-wrap gap-2">
          {soundModes.map(({ id, label }) => (
            <OptionPill
              key={id}
              selected={notifySoundMode === id}
              onSelect={() => setNotifySoundMode(id)}
            >
              {label}
            </OptionPill>
          ))}
        </div>
      </SettingsSection>
    </div>
  );
}
