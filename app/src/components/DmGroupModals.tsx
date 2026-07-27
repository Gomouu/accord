/**
 * Groupes de MP (jalon 5, `docs/DM_GROUPS.md`) : création et réglages.
 *
 * Chargées à la demande — deux surfaces qu'une session peut très bien ne
 * jamais ouvrir, et le budget du chunk initial n'a pas à les porter (voir
 * `scripts/check-bundle-budget.mjs`).
 *
 * Ce qu'on peut faire ici est exactement ce que le nœud accepte dans un tel
 * groupe : renommer, changer l'icône, partir. Aucune hiérarchie n'est
 * consultée — tout membre en a le droit, y compris sur un groupe qu'il n'a
 * pas fondé, et le fondateur n'a rien de plus.
 */

import { useEffect, useRef, useState } from 'react';
import { interpolate } from '../i18n';
import { api } from '../lib/client';
import { lireFichier } from '../lib/files';
import { displayNameOf, useFriends } from '../stores/friends';
import { dmThreadId, useGroups } from '../stores/groups';
import { serverLevel, useMute, type NotifLevel } from '../stores/mute';
import { selfDisplayName, useSession } from '../stores/session';
import { useUi, useT } from '../stores/ui';
import { Avatar } from './Avatar';
import { AvatarCropper } from './AvatarCropper';
import { ModalFrame } from './Modals';
import { messageOf } from './server/controls';

/**
 * Plafond du contrat (`MAX_DM_MEMBERS` côté nœud) : vingt personnes, soi
 * comprise. Au-delà, un serveur est la bonne forme — le nœud refuse l'ajout
 * de toute façon, cette borne ne fait que l'annoncer avant le refus.
 */
const DM_GROUP_MAX = 20;

/**
 * Un groupe de MP commence à trois. À deux, le message privé existe déjà et
 * fait mieux : pas d'op-log, pas de membres à gérer.
 */
const DM_GROUP_MIN = 3;

/** Bornes du nom, alignées sur `validate_label` côté nœud. */
const NAME_MAX = 100;

const boutonPrimaire =
  'rounded-lg bg-blurple px-4 py-2 text-sm font-medium text-white transition-colors duration-fast hover:bg-blurple-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-modal disabled:opacity-50 active:scale-[0.98]';
const boutonSecondaire =
  'rounded-sm px-4 py-2 text-sm font-medium text-muted transition-colors duration-fast hover:bg-chat-hover hover:text-norm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-modal';
const champ =
  'w-full rounded-md border border-transparent bg-input px-3 py-2.5 text-norm placeholder-faint outline-none transition-colors duration-fast focus:border-blurple/50';

/** Coche de sélection d'une rangée d'ami (décorative : le bouton porte `aria-pressed`). */
function CheckIcon() {
  return (
    <svg
      width="14"
      height="14"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={3}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
    >
      <path d="m5 12 5 5L20 7" />
    </svg>
  );
}

/**
 * Création d'un groupe de MP : on coche des amis, on nomme, on crée. Le nœud
 * fait le reste en un appel (`groups.create_dm` crée le groupe, son fil unique
 * et ajoute les membres), puis la conversation s'ouvre.
 */
