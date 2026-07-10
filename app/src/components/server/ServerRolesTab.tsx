/**
 * Onglet Rôles : création, renommage, couleur, cases à cocher par bit de
 * permission, attribution aux membres et suppression confirmée. Un
 * non-ADMIN ne modifie que les rôles strictement sous sa propre position
 * (règle du contrat, rappelée en lecture seule).
 */

import { useState } from 'react';
import { interpolate } from '../../i18n';
import type { Dict } from '../../i18n';
import type { GroupRole, GroupStateJson } from '../../lib/api';
import { displayNameOf, useFriends } from '../../stores/friends';
import {
  useGroups,
  hasPerm,
  highestRolePosition,
  roleColorCss,
  sortRoles,
  PERMISSIONS,
} from '../../stores/groups';
import { selfDisplayName, useSession } from '../../stores/session';
import { useUi, useT } from '../../stores/ui';
import { SettingsSection } from '../settings/controls';
import { ConfirmButton, messageOf } from './controls';

/** Bits de permission proposés, avec leur libellé i18n. */
const PERM_ITEMS: Array<{ bit: number; label: (t: Dict) => string }> = [
  { bit: PERMISSIONS.VIEW, label: (t) => t.serveur.permView },
  { bit: PERMISSIONS.SEND, label: (t) => t.serveur.permSend },
  { bit: PERMISSIONS.MANAGE_MESSAGES, label: (t) => t.serveur.permManageMessages },
  { bit: PERMISSIONS.MANAGE_CHANNELS, label: (t) => t.serveur.permManageChannels },
  { bit: PERMISSIONS.INVITE, label: (t) => t.serveur.permInvite },
  { bit: PERMISSIONS.KICK, label: (t) => t.serveur.permKick },
  { bit: PERMISSIONS.BAN, label: (t) => t.serveur.permBan },
  { bit: PERMISSIONS.MANAGE_ROLES, label: (t) => t.serveur.permManageRoles },
  { bit: PERMISSIONS.MANAGE_EMOJIS, label: (t) => t.serveur.permManageEmojis },
  { bit: PERMISSIONS.ADMIN, label: (t) => t.serveur.permAdmin },
];

/** Couleur par défaut du sélecteur quand le rôle n'a pas de couleur. */
const DEFAULT_PICKER_COLOR = 0x5865f2;

