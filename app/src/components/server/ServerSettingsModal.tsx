/**
 * Paramètres du serveur, même coquille flottante que SettingsModal : fond
 * assombri + flouté, panneau centré avec catégories à gauche (Profil,
 * Salons, Rôles, Membres, Bannis), contenu à droite, « Quitter le
 * serveur » en pied de colonne (refusé au fondateur tant qu'il reste
 * d'autres membres). Déclenché par
 * `ui.modal = { kind: 'serverSettings', groupId }`.
 */

import { useEffect, useRef, useState } from 'react';
import type { ComponentType } from 'react';
import { interpolate } from '../../i18n';
import type { Dict } from '../../i18n';
import { bouclerTab, deplacerFocusVertical } from '../../lib/focus';
import { useGroups, hasPerm, PERMISSIONS } from '../../stores/groups';
import { useSession } from '../../stores/session';
import { useUi, useT } from '../../stores/ui';
import { CloseIcon } from '../ContextMenu';
import { ConfirmButton, messageOf } from './controls';
import { ServerAuditTab } from './ServerAuditTab';
import { ServerAutomodTab } from './ServerAutomodTab';
import { ServerBansTab } from './ServerBansTab';
import { ServerChannelsTab } from './ServerChannelsTab';
import { ServerEmojisTab } from './ServerEmojisTab';
import { ServerMembersTab } from './ServerMembersTab';
import { ServerProfileTab } from './ServerProfileTab';
import { ServerRolesTab } from './ServerRolesTab';
import { ServerSoundsTab } from './ServerSoundsTab';
import { ServerStickersTab } from './ServerStickersTab';

export type ServerTabId =
  | 'profile'
  | 'channels'
  | 'automod'
  | 'roles'
  | 'emojis'
  | 'stickers'
  | 'soundboard'
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
  {
    id: 'automod',
    label: (t) => t.serveur.tabAutomod,
    Content: ServerAutomodTab,
    // Le filtre de mots relève de la gestion du serveur (MANAGE_CHANNELS),
    // comme la porte `groups.automod.set` côté nœud.
    visible: (perms) => hasPerm(perms, PERMISSIONS.MANAGE_CHANNELS),
  },
  { id: 'roles', label: (t) => t.serveur.tabRoles, Content: ServerRolesTab },
  {
    id: 'emojis',
    label: (t) => t.serveur.tabEmojis,
    Content: ServerEmojisTab,
    visible: (perms) => hasPerm(perms, PERMISSIONS.MANAGE_EMOJIS),
  },
  {
    id: 'stickers',
    label: (t) => t.serveur.tabStickers,
    Content: ServerStickersTab,
    // Stickers réutilisent MANAGE_EMOJIS (même famille que les émojis, contrat).
    visible: (perms) => hasPerm(perms, PERMISSIONS.MANAGE_EMOJIS),
  },
  {
    id: 'soundboard',
    label: (t) => t.soundboard.tabTitle,
    Content: ServerSoundsTab,
    // Sons de soundboard : même porte que les émojis/stickers (MANAGE_EMOJIS).
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

export function ServerSettingsModal({
  groupId,
  initialTab,
}: {
  groupId: string;
  /** Onglet initial (ex. le menu du serveur ouvre directement Salons). */
  initialTab?: ServerTabId;
}) {
  const t = useT();
  const closeModal = useUi((s) => s.closeModal);
  const setView = useUi((s) => s.setView);
  const toast = useUi((s) => s.toast);
  const state = useGroups((s) => s.states[groupId]);
  const leave = useGroups((s) => s.leave);
  const self = useSession((s) => s.self);
  const [tabId, setTabId] = useState<ServerTabId>(initialTab ?? 'profile');
  const navRef = useRef<HTMLElement>(null);
  const dialogRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const onKey = (e: KeyboardEvent): void => {
      if (e.key === 'Escape') closeModal();
    };
    window.addEventListener('keydown', onKey);
    // Le clavier démarre sur l'onglet actif, puis revient au déclencheur
    // (menu ou icône ayant ouvert les paramètres) à la fermeture — même
    // schéma que `SettingsModal`.
    const declencheur =
      document.activeElement instanceof HTMLElement ? document.activeElement : null;
    navRef.current?.querySelector<HTMLButtonElement>('[aria-current="page"]')?.focus();
    return () => {
      window.removeEventListener('keydown', onKey);
      if (declencheur !== null && declencheur.isConnected) declencheur.focus();
    };
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
      className="modal-overlay-enter fixed inset-0 z-40 flex items-center justify-center"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) closeModal();
      }}
    >
      <div
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-label={t.serveur.settingsTitle}
        onKeyDown={(e) => bouclerTab(e, dialogRef.current)}
        className="liquid-settings modal-panel-enter relative flex h-[94vh] w-[min(1100px,94vw)] overflow-hidden rounded-xl max-sm:h-full max-sm:w-full max-sm:rounded-none"
      >
        <nav
          ref={navRef}
          aria-label={t.serveur.settingsTitle}
          onKeyDown={(e) => deplacerFocusVertical(e, navRef.current)}
          className="liquid-settings-nav flex w-[30%] min-w-[180px] shrink-0 justify-end overflow-y-auto border-r pb-8 pl-3 pr-2 pt-12 max-sm:w-[132px] max-sm:min-w-[132px] max-sm:pl-2 max-sm:pt-14"
        >
          <div className="flex w-[212px] flex-col max-sm:w-full">
            <div className="truncate px-2.5 pb-1.5 text-xs font-medium uppercase tracking-wide text-faint">
              {state.name}
            </div>
            {visibleTabs.map((tab) => (
              <button
                key={tab.id}
                type="button"
                aria-current={tabId === tab.id ? 'page' : undefined}
                onClick={() => setTabId(tab.id)}
                className={`mb-0.5 block min-h-9 w-full truncate rounded-md px-2.5 py-1.5 text-left text-sm font-medium transition-colors duration-fast focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar ${
                  tabId === tab.id
                    ? 'bg-blurple/15 text-header ring-1 ring-inset ring-blurple/20'
                    : 'text-muted hover:bg-chat-hover hover:text-norm'
                }`}
              >
                {tab.label(t)}
              </button>
            ))}
            <div className="mx-2.5 my-2 h-px bg-input/60" role="separator" />
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
            className="liquid-settings-content min-w-0 max-w-[740px] flex-1 overflow-y-auto px-6 pb-20 pt-14 max-sm:px-4"
          >
            <h2 className="mb-6 text-xl font-semibold text-header">{active.label(t)}</h2>
            <Content groupId={groupId} />
          </section>
          <div className="absolute right-3 top-3">
            <button
              type="button"
              aria-label={t.app.close}
              onClick={closeModal}
              className="group flex flex-col items-center focus-visible:outline-none"
            >
              <span className="flex h-10 w-10 items-center justify-center rounded-full border-2 border-faint text-faint transition-colors duration-fast group-hover:border-norm group-hover:text-norm group-focus-visible:ring-2 group-focus-visible:ring-blurple group-focus-visible:ring-offset-2 group-focus-visible:ring-offset-chat">
                <CloseIcon size={18} />
              </span>
              <span className="mt-1.5 text-xs font-medium uppercase text-faint max-sm:hidden">
                {t.settings.escKey}
              </span>
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
