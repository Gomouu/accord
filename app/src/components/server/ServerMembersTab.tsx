/**
 * Onglet Membres : liste avec couleurs, rôles et pseudo de serveur, plus la
 * modération — expulsion (KICK), bannissement (BAN), sourdine temporaire
 * (KICK) et effacement du pseudo de serveur (MANAGE_ROLES). Le fondateur et
 * soi-même restent intouchables côté UI ; le nœud vérifie la hiérarchie.
 */

import { useCallback, useEffect, useState } from 'react';
import type { Dict } from '../../i18n';
import { interpolate } from '../../i18n';
import { formatTimestamp } from '../../lib/format';
import {
  avatarDecorationOf,
  avatarOf,
  displayNameOf,
  useFriends,
} from '../../stores/friends';
import {
  useGroups,
  hasPerm,
  memberColor,
  nicknameOf,
  serverAvatarOf,
  sortRoles,
  timeoutUntil,
  PERMISSIONS,
} from '../../stores/groups';
import { selfDisplayName, useSession } from '../../stores/session';
import { useUi, useT } from '../../stores/ui';
import { Avatar } from '../Avatar';
import { api } from '../../lib/client';
import type { PendingMemberEntry } from '../../lib/api';
import { ConfirmButton, messageOf } from './controls';

const MINUTE_MS = 60_000;

/** Durées proposées pour une sourdine temporaire (ordre du sélecteur). */
const TIMEOUT_OPTIONS = [
  { key: '5m', ms: 5 * MINUTE_MS },
  { key: '10m', ms: 10 * MINUTE_MS },
  { key: '1h', ms: 60 * MINUTE_MS },
  { key: '1d', ms: 24 * 60 * MINUTE_MS },
  { key: '1w', ms: 7 * 24 * 60 * MINUTE_MS },
] as const;

type TimeoutKey = (typeof TIMEOUT_OPTIONS)[number]['key'];

/** Libellé localisé d'une durée de sourdine. */
function durationLabel(t: Dict, key: TimeoutKey): string {
  const labels: Record<TimeoutKey, string> = {
    '5m': t.serveur.timeout5m,
    '10m': t.serveur.timeout10m,
    '1h': t.serveur.timeout1h,
    '1d': t.serveur.timeout1d,
    '1w': t.serveur.timeout1w,
  };
  return labels[key];
}

const CONTROL_CLASS =
  'rounded-lg border border-input px-3 py-1 text-sm font-medium text-muted transition-colors hover:text-norm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar';

/**
 * Contrôle de sourdine d'un membre : choix de durée puis application quand il
 * n'est pas en sourdine, bouton de levée sinon.
 */
function MemberTimeout({
  until,
  onApply,
  onClear,
}: {
  until: number | null;
  onApply: (untilMs: number) => void;
  onClear: () => void;
}) {
  const t = useT();
  const [choice, setChoice] = useState<TimeoutKey>('5m');

  if (until !== null) {
    return (
      <button type="button" onClick={onClear} className={CONTROL_CLASS}>
        {t.serveur.timeoutClear}
      </button>
    );
  }

  return (
    <div className="flex flex-wrap items-center gap-1.5">
      <select
        aria-label={t.serveur.timeoutDuration}
        value={choice}
        onChange={(e) => setChoice(e.target.value as TimeoutKey)}
        className="rounded-md bg-input px-1.5 py-1 text-xs text-norm outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar"
      >
        {TIMEOUT_OPTIONS.map((option) => (
          <option key={option.key} value={option.key}>
            {durationLabel(t, option.key)}
          </option>
        ))}
      </select>
      <button
        type="button"
        onClick={() => {
          const option = TIMEOUT_OPTIONS.find((o) => o.key === choice);
          if (option !== undefined) onApply(Date.now() + option.ms);
        }}
        className={CONTROL_CLASS}
      >
        {t.serveur.timeout}
      </button>
    </div>
  );
}

