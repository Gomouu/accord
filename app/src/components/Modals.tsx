/**
 * Modales : création de groupe/salon, invitation. Les paramètres ouvrent
 * l'écran plein format dédié (components/settings), même déclencheur
 * `ui.modal = { kind: 'settings' }`.
 */

import { useEffect, useRef, useState } from 'react';
import type { Dict } from '../i18n';
import { interpolate } from '../i18n';
import type { GroupChannelKind } from '../lib/api';
import { api } from '../lib/client';
import { copyToClipboard } from '../lib/clipboard';
import {
  estOptionSondageValide,
  estQuestionSondageValide,
  POLL_MAX_OPTIONS,
  POLL_MAX_PAR_GROUPE,
  POLL_MIN_OPTIONS,
  POLL_OPTION_MAX,
  POLL_QUESTION_MAX,
  utf8ByteLength,
} from '../lib/poll';
import { useFriends, displayNameOf } from '../stores/friends';
import { useGroups } from '../stores/groups';
import { useUi, useT } from '../stores/ui';
import { Avatar } from './Avatar';
import { ChannelIcon } from './Sidebar';
import { CloseIcon } from './ContextMenu';
import { EventsModal } from './EventsModal';
import { JoinServerModal } from './JoinServerModal';
import { messageOf } from './server/controls';
import { ServerSettingsModal } from './server/ServerSettingsModal';
import { SettingsModal } from './settings/SettingsModal';

/** Genres proposés dans le choix rapide (l'annonce reste réservée aux paramètres serveur). */
type QuickChannelKind = Extract<GroupChannelKind, 'text' | 'voice' | 'forum'>;

const QUICK_CHANNEL_KINDS: Array<{
  kind: QuickChannelKind;
  label: (t: Dict) => string;
  hint: (t: Dict) => string;
}> = [
  {
    kind: 'text',
    label: (t) => t.groups.kindTextChannel,
    hint: (t) => t.groups.kindTextHint,
  },
  {
    kind: 'voice',
    label: (t) => t.groups.kindVoiceChannel,
    hint: (t) => t.groups.kindVoiceHint,
  },
  {
    kind: 'forum',
    label: (t) => t.groups.kindForumChannel,
    hint: (t) => t.groups.kindForumHint,
  },
];

/**
 * Options du sélecteur « nombre d'utilisations » d'un lien partageable —
 * `value` est le `max_uses` du contrat (`0` = illimité).
 */
const INVITE_LINK_USES: Array<{ value: number; label: (t: Dict) => string }> = [
  { value: 1, label: (t) => t.inviteLink.uses1 },
  { value: 5, label: (t) => t.inviteLink.uses5 },
  { value: 10, label: (t) => t.inviteLink.uses10 },
  { value: 25, label: (t) => t.inviteLink.uses25 },
  { value: 0, label: (t) => t.inviteLink.usesUnlimited },
];

/**
 * Options du sélecteur « durée de validité » — `value` est le `expires_h` du
 * contrat en heures (`0` = jamais).
 */
const INVITE_LINK_DURATIONS: Array<{ value: number; label: (t: Dict) => string }> = [
  { value: 0.5, label: (t) => t.inviteLink.dur30m },
  { value: 1, label: (t) => t.inviteLink.dur1h },
  { value: 6, label: (t) => t.inviteLink.dur6h },
  { value: 12, label: (t) => t.inviteLink.dur12h },
  { value: 24, label: (t) => t.inviteLink.dur1d },
  { value: 168, label: (t) => t.inviteLink.dur7d },
  { value: 0, label: (t) => t.inviteLink.durNever },
];

