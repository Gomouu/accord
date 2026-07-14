/**
 * Rail des serveurs (colonne de gauche, 72 px) : accueil/MP, groupes (icône
 * publiée ou initiales en repli), création de groupe — fidèle à Discord.
 */

import { useEffect, useState } from 'react';
import { interpolate } from '../i18n';
import { copyToClipboard } from '../lib/clipboard';
import { useContextMenu, type ContextMenuItem } from '../stores/contextMenu';
import { folderOfServer, useFolders, type ServerFolder } from '../stores/folders';
import { totalDmMentions, totalDmUnread, useFriends } from '../stores/friends';
import {
  useGroups,
  sortChannels,
  channelKey,
  hasPerm,
  PERMISSIONS,
} from '../stores/groups';
import { serverLevel, useMute } from '../stores/mute';
import { useSession } from '../stores/session';
import { useUi, useT } from '../stores/ui';
import {
  BellOffMenuIcon,
  buildNotifLevelItems,
  CheckMenuIcon,
  CopyMenuIcon,
  EnvelopeMenuIcon,
  GearMenuIcon,
  LeaveMenuIcon,
  PlusMenuIcon,
} from './ContextMenu';
import { lireFichier } from '../lib/files';
import { estOuvertureMenu, pointAncrageMenu } from '../lib/focus';
import { initials } from '../lib/format';
import type { GroupStateJson } from '../lib/api';

/** Salon ouvert à l'arrivée dans un groupe : premier non-vocal en position. */
function defaultChannelId(state: GroupStateJson | undefined): string | null {
  if (state === undefined) return null;
  const first = sortChannels(state.channels).find((c) => c.kind !== 'voice');
  return first?.channel_id ?? null;
}

/**
 * Salon à ouvrir en resélectionnant un serveur : le dernier consulté s'il
 * existe encore et n'est pas vocal (supprimé entretemps sinon), le premier
 * salon par défaut à défaut de mémoire valide. Exportée pour être testée
 * isolément (garde anti-id-périmé).
 */
export function channelToRestore(
  state: GroupStateJson | undefined,
  remembered: string | undefined,
): string | null {
  if (remembered !== undefined) {
    const stillThere = state?.channels.some(
      (c) => c.channel_id === remembered && c.kind !== 'voice',
    );
    if (stillThere === true) return remembered;
  }
  return defaultChannelId(state);
}

/** Icône d'un serveur : image du magasin de fichiers, initiales en repli. */
function ServerIcon({ icon, name }: { icon: string | null; name: string }) {
  const [url, setUrl] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    setUrl(null);
    if (icon === null) return undefined;
    lireFichier(icon)
      .then((blobUrl) => {
        if (alive) setUrl(blobUrl);
      })
      .catch(() => {
        // Icône indisponible (téléchargement en échec) : repli initiales.
      });
    return () => {
      alive = false;
    };
  }, [icon]);

  if (url === null) return <>{initials(name)}</>;
  return <img src={url} alt="" className="h-full w-full object-cover" />;
}

/** Icône « calendrier » du jeu de menu (14 px, création d'événement). */
function EventMenuIcon() {
  return (
    <svg
      width={14}
      height={14}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
    >
      <rect width="18" height="18" x="3" y="4" rx="2" />
      <path d="M16 2v4" />
      <path d="M8 2v4" />
      <path d="M3 10h18" />
    </svg>
  );
}

/** Glyphe dossier partagé (pastille du rail et items de menu contextuel). */
function FolderSvg({ size }: { size: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="currentColor" aria-hidden>
      <path d="M4 5.5A1.5 1.5 0 0 1 5.5 4h4.2a1.5 1.5 0 0 1 1.13.52l1.77 1.98h5.9A1.5 1.5 0 0 1 20 8v10a1.5 1.5 0 0 1-1.5 1.5h-13A1.5 1.5 0 0 1 4 18V5.5Z" />
    </svg>
  );
}