/** Éditeur d'un rôle : nom, couleur, permissions, membres, suppression. */
function RoleEditor({
  groupId,
  role,
  state,
  editable,
  canMoveUp,
  canMoveDown,
}: {
  groupId: string;
  role: GroupRole;
  state: GroupStateJson;
  editable: boolean;
  /** Le voisin du dessus existe et est lui aussi gérable. */
  canMoveUp: boolean;
  /** Le voisin du dessous existe et est lui aussi gérable. */
  canMoveDown: boolean;
}) {
  const t = useT();
  const toast = useUi((s) => s.toast);
  const contacts = useFriends((s) => s.contacts);
  const self = useSession((s) => s.self);
  const editRole = useGroups((s) => s.editRole);
  const deleteRole = useGroups((s) => s.deleteRole);
  const moveRole = useGroups((s) => s.moveRole);
  const setMemberRole = useGroups((s) => s.setMemberRole);
  const [name, setName] = useState(role.name);
  const [color, setColor] = useState(role.color);
  const [perms, setPerms] = useState(role.permissions);
  const [busy, setBusy] = useState(false);

  const nameTrimmed = name.trim();
  const dirty =
    (nameTrimmed !== role.name && nameTrimmed !== '') ||
    color !== role.color ||
    perms !== role.permissions;

  const nameOf = (pubkey: string): string =>
    self !== null && pubkey === self.pubkey
      ? `${selfDisplayName(self)} (${t.app.you})`
      : displayNameOf(contacts, pubkey);

  const save = async (): Promise<void> => {
    if (!dirty || busy) return;
    setBusy(true);
    try {
      await editRole(groupId, role.role_id, {
        ...(nameTrimmed !== role.name && nameTrimmed !== '' ? { name: nameTrimmed } : {}),
        ...(color !== role.color ? { color } : {}),
        ...(perms !== role.permissions ? { permissions: perms } : {}),
      });
      toast('info', t.serveur.roleSaved);
    } catch (e) {
      toast('error', messageOf(e, t.errors.actionFailed));
    } finally {
      setBusy(false);
    }
  };

  const toggleMember = (pubkey: string, assigned: boolean): void => {
    setMemberRole(groupId, role.role_id, pubkey, !assigned).catch((e: unknown) =>
      toast('error', messageOf(e, t.errors.actionFailed)),
    );
  };

  const move = (direction: 'up' | 'down'): void => {
    moveRole(groupId, role.role_id, direction).catch((e: unknown) =>
      toast('error', messageOf(e, t.errors.actionFailed)),
    );
  };

  return (
    <div className="mb-3 rounded-lg bg-sidebar p-4">
      <div className="flex items-center gap-3">
        <span
          aria-hidden
          className="h-4 w-4 shrink-0 rounded-full border border-rail"
          style={{
            backgroundColor: role.color === 0 ? 'transparent' : roleColorCss(role.color),
          }}
        />
        {editable ? (
          <input
            aria-label={t.serveur.roleNameLabel}
            value={name}
            onChange={(e) => setName(e.target.value)}
            className="min-w-0 flex-1 rounded bg-rail px-3 py-1.5 text-norm outline-none focus-visible:ring-2 focus-visible:ring-blurple"
          />
        ) : (
          <span className="flex-1 truncate font-medium text-header">{role.name}</span>
        )}
        {editable && (
          <span className="flex shrink-0 items-center gap-1">
            <button
              type="button"
              aria-label={interpolate(t.serveur.roleMoveUp, { name: role.name })}
              disabled={!canMoveUp}
              onClick={() => move('up')}
              className="flex h-7 w-7 items-center justify-center rounded bg-rail text-norm transition-colors duration-150 hover:bg-input disabled:opacity-40"
            >
              <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor" aria-hidden>
                <path d="M12 5.8 4.9 13l1.4 1.4 4.7-4.7V19h2v-9.3l4.7 4.7 1.4-1.4L12 5.8Z" />
              </svg>
            </button>
            <button
              type="button"
              aria-label={interpolate(t.serveur.roleMoveDown, { name: role.name })}
              disabled={!canMoveDown}
              onClick={() => move('down')}
              className="flex h-7 w-7 items-center justify-center rounded bg-rail text-norm transition-colors duration-150 hover:bg-input disabled:opacity-40"
            >
              <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor" aria-hidden>
                <path d="m12 18.2 7.1-7.2-1.4-1.4-4.7 4.7V5h-2v9.3L6.3 9.6 4.9 11l7.1 7.2Z" />
              </svg>
            </button>
          </span>
        )}
      </div>

      {!editable && <p className="mt-2 text-xs text-faint">{t.serveur.roleLocked}</p>}

      {editable && (
        <>
          <div className="mt-3 flex items-center gap-3">
            <span className="text-xs font-semibold uppercase text-faint">
              {t.serveur.roleColor}
            </span>
            <input
              type="color"
              aria-label={t.serveur.roleColor}
              value={roleColorCss(color === 0 ? DEFAULT_PICKER_COLOR : color)}
              onChange={(e) => setColor(parseInt(e.target.value.slice(1), 16))}
              className="h-7 w-10 cursor-pointer rounded bg-rail"
            />
            <button
              type="button"
              aria-pressed={color === 0}
              onClick={() => setColor(0)}
              className={`rounded px-2 py-1 text-xs font-medium ${
                color === 0 ? 'bg-blurple text-white' : 'bg-rail text-norm'
              }`}
            >
              {t.serveur.roleNoColor}
            </button>
          </div>

          <div className="mt-3">
            <div className="mb-1 text-xs font-semibold uppercase text-faint">
              {t.serveur.rolePermissions}
            </div>
            <div className="grid grid-cols-1 gap-1 sm:grid-cols-2">
              {PERM_ITEMS.map(({ bit, label }) => (
                <label
                  key={bit}
                  className="flex cursor-pointer items-center gap-2 rounded px-1 py-0.5 text-sm text-norm hover:bg-chat-hover"
                >
                  <input
                    type="checkbox"
                    checked={(perms & bit) !== 0}
                    onChange={(e) =>
                      setPerms((p) => (e.target.checked ? p | bit : p & ~bit))
                    }
                    className="accent-blurple"
                  />
                  {label(t)}
                </label>
              ))}
            </div>
          </div>

          <div className="mt-3">
            <div className="mb-1 text-xs font-semibold uppercase text-faint">
              {t.serveur.roleMembersTitle}
            </div>
            {state.members.map((member) => {
              const assigned = member.roles.includes(role.role_id);
              return (
                <div
                  key={member.pubkey}
                  className="flex items-center justify-between gap-2 rounded px-1 py-1 hover:bg-chat-hover"
                >
                  <span className="min-w-0 truncate text-sm text-norm">
                    {nameOf(member.pubkey)}
                  </span>
                  <button
                    type="button"
                    aria-pressed={assigned}
                    onClick={() => toggleMember(member.pubkey, assigned)}
                    className={`shrink-0 rounded px-2.5 py-1 text-xs font-medium transition-colors ${
                      assigned
                        ? 'bg-blurple text-white hover:bg-blurple-hover'
                        : 'bg-rail text-norm hover:bg-input'
                    }`}
                  >
                    {assigned ? t.serveur.unassign : t.serveur.assign}
                  </button>
                </div>
              );
            })}
          </div>

          <div className="mt-3 flex items-center justify-between gap-3">
            <ConfirmButton
              action={t.serveur.roleDelete}
              question={interpolate(t.serveur.roleDeleteConfirm, { name: role.name })}
              onConfirm={() => {
                deleteRole(groupId, role.role_id).catch((e: unknown) =>
                  toast('error', messageOf(e, t.errors.actionFailed)),
                );
              }}
            />
            <button
              type="button"
              disabled={!dirty || busy}
              onClick={() => void save()}
              className="rounded bg-blurple px-4 py-1.5 text-sm font-medium text-white transition-colors duration-150 hover:bg-blurple-hover disabled:opacity-50"
            >
              {t.serveur.roleSave}
            </button>
          </div>
        </>
      )}
    </div>
  );
}

