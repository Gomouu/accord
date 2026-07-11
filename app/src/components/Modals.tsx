/**
 * Modales : création de groupe/salon, invitation. Les paramètres ouvrent
 * l'écran plein format dédié (components/settings), même déclencheur
 * `ui.modal = { kind: 'settings' }`.
 */

import { useEffect, useRef, useState } from 'react';
import type { Dict } from '../i18n';
import type { GroupChannelKind } from '../lib/api';
import { useFriends, displayNameOf } from '../stores/friends';
import { useGroups } from '../stores/groups';
import { useUi, useT } from '../stores/ui';
import { Avatar } from './Avatar';
import { ChannelIcon } from './Sidebar';
import { CloseIcon } from './ContextMenu';
import { ServerSettingsModal } from './server/ServerSettingsModal';
import { SettingsModal } from './settings/SettingsModal';

/** Genres proposés dans le choix rapide (l'annonce reste réservée aux paramètres serveur). */
type QuickChannelKind = Extract<GroupChannelKind, 'text' | 'voice'>;

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
];

function ModalFrame({
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
    </ModalFrame>
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
    case 'settings':
      return <SettingsModal />;
    case 'serverSettings':
      return (
        <ServerSettingsModal
          groupId={modal.groupId}
          {...(modal.initialTab !== undefined ? { initialTab: modal.initialTab } : {})}
        />
      );
  }
}