/** Compteur de non-lus/mentions à afficher sur une icône du rail. */
interface RailBadgeInfo {
  count: number;
  /** Mention (pastille distincte, « @ ») plutôt que simple non-lu. */
  mention: boolean;
}

/** Libellé accessible du compteur, ajouté au `label`/`title` du bouton. */
function badgeSuffix(t: ReturnType<typeof useT>, badge: RailBadgeInfo): string {
  if (badge.count <= 0) return '';
  const text = badge.mention
    ? interpolate(t.mentions.badge, { count: String(badge.count) })
    : interpolate(t.dm.unreadBadge, { count: String(badge.count) });
  return ` — ${text}`;
}

/** Pastille rouge (non-lu ou mention) posée sur le coin d'une icône du rail. */
function RailBadge({ badge }: { badge: RailBadgeInfo }) {
  if (badge.count <= 0) return null;
  return (
    <span
      aria-hidden
      className="badge-pop absolute right-1.5 top-0 z-10 flex min-w-[18px] items-center justify-center gap-0.5 rounded-full bg-red px-1 text-[11px] font-semibold leading-[18px] text-white ring-2 ring-rail"
    >
      {badge.mention && (
        <span aria-hidden className="font-semibold leading-none">
          @
        </span>
      )}
      {badge.count > 99 ? '99+' : badge.count}
    </span>
  );
}

function RailButton({
  label,
  active,
  accent,
  badge,
  muted,
  onClick,
  onMenu,
  children,
}: {
  label: string;
  active: boolean;
  accent?: boolean;
  badge?: RailBadgeInfo;
  /**
   * Sourdine active sur ce serveur (voir `stores/mute.ts`) : icône atténuée
   * (opacité, compositor-friendly) — la pastille de non-lu/mention, posée en
   * dehors du bouton, reste à pleine opacité (non-lu toujours suivi).
   */
  muted?: boolean;
  onClick: () => void;
  /**
   * Ouvre le menu contextuel de l'entrée aux coordonnées viewport données —
   * déclenché au clic droit (point de clic) comme au clavier (Maj+F10 ou
   * touche Menu, ancré au centre du bouton — voir `lib/focus.ts`).
   */
  onMenu?: (x: number, y: number) => void;
  children: React.ReactNode;
}) {
  return (
    <div className="group relative flex w-full justify-center">
      {/*
       * Pastille d'état à gauche, comme Discord : hauteur fixe (40px),
       * seule sa mise à l'échelle verticale (transform, compositor) anime
       * l'apparition/l'extension — jamais `height` directement.
       */}
      <span
        aria-hidden
        className={`absolute left-0 top-1/2 h-10 w-1 -translate-y-1/2 rounded-r bg-header transition-transform duration-normal ease-spring ${
          active ? 'scale-y-100' : 'scale-y-0 group-hover:scale-y-50'
        }`}
      />
      <button
        type="button"
        aria-label={label}
        aria-current={active ? 'page' : undefined}
        title={label}
        onClick={onClick}
        onContextMenu={
          onMenu === undefined
            ? undefined
            : (e) => {
                e.preventDefault();
                onMenu(e.clientX, e.clientY);
              }
        }
        onKeyDown={
          onMenu === undefined
            ? undefined
            : (e) => {
                if (!estOuvertureMenu(e)) return;
                e.preventDefault();
                const { x, y } = pointAncrageMenu(e.currentTarget);
                onMenu(x, y);
              }
        }
        className={`flex h-12 w-12 items-center justify-center overflow-hidden font-medium transition-[color,background-color,border-radius,transform] duration-normal active:scale-95 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-rail ${
          active
            ? 'rounded-server bg-blurple text-white'
            : accent
              ? 'rounded-full bg-sidebar text-green hover:rounded-server hover:bg-green hover:text-white'
              : 'rounded-full bg-sidebar text-norm hover:rounded-server hover:bg-blurple hover:text-white'
        } ${muted === true ? 'opacity-50' : ''}`}
      >
        {children}
      </button>
      {badge !== undefined && <RailBadge badge={badge} />}
    </div>
  );
}

