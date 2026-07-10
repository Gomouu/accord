/**
 * Onglet Salons : création (texte/vocal/annonces, catégorie optionnelle),
 * création/renommage/suppression de catégorie, liste par catégorie avec
 * renommage, sujet, déplacement de catégorie et suppression (confirmée) —
 * gouverné par MANAGE_CHANNELS. Éditeur d'overrides de permissions par rôle
 * (hériter/autoriser/refuser, refus prioritaire) — gouverné par MANAGE_ROLES.
 */

import { useState } from 'react';
import { interpolate } from '../../i18n';
import type { GroupCategory, GroupChannel, GroupChannelKind, GroupStateJson } from '../../lib/api';
import {
  useGroups,
  channelsByCategory,
  hasPerm,
  overrideOf,
  sortRoles,
  PERMISSIONS,
} from '../../stores/groups';
import { useUi, useT } from '../../stores/ui';
import type { Dict } from '../../i18n';
import { SettingsSection } from '../settings/controls';
import { ConfirmButton, messageOf } from './controls';

const KINDS: Array<{ kind: GroupChannelKind; label: (t: Dict) => string }> = [
  { kind: 'text', label: (t) => t.serveur.kindText },
  { kind: 'voice', label: (t) => t.serveur.kindVoice },
  { kind: 'announcement', label: (t) => t.serveur.kindAnnouncement },
];

/** Libellé du genre d'un salon. */
function kindLabel(t: Dict, kind: GroupChannelKind): string {
  return KINDS.find((k) => k.kind === kind)?.label(t) ?? kind;
}

/** Channel-scoped permission bits offered in the override editor. */
const OVERRIDE_BITS: Array<{ bit: number; label: (t: Dict) => string }> = [
  { bit: PERMISSIONS.VIEW, label: (t) => t.serveur.permView },
  { bit: PERMISSIONS.SEND, label: (t) => t.serveur.permSend },
  { bit: PERMISSIONS.MANAGE_MESSAGES, label: (t) => t.serveur.permManageMessages },
];

type TriState = 'inherit' | 'allow' | 'deny';

/** Tri-state of `bit` in an override pair (deny wins). */
function triStateOf(allow: number, deny: number, bit: number): TriState {
  if ((deny & bit) !== 0) return 'deny';
  if ((allow & bit) !== 0) return 'allow';
  return 'inherit';
}

/**
 * Éditeur des overrides d'un salon : par rôle, un sélecteur tri-état par
 * permission. Chaque changement est appliqué immédiatement (op signée).
 */
function ChannelPermsEditor({
  groupId,
  channel,
  state,
}: {
  groupId: string;
  channel: GroupChannel;
  state: GroupStateJson;
}) {
  const t = useT();
  const toast = useUi((s) => s.toast);
  const setChannelPerms = useGroups((s) => s.setChannelPerms);
  const roles = sortRoles(state.roles);

  const applyChange = (roleId: string, bit: number, choice: TriState): void => {
    const current = overrideOf(state, channel.channel_id, roleId);
    const allow = (current.allow & ~bit) | (choice === 'allow' ? bit : 0);
    const deny = (current.deny & ~bit) | (choice === 'deny' ? bit : 0);
    setChannelPerms(groupId, channel.channel_id, roleId, allow, deny)
      .then(() => toast('info', t.serveur.channelPermsSaved))
      .catch((e: unknown) => toast('error', messageOf(e, t.errors.actionFailed)));
  };

  if (roles.length === 0) {
    return <p className="mt-2 text-xs text-faint">{t.serveur.channelPermsNoRoles}</p>;
  }

  return (
    <div className="mt-2 rounded bg-rail p-3">
      <p className="mb-2 text-xs text-faint">{t.serveur.channelPermsHint}</p>
      {roles.map((role) => {
        const current = overrideOf(state, channel.channel_id, role.role_id);
        return (
          <div key={role.role_id} className="mb-2">
            <div className="mb-1 text-xs font-semibold uppercase text-faint">
              {role.name}
            </div>
            <div className="flex flex-wrap gap-2">
              {OVERRIDE_BITS.map(({ bit, label }) => (
                <label
                  key={bit}
                  className="flex items-center gap-1.5 text-xs text-norm"
                >
                  {label(t)}
                  <select
                    aria-label={`${role.name} — ${label(t)}`}
                    value={triStateOf(current.allow, current.deny, bit)}
                    onChange={(e) =>
                      applyChange(role.role_id, bit, e.target.value as TriState)
                    }
                    className="rounded bg-sidebar px-1.5 py-1 text-xs text-norm outline-none"
                  >
                    <option value="inherit">{t.serveur.permInherit}</option>
                    <option value="allow">{t.serveur.permAllow}</option>
                    <option value="deny">{t.serveur.permDeny}</option>
                  </select>
                </label>
              ))}
            </div>
          </div>
        );
      })}
    </div>
  );
}

