/**
 * Paramètres du serveur en plein écran, même disposition que SettingsModal :
 * catégories à gauche (Profil, Salons, Rôles, Membres, Bannis), contenu à
 * droite, « Quitter le serveur » en pied de colonne (refusé au fondateur
 * tant qu'il reste d'autres membres). Déclenché par
 * `ui.modal = { kind: 'serverSettings', groupId }`.
 */

import { useEffect, useState } from 'react';
import type { ComponentType } from 'react';
import { interpolate } from '../../i18n';
import type { Dict } from '../../i18n';
import { useGroups, hasPerm, PERMISSIONS } from '../../stores/groups';
import { useSession } from '../../stores/session';
import { useUi, useT } from '../../stores/ui';
import { ConfirmButton, messageOf } from './controls';
import { ServerAuditTab } from './ServerAuditTab';
import { ServerBansTab } from './ServerBansTab';
import { ServerChannelsTab } from './ServerChannelsTab';
import { ServerEmojisTab } from './ServerEmojisTab';
import { ServerMembersTab } from './ServerMembersTab';
import { ServerProfileTab } from './ServerProfileTab';
import { ServerRolesTab } from './ServerRolesTab';

type ServerTabId =
  | 'profile'
  | 'channels'
  | 'roles'
  | 'emojis'
  | 'members'
  | 'bans'
  | 'audit';

interface ServerTab {
  id: ServerTabId;
  label: (t: Dict) => string;
  Content: ComponentType<{ groupId: string }>;
  /** Prédicat de visibilité selon le bitfield de permissions courant. */
  visible?: (perms: number) => boolean;
}

const TABS: ServerTab[] = [
  { id: 'profile', label: (t) => t.serveur.tabProfile, Content: ServerProfileTab },
  { id: 'channels', label: (t) => t.serveur.tabChannels, Content: ServerChannelsTab },
  { id: 'roles', label: (t) => t.serveur.tabRoles, Content: ServerRolesTab },
  {
    id: 'emojis',
    label: (t) => t.serveur.tabEmojis,
    Content: ServerEmojisTab,
    visible: (perms) => hasPerm(perms, PERMISSIONS.MANAGE_EMOJIS),
  },
  { id: 'members', label: (t) => t.serveur.tabMembers, Content: ServerMembersTab },
  { id: 'bans', label: (t) => t.serveur.tabBans, Content: ServerBansTab },
  {
    id: 'audit',
    label: (t) => t.serveur.tabAudit,
    Content: ServerAuditTab,
    // Read-only view of the signed op-log: ADMIN (the founder holds it
    // implicitly through my_permissions).
    visible: (perms) => hasPerm(perms, PERMISSIONS.ADMIN),
  },
];

export function ServerSettingsModal({ groupId }: { groupId: string }) {
  const t = useT();
  const closeModal = useUi((s) => s.closeModal);
  const setView = useUi((s) => s.setView);
  const toast = useUi((s) => s.toast);
  const state = useGroups((s) => s.states[groupId]);
  const leave = useGroups((s) => s.leave);
  const self = useSession((s) => s.self);
  const [tabId, setTabId] = useState<ServerTabId>('profile');

  useEffect(() => {
    const onKey = (e: KeyboardEvent): void => {
      if (e.key === 'Escape') closeModal();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [closeModal]);

  if (!state) return null;

  const visibleTabs = TABS.filter(
    (tab) => tab.visible === undefined || tab.visible(state.my_permissions),
  );
  const active = visibleTabs.find((tab) => tab.id === tabId) ?? visibleTabs[0];
  if (active === undefined) return null;
  const Content = active.Content;

  const isFounder = self !== null && state.founder === self.pubkey;
  // Le contrat refuse le départ du fondateur tant qu'il reste des membres.
  const founderBlocked = isFounder && state.members.length > 1;

  const doLeave = (): void => {
    leave(groupId)
      .then(() => {
        toast('info', t.serveur.left);
        closeModal();
        setView({ kind: 'friends' });
      })
      .catch((e: unknown) => toast('error', messageOf(e, t.errors.actionFailed)));
  };

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-label={t.serveur.settingsTitle}
      className="modal-overlay-enter fixed inset-0 z-40 flex bg-chat"
    >
      <nav
        aria-label={t.serveur.settingsTitle}
        className="flex w-1/3 min-w-[232px] shrink-0 justify-end overflow-y-auto bg-sidebar pb-10 pl-4 pr-2 pt-14"
      >
        <div className="flex w-[212px] flex-col">
          <div className="truncate px-2.5 pb-1.5 text-xs font-bold uppercase tracking-wide text-faint">
            {state.name}
          </div>
          {visibleTabs.map((tab) => (
            <button
              key={tab.id}
              type="button"
              aria-current={tabId === tab.id ? 'page' : undefined}
              onClick={() => setTabId(tab.id)}
              className={`mb-0.5 block w-full rounded px-2.5 py-1.5 text-left font-medium transition-colors duration-150 ${
                tabId === tab.id
                  ? 'bg-chat-hover text-header'
                  : 'text-muted hover:bg-chat-hover hover:text-norm'
              }`}
            >
              {tab.label(t)}
            </button>
          ))}
          <div className="mx-2.5 my-2 h-px bg-input" role="separator" />
          <div className="px-2.5">
            <ConfirmButton
              action={t.serveur.leave}
              question={interpolate(t.serveur.leaveConfirm, { name: state.name })}
              onConfirm={doLeave}
              disabled={founderBlocked}
            />
            {founderBlocked && (
              <p className="mt-2 text-xs text-faint">{t.serveur.founderCannotLeave}</p>
            )}
          </div>
        </div>
      </nav>

      <div className="flex min-w-0 flex-1">
        <section
          aria-label={active.label(t)}
          className="min-w-0 max-w-[740px] flex-1 overflow-y-auto px-10 pb-20 pt-14"
        >
          <h2 className="mb-6 text-xl font-bold text-header">{active.label(t)}</h2>
          <Content groupId={groupId} />
        </section>
        <div className="w-[84px] shrink-0 pt-14">
          <button
            type="button"
            aria-label={t.app.close}
            onClick={closeModal}
            className="group flex flex-col items-center"
          >
            <span className="flex h-9 w-9 items-center justify-center rounded-full border-2 border-faint text-faint transition-colors duration-150 group-hover:border-norm group-hover:text-norm">
              <svg
                width="18"
                height="18"
                viewBox="0 0 24 24"
                fill="currentColor"
                aria-hidden
              >
                <path d="M6.3 5 12 10.6 17.7 5 19 6.3 13.4 12l5.6 5.7-1.3 1.3-5.7-5.6L6.3 19 5 17.7l5.6-5.7L5 6.3 6.3 5Z" />
              </svg>
            </span>
            <span className="mt-1.5 text-xs font-semibold uppercase text-faint">
              {t.settings.escKey}
            </span>
          </button>
        </div>
      </div>
    </div>
  );
}
