/** Vue Amis : onglets Tous / En attente / Bloqués / Ajouter, actions. */

import { useEffect, useState } from 'react';
import type { Contact } from '../lib/api';
import { presenceOf, useFriends } from '../stores/friends';
import { useSession } from '../stores/session';
import { useUi, useT } from '../stores/ui';
import { Avatar } from './Avatar';
import { PresenceDot } from './PresenceDot';

type Tab = 'all' | 'pending' | 'blocked' | 'add';

function FriendRow({ contact }: { contact: Contact }) {
  const t = useT();
  const setView = useUi((s) => s.setView);
  const toast = useUi((s) => s.toast);
  const respond = useFriends((s) => s.respond);
  const remove = useFriends((s) => s.remove);
  const block = useFriends((s) => s.block);
  const unblock = useFriends((s) => s.unblock);
  /** Confirmation en ligne du retrait d'ami (remplace les actions). */
  const [confirmRemove, setConfirmRemove] = useState(false);

  const act = (fn: () => Promise<void>) => {
    void fn().catch(() => toast('error', t.errors.actionFailed));
  };

  const isFriend = contact.state === 'friend';
  const status = presenceOf(contact);
  const statusText = contact.status_text ?? null;

  return (
    <div className="group flex items-center gap-3 rounded-lg px-3 py-2.5 hover:bg-chat-hover">
      <div className="relative shrink-0">
        <Avatar
          id={contact.pubkey}
          name={contact.display_name || contact.friend_code}
          avatarHash={contact.avatar}
          hint={contact.pubkey}
        />
        {isFriend && (
          <PresenceDot
            status={status}
            label={t.profil[status]}
            className="absolute -bottom-0.5 -right-0.5 rounded-full ring-2 ring-chat"
          />
        )}
      </div>
      <div className="min-w-0 flex-1">
        <div className="truncate font-medium text-header">
          {contact.display_name || contact.friend_code}
        </div>
        <div className="truncate text-xs text-faint">
          {contact.state === 'pending_in'
            ? t.friends.incoming
            : contact.state === 'pending_out'
              ? t.friends.outgoing
              : statusText !== null && statusText !== ''
                ? statusText
                : contact.bio !== null && contact.bio !== ''
                  ? contact.bio
                  : contact.friend_code}
        </div>
      </div>
      <div className="flex items-center gap-2">
        {isFriend && confirmRemove && (
          <>
            <span className="text-sm text-muted">{t.friends.removeQuestion}</span>
            <button
              type="button"
              onClick={() => {
                setConfirmRemove(false);
                act(() => remove(contact.pubkey));
              }}
              className="rounded bg-red px-3 py-1.5 text-sm font-medium text-white hover:brightness-110"
            >
              {t.friends.remove}
            </button>
            <button
              type="button"
              onClick={() => setConfirmRemove(false)}
              className="rounded bg-sidebar px-3 py-1.5 text-sm font-medium text-norm hover:bg-input"
            >
              {t.app.cancel}
            </button>
          </>
        )}
        {isFriend && !confirmRemove && (
          <>
            <button
              type="button"
              title={t.friends.sendDm}
              aria-label={t.friends.sendDm}
              onClick={() => setView({ kind: 'dm', peer: contact.pubkey })}
              className="rounded-full bg-sidebar p-2.5 text-muted hover:text-header"
            >
              <svg
                width="18"
                height="18"
                viewBox="0 0 24 24"
                fill="currentColor"
                aria-hidden
              >
                <path d="M4 5.5A2.5 2.5 0 0 1 6.5 3h11A2.5 2.5 0 0 1 20 5.5v9a2.5 2.5 0 0 1-2.5 2.5H9.4l-4 3a.9.9 0 0 1-1.4-.7V5.5Z" />
              </svg>
            </button>
            <button
              type="button"
              title={t.friends.remove}
              aria-label={t.friends.remove}
              onClick={() => setConfirmRemove(true)}
              className="rounded-full bg-sidebar p-2.5 text-muted hover:text-red"
            >
              <svg
                width="18"
                height="18"
                viewBox="0 0 24 24"
                fill="currentColor"
                aria-hidden
              >
                <path d="M9 11a4 4 0 1 0 0-8 4 4 0 0 0 0 8Zm0 2c-3.3 0-7 1.7-7 4v2a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-2c0-2.3-3.7-4-7-4Zm13-5a1 1 0 0 1-1 1h-6a1 1 0 1 1 0-2h6a1 1 0 0 1 1 1Z" />
              </svg>
            </button>
            <button
              type="button"
              title={t.friends.block}
              aria-label={t.friends.block}
              onClick={() => act(() => block(contact.pubkey))}
              className="rounded-full bg-sidebar p-2.5 text-muted hover:text-red"
            >
              <svg
                width="18"
                height="18"
                viewBox="0 0 24 24"
                fill="currentColor"
                aria-hidden
              >
                <path d="M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20ZM4.5 12a7.5 7.5 0 0 1 12-6L6 16.5A7.4 7.4 0 0 1 4.5 12Zm7.5 7.5A7.4 7.4 0 0 1 7.5 18L18 7.5A7.5 7.5 0 0 1 12 19.5Z" />
              </svg>
            </button>
          </>
        )}
        {contact.state === 'pending_in' && (
          <>
            <button
              type="button"
              onClick={() => act(() => respond(contact.pubkey, true))}
              className="rounded bg-green px-3 py-1.5 text-sm font-medium text-white hover:brightness-110"
            >
              {t.friends.accept}
            </button>
            <button
              type="button"
              onClick={() => act(() => respond(contact.pubkey, false))}
              className="rounded bg-sidebar px-3 py-1.5 text-sm font-medium text-norm hover:bg-input"
            >
              {t.friends.decline}
            </button>
          </>
        )}
        {contact.state === 'blocked' && (
          <button
            type="button"
            onClick={() => act(() => unblock(contact.pubkey))}
            className="rounded bg-sidebar px-3 py-1.5 text-sm font-medium text-norm hover:bg-input"
          >
            {t.friends.unblock}
          </button>
        )}
      </div>
    </div>
  );
}