export function ModalFrame({
  title,
  hint,
  children,
}: {
  title: string;
  hint?: string;
  children: React.ReactNode;
}) {
  const t = useT();
  const closeModal = useUi((s) => s.closeModal);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') closeModal();
    };
    window.addEventListener('keydown', onKey);
    ref.current?.querySelector('input')?.focus();
    return () => window.removeEventListener('keydown', onKey);
  }, [closeModal]);

  return (
    <div
      className="modal-overlay-enter fixed inset-0 z-40 flex items-center justify-center bg-black/75 backdrop-blur-sm"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) closeModal();
      }}
    >
      <div
        ref={ref}
        role="dialog"
        aria-modal="true"
        aria-label={title}
        className="glass modal-panel-enter w-[440px] max-w-[92vw] rounded-xl shadow-3"
      >
        <div className="p-5">
          <div className="flex items-start justify-between">
            <h2 className="text-lg font-semibold text-header">{title}</h2>
            <button
              type="button"
              aria-label={t.app.close}
              onClick={closeModal}
              className="rounded-sm p-1 text-faint transition-colors duration-fast hover:text-norm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-modal active:scale-95"
            >
              <CloseIcon size={20} />
            </button>
          </div>
          {hint && <p className="mt-1 text-sm text-muted">{hint}</p>}
          <div className="mt-4">{children}</div>
        </div>
      </div>
    </div>
  );
}

function NameForm({
  placeholder,
  action,
  onSubmit,
}: {
  placeholder: string;
  action: string;
  onSubmit: (name: string) => Promise<void>;
}) {
  const t = useT();
  const toast = useUi((s) => s.toast);
  const closeModal = useUi((s) => s.closeModal);
  const [name, setName] = useState('');
  const [busy, setBusy] = useState(false);

  const submit = async () => {
    if (name.trim() === '' || busy) return;
    setBusy(true);
    try {
      await onSubmit(name.trim());
      closeModal();
    } catch {
      toast('error', t.errors.actionFailed);
      setBusy(false);
    }
  };

  return (
    <>
      <input
        aria-label={placeholder}
        placeholder={placeholder}
        value={name}
        onChange={(e) => setName(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === 'Enter') void submit();
        }}
        className="w-full rounded-md border border-transparent bg-input px-3 py-2.5 text-norm placeholder-faint outline-none transition-colors duration-fast focus:border-blurple/50"
      />
      <div className="mt-4 flex justify-end gap-3">
        <button
          type="button"
          onClick={closeModal}
          className="rounded-sm px-4 py-2 text-sm font-medium text-muted transition-colors duration-fast hover:bg-chat-hover hover:text-norm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-modal"
        >
          {t.app.cancel}
        </button>
        <button
          type="button"
          disabled={name.trim() === '' || busy}
          onClick={() => void submit()}
          className="rounded-lg bg-blurple px-4 py-2 text-sm font-medium text-white transition-colors duration-fast hover:bg-blurple-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-modal disabled:opacity-50 active:scale-[0.98]"
        >
          {action}
        </button>
      </div>
    </>
  );
}

function CreateGroupModal() {
  const t = useT();
  const create = useGroups((s) => s.create);
  const setView = useUi((s) => s.setView);
  const loadState = useGroups((s) => s.loadState);
  return (
    <ModalFrame title={t.groups.createTitle} hint={t.groups.createHint}>
      <NameForm
        placeholder={t.groups.namePlaceholder}
        action={t.groups.createAction}
        onSubmit={async (name) => {
          const groupId = await create(name, 'général');
          await loadState(groupId);
          const channelId =
            useGroups.getState().states[groupId]?.channels[0]?.channel_id ?? null;
          setView({ kind: 'group', groupId, channelId });
        }}
      />
    </ModalFrame>
  );
}

/** Carte de choix de genre (texte/vocal), clavier-accessible via un radiogroup. */
function ChannelKindOption({
  kind,
  selected,
  onSelect,
}: {
  kind: QuickChannelKind;
  selected: boolean;
  onSelect: (kind: QuickChannelKind) => void;
}) {
  const t = useT();
  const option = QUICK_CHANNEL_KINDS.find((k) => k.kind === kind);
  if (!option) return null;
  return (
    <button
      type="button"
      aria-pressed={selected}
      onClick={() => onSelect(kind)}
      className={`flex flex-1 items-start gap-2.5 rounded-lg border px-3 py-2.5 text-left transition-colors duration-fast focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-modal ${
        selected
          ? 'border-blurple bg-blurple/10'
          : 'border-transparent bg-rail hover:bg-chat-hover'
      }`}
    >
      <span
        aria-hidden
        className={`mt-0.5 flex h-8 w-8 shrink-0 items-center justify-center rounded-full ${
          selected ? 'bg-blurple text-white' : 'bg-modal text-faint'
        }`}
      >
        <ChannelIcon kind={kind} />
      </span>
      <span className="min-w-0">
        <span className="block text-sm font-medium text-header">{option.label(t)}</span>
        <span className="mt-0.5 block text-xs text-muted">{option.hint(t)}</span>
      </span>
    </button>
  );
}