export function CreateDmGroupModal() {
  const t = useT();
  const toast = useUi((s) => s.toast);
  const closeModal = useUi((s) => s.closeModal);
  const setView = useUi((s) => s.setView);
  const contacts = useFriends((s) => s.contacts);
  const createDm = useGroups((s) => s.createDm);
  const [query, setQuery] = useState('');
  const [chosen, setChosen] = useState<ReadonlySet<string>>(new Set());
  const [name, setName] = useState('');
  const [busy, setBusy] = useState(false);

  const friends = contacts.filter((c) => c.state === 'friend');
  const needle = query.trim().toLowerCase();
  const visible =
    needle === ''
      ? friends
      : friends.filter((c) =>
          displayNameOf(contacts, c.pubkey).toLowerCase().includes(needle),
        );

  // Soi-même compte déjà : on choisit au plus dix-neuf autres personnes.
  const maxOthers = DM_GROUP_MAX - 1;
  const minOthers = DM_GROUP_MIN - 1;
  const trimmed = name.trim();
  const canSubmit =
    trimmed !== '' && chosen.size >= minOthers && chosen.size <= maxOthers && !busy;

  const toggle = (pubkey: string): void =>
    setChosen((prev) => {
      const next = new Set(prev);
      if (next.has(pubkey)) next.delete(pubkey);
      else if (next.size < maxOthers) next.add(pubkey);
      return next;
    });

  const submit = async (): Promise<void> => {
    if (!canSubmit) return;
    setBusy(true);
    try {
      const groupId = await createDm(trimmed, [...chosen]);
      const state = useGroups.getState().states[groupId];
      setView({ kind: 'group', groupId, channelId: dmThreadId(state) });
      closeModal();
    } catch (e) {
      toast('error', messageOf(e, t.errors.actionFailed));
      setBusy(false);
    }
  };

  return (
    <ModalFrame title={t.dmGroups.createTitle} hint={t.dmGroups.createHint}>
      {friends.length === 0 ? (
        <p className="py-4 text-center text-sm text-muted">{t.dmGroups.noFriends}</p>
      ) : (
        <>
          <input
            aria-label={t.dmGroups.searchPlaceholder}
            placeholder={t.dmGroups.searchPlaceholder}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            className="mb-3 w-full rounded-md border border-transparent bg-input px-3 py-2 text-sm text-norm placeholder-faint outline-none transition-colors duration-fast focus:border-blurple/50"
          />
          <p className="mb-2 text-xs text-faint">
            {interpolate(t.dmGroups.selected, {
              count: String(chosen.size),
              max: String(maxOthers),
            })}
          </p>
          {visible.length === 0 && (
            <p className="py-4 text-center text-sm text-muted">{t.dmGroups.noResults}</p>
          )}
          <div className="max-h-56 space-y-1 overflow-y-auto">
            {visible.map((c) => {
              const picked = chosen.has(c.pubkey);
              const label = displayNameOf(contacts, c.pubkey);
              return (
                <button
                  key={c.pubkey}
                  type="button"
                  aria-pressed={picked}
                  disabled={!picked && chosen.size >= maxOthers}
                  onClick={() => toggle(c.pubkey)}
                  className={`flex w-full items-center gap-3 rounded-md px-2 py-1.5 text-start transition-colors duration-fast focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-modal disabled:opacity-40 ${
                    picked ? 'bg-blurple/15' : 'hover:bg-chat-hover'
                  }`}
                >
                  <Avatar
                    id={c.pubkey}
                    name={label}
                    size={32}
                    avatarHash={c.avatar}
                    hint={c.pubkey}
                  />
                  <span className="min-w-0 flex-1 truncate text-norm">{label}</span>
                  <span
                    aria-hidden
                    className={`flex h-5 w-5 shrink-0 items-center justify-center rounded-full ${
                      picked ? 'bg-blurple text-white' : 'border border-input'
                    }`}
                  >
                    {picked && <CheckIcon />}
                  </span>
                </button>
              );
            })}
          </div>
          <input
            aria-label={t.dmGroups.namePlaceholder}
            placeholder={t.dmGroups.namePlaceholder}
            value={name}
            maxLength={NAME_MAX}
            onChange={(e) => setName(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') void submit();
            }}
            className={`mt-4 ${champ}`}
          />
          {chosen.size < minOthers && (
            <p className="mt-2 text-xs text-faint">{t.dmGroups.needMore}</p>
          )}
        </>
      )}
      <div className="mt-4 flex justify-end gap-3">
        <button type="button" onClick={closeModal} className={boutonSecondaire}>
          {t.app.cancel}
        </button>
        <button
          type="button"
          disabled={!canSubmit}
          onClick={() => void submit()}
          className={boutonPrimaire}
        >
          {t.dmGroups.createAction}
        </button>
      </div>
    </ModalFrame>
  );
}

