/** Onglet Bannis : liste des clés bannies et réhabilitation (BAN). */

import {
  avatarDecorationOf,
  displayNameOf,
  useFriends,
} from '../../stores/friends';
import { useGroups, hasPerm, PERMISSIONS } from '../../stores/groups';
import { useUi, useT } from '../../stores/ui';
import { Avatar } from '../Avatar';
import { messageOf } from './controls';

export function ServerBansTab({ groupId }: { groupId: string }) {
  const t = useT();
  const toast = useUi((s) => s.toast);
  const contacts = useFriends((s) => s.contacts);
  const state = useGroups((s) => s.states[groupId]);
  const unban = useGroups((s) => s.unban);

  if (!state) return null;

  const canBan = hasPerm(state.my_permissions, PERMISSIONS.BAN);

  if (state.bans.length === 0) {
    return <p className="text-sm text-muted">{t.serveur.noBans}</p>;
  }

  return (
    <div>
      {state.bans.map((pubkey) => {
        const name = displayNameOf(contacts, pubkey);
        return (
          <div
            key={pubkey}
            className="mb-1 flex items-center gap-3 rounded-lg bg-sidebar px-3 py-2"
          >
            <Avatar
              id={pubkey}
              name={name}
              size={32}
              decoration={avatarDecorationOf(contacts, pubkey)}
            />
            <span className="min-w-0 flex-1 truncate text-sm font-medium text-header">
              {name}
            </span>
            {canBan && (
              <button
                type="button"
                onClick={() => {
                  unban(groupId, pubkey).catch((e: unknown) =>
                    toast('error', messageOf(e, t.errors.actionFailed)),
                  );
                }}
                className="shrink-0 rounded-lg border border-green px-3 py-1 text-sm font-medium text-green transition-colors hover:bg-green hover:text-on-green focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-green focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar"
              >
                {t.serveur.unban}
              </button>
            )}
          </div>
        );
      })}
    </div>
  );
}