function AddFriend() {
  const t = useT();
  const self = useSession((s) => s.self);
  const addByCode = useFriends((s) => s.addByCode);
  const [code, setCode] = useState('');
  const [status, setStatus] = useState<'idle' | 'busy' | 'sent' | 'error'>('idle');

  const submit = async () => {
    if (code.trim() === '' || status === 'busy') return;
    setStatus('busy');
    try {
      await addByCode(code, self?.friend_code ?? '');
      setStatus('sent');
      setCode('');
    } catch {
      setStatus('error');
    }
  };

  return (
    <div className="max-w-2xl p-6">
      <h2 className="mb-1 font-semibold uppercase text-header">{t.friends.addTitle}</h2>
      <p className="mb-4 text-sm text-muted">{t.friends.addHint}</p>
      <div className="flex gap-3 rounded-lg bg-rail p-3">
        <input
          aria-label={t.friends.addPlaceholder}
          placeholder={t.friends.addPlaceholder}
          value={code}
          onChange={(e) => {
            setCode(e.target.value);
            setStatus('idle');
          }}
          onKeyDown={(e) => {
            if (e.key === 'Enter') void submit();
          }}
          className="flex-1 bg-transparent text-norm placeholder-faint outline-none"
        />
        <button
          type="button"
          disabled={code.trim() === '' || status === 'busy'}
          onClick={() => void submit()}
          className="rounded bg-blurple px-4 py-1.5 text-sm font-medium text-white hover:bg-blurple-hover disabled:opacity-50"
        >
          {t.friends.addSend}
        </button>
      </div>
      {status === 'sent' && (
        <p className="mt-2 text-sm text-green">{t.friends.addSent}</p>
      )}
      {status === 'error' && (
        <p className="mt-2 text-sm text-red">{t.friends.addNotFound}</p>
      )}
      {self && (
        <div className="mt-8 rounded-lg bg-sidebar p-4">
          <div className="text-xs font-semibold uppercase text-faint">
            {t.friends.myCode}
          </div>
          <div className="selectable mt-1 font-mono text-lg text-header">
            {self.friend_code}
          </div>
        </div>
      )}
    </div>
  );
}

export function FriendsView() {
  const t = useT();
  const contacts = useFriends((s) => s.contacts);
  const load = useFriends((s) => s.load);
  const [tab, setTab] = useState<Tab>('all');

  useEffect(() => {
    void load();
  }, [load]);

  const byTab: Record<Exclude<Tab, 'add'>, Contact[]> = {
    all: contacts.filter((c) => c.state === 'friend'),
    pending: contacts.filter(
      (c) => c.state === 'pending_in' || c.state === 'pending_out',
    ),
    blocked: contacts.filter((c) => c.state === 'blocked'),
  };
  const emptyLabel: Record<Exclude<Tab, 'add'>, string> = {
    all: t.friends.empty,
    pending: t.friends.emptyPending,
    blocked: t.friends.emptyBlocked,
  };

  const tabs: { id: Tab; label: string }[] = [
    { id: 'all', label: t.friends.all },
    { id: 'pending', label: t.friends.pending },
    { id: 'blocked', label: t.friends.blocked },
  ];

  return (
    <div className="flex h-full flex-col">
      <header className="flex h-12 items-center gap-4 border-b border-rail px-4 shadow-sm">
        <div className="flex items-center gap-2 font-semibold text-header">
          <svg
            width="20"
            height="20"
            viewBox="0 0 24 24"
            fill="currentColor"
            aria-hidden
            className="text-faint"
          >
            <path d="M9 11a4 4 0 1 0 0-8 4 4 0 0 0 0 8Zm0 2c-3.3 0-7 1.7-7 4v2a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-2c0-2.3-3.7-4-7-4Z" />
          </svg>
          {t.friends.title}
        </div>
        <div className="h-6 w-px bg-input" role="separator" />
        <nav className="flex gap-2" aria-label={t.friends.title}>
          {tabs.map(({ id, label }) => (
            <button
              key={id}
              type="button"
              onClick={() => setTab(id)}
              className={`rounded px-2.5 py-0.5 text-sm font-medium ${
                tab === id
                  ? 'bg-chat-hover text-header'
                  : 'text-muted hover:bg-chat-hover hover:text-norm'
              }`}
            >
              {label}
            </button>
          ))}
          <button
            type="button"
            onClick={() => setTab('add')}
            className={`rounded px-2.5 py-0.5 text-sm font-medium ${
              tab === 'add' ? 'bg-green/20 text-green' : 'bg-green text-white'
            }`}
          >
            {t.friends.add}
          </button>
        </nav>
      </header>
      {tab === 'add' ? (
        <AddFriend />
      ) : (
        <div className="flex-1 overflow-y-auto p-4">
          <div className="mb-2 text-xs font-semibold uppercase text-faint">
            {tabs.find((x) => x.id === tab)?.label} — {byTab[tab].length}
          </div>
          {byTab[tab].length === 0 && (
            <p className="py-8 text-center text-muted">{emptyLabel[tab]}</p>
          )}
          {byTab[tab].map((c) => (
            <FriendRow key={c.pubkey} contact={c} />
          ))}
        </div>
      )}
    </div>
  );
}