/** Section « icône » : image carrée recadrée puis publiée par `SetMeta`. */
function IconSection({ groupId }: { groupId: string }) {
  const t = useT();
  const toast = useUi((s) => s.toast);
  const state = useGroups((s) => s.states[groupId]);
  const setIcon = useGroups((s) => s.setIcon);
  const [busy, setBusy] = useState(false);
  /** Aperçu courant : icône publiée, remplacée par l'image fraîche choisie. */
  const [preview, setPreview] = useState<string | null>(null);
  /** Image en cours de recadrage (recadreur ouvert tant que non nulle). */
  const [cropFile, setCropFile] = useState<File | null>(null);
  const fileRef = useRef<HTMLInputElement>(null);

  const icon = state?.icon ?? null;
  useEffect(() => {
    let alive = true;
    setPreview(null);
    if (icon === null) return undefined;
    lireFichier(icon)
      .then((url) => {
        if (alive) setPreview(url);
      })
      .catch(() => {
        // Icône indisponible : l'aperçu retombe sur les initiales.
      });
    return () => {
      alive = false;
    };
  }, [icon]);

  if (state === undefined) return null;

  const publier = async (dataB64: string, mime: string, dataUrl: string) => {
    setBusy(true);
    try {
      await setIcon(groupId, dataB64, mime);
      setPreview(dataUrl);
      toast('info', t.dmGroups.iconUpdated);
    } catch (e) {
      toast('error', messageOf(e, t.errors.actionFailed));
    } finally {
      setBusy(false);
      setCropFile(null);
    }
  };

  return (
    <>
      <h3 className="mt-5 text-xs font-medium uppercase tracking-wide text-faint">
        {t.dmGroups.icon}
      </h3>
      <div className="mt-2 flex items-center gap-4 rounded-lg bg-sidebar p-3">
        <span className="shrink-0">
          <Avatar
            id={groupId}
            name={state.name}
            size={56}
            imageUrl={preview}
            avatarHash={state.icon}
          />
        </span>
        <input
          ref={fileRef}
          type="file"
          accept="image/*"
          aria-label={t.dmGroups.chooseImage}
          className="hidden"
          onChange={(e) => {
            const file = e.target.files?.[0];
            // Autorise de re-choisir le même fichier plus tard.
            e.target.value = '';
            if (file !== undefined) setCropFile(file);
          }}
        />
        <button
          type="button"
          disabled={busy}
          onClick={() => fileRef.current?.click()}
          className={boutonPrimaire}
        >
          {t.dmGroups.chooseImage}
        </button>
      </div>
      {cropFile !== null && (
        <AvatarCropper
          fichier={cropFile}
          forme="carre"
          onAnnuler={() => setCropFile(null)}
          onValider={(r) => publier(r.dataB64, r.mime, r.dataUrl)}
        />
      )}
    </>
  );
}

/** Membres d'un groupe de MP, au plus — miroir de `MAX_DM_MEMBERS` côté nœud. */
const MAX_DM_MEMBERS = 20;

/**
 * Inviter quelqu'un dans un groupe de MP existant.
 *
 * 🔒 **Inviter, pas ajouter.** La personne reçoit un ticket signé et n'entre
 * qu'après avoir accepté (D-045). C'est pourquoi le libellé dit « invitation
 * envoyée » et non « membre ajouté » : promettre l'ajout serait mentir sur ce
 * qui vient de se passer, et l'attente peut durer.
 *
 * N'importe quel membre le peut : un groupe de MP n'a pas de hiérarchie à
 * consulter. Le plafond, lui, est réel et vérifié aussi côté nœud.
 */
/**
 * Niveau de notification du groupe (jalon 5, « réglage indépendant »).
 *
 * La tuyauterie existait déjà et couvrait les groupes de MP sans le savoir :
 * `isConversationSilenced` interroge `channelLevel`, qui retombe sur le niveau
 * du groupe, et `AppShell` l'appelle avant toute notification. Il ne manquait
 * qu'une surface pour le régler — le rail des serveurs et le menu d'en-tête,
 * qui la portent pour un serveur, n'existent pas pour un groupe de MP.
 */
function NotifSection({ groupId }: { groupId: string }) {
  const t = useT();
  const serverLevels = useMute((s) => s.serverLevels);
  const courant = serverLevel({ serverLevels, channelLevels: {} }, groupId);
  const niveaux: ReadonlyArray<{ id: NotifLevel; label: string }> = [
    { id: 'all', label: t.notifLevel.all },
    { id: 'mentions', label: t.notifLevel.mentions },
    { id: 'none', label: t.notifLevel.none },
  ];

  return (
    <>
      <h3 className="mt-5 text-xs font-medium uppercase tracking-wide text-faint">
        {t.notifLevel.title}
      </h3>
      <div
        role="radiogroup"
        aria-label={t.notifLevel.title}
        className="mt-2 flex flex-wrap gap-2"
      >
        {niveaux.map((n) => (
          <button
            key={n.id}
            type="button"
            role="radio"
            aria-checked={courant === n.id}
            onClick={() => useMute.getState().setServerLevel(groupId, n.id)}
            className={`rounded-md px-3 py-1.5 text-sm transition-colors duration-fast focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-modal ${
              courant === n.id
                ? 'bg-blurple text-white'
                : 'bg-input text-muted hover:text-norm'
            }`}
          >
            {n.label}
          </button>
        ))}
      </div>
    </>
  );
}

