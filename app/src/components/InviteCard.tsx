/**
 * Carte d'invitation de serveur DANS un message privé (parité Discord).
 * Rendue par `MessageList` pour les corps `type: 'invite'` — une ligne locale
 * d'historique posée des deux côtés par le nœud (le ticket filaire, lui, est
 * inchangé). L'état est DÉRIVÉ à chaque rendu, jamais dupliqué :
 * - membre du serveur → « Rejoint » + « Aller au serveur » ;
 * - invitation encore en attente → « Rejoindre » / « Refuser » ;
 * - carte de l'inviteur → « Invitation envoyée » (ou « Rejoint » si l'invité
 *   figure déjà parmi les membres) ;
 * - sinon → invitation expirée, refusée ou retirée (l'acceptation locale de
 *   la session en cours est gardée en état de composant pour l'attente de
 *   l'inviteur).
 */

import { useState } from 'react';
import { interpolate } from '../i18n';
import type { MsgBody } from '../lib/api';
import { useGroups } from '../stores/groups';
import { useT, useUi } from '../stores/ui';
import { channelToRestore } from './ServerRail';

type InviteBody = Extract<MsgBody, { type: 'invite' }>;

type Props = {
  body: InviteBody;
  isOwn: boolean;
  /** Pair de la conversation (l'invité, vu de l'inviteur), sinon null. */
  peer: string | null;
};

export function InviteCard({ body, isOwn, peer }: Props) {
  const t = useT();
  const toast = useUi((s) => s.toast);
  const setView = useUi((s) => s.setView);
  const lastChannelByServer = useUi((s) => s.lastChannelByServer);
  const ids = useGroups((s) => s.ids);
  const states = useGroups((s) => s.states);
  const pendingInvites = useGroups((s) => s.pendingInvites);
  const acceptInvite = useGroups((s) => s.acceptInvite);
  const declineInvite = useGroups((s) => s.declineInvite);
  const [decision, setDecision] = useState<'accepted' | 'declined' | null>(null);

  const member = ids.includes(body.group_id);
  const pending = pendingInvites.some(
    (i) => i.group_id === body.group_id && i.invite_id === body.invite_id,
  );

  const act = (fn: () => Promise<void>, next: 'accepted' | 'declined'): void => {
    void fn()
      .then(() => setDecision(next))
      .catch(() => toast('error', t.errors.actionFailed));
  };

  const ouvrirServeur = (): void =>
    setView({
      kind: 'group',
      groupId: body.group_id,
      channelId: channelToRestore(
        states[body.group_id],
        lastChannelByServer[body.group_id],
      ),
    });

  const peerJoined =
    peer !== null &&
    (states[body.group_id]?.members.some((m) => m.pubkey === peer) ?? false);

  let statut: string | null = null;
  let actions: 'join' | 'go' | null = null;
  if (isOwn) {
    statut = peerJoined ? t.inviteCard.joined : null;
    actions = null;
  } else if (member) {
    statut = t.inviteCard.joined;
    actions = 'go';
  } else if (pending) {
    statut = null;
    actions = 'join';
  } else if (decision === 'accepted') {
    statut = t.inviteCard.accepted;
  } else if (decision === 'declined') {
    statut = t.inviteCard.declined;
  } else {
    statut = t.inviteCard.stale;
  }

  return (
    <div className="mt-1 flex w-full max-w-md flex-col gap-3 rounded-lg border border-input bg-sidebar/80 p-3">
      <div className="text-xs font-semibold uppercase tracking-wide text-faint">
        {isOwn ? t.inviteCard.sentTitle : t.inviteCard.title}
      </div>
      <div className="flex items-center gap-3">
        <span
          aria-hidden
          className="flex h-10 w-10 shrink-0 items-center justify-center rounded-2xl bg-blurple/20 font-semibold text-blurple"
        >
          {body.group_name.slice(0, 2).toUpperCase()}
        </span>
        <div className="min-w-0 flex-1">
          <div className="truncate font-semibold text-header">{body.group_name}</div>
          {statut !== null && (
            <div className="truncate text-xs text-muted" role="status">
              {statut}
            </div>
          )}
        </div>
        {actions === 'join' && (
          <div className="flex shrink-0 items-center gap-2">
            <button
              type="button"
              aria-label={interpolate(t.inviteCard.joinLabel, { name: body.group_name })}
              onClick={() =>
                act(() => acceptInvite(body.group_id, body.invite_id), 'accepted')
              }
              className="rounded-sm bg-green px-3 py-1.5 text-sm font-medium text-on-green transition-colors duration-fast hover:brightness-110 active:scale-95 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-chat"
            >
              {t.inviteCard.join}
            </button>
            <button
              type="button"
              aria-label={interpolate(t.inviteCard.declineLabel, {
                name: body.group_name,
              })}
              onClick={() =>
                act(() => declineInvite(body.group_id, body.invite_id), 'declined')
              }
              className="rounded-sm bg-chat-hover px-3 py-1.5 text-sm font-medium text-norm transition-colors duration-fast hover:bg-input active:scale-95 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-chat"
            >
              {t.invitations.decline}
            </button>
          </div>
        )}
        {actions === 'go' && (
          <button
            type="button"
            onClick={ouvrirServeur}
            className="shrink-0 rounded-sm bg-blurple px-3 py-1.5 text-sm font-medium text-white transition-colors duration-fast hover:bg-blurple-hover active:scale-95 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-chat"
          >
            {t.inviteCard.goToServer}
          </button>
        )}
      </div>
    </div>
  );
}