/**
 * Éditeur en place d'un salon : nom, sujet, catégorie, suppression
 * confirmée, et overrides de permissions (MANAGE_ROLES).
 */
function ChannelEditor({
  groupId,
  channel,
  state,
  canManage,
  canManageRoles,
}: {
  groupId: string;
  channel: GroupChannel;
  state: GroupStateJson;
  canManage: boolean;
  canManageRoles: boolean;
}) {
  const t = useT();
  const toast = useUi((s) => s.toast);
  const renameChannel = useGroups((s) => s.renameChannel);
  const setChannelCategory = useGroups((s) => s.setChannelCategory);
  const setTopic = useGroups((s) => s.setTopic);
  const deleteChannel = useGroups((s) => s.deleteChannel);
  const [name, setName] = useState(channel.name);
  const [topic, setTopicDraft] = useState(channel.topic);
  const [category, setCategory] = useState(channel.category ?? '');
  const [permsOpen, setPermsOpen] = useState(false);
  const [busy, setBusy] = useState(false);

  const hasTopic = channel.kind !== 'voice';
  const nameTrimmed = name.trim();
  const nameDirty = nameTrimmed !== channel.name && nameTrimmed !== '';
  const topicDirty = hasTopic && topic.trim() !== channel.topic;
  const categoryDirty = category !== (channel.category ?? '');
  const dirty = nameDirty || topicDirty || categoryDirty;

  const save = async (): Promise<void> => {
    if (busy || !dirty) return;
    setBusy(true);
    try {
      if (nameDirty) await renameChannel(groupId, channel.channel_id, nameTrimmed);
      if (topicDirty) await setTopic(groupId, channel.channel_id, topic.trim());
      if (categoryDirty) {
        await setChannelCategory(
          groupId,
          channel.channel_id,
          category === '' ? null : category,
        );
      }
      toast('info', t.serveur.channelSaved);
    } catch (e) {
      toast('error', messageOf(e, t.errors.actionFailed));
    } finally {
      setBusy(false);
    }
  };

  if (!canManage) {
    return (
      <div className="mb-2 rounded-lg bg-sidebar px-4 py-3">
        <div className="flex items-center gap-2">
          <span className="font-medium text-header">{channel.name}</span>
          <span className="text-xs text-faint">{kindLabel(t, channel.kind)}</span>
        </div>
        {channel.topic !== '' && (
          <div className="mt-1 text-sm text-muted">{channel.topic}</div>
        )}
      </div>
    );
  }

  return (
    <div className="mb-2 rounded-lg bg-sidebar p-3">
      <div className="flex items-center gap-3">
        <input
          aria-label={t.serveur.channelNameLabel}
          value={name}
          onChange={(e) => setName(e.target.value)}
          className="min-w-0 flex-1 rounded bg-rail px-3 py-2 text-norm outline-none focus-visible:ring-2 focus-visible:ring-blurple"
        />
        <span className="shrink-0 text-xs text-faint">{kindLabel(t, channel.kind)}</span>
      </div>
      {hasTopic && (
        <input
          aria-label={t.serveur.topicLabel}
          placeholder={t.serveur.topicPlaceholder}
          value={topic}
          onChange={(e) => setTopicDraft(e.target.value)}
          className="mt-2 w-full rounded bg-rail px-3 py-2 text-sm text-norm placeholder-faint outline-none focus-visible:ring-2 focus-visible:ring-blurple"
        />
      )}
      {state.categories.length > 0 && (
        <div className="mt-2 flex items-center gap-2">
          <span className="text-xs font-semibold uppercase text-faint">
            {t.serveur.categoryLabel}
          </span>
          <select
            aria-label={t.serveur.categoryLabel}
            value={category}
            onChange={(e) => setCategory(e.target.value)}
            className="rounded bg-rail px-2 py-1.5 text-sm text-norm outline-none"
          >
            <option value="">{t.serveur.noCategory}</option>
            {state.categories.map((c) => (
              <option key={c.category_id} value={c.category_id}>
                {c.name}
              </option>
            ))}
          </select>
        </div>
      )}
      {canManageRoles && (
        <div className="mt-2">
          <button
            type="button"
            aria-expanded={permsOpen}
            onClick={() => setPermsOpen((open) => !open)}
            className="rounded bg-rail px-3 py-1.5 text-xs font-medium text-norm transition-colors duration-150 hover:bg-input"
          >
            {t.serveur.channelPermsToggle}
          </button>
          {permsOpen && (
            <ChannelPermsEditor groupId={groupId} channel={channel} state={state} />
          )}
        </div>
      )}
      <div className="mt-2 flex items-center justify-between gap-3">
        <ConfirmButton
          action={t.serveur.deleteChannel}
          question={interpolate(t.serveur.deleteChannelConfirm, {
            name: channel.name,
          })}
          onConfirm={() => {
            deleteChannel(groupId, channel.channel_id).catch((e: unknown) =>
              toast('error', messageOf(e, t.errors.actionFailed)),
            );
          }}
        />
        <button
          type="button"
          disabled={busy || !dirty}
          onClick={() => void save()}
          className="rounded bg-blurple px-4 py-1.5 text-sm font-medium text-white transition-colors duration-150 hover:bg-blurple-hover disabled:opacity-50"
        >
          {t.serveur.channelSave}
        </button>
      </div>
    </div>
  );
}