function CreateChannelModal({ groupId }: { groupId: string }) {
  const t = useT();
  const addChannel = useGroups((s) => s.addChannel);
  const setView = useUi((s) => s.setView);
  const closeModal = useUi((s) => s.closeModal);
  const toast = useUi((s) => s.toast);
  const [kind, setKind] = useState<QuickChannelKind>('text');
  const [name, setName] = useState('');
  const [busy, setBusy] = useState(false);

  const submit = async (): Promise<void> => {
    const trimmed = name.trim();
    if (trimmed === '' || busy) return;
    setBusy(true);
    try {
      const channelId = await addChannel(groupId, trimmed, kind);
      setView({ kind: 'group', groupId, channelId });
      closeModal();
    } catch {
      toast('error', t.errors.actionFailed);
      setBusy(false);
    }
  };

  return (
    <ModalFrame title={t.groups.addChannel}>
      <div role="group" aria-label={t.groups.channelKindLabel} className="flex gap-2">
        {QUICK_CHANNEL_KINDS.map(({ kind: candidate }) => (
          <ChannelKindOption
            key={candidate}
            kind={candidate}
            selected={kind === candidate}
            onSelect={setKind}
          />
        ))}
      </div>
      <input
        aria-label={t.groups.channelNamePlaceholder}
        placeholder={t.groups.channelNamePlaceholder}
        value={name}
        onChange={(e) => setName(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === 'Enter') void submit();
        }}
        className="mt-4 w-full rounded-md border border-transparent bg-input px-3 py-2.5 text-norm placeholder-faint outline-none transition-colors duration-fast focus:border-blurple/50"
      />
      <div className="mt-4 flex justify-end gap-3">
        <button
          type="button"
          onClick={closeModal}
          className="rounded-sm px-4 py-2 text-sm font-medium text-muted transition-colors duration-fast hover:bg-chat-hover hover:text-norm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-modal"
        >
          {t.app.cancel}
        </button>
        <button
          type="button"
          disabled={name.trim() === '' || busy}
          onClick={() => void submit()}
          className="rounded-lg bg-blurple px-4 py-2 text-sm font-medium text-white transition-colors duration-fast hover:bg-blurple-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-modal disabled:opacity-50 active:scale-[0.98]"
        >
          {t.groups.addChannelAction}
        </button>
      </div>
    </ModalFrame>
  );
}

/**
 * Section « lien d'invitation partageable » : deux sélecteurs (usages, durée)
 * et un bouton qui crée un code `accord://invite/…` via `invite_link_create`,
 * affiché en lecture seule avec un bouton « Copier ». Autonome (son propre
 * état local), rendue sous la liste d'amis d'`InviteModal`.
 */
