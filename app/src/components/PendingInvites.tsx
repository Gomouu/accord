/**
 * Invitations de serveur reçues (consentement explicite, D-045 : plus de
 * force-join) : nom du serveur (déjà assaini côté nœud, échappé par React de
 * toute façon), qui a invité, actions Accepter/Refuser. Alimentée par
 * `useGroups.pendingInvites` (chargée au démarrage, complétée par
 * `event.group_invite_pending` — voir `AppShell`). Rendue dans l'onglet
 * « Invitations » de `FriendsView`.
 */

import { interpolate } from '../i18n';
import type { PendingInvite } from '../lib/api';
import { avatarOf, displayNameOf, useFriends } from '../stores/friends';
import { useGroups } from '../stores/groups';
import { useT, useUi } from '../stores/ui';
import { Avatar } from './Avatar';

function InviteRow({ invite }: { invite: PendingInvite }) {
  const t = useT();
  const toast = useUi((s) => s.toast);
  const contacts = useFriends((s) => s.contacts);
  const acceptInvite = useGroups((s) => s.acceptInvite);
  const declineInvite = useGroups((s) => s.declineInvite);

  const inviterName = displayNameOf(contacts, invite.inviter);

  const act = (fn: () => Promise<void>): void => {
    void fn().catch(() => toast('error', t.errors.actionFailed));
  };

  return (
    <div className="flex items-center gap-3 rounded-lg px-3 py-2.5 transition-colors duration-fast hover:bg-chat-hover">
      <Avatar
        id={invite.inviter}
        name={inviterName}
        avatarHash={avatarOf(contacts, invite.inviter)}
        hint={invite.inviter}
      />
      <div className="min-w-0 flex-1">
        <div className="truncate font-medium text-header">{invite.group_name}</div>
        <div className="truncate text-xs text-faint">
          {interpolate(t.invitations.invitedBy, { name: inviterName })}
        </div>
      </div>
      <div className="flex items-center gap-2">
        <button
          type="button"
          onClick={() => act(() => acceptInvite(invite.group_id, invite.invite_id))}
          className="rounded bg-green px-3 py-1.5 text-sm font-medium text-white transition-colors duration-fast hover:brightness-110 active:scale-95"
        >
          {t.invitations.accept}
        </button>
        <button
          type="button"
          onClick={() => act(() => declineInvite(invite.group_id, invite.invite_id))}
          className="rounded bg-sidebar px-3 py-1.5 text-sm font-medium text-norm transition-colors duration-fast hover:bg-input active:scale-95"
        >
          {t.invitations.decline}
        </button>
      </div>
    </div>
  );
}

export function PendingInvites() {
  const t = useT();
  const invites = useGroups((s) => s.pendingInvites);

  if (invites.length === 0) {
    return <p className="py-8 text-center text-muted">{t.invitations.empty}</p>;
  }

  return (
    <div>
      {invites.map((invite) => (
        <InviteRow key={`${invite.group_id}/${invite.invite_id}`} invite={invite} />
      ))}
    </div>
  );
}
