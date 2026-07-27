/**
 * Onglet Confidentialité : verrouillage automatique du coffre, accusés de lecture (émission désactivable),
 * indicateur de frappe (émission désactivable — réception intacte), statut
 * de présence forcé au démarrage, liste des utilisateurs bloqués (avec
 * déblocage) et rappel du fonctionnement anti-spam des demandes d'amis.
 */

import { useEffect, useState } from 'react';
import { api } from '../../lib/client';
import { useFriends } from '../../stores/friends';
import { interpolate } from '../../i18n';
import {
  AUTO_LOCK_CHOICES,
  useUi,
  useSettingsT,
  useT,
  type StartupPresence,
} from '../../stores/ui';
import { Avatar } from '../Avatar';
import { PrivacyDashboard } from '../PrivacyDashboard';
import { OptionPill, SettingsSection, ToggleRow } from './controls';

export function PrivacyTab() {
  const t = useT();
  const ts = useSettingsT();
  const toast = useUi((s) => s.toast);
  const contacts = useFriends((s) => s.contacts);
  const load = useFriends((s) => s.load);
  const unblock = useFriends((s) => s.unblock);
  const typingIndicatorEnabled = useUi((s) => s.typingIndicatorEnabled);
  const setTypingIndicatorEnabled = useUi((s) => s.setTypingIndicatorEnabled);
  const streamerMode = useUi((s) => s.streamerMode);
  const linkPreviews = useUi((s) => s.linkPreviews);
  const setLinkPreviews = useUi((s) => s.setLinkPreviews);
  const setStreamerMode = useUi((s) => s.setStreamerMode);
  const autoLockMinutes = useUi((s) => s.autoLockMinutes);
  const setAutoLockMinutes = useUi((s) => s.setAutoLockMinutes);
  const startupPresence = useUi((s) => s.startupPresence);
  const setStartupPresence = useUi((s) => s.setStartupPresence);
  /** Réglage nœud (`dm.get_read_receipts`) ; `null` tant qu'il n'est pas lu. */
  const [readReceipts, setReadReceipts] = useState<boolean | null>(null);

  /** Choix d'un statut : re-cliquer le statut déjà choisi l'efface (`null`). */
  const toggleStartupPresence = (status: Exclude<StartupPresence, null>): void => {
    setStartupPresence(startupPresence === status ? null : status);
  };

  useEffect(() => {
    load().catch(() => toast('error', t.errors.loadFailed));
  }, [load, toast, t]);

  useEffect(() => {
    let alive = true;
    api
      .dmGetReadReceipts()
      .then(({ enabled }) => {
        if (alive) setReadReceipts(enabled);
      })
      .catch(() => {
        if (alive) toast('error', t.errors.loadFailed);
      });
    return () => {
      alive = false;
    };
  }, [toast, t]);

  const toggleReadReceipts = (enabled: boolean): void => {
    // Optimiste : reflet immédiat, retour arrière si le nœud refuse.
    setReadReceipts(enabled);
    api.dmSetReadReceipts(enabled).catch(() => {
      setReadReceipts(!enabled);
      toast('error', t.errors.actionFailed);
    });
  };

  const blocked = contacts.filter((c) => c.state === 'blocked');

  const autoLockLabel = (minutes: number): string => {
    if (minutes === 0) return ts.settings.autoLockOff;
    if (minutes === 60) return ts.settings.autoLockHour;
    return interpolate(ts.settings.autoLockMinutes, { n: String(minutes) });
  };

  return (
    <div>
      <SettingsSection title={ts.settings.streamerTitle}>
        <ToggleRow
          label={ts.settings.streamerLabel}
          hint={ts.settings.streamerHint}
          checked={streamerMode}
          onChange={setStreamerMode}
        />
      </SettingsSection>

      <SettingsSection title={ts.settings.linkPreviewsTitle}>
        <ToggleRow
          label={ts.settings.linkPreviewsLabel}
          hint={ts.settings.linkPreviewsHint}
          checked={linkPreviews}
          onChange={setLinkPreviews}
        />
      </SettingsSection>

      <SettingsSection title={ts.settings.autoLockTitle} hint={ts.settings.autoLockHint}>
        <div
          role="group"
          aria-label={ts.settings.autoLockTitle}
          className="flex flex-wrap gap-2"
        >
          {AUTO_LOCK_CHOICES.map((minutes) => (
            <button
              key={minutes}
              type="button"
              aria-pressed={autoLockMinutes === minutes}
              onClick={() => setAutoLockMinutes(minutes)}
              className={`min-h-9 rounded-full px-4 py-1.5 text-sm font-medium transition-colors duration-fast focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple ${
                autoLockMinutes === minutes
                  ? 'bg-blurple text-white'
                  : 'bg-input text-norm hover:bg-chat-hover'
              }`}
            >
              {autoLockLabel(minutes)}
            </button>
          ))}
        </div>
      </SettingsSection>

      <SettingsSection title={ts.settings.readReceiptsTitle}>
        <ToggleRow
          label={ts.settings.readReceiptsLabel}
          hint={ts.settings.readReceiptsHint}
          checked={readReceipts ?? true}
          onChange={toggleReadReceipts}
        />
      </SettingsSection>

      <SettingsSection title={ts.settings.typingIndicatorTitle}>
        <ToggleRow
          label={ts.settings.typingIndicatorLabel}
          hint={ts.settings.typingIndicatorHint}
          checked={typingIndicatorEnabled}
          onChange={setTypingIndicatorEnabled}
        />
      </SettingsSection>

      <SettingsSection
        title={ts.settings.startupPresenceTitle}
        hint={ts.settings.startupPresenceHint}
      >
        <div className="flex flex-wrap gap-2">
          <OptionPill
            selected={startupPresence === 'online'}
            onSelect={() => toggleStartupPresence('online')}
          >
            {t.profil.online}
          </OptionPill>
          <OptionPill
            selected={startupPresence === 'invisible'}
            onSelect={() => toggleStartupPresence('invisible')}
          >
            {t.profil.invisible}
          </OptionPill>
        </div>
      </SettingsSection>

      <SettingsSection title={ts.settings.blockedUsers}>
        {blocked.length === 0 ? (
          <p className="rounded-lg bg-sidebar px-4 py-6 text-center text-sm text-muted">
            {t.friends.emptyBlocked}
          </p>
        ) : (
          <ul className="divide-y divide-input rounded-lg bg-sidebar px-2">
            {blocked.map((contact) => {
              const name = contact.display_name.trim() || contact.friend_code;
              return (
                <li key={contact.pubkey} className="flex items-center gap-3 px-2 py-2.5">
                  <Avatar
                    id={contact.pubkey}
                    name={name}
                    size={32}
                    decoration={contact.avatar_decoration ?? null}
                  />
                  <div className="min-w-0 flex-1">
                    <div className="truncate text-sm font-medium text-header">{name}</div>
                    <div className="truncate text-xs text-faint">
                      {contact.friend_code}
                    </div>
                  </div>
                  <button
                    type="button"
                    onClick={() => {
                      unblock(contact.pubkey).catch(() =>
                        toast('error', t.errors.actionFailed),
                      );
                    }}
                    className="rounded-md bg-rail px-3 py-1.5 text-sm font-medium text-norm transition-colors duration-fast hover:bg-red hover:text-on-red focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar"
                  >
                    {t.friends.unblock}
                  </button>
                </li>
              );
            })}
          </ul>
        )}
      </SettingsSection>

      <SettingsSection title={ts.settings.antiSpamTitle}>
        <p className="rounded-lg border-s-4 border-blurple bg-sidebar px-4 py-3 text-sm leading-relaxed text-muted">
          {ts.settings.antiSpamHint}
        </p>
      </SettingsSection>

      <PrivacyDashboard />
    </div>
  );
}
