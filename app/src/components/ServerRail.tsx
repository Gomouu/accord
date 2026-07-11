/**
 * Rail des serveurs (colonne de gauche, 72 px) : accueil/MP, groupes (icône
 * publiée ou initiales en repli), création de groupe — fidèle à Discord.
 */

import { useEffect, useState } from 'react';
import { interpolate } from '../i18n';
import { copyToClipboard } from '../lib/clipboard';
import { useContextMenu, type ContextMenuItem } from '../stores/contextMenu';
import { totalDmMentions, totalDmUnread, useFriends } from '../stores/friends';
import { useGroups, sortChannels, hasPerm, PERMISSIONS } from '../stores/groups';
import { useSession } from '../stores/session';
import { useUi, useT } from '../stores/ui';
import { CopyMenuIcon, EnvelopeMenuIcon, GearMenuIcon, LeaveMenuIcon } from './ContextMenu';
import { lireFichier } from '../lib/files';
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
        <span aria-hidden className="font-bold leading-none">
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
  onClick,
  onContextMenu,
  children,
}: {
  label: string;
  active: boolean;
  accent?: boolean;
  badge?: RailBadgeInfo;
  onClick: () => void;
  onContextMenu?: (e: React.MouseEvent<HTMLButtonElement>) => void;
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
        className={`absolute -left-0 top-1/2 h-10 w-1 -translate-y-1/2 rounded-r bg-white transition-transform duration-normal ease-spring ${
          active ? 'scale-y-100' : 'scale-y-0 group-hover:scale-y-50'
        }`}
      />
      <button
        type="button"
        aria-label={label}
        title={label}
        onClick={onClick}
        onContextMenu={onContextMenu}
        className={`flex h-12 w-12 items-center justify-center overflow-hidden font-medium transition-[color,background-color,border-radius,transform] duration-normal active:scale-95 ${
          active
            ? 'rounded-server bg-blurple text-white'
            : accent
              ? 'rounded-full bg-sidebar text-green hover:rounded-server hover:bg-green hover:text-white'
              : 'rounded-full bg-sidebar text-norm hover:rounded-server hover:bg-blurple hover:text-white'
        }`}
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
  const lastChannelByServer = useUi((s) => s.lastChannelByServer);
  const lastDmPeer = useUi((s) => s.lastDmPeer);
  const contacts = useFriends((s) => s.contacts);
  const self = useSession((s) => s.self);
  const toast = useUi((s) => s.toast);

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

      <div className="h-0.5 w-8 rounded bg-sidebar" role="separator" />

      {ids.map((id) => {
        const name = states[id]?.name ?? '…';
        const active = view.kind === 'group' && view.groupId === id;
        // Pastille de mention (rouge) : seules les mentions non lues du
        // serveur remontent ici (compteur `groups.list.mentions`, par
        // serveur) — le détail par salon reste dans la barre latérale.
        const badge: RailBadgeInfo = { count: groupMentions[id] ?? 0, mention: true };

        /**
         * Items du menu contextuel du serveur : copie d'identifiant,
         * invitation (si permis, D-045 : consentement explicite — ouvre le
         * sélecteur d'ami existant), paramètres (mêmes actions que l'icône
         * ⚙️ du salon) et départ — omis si le fondateur ne peut pas encore
         * quitter (règle du contrat : d'autres membres restent). Pas de
         * « marquer comme lu » global : aucune action équivalente n'existe
         * côté store (seulement par salon, une fois ouvert).
         */
        const buildServerItems = (): ContextMenuItem[] => {
          const groupState = states[id];
          const isFounder = self !== null && groupState?.founder === self.pubkey;
          const founderBlocked = isFounder && (groupState?.members.length ?? 0) > 1;
          const canInvite = hasPerm(groupState?.my_permissions ?? 0, PERMISSIONS.INVITE);
          const items: ContextMenuItem[] = [
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
          ];
          if (canInvite) {
            items.push({
              label: t.groups.invitePeople,
              icon: <EnvelopeMenuIcon />,
              onClick: () => useUi.getState().openModal({ kind: 'invite', groupId: id }),
            });
          }
          items.push({
            label: t.serveur.settingsTitle,
            icon: <GearMenuIcon />,
            onClick: () => useUi.getState().openModal({ kind: 'serverSettings', groupId: id }),
          });
          if (!founderBlocked) {
            items.push({
              label: t.serveur.leave,
              icon: <LeaveMenuIcon />,
              danger: true,
              separatorBefore: true,
              onClick: () => {
                if (!window.confirm(interpolate(t.serveur.leaveConfirm, { name }))) return;
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
            });
          }
          return items;
        };

        return (
          <RailButton
            key={id}
            label={`${name}${badgeSuffix(t, badge)}`}
            active={active}
            badge={badge}
            onClick={() =>
              setView({
                kind: 'group',
                groupId: id,
                channelId: channelToRestore(states[id], lastChannelByServer[id]),
              })
            }
            onContextMenu={(e) => {
              e.preventDefault();
              useContextMenu.getState().openMenu(e.clientX, e.clientY, buildServerItems());
            }}
          >
            <ServerIcon icon={states[id]?.icon ?? null} name={name} />
          </RailButton>
        );
      })}

      <RailButton
        label={t.groups.create}
        active={false}
        accent
        onClick={() => openModal({ kind: 'createGroup' })}
      >
        <svg width="22" height="22" viewBox="0 0 24 24" fill="currentColor" aria-hidden>
          <path d="M11 5a1 1 0 1 1 2 0v6h6a1 1 0 1 1 0 2h-6v6a1 1 0 1 1-2 0v-6H5a1 1 0 1 1 0-2h6V5Z" />
        </svg>
      </RailButton>
    </nav>
  );
}
