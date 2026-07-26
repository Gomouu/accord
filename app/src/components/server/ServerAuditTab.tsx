/**
 * Onglet Journal d'audit (ADMIN/fondateur) : l'op-log signé du groupe rendu
 * lisible — acteur (avatar + nom), description i18n de l'action, horodatage —
 * paginé du plus récent au plus ancien (`groups.audit`). Lecture seule.
 */

import { useCallback, useEffect, useState } from 'react';
import { interpolate } from '../../i18n';
import type { Dict } from '../../i18n';
import { api } from '../../lib/client';
import { buildAuditExport } from '../../lib/auditExport';
import { copyToClipboard } from '../../lib/clipboard';
import type { AuditEntry, GroupStateJson } from '../../lib/api';
import { formatTimestamp, shortId } from '../../lib/format';
import { avatarDecorationOf, displayNameOf, useFriends } from '../../stores/friends';
import { useGroups } from '../../stores/groups';
import { selfDisplayName, useSession } from '../../stores/session';
import { useUi, useT } from '../../stores/ui';
import { Avatar } from '../Avatar';
import { messageOf } from './controls';

/** Page size of the audit log (node caps at 100). */
const AUDIT_PAGE = 50;

/**
 * Borne de l'export, en entrées.
 *
 * Un serveur ancien peut porter des dizaines de milliers d'opérations, et les
 * tirer toutes tiendrait la base pendant tout ce temps. La borne est annoncée
 * DANS le fichier produit quand elle mord : un registre incomplet qui ne le
 * dit pas est pire qu'un registre court, parce que le lecteur en conclut
 * qu'il n'y a rien eu avant.
 */
const AUDIT_EXPORT_MAX = 500;

/** i18n template of each op kind (see `groups.audit` in API.md). */
const KIND_LABELS: Record<string, (t: Dict) => string> = {
  create: (t) => t.serveur.auditCreate,
  set_meta: (t) => t.serveur.auditSetMeta,
  add_channel: (t) => t.serveur.auditAddChannel,
  edit_channel: (t) => t.serveur.auditEditChannel,
  del_channel: (t) => t.serveur.auditDelChannel,
  add_category: (t) => t.serveur.auditAddCategory,
  edit_category: (t) => t.serveur.auditEditCategory,
  del_category: (t) => t.serveur.auditDelCategory,
  set_channel_category: (t) => t.serveur.auditSetChannelCategory,
  add_member: (t) => t.serveur.auditAddMember,
  kick: (t) => t.serveur.auditKick,
  ban: (t) => t.serveur.auditBan,
  unban: (t) => t.serveur.auditUnban,
  add_role: (t) => t.serveur.auditAddRole,
  edit_role: (t) => t.serveur.auditEditRole,
  del_role: (t) => t.serveur.auditDelRole,
  assign_role: (t) => t.serveur.auditAssignRole,
  unassign_role: (t) => t.serveur.auditUnassignRole,
  set_channel_perms: (t) => t.serveur.auditSetChannelPerms,
  pin: (t) => t.serveur.auditPin,
  unpin: (t) => t.serveur.auditUnpin,
  delete_msg: (t) => t.serveur.auditDeleteMsg,
  set_topic: (t) => t.serveur.auditSetTopic,
  invite_create: (t) => t.serveur.auditInviteCreate,
  invite_revoke: (t) => t.serveur.auditInviteRevoke,
  leave: (t) => t.serveur.auditLeave,
  add_emoji: (t) => t.serveur.auditAddEmoji,
  del_emoji: (t) => t.serveur.auditDelEmoji,
};

/** String param of an audit entry, or `undefined`. */
function strParam(entry: AuditEntry, key: string): string | undefined {
  const value = entry.params[key];
  return typeof value === 'string' ? value : undefined;
}

/**
 * Human description of an entry. Ids are resolved against the *current*
 * state when possible (a deleted channel/role falls back to the name carried
 * by the op, then to a short id).
 */
export function describeEntry(
  t: Dict,
  entry: AuditEntry,
  state: GroupStateJson | undefined,
  nameOf: (pubkey: string) => string,
): string {
  const template = KIND_LABELS[entry.kind];
  if (template === undefined) return t.serveur.auditUnknown;

  const channelName = (): string | undefined => {
    const id = strParam(entry, 'channel_id');
    if (id === undefined) return undefined;
    return state?.channels.find((c) => c.channel_id === id)?.name ?? shortId(id);
  };
  const roleName = (): string | undefined => {
    const id = strParam(entry, 'role_id');
    if (id === undefined) return undefined;
    return state?.roles.find((r) => r.role_id === id)?.name ?? shortId(id);
  };
  const categoryName = (): string | undefined => {
    const id = strParam(entry, 'category_id');
    if (id === undefined) return undefined;
    return state?.categories.find((c) => c.category_id === id)?.name ?? shortId(id);
  };

  // `name` carried by the op wins (it is the value at the time of the
  // action); channel/category/role resolution covers name-less ops.
  const name =
    strParam(entry, 'name') ?? channelName() ?? categoryName() ?? roleName() ?? '';
  const member = strParam(entry, 'member');
  return interpolate(template(t), {
    name,
    role: roleName() ?? strParam(entry, 'name') ?? '',
    member: member === undefined ? t.serveur.auditUnknownActor : nameOf(member),
  });
}