function InviteSection({ groupId }: { groupId: string }) {
  const t = useT();
  const toast = useUi((s) => s.toast);
  const contacts = useFriends((s) => s.contacts);
  const state = useGroups((s) => s.states[groupId]);
  const reload = useGroups((s) => s.loadState);
  const [query, setQuery] = useState('');
  const [busy, setBusy] = useState<string | null>(null);

  if (state === undefined) return null;

  const membres = new Set(state.members.map((m) => m.pubkey));
  const invitables = contacts.filter(
    (c) => c.state === 'friend' && !membres.has(c.pubkey),
  );
  const q = query.trim().toLowerCase();
  const visibles =
    q === ''
      ? invitables
      : invitables.filter((c) =>
          displayNameOf(contacts, c.pubkey).toLowerCase().includes(q),
        );
  const complet = state.members.length >= MAX_DM_MEMBERS;

  const inviter = async (pubkey: string) => {
    setBusy(pubkey);
    try {
      await api.groupsInviteCreate(groupId, pubkey);
      toast(
        'success',
        interpolate(t.dmGroups.invited, { name: displayNameOf(contacts, pubkey) }),
      );
      await reload(groupId);
    } catch (e) {
      toast('error', messageOf(e, t.errors.actionFailed));
    } finally {
      setBusy(null);
    }
  };

  return (
    <>
      <h3 className="mt-5 text-xs font-medium uppercase tracking-wide text-faint">
        {t.dmGroups.inviteSection}
      </h3>
      <p className="mt-1 text-xs text-muted">{t.dmGroups.inviteHint}</p>
      {complet ? (
        <p className="mt-2 text-sm text-muted">{t.dmGroups.inviteFull}</p>
      ) : invitables.length === 0 ? (
        <p className="mt-2 text-sm text-muted">{t.dmGroups.inviteNobody}</p>
      ) : (
        <>
          <input
            aria-label={t.dmGroups.searchPlaceholder}
            placeholder={t.dmGroups.searchPlaceholder}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            className="mt-2 w-full rounded-md border border-transparent bg-input px-3 py-2 text-sm text-norm placeholder-faint outline-none transition-colors duration-fast focus:border-blurple/50"
          />
          {visibles.length === 0 ? (
            <p className="mt-2 text-sm text-muted">{t.dmGroups.noResults}</p>
          ) : (
            <ul
              aria-label={t.dmGroups.inviteSection}
              className="mt-2 max-h-40 space-y-1 overflow-y-auto"
            >
              {visibles.map((c) => (
                <li key={c.pubkey} className="flex items-center gap-3 px-2 py-1">
                  <Avatar
                    id={c.pubkey}
                    name={displayNameOf(contacts, c.pubkey)}
                    size={28}
                  />
                  <span className="min-w-0 flex-1 truncate text-sm text-norm">
                    {displayNameOf(contacts, c.pubkey)}
                  </span>
                  <button
                    type="button"
                    disabled={busy !== null}
                    onClick={() => void inviter(c.pubkey)}
                    className="shrink-0 rounded-md bg-blurple px-3 py-1.5 text-sm font-medium text-white transition-opacity duration-fast hover:opacity-90 disabled:opacity-50"
                  >
                    {t.dmGroups.inviteAction}
                  </button>
                </li>
              ))}
            </ul>
          )}
        </>
      )}
    </>
  );
}

/**
 * Réglages d'un groupe de MP : nom, icône, membres, départ. Tout est ouvert à
 * tout membre — il n'y a pas de modérateur à consulter, et partir est le seul
 * recours prévu par la conception (`docs/DM_GROUPS.md` §5).
 */