export function ServerRail() {
  const t = useT();
  const view = useUi((s) => s.view);
  const setView = useUi((s) => s.setView);
  const openModal = useUi((s) => s.openModal);
  const ids = useGroups((s) => s.ids);
  const states = useGroups((s) => s.states);
  const groupMentions = useGroups((s) => s.mentions);
  const groupUnread = useGroups((s) => s.unread);
  const hideMutedChannels = useUi((s) => s.hideMutedChannels);
  const serverLevels = useMute((s) => s.serverLevels);
  const lastChannelByServer = useUi((s) => s.lastChannelByServer);
  const lastDmPeer = useUi((s) => s.lastDmPeer);
  const contacts = useFriends((s) => s.contacts);
  const self = useSession((s) => s.self);
  const toast = useUi((s) => s.toast);
  const folders = useFolders((s) => s.folders);

  const isHome = view.kind === 'friends' || view.kind === 'dm';

  /**
   * Pastille du bouton Accueil/MP : agrège tous les MP — une mention prime
   * sur le simple non-lu (même convention que la liste de conversations).
   */
  const dmMentionTotal = totalDmMentions(contacts);
  const dmBadge: RailBadgeInfo =
    dmMentionTotal > 0
      ? { count: dmMentionTotal, mention: true }
      : { count: totalDmUnread(contacts), mention: false };

  /**
   * Icône accueil/MP : rouvre la dernière conversation privée si l'amitié
   * tient toujours (pas retirée/bloquée entretemps), sinon la liste d'amis —
   * qui reste par ailleurs toujours atteignable via le bouton « Amis » de la
   * barre latérale.
   */
  const openHome = (): void => {
    const peer = lastDmPeer;
    if (
      peer !== null &&
      contacts.some((c) => c.pubkey === peer && c.state === 'friend')
    ) {
      setView({ kind: 'dm', peer });
      return;
    }
    setView({ kind: 'friends' });
  };

  /**
   * Entrées du rail : un serveur rangé dans un dossier (voir
   * `stores/folders.ts`) est rendu groupé sous la pastille de son dossier, à
   * la position du premier membre dans l'ordre du rail ; les serveurs hors
   * dossier restent à la racine, inchangés.
   */
  const railEntries: Array<
    | { kind: 'server'; id: string }
    | { kind: 'folder'; folder: ServerFolder; memberIds: string[] }
  > = [];
  const seenFolders = new Set<string>();
  for (const id of ids) {
    const folder = folderOfServer(folders, id);
    if (folder === null) {
      railEntries.push({ kind: 'server', id });
    } else if (!seenFolders.has(folder.id)) {
      seenFolders.add(folder.id);
      railEntries.push({
        kind: 'folder',
        folder,
        memberIds: ids.filter((memberId) => folder.serverIds.includes(memberId)),
      });
    }
  }

  /**
   * Menu contextuel d'un dossier : renommer, plier/déplier, supprimer —
   * suppression sans confirmation, rien n'est perdu (les serveurs
   * retournent simplement à la racine du rail).
   */
  const buildFolderItems = (folder: ServerFolder): ContextMenuItem[] => [
    {
      label: t.folders.rename,
      icon: <FolderSvg size={16} />,
      onClick: () => {
        const name = window.prompt(t.folders.namePrompt, folder.name);
        if (name === null || name.trim() === '') return;
        useFolders.getState().renameFolder(folder.id, name.trim());
      },
    },
    {
      label: folder.collapsed ? t.folders.expand : t.folders.collapse,
      icon: <FolderSvg size={16} />,
      onClick: () => useFolders.getState().toggleCollapsed(folder.id),
    },
    {
      label: t.folders.delete,
      icon: <FolderSvg size={16} />,
      danger: true,
      separatorBefore: true,
      onClick: () => useFolders.getState().deleteFolder(folder.id),
    },
  ];

  /** Icône d'un serveur du rail (racine ou membre d'un dossier déplié). */
  const renderServer = (id: string) => {
    const name = states[id]?.name ?? '…';
    const active = view.kind === 'group' && view.groupId === id;
    // Pastille de mention (rouge) : seules les mentions non lues du
    // serveur remontent ici (compteur `groups.list.mentions`, par
    // serveur) — le détail par salon reste dans la barre latérale.
    const badge: RailBadgeInfo = { count: groupMentions[id] ?? 0, mention: true };
    const level = serverLevel({ serverLevels, channelLevels: {} }, id);
    const muted = level === 'none';

    /**
     * Items du menu contextuel du serveur, ordonnés façon Discord : marquer
     * comme lu (si non-lus), invitation (D-045)/notifications/masquage des
     * salons muets, paramètres, créations (salon, catégorie, événement, si
     * MANAGE_CHANNELS), copie d'identifiant, rangement dans un dossier (local,
     * voir `stores/folders.ts`) et départ — omis si le fondateur ne peut pas
     * encore quitter (règle du contrat : d'autres membres restent). Le sous-menu
     * de notifications se rouvre au point de clic `x`/`y`. Les groupes sont
     * assemblés puis aplatis, un séparateur ouvrant chaque groupe non vide.
     */
    const buildServerItems = (x: number, y: number): ContextMenuItem[] => {
      const groupState = states[id];
      const isFounder = self !== null && groupState?.founder === self.pubkey;
      const founderBlocked = isFounder && (groupState?.members.length ?? 0) > 1;
      const perms = groupState?.my_permissions ?? 0;
      const canInvite = hasPerm(perms, PERMISSIONS.INVITE);
      const canManageChannels = hasPerm(perms, PERMISSIONS.MANAGE_CHANNELS);
      const serverHasUnread =
        (groupMentions[id] ?? 0) > 0 ||
        Object.values(groupUnread[id] ?? {}).some((n) => n > 0);

      const sections: ContextMenuItem[][] = [];

      // Marquer comme lu — seulement si le serveur a des non-lus (jamais un
      // no-op) : parcourt les salons texte/annonces à non-lus et réutilise
      // `markRead` salon par salon (même flux qu'à l'ouverture d'un salon).
      sections.push(
        serverHasUnread
          ? [
              {
                label: t.contextMenu.markAsRead,
                icon: <CheckMenuIcon />,
                onClick: () => {
                  void (async () => {
                    const g = useGroups.getState();
                    const chans = (g.states[id]?.channels ?? []).filter(
                      (c) => c.kind !== 'voice',
                    );
                    for (const ch of chans) {
                      if ((g.unread[id]?.[ch.channel_id] ?? 0) === 0) continue;
                      try {
                        await g.refreshHistory(id, ch.channel_id);
                        const last = (
                          useGroups.getState().messages[channelKey(id, ch.channel_id)] ??
                          []
                        ).at(-1);
                        if (last !== undefined) {
                          await useGroups
                            .getState()
                            .markRead(id, ch.channel_id, last.lamport);
                        }
                      } catch {
                        // Best effort : les autres salons continuent d'être marqués.
                      }
                    }
                  })();
                },
              },
            ]
          : [],
      );

      // Invitation, notifications, masquage des salons muets.
      const notifGroup: ContextMenuItem[] = [];
      if (canInvite) {
        notifGroup.push({
          label: t.groups.invitePeople,
          icon: <EnvelopeMenuIcon />,
          onClick: () => useUi.getState().openModal({ kind: 'invite', groupId: id }),
        });
      }
      notifGroup.push({
        label: t.notifLevel.title,
        icon: <BellOffMenuIcon />,
        onClick: () =>
          useContextMenu.getState().openMenu(
            x,
            y,
            buildNotifLevelItems(t.notifLevel, level, (lvl) =>
              useMute.getState().setServerLevel(id, lvl),
            ),
          ),
      });
      notifGroup.push({
        label: t.serveur.hideMutedChannels,
        icon: <BellOffMenuIcon />,
        checked: hideMutedChannels,
        onClick: () => useUi.getState().toggleHideMutedChannels(),
      });
      sections.push(notifGroup);

      // Paramètres du serveur.
      sections.push([
        {
          label: t.serveur.settingsTitle,
          icon: <GearMenuIcon />,
          onClick: () =>
            useUi.getState().openModal({ kind: 'serverSettings', groupId: id }),
        },
      ]);

      // Créations (MANAGE_CHANNELS) : salon, catégorie (onglet Salons des
      // paramètres, pas de modale dédiée — même choix que le menu du nom de
      // serveur), événement.
      const creations: ContextMenuItem[] = [];
      if (canManageChannels) {
        creations.push(
          {
            label: t.groups.addChannel,
            icon: <PlusMenuIcon />,
            onClick: () =>
              useUi.getState().openModal({ kind: 'createChannel', groupId: id }),
          },
          {
            label: t.serveur.createCategoryAction,
            icon: <PlusMenuIcon />,
            onClick: () =>
              useUi.getState().openModal({
                kind: 'serverSettings',
                groupId: id,
                initialTab: 'channels',
              }),
          },
          {
            label: t.groups.eventCreate,
            icon: <EventMenuIcon />,
            onClick: () => useUi.getState().openModal({ kind: 'events', groupId: id }),
          },
        );
      }
      sections.push(creations);

      // Copie d'identifiant.
      sections.push([
        {
          label: t.contextMenu.copyServerId,
          icon: <CopyMenuIcon />,
          onClick: () =>
            copyToClipboard(
              id,
              () => toast('info', t.app.copied),
              () => toast('error', t.errors.actionFailed),
            ),
        },
      ]);

      // Rangement dans un dossier (local, propre à Accord).
      const folderItems: ContextMenuItem[] = [];
      if (folderOfServer(folders, id) !== null) {
        folderItems.push({
          label: t.folders.removeFromFolder,
          icon: <FolderSvg size={16} />,
          onClick: () => useFolders.getState().removeServer(id),
        });
      } else {
        for (const f of folders) {
          folderItems.push({
            label: interpolate(t.folders.addToFolder, { name: f.name }),
            icon: <FolderSvg size={16} />,
            onClick: () => useFolders.getState().addServer(f.id, id),
          });
        }
        folderItems.push({
          label: t.folders.addToNew,
          icon: <FolderSvg size={16} />,
          onClick: () => {
            const folderName = window.prompt(t.folders.namePrompt, t.folders.defaultName);
            if (folderName === null || folderName.trim() === '') return;
            useFolders.getState().createFolder(folderName.trim(), [id]);
          },
        });
      }
      sections.push(folderItems);

      // Départ (omis si le fondateur ne peut pas encore quitter).
      sections.push(
        founderBlocked
          ? []
          : [
              {
                label: t.serveur.leave,
                icon: <LeaveMenuIcon />,
                danger: true,
                onClick: () => {
                  if (!window.confirm(interpolate(t.serveur.leaveConfirm, { name }))) {
                    return;
                  }
                  useGroups
                    .getState()
                    .leave(id)
                    .then(() => {
                      toast('info', t.serveur.left);
                      const current = useUi.getState().view;
                      if (current.kind === 'group' && current.groupId === id) {
                        setView({ kind: 'friends' });
                      }
                    })
                    .catch(() => toast('error', t.errors.actionFailed));
                },
              },
            ],
      );

      // Aplatit : un séparateur ouvre chaque groupe non vide après le premier.
      const items: ContextMenuItem[] = [];
      for (const section of sections) {
        section.forEach((item, i) => {
          items.push(
            i === 0 && items.length > 0 ? { ...item, separatorBefore: true } : item,
          );
        });
      }
      return items;
    };

    return (
      <RailButton
        key={id}
        label={`${name}${badgeSuffix(t, badge)}${muted ? ` — ${t.serveur.mutedLabel}` : ''}`}
        active={active}
        badge={badge}
        muted={muted}
        onClick={() =>
          setView({
            kind: 'group',
            groupId: id,
            channelId: channelToRestore(states[id], lastChannelByServer[id]),
          })
        }
        onMenu={(x, y) =>
          useContextMenu.getState().openMenu(x, y, buildServerItems(x, y))
        }
      >
        <ServerIcon icon={states[id]?.icon ?? null} name={name} />
      </RailButton>
    );
  };

  return (
    <nav
      aria-label={t.app.name}
      className="flex h-full w-[72px] flex-col items-center gap-2 overflow-y-auto bg-rail py-3"
    >
      <RailButton
        label={`${t.dm.directMessages}${badgeSuffix(t, dmBadge)}`}
        active={isHome}
        badge={dmBadge}
        onClick={openHome}
      >
        {/* Marque Accord : deux bulles liées. */}
        <svg width="26" height="26" viewBox="0 0 24 24" fill="currentColor" aria-hidden>
          <path d="M4 5.5A2.5 2.5 0 0 1 6.5 3h8A2.5 2.5 0 0 1 17 5.5v5a2.5 2.5 0 0 1-2.5 2.5H9l-3.6 2.7A.9.9 0 0 1 4 15V5.5Z" />
          <path
            opacity=".55"
            d="M18.5 8H19a2.5 2.5 0 0 1 2.5 2.5v8.6a.9.9 0 0 1-1.4.7L16.5 17H11a2.5 2.5 0 0 1-2.4-1.8h5.9a4 4 0 0 0 4-4V8Z"
          />
        </svg>
      </RailButton>

      <div className="h-0.5 w-8 rounded-full bg-sidebar" role="separator" />

      {railEntries.map((entry) => {
        if (entry.kind === 'server') return renderServer(entry.id);
        const { folder, memberIds } = entry;
        const folderLabel = interpolate(t.folders.folderLabel, { name: folder.name });
        return (
          <div
            key={`dossier-${folder.id}`}
            className={`flex w-full flex-col items-center gap-2 ${
              folder.collapsed ? '' : 'rounded-2xl bg-sidebar/40 pb-2'
            }`}
          >
            <RailButton
              label={`${folderLabel} — ${
                folder.collapsed ? t.folders.expand : t.folders.collapse
              }`}
              active={false}
              onClick={() => useFolders.getState().toggleCollapsed(folder.id)}
              onMenu={(x, y) =>
                useContextMenu.getState().openMenu(x, y, buildFolderItems(folder))
              }
            >
              {folder.collapsed ? (
                /* Plié : mini-aperçus des premières icônes du dossier. */
                <span aria-hidden className="grid h-9 w-9 grid-cols-2 gap-0.5">
                  {memberIds.slice(0, 4).map((memberId) => (
                    <span
                      key={memberId}
                      className="flex items-center justify-center overflow-hidden rounded-full bg-rail text-[8px] text-norm"
                    >
                      <ServerIcon
                        icon={states[memberId]?.icon ?? null}
                        name={states[memberId]?.name ?? '…'}
                      />
                    </span>
                  ))}
                </span>
              ) : (
                <span
                  className="flex items-center justify-center"
                  style={folder.color !== undefined ? { color: folder.color } : undefined}
                >
                  <FolderSvg size={22} />
                </span>
              )}
            </RailButton>
            {!folder.collapsed && memberIds.map((memberId) => renderServer(memberId))}
          </div>
        );
      })}

      <RailButton
        label={t.groups.create}
        active={false}
        accent
        onClick={() => openModal({ kind: 'createGroup' })}
      >
        <svg
          width="22"
          height="22"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth={2}
          strokeLinecap="round"
          strokeLinejoin="round"
          aria-hidden
        >
          <path d="M5 12h14" />
          <path d="M12 5v14" />
        </svg>
      </RailButton>
    </nav>
  );
}