/**
 * Vérification à l'entrée (§9.4) : interrupteur, et file des demandes déjà
 * PROUVÉES en attente de validation.
 *
 * Réglage local du créateur des invitations : c'est son nœud qui traite les
 * rachats, donc c'est lui qui décide. « Mes invitations ne prennent effet
 * qu'après mon accord » — et non « ce serveur exige une validation », qui
 * serait un mode répliqué et vérifié par tous les membres, un autre travail.
 */
function EntryCheckSection({ groupId }: { groupId: string }) {
  const t = useT();
  const lang = useUi((s) => s.lang);
  const toast = useUi((s) => s.toast);
  const contacts = useFriends((s) => s.contacts);
  const [actif, setActif] = useState<boolean | null>(null);
  const [attente, setAttente] = useState<PendingMemberEntry[]>([]);

  const echec = t.errors.actionFailed;
  const recharger = useCallback((): void => {
    api
      .groupsPendingMembers(groupId)
      .then((r) => setAttente(r.entries))
      .catch(() => {});
  }, [groupId]);

  useEffect(() => {
    api
      .groupsEntryCheck(groupId)
      .then((r) => setActif(r.enabled))
      .catch(() => setActif(null));
    recharger();
  }, [groupId, recharger]);

  // Le nœud n'expose pas ce réglage : version antérieure, rien à afficher.
  if (actif === null) return null;

  const basculer = (valeur: boolean): void => {
    api
      .groupsSetEntryCheck(groupId, valeur)
      .then(() => setActif(valeur))
      .catch(() => toast('error', echec));
  };

  const trancher = (membre: string, approuver: boolean): void => {
    const action = approuver
      ? api.groupsApproveMember(groupId, membre)
      : api.groupsRefuseMember(groupId, membre);
    action
      .then(() => setAttente((a) => a.filter((p) => p.member !== membre)))
      .catch(() => toast('error', echec));
  };

  return (
    <div className="mb-4 rounded-lg bg-sidebar p-3">
      <label className="flex items-center gap-2 text-sm text-norm">
        <input
          type="checkbox"
          checked={actif}
          onChange={(e) => basculer(e.target.checked)}
          className="size-4"
        />
        {t.serveur.entryCheck}
      </label>
      <p className="mt-1 text-xs text-muted">{t.serveur.entryCheckHint}</p>

      {attente.length > 0 && (
        <ul className="mt-3 m-0 list-none p-0">
          {attente.map((p) => (
            <li
              key={p.member}
              className="mb-1 flex items-center gap-2 rounded-md bg-input px-2 py-1.5"
            >
              <Avatar id={p.member} name={displayNameOf(contacts, p.member)} size={24} />
              <span className="min-w-0 flex-1 truncate text-sm text-norm">
                {displayNameOf(contacts, p.member)}
              </span>
              <span className="shrink-0 text-xs text-faint">
                {formatTimestamp(p.at_ms, lang)}
              </span>
              <button
                type="button"
                onClick={() => trancher(p.member, true)}
                className="shrink-0 rounded-md bg-blurple px-3 py-1.5 text-xs font-medium text-white hover:bg-blurple-hover"
              >
                {t.serveur.entryApprove}
              </button>
              <button
                type="button"
                onClick={() => trancher(p.member, false)}
                className="shrink-0 rounded-md bg-rail px-3 py-1.5 text-xs font-medium text-norm hover:bg-hover"
              >
                {t.serveur.entryRefuse}
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

export function ServerMembersTab({ groupId }: { groupId: string }) {
  const t = useT();
  const lang = useUi((s) => s.lang);
  const toast = useUi((s) => s.toast);
  const contacts = useFriends((s) => s.contacts);
  const self = useSession((s) => s.self);
  const state = useGroups((s) => s.states[groupId]);
  const kick = useGroups((s) => s.kick);
  const ban = useGroups((s) => s.ban);
  const timeoutMember = useGroups((s) => s.timeout);
  const clearMemberTimeout = useGroups((s) => s.clearTimeout);
  const setNickname = useGroups((s) => s.setNickname);

  if (!state) return null;

  const canKick = hasPerm(state.my_permissions, PERMISSIONS.KICK);
  const canBan = hasPerm(state.my_permissions, PERMISSIONS.BAN);
  const canManageRoles = hasPerm(state.my_permissions, PERMISSIONS.MANAGE_ROLES);

  const nameOf = (pubkey: string): string => {
    const nick = nicknameOf(state, pubkey);
    if (self !== null && pubkey === self.pubkey) {
      return `${nick ?? selfDisplayName(self)} (${t.app.you})`;
    }
    return nick ?? displayNameOf(contacts, pubkey);
  };

  const onError = (e: unknown): void =>
    toast('error', messageOf(e, t.errors.actionFailed));

  return (
    <div>
      {canKick && <EntryCheckSection groupId={groupId} />}
      <div className="mb-2 text-xs font-medium uppercase tracking-wide text-faint">
        {t.groups.members} — {state.members.length}
      </div>
      {state.members.map((member) => {
        const color = memberColor(member, state.roles);
        const owned = new Set(member.roles);
        const roleNames = sortRoles(state.roles)
          .filter((r) => owned.has(r.role_id))
          .map((r) => r.name);
        const isSelf = self !== null && member.pubkey === self.pubkey;
        const isFounder = state.founder === member.pubkey;
        const name = nameOf(member.pubkey);
        const until = timeoutUntil(member);
        const hasNickname = nicknameOf(state, member.pubkey) !== null;
        // Avatar de serveur self-service s'il est défini, sinon l'avatar
        // global (le sien, ou celui du contact ami connu) — même résolution
        // que la liste des membres du salon (ChatView).
        const globalAvatar =
          self !== null && member.pubkey === self.pubkey
            ? self.avatar
            : avatarOf(contacts, member.pubkey);
        const avatarHash = serverAvatarOf(state, contacts, member.pubkey) ?? globalAvatar;
        const avatarDecoration =
          self !== null && member.pubkey === self.pubkey
            ? self.avatar_decoration
            : avatarDecorationOf(contacts, member.pubkey);
        return (
          <div
            key={member.pubkey}
            className="mb-1 flex flex-wrap items-center gap-3 rounded-lg bg-sidebar px-3 py-2"
          >
            <Avatar
              id={member.pubkey}
              name={name}
              size={32}
              avatarHash={avatarHash}
              hint={member.pubkey}
              decoration={avatarDecoration}
            />
            <div className="min-w-0 flex-1">
              <div
                className="truncate text-sm font-medium text-header"
                style={color !== null ? { color } : undefined}
              >
                {name}
              </div>
              <div className="truncate text-[11px] text-faint">
                {isFounder && (
                  <span className="uppercase text-yellow">{t.groups.founder}</span>
                )}
                {isFounder && roleNames.length > 0 && ' · '}
                {roleNames.join(' · ')}
              </div>
              {until !== null && (
                <div className="mt-0.5 text-[11px] font-medium text-red">
                  {interpolate(t.serveur.timedOutUntil, {
                    time: formatTimestamp(until, lang),
                  })}
                </div>
              )}
            </div>
            {!isSelf && !isFounder && (
              <div className="flex min-w-0 basis-full flex-wrap items-center justify-end gap-2 ps-11">
                {canKick && (
                  <MemberTimeout
                    until={until}
                    onApply={(untilMs) => {
                      timeoutMember(groupId, member.pubkey, untilMs).catch(onError);
                    }}
                    onClear={() => {
                      clearMemberTimeout(groupId, member.pubkey).catch(onError);
                    }}
                  />
                )}
                {canManageRoles && hasNickname && (
                  <button
                    type="button"
                    onClick={() => {
                      setNickname(groupId, '', member.pubkey).catch(onError);
                    }}
                    className={CONTROL_CLASS}
                  >
                    {t.serveur.clearNickname}
                  </button>
                )}
                {canKick && (
                  <ConfirmButton
                    action={t.serveur.kick}
                    question={interpolate(t.serveur.kickConfirm, { name })}
                    onConfirm={() => {
                      kick(groupId, member.pubkey).catch(onError);
                    }}
                  />
                )}
                {canBan && (
                  <ConfirmButton
                    action={t.serveur.ban}
                    question={interpolate(t.serveur.banConfirm, { name })}
                    onConfirm={() => {
                      ban(groupId, member.pubkey).catch(onError);
                    }}
                  />
                )}
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}
