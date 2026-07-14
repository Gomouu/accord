/**
 * Onglet Confidentialité : accusés de lecture (émission désactivable),
 * indicateur de frappe (émission désactivable — réception intacte), statut
 * de présence forcé au démarrage, liste des utilisateurs bloqués (avec
 * déblocage) et rappel du fonctionnement anti-spam des demandes d'amis.
 */

import { useEffect, useState } from 'react';
import { api } from '../../lib/client';
import { useFriends } from '../../stores/friends';
import { useUi, useT, type StartupPresence } from '../../stores/ui';
import { Avatar } from '../Avatar';
import { OptionPill, SettingsSection, ToggleRow } from './controls';

export function PrivacyTab() {
  const t = useT();
  const toast = useUi((s) => s.toast);
  const contacts = useFriends((s) => s.contacts);
  const load = useFriends((s) => s.load);
  const unblock = useFriends((s) => s.unblock);
  const typingIndicatorEnabled = useUi((s) => s.typingIndicatorEnabled);
  const setTypingIndicatorEnabled = useUi((s) => s.setTypingIndicatorEnabled);
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

  return (
    <div>
      <SettingsSection title={t.settings.readReceiptsTitle}>
        <ToggleRow
          label={t.settings.readReceiptsLabel}
          hint={t.settings.readReceiptsHint}
          checked={readReceipts ?? true}
          onChange={toggleReadReceipts}
        />
      </SettingsSection>

      <SettingsSection title={t.settings.typingIndicatorTitle}>
        <ToggleRow
          label={t.settings.typingIndicatorLabel}
          hint={t.settings.typingIndicatorHint}
          checked={typingIndicatorEnabled}
          onChange={setTypingIndicatorEnabled}
        />
      </SettingsSection>

      <SettingsSection
        title={t.settings.startupPresenceTitle}
        hint={t.settings.startupPresenceHint}
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

      <SettingsSection title={t.settings.blockedUsers}>
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
                    className="rounded-md bg-rail px-3 py-1.5 text-sm font-medium text-norm transition-colors duration-fast hover:bg-red hover:text-white focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar"
                  >
                    {t.friends.unblock}
                  </button>
                </li>
              );
            })}
          </ul>
        )}
      </SettingsSection>

      <SettingsSection title={t.settings.antiSpamTitle}>
        <p className="rounded-lg border-l-4 border-blurple bg-sidebar px-4 py-3 text-sm leading-relaxed text-muted">
          {t.settings.antiSpamHint}
        </p>
      </SettingsSection>
    </div>
  );
}