function ShareableLinkSection({ groupId }: { groupId: string }) {
  const t = useT();
  const toast = useUi((s) => s.toast);
  const [maxUses, setMaxUses] = useState(0);
  const [expiresH, setExpiresH] = useState(168);
  const [code, setCode] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const createLink = (): void => {
    setBusy(true);
    api
      .groupsInviteLinkCreate(groupId, maxUses, expiresH)
      .then((res) => {
        setCode(res.code);
        setBusy(false);
      })
      .catch(() => {
        toast('error', t.errors.actionFailed);
        setBusy(false);
      });
  };

  const copyCode = (): void => {
    if (code === null) return;
    copyToClipboard(
      code,
      () => toast('info', t.app.copied),
      () => toast('error', t.errors.actionFailed),
    );
  };

  const selectClass =
    'w-full rounded-md border border-transparent bg-input px-3 py-2 text-sm text-norm outline-none transition-colors duration-fast focus:border-blurple/50';
  const labelClass = 'mb-1 block text-xs font-medium uppercase tracking-wide text-faint';

  return (
    <div className="mt-4 border-t border-input/50 pt-4">
      <h3 className="text-sm font-semibold text-header">{t.inviteLink.title}</h3>
      <p className="mt-1 text-xs text-muted">{t.inviteLink.hint}</p>
      <div className="mt-3 flex gap-2">
        <label className="min-w-0 flex-1">
          <span className={labelClass}>{t.inviteLink.usesLabel}</span>
          <select
            aria-label={t.inviteLink.usesLabel}
            value={maxUses}
            onChange={(e) => setMaxUses(Number(e.target.value))}
            className={selectClass}
          >
            {INVITE_LINK_USES.map((u) => (
              <option key={u.value} value={u.value}>
                {u.label(t)}
              </option>
            ))}
          </select>
        </label>
        <label className="min-w-0 flex-1">
          <span className={labelClass}>{t.inviteLink.durationLabel}</span>
          <select
            aria-label={t.inviteLink.durationLabel}
            value={expiresH}
            onChange={(e) => setExpiresH(Number(e.target.value))}
            className={selectClass}
          >
            {INVITE_LINK_DURATIONS.map((d) => (
              <option key={d.value} value={d.value}>
                {d.label(t)}
              </option>
            ))}
          </select>
        </label>
      </div>
      <button
        type="button"
        disabled={busy}
        onClick={createLink}
        className="mt-3 w-full rounded-lg bg-blurple px-4 py-2 text-sm font-medium text-white transition-colors duration-fast hover:bg-blurple-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-modal disabled:opacity-50 active:scale-[0.98]"
      >
        {busy ? t.inviteLink.creating : t.inviteLink.create}
      </button>
      {code !== null && (
        <div className="mt-3 flex gap-2">
          <input
            readOnly
            aria-label={t.inviteLink.codeLabel}
            value={code}
            onFocus={(e) => e.currentTarget.select()}
            className="min-w-0 flex-1 rounded-md border border-transparent bg-input px-3 py-2 text-sm text-norm outline-none"
          />
          <button
            type="button"
            onClick={copyCode}
            className="shrink-0 rounded-lg border border-blurple px-3 py-2 text-sm font-medium text-blurple transition-colors duration-fast hover:bg-blurple hover:text-white focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-modal"
          >
            {t.app.copy}
          </button>
        </div>
      )}
    </div>
  );
}

function InviteModal({ groupId }: { groupId: string }) {
  const t = useT();
  const toast = useUi((s) => s.toast);
  const closeModal = useUi((s) => s.closeModal);
  const contacts = useFriends((s) => s.contacts);
  const state = useGroups((s) => s.states[groupId]);
  const invite = useGroups((s) => s.invite);

  const members = new Set((state?.members ?? []).map((m) => m.pubkey));
  const candidates = contacts.filter(
    (c) => c.state === 'friend' && !members.has(c.pubkey),
  );

  return (
    <ModalFrame title={t.groups.inviteTitle} hint={t.groups.inviteHint}>
      {candidates.length === 0 && (
        <p className="py-4 text-center text-sm text-muted">
          {t.groups.noFriendsToInvite}
        </p>
      )}
      <div className="max-h-72 space-y-1 overflow-y-auto">
        {candidates.map((c) => (
          <div
            key={c.pubkey}
            className="flex items-center gap-3 rounded-md px-2 py-1.5 transition-colors duration-fast hover:bg-chat-hover"
          >
            <Avatar
              id={c.pubkey}
              name={displayNameOf(contacts, c.pubkey)}
              size={32}
              avatarHash={c.avatar}
              hint={c.pubkey}
            />
            <span className="min-w-0 flex-1 truncate text-norm">
              {displayNameOf(contacts, c.pubkey)}
            </span>
            <button
              type="button"
              onClick={() => {
                invite(groupId, c.pubkey)
                  .then(() => {
                    toast('info', t.groups.invited);
                    closeModal();
                  })
                  .catch(() => toast('error', t.errors.actionFailed));
              }}
              className="rounded-lg border border-green px-3 py-1 text-sm font-medium text-green transition-colors duration-fast hover:bg-green hover:text-white focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-green focus-visible:ring-offset-2 focus-visible:ring-offset-modal"
            >
              {t.groups.invite}
            </button>
          </div>
        ))}
      </div>
      <ShareableLinkSection groupId={groupId} />
    </ModalFrame>
  );
}

