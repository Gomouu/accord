/**
 * Rail des serveurs (colonne de gauche, 72 px) : accueil/MP, groupes (icône
 * publiée ou initiales en repli), création de groupe — fidèle à Discord.
 */

import { useEffect, useState } from 'react';
import { useFriends } from '../stores/friends';
import { useGroups, sortChannels } from '../stores/groups';
import { useUi, useT } from '../stores/ui';
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

function RailButton({
  label,
  active,
  accent,
  onClick,
  children,
}: {
  label: string;
  active: boolean;
  accent?: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <div className="group relative flex w-full justify-center">
      {/* Pastille d'état à gauche, comme Discord. */}
      <span
        className={`absolute -left-0 top-1/2 w-1 -translate-y-1/2 rounded-r bg-white transition-all duration-200 ${
          active ? 'h-10' : 'h-2 scale-0 group-hover:scale-100 group-hover:h-5'
        }`}
      />
      <button
        type="button"
        aria-label={label}
        title={label}
        onClick={onClick}
        className={`flex h-12 w-12 items-center justify-center overflow-hidden font-medium transition-all duration-200 ${
          active
            ? 'rounded-server bg-blurple text-white'
            : accent
              ? 'rounded-full bg-sidebar text-green hover:rounded-server hover:bg-green hover:text-white'
              : 'rounded-full bg-sidebar text-norm hover:rounded-server hover:bg-blurple hover:text-white'
        }`}
      >
        {children}
      </button>
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
  const lastChannelByServer = useUi((s) => s.lastChannelByServer);
  const lastDmPeer = useUi((s) => s.lastDmPeer);
  const contacts = useFriends((s) => s.contacts);

  const isHome = view.kind === 'friends' || view.kind === 'dm';

  /**
   * Icône accueil/MP : rouvre la dernière conversation privée si l'amitié
   * tient toujours (pas retirée/bloquée entretemps), sinon la liste d'amis —
   * qui reste par ailleurs toujours atteignable via le bouton « Amis » de la
   * barre latérale.
   */
  const openHome = (): void => {
    const peer = lastDmPeer;
    if (peer !== null && contacts.some((c) => c.pubkey === peer && c.state === 'friend')) {
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
      <RailButton label={t.dm.directMessages} active={isHome} onClick={openHome}>
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
        return (
          <RailButton
            key={id}
            label={name}
            active={active}
            onClick={() =>
              setView({
                kind: 'group',
                groupId: id,
                channelId: channelToRestore(states[id], lastChannelByServer[id]),
              })
            }
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