export function ServerAuditTab({ groupId }: { groupId: string }) {
  const t = useT();
  const lang = useUi((s) => s.lang);
  const toast = useUi((s) => s.toast);
  const contacts = useFriends((s) => s.contacts);
  const self = useSession((s) => s.self);
  const state = useGroups((s) => s.states[groupId]);
  const [entries, setEntries] = useState<AuditEntry[]>([]);
  const [hasMore, setHasMore] = useState(false);
  const [loading, setLoading] = useState(false);

  const nameOf = (pubkey: string): string =>
    self !== null && pubkey === self.pubkey
      ? `${selfDisplayName(self)} (${t.app.you})`
      : displayNameOf(contacts, pubkey);

  const errorLabel = t.errors.actionFailed;
  const loadPage = useCallback(
    async (before?: string): Promise<void> => {
      setLoading(true);
      try {
        const { entries: page } = await api.groupsAudit(groupId, before, AUDIT_PAGE);
        setEntries((prev) => (before === undefined ? page : [...prev, ...page]));
        setHasMore(page.length === AUDIT_PAGE);
      } catch (e) {
        toast('error', messageOf(e, errorLabel));
      } finally {
        setLoading(false);
      }
    },
    [groupId, toast, errorLabel],
  );

  useEffect(() => {
    setEntries([]);
    void loadPage();
  }, [loadPage]);

  const [exportEnCours, setExportEnCours] = useState(false);

  /**
   * Copie le journal complet (borné) en Markdown.
   *
   * Refait la pagination depuis le début plutôt que d'exporter ce qui est
   * affiché : l'onglet ne montre que ce que l'utilisateur a déroulé, et un
   * export qui s'arrête là où le regard s'est arrêté n'est pas un registre.
   */
  const exporter = async (): Promise<void> => {
    if (exportEnCours) return;
    setExportEnCours(true);
    try {
      const toutes: AuditEntry[] = [];
      let avant: string | undefined;
      let tronque = false;
      for (;;) {
        const { entries: page } = await api.groupsAudit(groupId, avant, AUDIT_PAGE);
        toutes.push(...page);
        const dernier = page[page.length - 1];
        if (page.length < AUDIT_PAGE || dernier === undefined) break;
        if (toutes.length >= AUDIT_EXPORT_MAX) {
          tronque = true;
          break;
        }
        avant = dernier.op_id;
      }
      const markdown = buildAuditExport(
        toutes.map((e) => ({
          actor: nameOf(e.author),
          action: describeEntry(t, e, state, nameOf),
          at: formatTimestamp(e.wall_ms, lang),
        })),
        {
          heading: interpolate(t.serveur.auditExportHeading, {
            server: state?.name ?? '',
          }),
          subtitle: interpolate(t.serveur.auditExportSubtitle, {
            count: String(toutes.length),
          }),
          empty: t.serveur.auditEmpty,
          columnAt: t.serveur.auditExportColumnAt,
          columnActor: t.serveur.auditExportColumnActor,
          columnAction: t.serveur.auditExportColumnAction,
          truncated: tronque
            ? interpolate(t.serveur.auditExportTruncated, {
                max: String(AUDIT_EXPORT_MAX),
              })
            : null,
        },
      );
      // `copyToClipboard` est best-effort : le presse-papiers peut être
      // refusé (environnement restreint). L'échec se dit, il ne se tait pas —
      // sinon l'utilisateur croit tenir son registre et n'a rien.
      copyToClipboard(
        markdown,
        () => toast('success', t.serveur.auditExportDone),
        () => toast('error', errorLabel),
      );
    } catch (e) {
      toast('error', messageOf(e, errorLabel));
    } finally {
      setExportEnCours(false);
    }
  };

  const oldest = entries[entries.length - 1];

  return (
    <div>
      {entries.length === 0 && !loading && (
        <p className="text-sm text-muted">{t.serveur.auditEmpty}</p>
      )}
      {entries.length > 0 && (
        <button
          type="button"
          disabled={exportEnCours}
          onClick={() => void exporter()}
          className="mb-3 rounded-lg bg-rail px-4 py-2 text-sm font-medium text-norm transition-colors duration-fast hover:bg-input focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple disabled:opacity-50"
        >
          {t.serveur.auditExport}
        </button>
      )}
      <ol className="m-0 list-none p-0">
        {entries.map((entry) => {
          const actor = nameOf(entry.author);
          return (
            <li
              key={entry.op_id}
              className="mb-1 flex items-center gap-3 rounded-lg bg-sidebar px-3 py-2"
            >
              <Avatar
                id={entry.author}
                name={actor}
                size={32}
                decoration={
                  self !== null && entry.author === self.pubkey
                    ? self.avatar_decoration
                    : avatarDecorationOf(contacts, entry.author)
                }
              />
              <div className="min-w-0 flex-1">
                <span className="text-sm text-norm">
                  <span className="font-medium text-header">{actor}</span>{' '}
                  {describeEntry(t, entry, state, nameOf)}
                </span>
              </div>
              <span className="shrink-0 text-xs text-faint">
                {formatTimestamp(entry.wall_ms, lang)}
              </span>
            </li>
          );
        })}
      </ol>
      {hasMore && oldest !== undefined && (
        <button
          type="button"
          disabled={loading}
          onClick={() => void loadPage(oldest.op_id)}
          className="mt-2 rounded-lg bg-rail px-4 py-2 text-sm font-medium text-norm transition-colors duration-fast hover:bg-input focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-chat disabled:opacity-50"
        >
          {t.serveur.auditLoadMore}
        </button>
      )}
    </div>
  );
}