/**
 * Formulaire de création d'un sondage (`groups.send` étendu, D-048) :
 * question (compteur d'octets UTF-8 en direct), 2 à 10 options (rangées
 * ajoutables/retirables), Créer désactivé tant que les bornes du contrat ne
 * sont pas respectées. Coquille propre (pas `ModalFrame`, sans défilement) —
 * jusqu'à 10 options peut dépasser un petit viewport, même schéma flottant
 * qu'`EventsModal`.
 */
function CreatePollModal({ groupId, channelId }: { groupId: string; channelId: string }) {
  const t = useT();
  const closeModal = useUi((s) => s.closeModal);
  const sendPoll = useGroups((s) => s.sendPoll);
  const pollCount = useGroups((s) => s.states[groupId]?.polls?.length ?? 0);
  const [question, setQuestion] = useState('');
  const [options, setOptions] = useState<string[]>(['', '']);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const onKey = (e: KeyboardEvent): void => {
      if (e.key === 'Escape') closeModal();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [closeModal]);

  // Indication client (la borne fait foi côté nœud, `APP_ERROR` sinon) : le
  // décompte peut manquer côté état (nœud plus ancien) — on laisse alors
  // passer, le nœud tranchera.
  const atCap = pollCount >= POLL_MAX_PAR_GROUPE;
  const trimmedQuestion = question.trim();
  const trimmedOptions = options.map((o) => o.trim());
  const canSubmit =
    estQuestionSondageValide(trimmedQuestion) &&
    trimmedOptions.length >= POLL_MIN_OPTIONS &&
    trimmedOptions.every(estOptionSondageValide) &&
    !atCap &&
    !busy;

  const updateOption = (index: number, value: string): void =>
    setOptions((prev) => prev.map((o, i) => (i === index ? value : o)));
  const addOption = (): void =>
    setOptions((prev) => (prev.length >= POLL_MAX_OPTIONS ? prev : [...prev, '']));
  const removeOption = (index: number): void =>
    setOptions((prev) =>
      prev.length <= POLL_MIN_OPTIONS ? prev : prev.filter((_, i) => i !== index),
    );

  const submit = async (): Promise<void> => {
    if (!canSubmit) return;
    setBusy(true);
    setError(null);
    try {
      await sendPoll(groupId, channelId, trimmedQuestion, trimmedOptions);
      closeModal();
    } catch (e) {
      setError(messageOf(e, t.errors.actionFailed));
      setBusy(false);
    }
  };

  return (
    <div
      className="modal-overlay-enter fixed inset-0 z-40 flex items-center justify-center bg-black/75 backdrop-blur-sm"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) closeModal();
      }}
    >
      <div
        ref={ref}
        role="dialog"
        aria-modal="true"
        aria-label={t.groups.pollCreateTitle}
        className="glass modal-panel-enter flex max-h-[85vh] w-[440px] max-w-[92vw] flex-col overflow-hidden rounded-xl shadow-3"
      >
        <div className="flex items-center justify-between border-b border-input/50 p-5 pb-4">
          <h2 className="text-lg font-semibold text-header">{t.groups.pollCreateTitle}</h2>
          <button
            type="button"
            aria-label={t.app.close}
            onClick={closeModal}
            className="rounded-sm p-1 text-faint transition-colors duration-fast hover:text-norm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-modal active:scale-95"
          >
            <CloseIcon size={20} />
          </button>
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto p-5 pt-4">
          <label
            htmlFor="poll-question"
            className="mb-1 block text-xs font-medium uppercase tracking-wide text-faint"
          >
            {t.groups.pollQuestionLabel}
          </label>
          <textarea
            id="poll-question"
            value={question}
            rows={2}
            maxLength={POLL_QUESTION_MAX + 20}
            placeholder={t.groups.pollQuestionPlaceholder}
            onChange={(e) => setQuestion(e.target.value)}
            className="w-full resize-none rounded-md border border-transparent bg-input px-3 py-2 text-sm text-norm placeholder-faint outline-none transition-colors duration-fast focus:border-blurple/50"
          />
          <div className="mb-3 mt-1 text-right text-xs text-faint">
            {utf8ByteLength(question)}/{POLL_QUESTION_MAX}
          </div>
          <div className="space-y-2">
            {options.map((option, index) => (
              <div key={index} className="flex items-center gap-2">
                <input
                  aria-label={interpolate(t.groups.pollOptionPlaceholder, {
                    index: String(index + 1),
                  })}
                  placeholder={interpolate(t.groups.pollOptionPlaceholder, {
                    index: String(index + 1),
                  })}
                  value={option}
                  maxLength={POLL_OPTION_MAX + 20}
                  onChange={(e) => updateOption(index, e.target.value)}
                  className="min-w-0 flex-1 rounded-md border border-transparent bg-input px-3 py-2 text-sm text-norm placeholder-faint outline-none transition-colors duration-fast focus:border-blurple/50"
                />
                {options.length > POLL_MIN_OPTIONS && (
                  <button
                    type="button"
                    aria-label={interpolate(t.groups.pollRemoveOption, {
                      index: String(index + 1),
                    })}
                    onClick={() => removeOption(index)}
                    className="shrink-0 rounded-sm p-1.5 text-faint transition-colors duration-fast hover:text-red focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-modal"
                  >
                    <CloseIcon size={16} />
                  </button>
                )}
              </div>
            ))}
          </div>
          {options.length < POLL_MAX_OPTIONS && (
            <button
              type="button"
              onClick={addOption}
              className="mt-2 text-sm font-medium text-blurple transition-colors duration-fast hover:text-blurple-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-modal"
            >
              + {t.groups.pollAddOption}
            </button>
          )}
          {atCap && <p className="mt-3 text-xs text-faint">{t.groups.pollLimit}</p>}
          {error !== null && (
            <p className="mt-3 text-sm text-red" role="alert">
              {error}
            </p>
          )}
          <div className="mt-4 flex justify-end gap-3">
            <button
              type="button"
              onClick={closeModal}
              className="rounded-sm px-4 py-2 text-sm font-medium text-muted transition-colors duration-fast hover:bg-chat-hover hover:text-norm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-modal"
            >
              {t.app.cancel}
            </button>
            <button
              type="button"
              disabled={!canSubmit}
              onClick={() => void submit()}
              className="rounded-lg bg-blurple px-4 py-2 text-sm font-medium text-white transition-colors duration-fast hover:bg-blurple-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-modal disabled:opacity-50 active:scale-[0.98]"
            >
              {t.groups.pollCreateAction}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}

export function Modals() {
  const modal = useUi((s) => s.modal);
  if (modal === null) return null;
  switch (modal.kind) {
    case 'createGroup':
      return <CreateGroupModal />;
    case 'createChannel':
      return <CreateChannelModal groupId={modal.groupId} />;
    case 'invite':
      return <InviteModal groupId={modal.groupId} />;
    case 'joinServer':
      return <JoinServerModal />;
    case 'settings':
      return <SettingsModal />;
    case 'serverSettings':
      return (
        <ServerSettingsModal
          groupId={modal.groupId}
          {...(modal.initialTab !== undefined ? { initialTab: modal.initialTab } : {})}
        />
      );
    case 'events':
      return <EventsModal groupId={modal.groupId} />;
    case 'createPoll':
      return <CreatePollModal groupId={modal.groupId} channelId={modal.channelId} />;
  }
}
