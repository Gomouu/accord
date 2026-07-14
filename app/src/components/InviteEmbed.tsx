/**
 * Encart d'invitation façon Discord : rendu sous un message contenant un lien
 * `accord://invite/…`. Décode le lien via `groups.invite_link_info` (aucun
 * effet de bord côté nœud) pour afficher le nom, l'icône et la bannière du
 * serveur avant même de le rejoindre — l'icône/bannière sont récupérées en
 * P2P auprès de l'inviteur (indice de source = sa clé publique). Un bouton
 * rejoint le serveur (ou l'ouvre si l'on en est déjà membre).
 */

import { useEffect, useState } from 'react';
import { interpolate } from '../i18n';
import { api } from '../lib/client';
import { lireFichier } from '../lib/files';
import { useGroups } from '../stores/groups';
import { useT, useUi } from '../stores/ui';
import { ProfileBanner } from './ProfileBanner';

interface InviteInfo {
  group_id: string;
  invite_id: string;
  inviter: string;
  group_name: string;
  icon: string | null;
  banner: string | null;
  banner_color: number | null;
}

/** Icône du serveur (blob P2P via l'inviteur) ou repli sur les initiales. */
function InviteIcon({
  hash,
  hint,
  name,
}: {
  hash: string | null;
  hint: string;
  name: string;
}) {
  const [url, setUrl] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    setUrl(null);
    if (hash === null) return undefined;
    lireFichier(hash, hint)
      .then((blobUrl) => {
        if (alive) setUrl(blobUrl);
      })
      .catch(() => undefined);
    return () => {
      alive = false;
    };
  }, [hash, hint]);

  if (url === null) {
    return <span>{name.slice(0, 2).toUpperCase()}</span>;
  }
  return <img src={url} alt="" className="h-full w-full object-cover" />;
}

export function InviteEmbed({ link }: { link: string }) {
  const t = useT();
  const toast = useUi((s) => s.toast);
  const setView = useUi((s) => s.setView);
  const states = useGroups((s) => s.states);
  const loadList = useGroups((s) => s.loadList);
  const [info, setInfo] = useState<InviteInfo | null>(null);
  const [failed, setFailed] = useState(false);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    let alive = true;
    setInfo(null);
    setFailed(false);
    api
      .groupsInviteLinkInfo(link)
      .then((i) => {
        if (alive) setInfo(i);
      })
      .catch(() => {
        if (alive) setFailed(true);
      });
    return () => {
      alive = false;
    };
  }, [link]);

  if (failed || info === null) return null;

  const isMember = states[info.group_id] !== undefined;

  const rejoindre = async (): Promise<void> => {
    if (busy) return;
    setBusy(true);
    try {
      const res = await api.groupsInviteLinkRedeem(link);
      if (!res.ok) {
        toast('error', t.inviteEmbed.failed);
        setBusy(false);
        return;
      }
      await loadList();
      setView({ kind: 'group', groupId: res.group_id, channelId: null });
      toast('info', interpolate(t.inviteEmbed.joined, { name: res.group_name }));
    } catch {
      toast('error', t.inviteEmbed.failed);
    } finally {
      setBusy(false);
    }
  };

  const ouvrir = (): void => {
    setView({ kind: 'group', groupId: info.group_id, channelId: null });
  };

  return (
    <div className="mt-1.5 w-full max-w-sm overflow-hidden rounded-xl border border-rail bg-sidebar shadow-2 transition-shadow duration-fast hover:shadow-3">
      <ProfileBanner
        hash={info.banner}
        hint={info.inviter}
        color={info.banner_color}
        heightClassName="h-16"
      />
      <div className="flex items-center gap-3 px-3 pb-3">
        <div className="-mt-6 flex h-14 w-14 shrink-0 items-center justify-center overflow-hidden rounded-2xl border-4 border-sidebar bg-rail text-sm font-bold text-header">
          <InviteIcon hash={info.icon} hint={info.inviter} name={info.group_name} />
        </div>
        <div className="min-w-0 flex-1 pt-1">
          <div className="text-[11px] font-semibold uppercase tracking-wide text-faint">
            {t.inviteEmbed.label}
          </div>
          <div className="truncate text-[15px] font-semibold text-header">
            {info.group_name}
          </div>
        </div>
        <button
          type="button"
          disabled={busy}
          onClick={() => (isMember ? ouvrir() : void rejoindre())}
          className="mt-1 shrink-0 rounded-md bg-green px-4 py-2 text-sm font-medium text-on-green shadow-sm transition-[transform,filter,box-shadow,opacity] duration-fast hover:-translate-y-px hover:brightness-110 hover:shadow-md active:translate-y-0 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-green focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar disabled:opacity-60"
        >
          {isMember ? t.inviteEmbed.open : t.inviteEmbed.join}
        </button>
      </div>
    </div>
  );
}