export function DmGroupModal({ groupId }: { groupId: string }) {
  const t = useT();
  const toast = useUi((s) => s.toast);
  const closeModal = useUi((s) => s.closeModal);
  const setView = useUi((s) => s.setView);
  const contacts = useFriends((s) => s.contacts);
  const self = useSession((s) => s.self);
  const state = useGroups((s) => s.states[groupId]);
  const rename = useGroups((s) => s.rename);
  const leave = useGroups((s) => s.leave);
  const [draft, setDraft] = useState(state?.name ?? '');
  const [busy, setBusy] = useState(false);
  const [confirmLeave, setConfirmLeave] = useState(false);

  if (state === undefined) return null;

  const trimmed = draft.trim();
  const canRename = trimmed !== '' && trimmed !== state.name && !busy;

  const nameOf = (pubkey: string): string =>
    self !== null && pubkey === self.pubkey
      ? selfDisplayName(self)
      : displayNameOf(contacts, pubkey);

  const renommer = async (): Promise<void> => {
    if (!canRename) return;
    setBusy(true);
    try {
      await rename(groupId, trimmed);
      toast('info', t.dmGroups.renamed);
    } catch (e) {
      toast('error', messageOf(e, t.errors.actionFailed));
    } finally {
      setBusy(false);
    }
  };

  const partir = async (): Promise<void> => {
    if (busy) return;
    setBusy(true);
    try {
      await leave(groupId);
      toast('info', t.dmGroups.left);
      closeModal();
      setView({ kind: 'friends' });
    } catch (e) {
      toast('error', messageOf(e, t.errors.actionFailed));
      setBusy(false);
    }
  };

  return (
    <ModalFrame title={t.dmGroups.settings} hint={t.dmGroups.anyoneCanEdit}>
      <h3 className="text-xs font-medium uppercase tracking-wide text-faint">
        {t.dmGroups.nameLabel}
      </h3>
      <div className="mt-2 flex gap-3">
        <input
          aria-label={t.dmGroups.nameLabel}
          value={draft}
          maxLength={NAME_MAX}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') void renommer();
          }}
          className={champ}
        />
        <button
          type="button"
          disabled={!canRename}
          onClick={() => void renommer()}
          className={`${boutonPrimaire} shrink-0`}
        >
          {t.dmGroups.rename}
        </button>
      </div>

      <IconSection groupId={groupId} />

      <InviteSection groupId={groupId} />

      <NotifSection groupId={groupId} />

      <h3 className="mt-5 text-xs font-medium uppercase tracking-wide text-faint">
        {interpolate(t.dmGroups.members, { count: String(state.members.length) })}
      </h3>
      <ul
        aria-label={interpolate(t.dmGroups.members, {
          count: String(state.members.length),
        })}
        className="mt-2 max-h-48 space-y-1 overflow-y-auto"
      >
        {state.members.map((m) => (
          <li key={m.pubkey} className="flex items-center gap-3 rounded-md px-2 py-1.5">
            <Avatar
              id={m.pubkey}
              name={nameOf(m.pubkey)}
              size={32}
              avatarHash={m.avatar ?? null}
              hint={m.pubkey}
            />
            <span className="min-w-0 flex-1 truncate text-sm text-norm">
              {nameOf(m.pubkey)}
            </span>
            {self !== null && m.pubkey === self.pubkey && (
              <span className="shrink-0 text-xs text-faint">{t.dmGroups.you}</span>
            )}
          </li>
        ))}
      </ul>

      <div className="mt-5 border-t border-input/50 pt-4">
        {confirmLeave ? (
          <>
            <p className="text-sm text-norm">
              {interpolate(t.dmGroups.leaveConfirm, { name: state.name })}
            </p>
            <div className="mt-3 flex justify-end gap-3">
              <button
                type="button"
                onClick={() => setConfirmLeave(false)}
                className={boutonSecondaire}
              >
                {t.app.cancel}
              </button>
              <button
                type="button"
                disabled={busy}
                onClick={() => void partir()}
                className="rounded-lg bg-red px-4 py-2 text-sm font-medium text-on-red transition-colors duration-fast hover:brightness-110 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-red focus-visible:ring-offset-2 focus-visible:ring-offset-modal disabled:opacity-50 active:scale-[0.98]"
              >
                {t.app.confirm}
              </button>
            </div>
          </>
        ) : (
          <button
            type="button"
            onClick={() => setConfirmLeave(true)}
            className="rounded-lg border border-red px-4 py-2 text-sm font-medium text-red transition-colors duration-fast hover:bg-red hover:text-on-red focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-red focus-visible:ring-offset-2 focus-visible:ring-offset-modal"
          >
            {t.dmGroups.leave}
          </button>
        )}
      </div>
    </ModalFrame>
  );
}