/** Entête éditable d'une catégorie : renommage et suppression confirmée. */
function CategoryEditor({
  groupId,
  category,
}: {
  groupId: string;
  category: GroupCategory;
}) {
  const t = useT();
  const toast = useUi((s) => s.toast);
  const renameCategory = useGroups((s) => s.renameCategory);
  const deleteCategory = useGroups((s) => s.deleteCategory);
  const [name, setName] = useState(category.name);
  const [busy, setBusy] = useState(false);

  const nameTrimmed = name.trim();
  const dirty = nameTrimmed !== category.name && nameTrimmed !== '';

  const save = async (): Promise<void> => {
    if (busy || !dirty) return;
    setBusy(true);
    try {
      await renameCategory(groupId, category.category_id, nameTrimmed);
      toast('info', t.serveur.categorySaved);
    } catch (e) {
      toast('error', messageOf(e, t.errors.actionFailed));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="mb-2 flex flex-wrap items-center gap-2 rounded-lg bg-sidebar p-2">
      <input
        aria-label={interpolate(t.serveur.categoryRenameLabel, { name: category.name })}
        value={name}
        onChange={(e) => setName(e.target.value)}
        className="min-w-0 flex-1 rounded bg-rail px-3 py-1.5 text-sm text-norm outline-none focus-visible:ring-2 focus-visible:ring-blurple"
      />
      <button
        type="button"
        disabled={busy || !dirty}
        onClick={() => void save()}
        className="rounded bg-blurple px-3 py-1.5 text-xs font-medium text-white transition-colors duration-150 hover:bg-blurple-hover disabled:opacity-50"
      >
        {t.serveur.categoryRename}
      </button>
      <ConfirmButton
        action={t.serveur.deleteCategory}
        question={interpolate(t.serveur.deleteCategoryConfirm, {
          name: category.name,
        })}
        onConfirm={() => {
          deleteCategory(groupId, category.category_id).catch((e: unknown) =>
            toast('error', messageOf(e, t.errors.actionFailed)),
          );
        }}
      />
    </div>
  );
}

export function ServerChannelsTab({ groupId }: { groupId: string }) {
  const t = useT();
  const toast = useUi((s) => s.toast);
  const state = useGroups((s) => s.states[groupId]);
  const addChannel = useGroups((s) => s.addChannel);
  const addCategory = useGroups((s) => s.addCategory);
  const [newName, setNewName] = useState('');
  const [newKind, setNewKind] = useState<GroupChannelKind>('text');
  const [newCategory, setNewCategory] = useState('');
  const [newCategoryName, setNewCategoryName] = useState('');
  const [busy, setBusy] = useState(false);

  if (!state) return null;

  const canManage = hasPerm(state.my_permissions, PERMISSIONS.MANAGE_CHANNELS);
  const canManageRoles = hasPerm(state.my_permissions, PERMISSIONS.MANAGE_ROLES);
  const sections = channelsByCategory(state.channels, state.categories);

  const createChannel = async (): Promise<void> => {
    const name = newName.trim();
    if (name === '' || busy) return;
    setBusy(true);
    try {
      await addChannel(
        groupId,
        name,
        newKind,
        newCategory === '' ? undefined : newCategory,
      );
      setNewName('');
    } catch (e) {
      toast('error', messageOf(e, t.errors.actionFailed));
    } finally {
      setBusy(false);
    }
  };

  const createCategory = async (): Promise<void> => {
    const name = newCategoryName.trim();
    if (name === '' || busy) return;
    setBusy(true);
    try {
      await addCategory(groupId, name);
      setNewCategoryName('');
    } catch (e) {
      toast('error', messageOf(e, t.errors.actionFailed));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div>
      {canManage && (
        <>
          <SettingsSection title={t.serveur.newChannelTitle}>
            <div className="flex flex-wrap items-center gap-3 rounded-lg bg-sidebar p-3">
              <input
                aria-label={t.serveur.channelNameLabel}
                placeholder={t.groups.channelNamePlaceholder}
                value={newName}
                onChange={(e) => setNewName(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter') void createChannel();
                }}
                className="min-w-0 flex-1 rounded bg-rail px-3 py-2 text-norm placeholder-faint outline-none focus-visible:ring-2 focus-visible:ring-blurple"
              />
              <select
                aria-label={t.serveur.kindLabel}
                value={newKind}
                onChange={(e) => setNewKind(e.target.value as GroupChannelKind)}
                className="rounded bg-rail px-2 py-2 text-sm text-norm outline-none"
              >
                {KINDS.map(({ kind, label }) => (
                  <option key={kind} value={kind}>
                    {label(t)}
                  </option>
                ))}
              </select>
              <select
                aria-label={t.serveur.categoryLabel}
                value={newCategory}
                onChange={(e) => setNewCategory(e.target.value)}
                className="rounded bg-rail px-2 py-2 text-sm text-norm outline-none"
              >
                <option value="">{t.serveur.noCategory}</option>
                {state.categories.map((c) => (
                  <option key={c.category_id} value={c.category_id}>
                    {c.name}
                  </option>
                ))}
              </select>
              <button
                type="button"
                disabled={newName.trim() === '' || busy}
                onClick={() => void createChannel()}
                className="rounded bg-blurple px-4 py-2 text-sm font-medium text-white transition-colors duration-150 hover:bg-blurple-hover disabled:opacity-50"
              >
                {t.groups.addChannelAction}
              </button>
            </div>
          </SettingsSection>

          <SettingsSection title={t.serveur.newCategoryTitle}>
            <div className="flex gap-3 rounded-lg bg-sidebar p-3">
              <input
                aria-label={t.serveur.categoryNamePlaceholder}
                placeholder={t.serveur.categoryNamePlaceholder}
                value={newCategoryName}
                onChange={(e) => setNewCategoryName(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter') void createCategory();
                }}
                className="min-w-0 flex-1 rounded bg-rail px-3 py-2 text-norm placeholder-faint outline-none focus-visible:ring-2 focus-visible:ring-blurple"
              />
              <button
                type="button"
                disabled={newCategoryName.trim() === '' || busy}
                onClick={() => void createCategory()}
                className="rounded bg-blurple px-4 py-2 text-sm font-medium text-white transition-colors duration-150 hover:bg-blurple-hover disabled:opacity-50"
              >
                {t.serveur.createCategoryAction}
              </button>
            </div>
          </SettingsSection>
        </>
      )}

      {sections.map((section) => {
        if (section.channels.length === 0 && section.category === null) return null;
        return (
          <SettingsSection
            key={section.category?.category_id ?? 'sans-categorie'}
            title={section.category?.name ?? t.serveur.noCategory}
          >
            {canManage && section.category !== null && (
              <CategoryEditor groupId={groupId} category={section.category} />
            )}
            {section.channels.map((channel) => (
              <ChannelEditor
                key={channel.channel_id}
                groupId={groupId}
                channel={channel}
                state={state}
                canManage={canManage}
                canManageRoles={canManageRoles}
              />
            ))}
          </SettingsSection>
        );
      })}
    </div>
  );
}