export function ServerRolesTab({ groupId }: { groupId: string }) {
  const t = useT();
  const toast = useUi((s) => s.toast);
  const state = useGroups((s) => s.states[groupId]);
  const addRole = useGroups((s) => s.addRole);
  const self = useSession((s) => s.self);
  const [newName, setNewName] = useState('');
  const [busy, setBusy] = useState(false);

  if (!state) return null;

  const canManage = hasPerm(state.my_permissions, PERMISSIONS.MANAGE_ROLES);
  const isAdmin =
    hasPerm(state.my_permissions, PERMISSIONS.ADMIN) ||
    (self !== null && state.founder === self.pubkey);
  const me = state.members.find((m) => self !== null && m.pubkey === self.pubkey);
  const myTop = highestRolePosition(me, state.roles);
  const roles = sortRoles(state.roles);

  const createRole = async (): Promise<void> => {
    const name = newName.trim();
    if (name === '' || busy) return;
    setBusy(true);
    try {
      await addRole(groupId, name, 0, 0);
      setNewName('');
    } catch (e) {
      toast('error', messageOf(e, t.errors.actionFailed));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div>
      {canManage && (
        <SettingsSection title={t.serveur.newRoleTitle}>
          <div className="flex gap-3 rounded-lg bg-sidebar p-3">
            <input
              aria-label={t.serveur.roleNamePlaceholder}
              placeholder={t.serveur.roleNamePlaceholder}
              value={newName}
              onChange={(e) => setNewName(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter') void createRole();
              }}
              className="min-w-0 flex-1 rounded bg-rail px-3 py-2 text-norm placeholder-faint outline-none focus-visible:ring-2 focus-visible:ring-blurple"
            />
            <button
              type="button"
              disabled={newName.trim() === '' || busy}
              onClick={() => void createRole()}
              className="rounded bg-blurple px-4 py-2 text-sm font-medium text-white transition-colors duration-150 hover:bg-blurple-hover disabled:opacity-50"
            >
              {t.serveur.createRoleAction}
            </button>
          </div>
        </SettingsSection>
      )}

      {roles.length === 0 && <p className="text-sm text-muted">{t.serveur.noRoles}</p>}
      {roles.map((role, index) => {
        // Hiérarchie du contrat : un non-ADMIN ne gère que les rôles
        // strictement sous sa position.
        const isEditable = (r: GroupRole): boolean =>
          canManage && (isAdmin || r.position < myTop);
        const above = roles[index - 1];
        const below = roles[index + 1];
        return (
          <RoleEditor
            key={role.role_id}
            groupId={groupId}
            role={role}
            state={state}
            editable={isEditable(role)}
            // A move swaps positions with the neighbor: both roles must be
            // manageable by the current member.
            canMoveUp={above !== undefined && isEditable(above)}
            canMoveDown={below !== undefined && isEditable(below)}
          />
        );
      })}
    </div>
  );
}
