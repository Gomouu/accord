/** Vue Amis : onglets Tous / En attente / Invitations / Bloqués / Ajouter, actions. */

import { useEffect, useState } from 'react';
import { interpolate } from '../i18n';
import type { Contact } from '../lib/api';
import { presenceOf, useFriends } from '../stores/friends';
import { useGroups } from '../stores/groups';
import { useSession } from '../stores/session';
import { useUi, useT } from '../stores/ui';
import { Avatar } from './Avatar';
import { NetworkPanel } from './NetworkPanel';
import { PendingInvites } from './PendingInvites';
import { PresenceDot } from './PresenceDot';

type Tab = 'all' | 'pending' | 'invitations' | 'blocked' | 'add';
/** Onglets adossés à la liste de contacts (`byTab`) — distincts d'« invitations » et « add ». */
type ContactTab = 'all' | 'pending' | 'blocked';

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
    <div className="group flex min-h-14 flex-wrap items-center gap-x-3 gap-y-2 rounded-lg px-3 py-2 transition-colors duration-fast hover:bg-chat-hover">
      <div className="relative shrink-0">
        <Avatar
          id={contact.pubkey}
          name={contact.display_name || contact.friend_code}
          size={32}
          avatarHash={contact.avatar}
          hint={contact.pubkey}
          decoration={contact.avatar_decoration ?? null}
        />
        {isFriend && (
          <PresenceDot
            status={status}
            label={t.profil[status]}
            className="absolute -bottom-0.5 -right-0.5 rounded-full ring-2 ring-chat"
          />
        )}
      </div>
      <div className="min-w-[8rem] flex-1">
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
      <div className="ml-auto flex min-w-0 max-w-full flex-wrap items-center justify-end gap-2">
        {isFriend && confirmRemove && (
          <>
            <span className="whitespace-nowrap text-sm text-muted">
              {t.friends.removeQuestion}
            </span>
            <button
              type="button"
              onClick={() => {
                setConfirmRemove(false);
                act(() => remove(contact.pubkey));
              }}
              className="h-9 shrink-0 whitespace-nowrap rounded-sm bg-red px-3 text-sm font-medium text-on-red transition-colors duration-fast hover:brightness-110 active:scale-95"
            >
              {t.friends.remove}
            </button>
            <button
              type="button"
              onClick={() => setConfirmRemove(false)}
              className="h-9 shrink-0 whitespace-nowrap rounded-sm bg-sidebar px-3 text-sm font-medium text-norm transition-colors duration-fast hover:bg-input active:scale-95"
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
              className="flex h-9 w-9 shrink-0 items-center justify-center rounded-full bg-sidebar text-muted transition-colors duration-fast hover:text-header active:scale-95 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-chat"
            >
              <svg
                width="18"
                height="18"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                strokeWidth={2}
                strokeLinecap="round"
                strokeLinejoin="round"
                aria-hidden
              >
                <path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" />
              </svg>
            </button>
            <button
              type="button"
              title={t.friends.remove}
              aria-label={t.friends.remove}
              onClick={() => setConfirmRemove(true)}
              className="flex h-9 w-9 shrink-0 items-center justify-center rounded-full bg-sidebar text-muted transition-colors duration-fast hover:text-red active:scale-95 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-chat"
            >
              <svg
                width="18"
                height="18"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                strokeWidth={2}
                strokeLinecap="round"
                strokeLinejoin="round"
                aria-hidden
              >
                <path d="M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2" />
                <circle cx="9" cy="7" r="4" />
                <line x1="22" x2="16" y1="11" y2="11" />
              </svg>
            </button>
            <button
              type="button"
              title={t.friends.block}
              aria-label={t.friends.block}
              onClick={() => act(() => block(contact.pubkey))}
              className="flex h-9 w-9 shrink-0 items-center justify-center rounded-full bg-sidebar text-muted transition-colors duration-fast hover:text-red active:scale-95 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-chat"
            >
              <svg
                width="18"
                height="18"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                strokeWidth={2}
                strokeLinecap="round"
                strokeLinejoin="round"
                aria-hidden
              >
                <circle cx="12" cy="12" r="10" />
                <line x1="4.93" x2="19.07" y1="4.93" y2="19.07" />
              </svg>
            </button>
          </>
        )}
        {contact.state === 'pending_in' && (
          <>
            <button
              type="button"
              onClick={() => act(() => respond(contact.pubkey, true))}
              className="h-9 shrink-0 whitespace-nowrap rounded-sm bg-blurple px-3 text-sm font-medium text-white transition-colors duration-fast hover:bg-blurple-hover active:scale-95"
            >
              {t.friends.accept}
            </button>
            <button
              type="button"
              onClick={() => act(() => respond(contact.pubkey, false))}
              className="h-9 shrink-0 whitespace-nowrap rounded-sm bg-sidebar px-3 text-sm font-medium text-norm transition-colors duration-fast hover:bg-input active:scale-95"
            >
              {t.friends.decline}
            </button>
          </>
        )}
        {contact.state === 'blocked' && (
          <button
            type="button"
            onClick={() => act(() => unblock(contact.pubkey))}
            className="h-9 shrink-0 whitespace-nowrap rounded-sm bg-sidebar px-3 text-sm font-medium text-norm transition-colors duration-fast hover:bg-input active:scale-95"
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
    <div className="h-full overflow-y-auto">
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
            className="rounded-sm bg-blurple px-4 py-1.5 text-sm font-medium text-white transition-colors duration-fast hover:bg-blurple-hover disabled:opacity-50 active:scale-95"
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
          <div className="mb-8 mt-8 rounded-lg bg-sidebar p-4">
            <div className="text-xs font-medium uppercase text-faint">
              {t.friends.myCode}
            </div>
            <div className="selectable mt-1 font-mono text-lg text-header">
              {self.friend_code}
            </div>
          </div>
        )}

        {/* Toute la partie réseau (ton adresse, ajout par adresse, état de la
            connexion) vit désormais ici : « se connecter à un ami » au même
            endroit que l'ajout par code. */}
        <div className="border-t border-rail pt-6">
          <NetworkPanel />
        </div>
      </div>
    </div>
  );
}

/** État vide d'un onglet de la liste d'amis : icône muette centrée + libellé. */
function EmptyFriends({ label }: { label: string }) {
  return (
    <div className="flex flex-col items-center gap-3 py-12 text-center text-muted">
      <span
        aria-hidden
        className="flex h-11 w-11 items-center justify-center rounded-full bg-sidebar text-faint"
      >
        <svg
          width="22"
          height="22"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth={2}
          strokeLinecap="round"
          strokeLinejoin="round"
          aria-hidden
        >
          <path d="M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2" />
          <circle cx="9" cy="7" r="4" />
          <path d="M22 21v-2a4 4 0 0 0-3-3.87" />
          <path d="M16 3.13a4 4 0 0 1 0 7.75" />
        </svg>
      </span>
      <p>{label}</p>
    </div>
  );
}

export function FriendsView() {
  const t = useT();
  const contacts = useFriends((s) => s.contacts);
  const load = useFriends((s) => s.load);
  const pendingInvites = useGroups((s) => s.pendingInvites);
  const [tab, setTab] = useState<Tab>('all');

  useEffect(() => {
    void load();
  }, [load]);

  const byTab: Record<ContactTab, Contact[]> = {
    all: contacts.filter((c) => c.state === 'friend'),
    pending: contacts.filter(
      (c) => c.state === 'pending_in' || c.state === 'pending_out',
    ),
    blocked: contacts.filter((c) => c.state === 'blocked'),
  };
  const emptyLabel: Record<ContactTab, string> = {
    all: t.friends.empty,
    pending: t.friends.emptyPending,
    blocked: t.friends.emptyBlocked,
  };

  const tabs: { id: Tab; label: string; badge?: number }[] = [
    { id: 'all', label: t.friends.all },
    { id: 'pending', label: t.friends.pending },
    { id: 'invitations', label: t.invitations.tabLabel, badge: pendingInvites.length },
    { id: 'blocked', label: t.friends.blocked },
  ];

  return (
    <div className="flex h-full flex-col">
      <header className="flex h-12 min-w-0 shrink-0 items-center gap-4 border-b border-rail px-4 shadow-1">
        <div className="flex shrink-0 items-center gap-2 font-semibold text-header">
          <svg
            width="20"
            height="20"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth={2}
            strokeLinecap="round"
            strokeLinejoin="round"
            aria-hidden
            className="text-faint"
          >
            <path d="M19 21v-2a4 4 0 0 0-4-4H9a4 4 0 0 0-4 4v2" />
            <circle cx="12" cy="7" r="4" />
          </svg>
          {t.friends.title}
        </div>
        <div className="h-6 w-px shrink-0 bg-input" role="separator" />
        <nav
          className="flex h-11 min-w-0 flex-1 items-center gap-1.5 overflow-x-auto overscroll-x-contain [scrollbar-width:none] [&::-webkit-scrollbar]:hidden"
          aria-label={t.friends.title}
        >
          {tabs.map(({ id, label, badge }) => (
            <button
              key={id}
              type="button"
              aria-current={tab === id ? 'page' : undefined}
              onClick={() => setTab(id)}
              aria-label={
                badge !== undefined && badge > 0
                  ? `${label} — ${interpolate(t.invitations.badge, { count: String(badge) })}`
                  : undefined
              }
              className={`flex h-9 shrink-0 items-center rounded-full px-3 text-sm font-medium transition-colors duration-fast focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-chat ${
                tab === id
                  ? 'bg-blurple/15 text-header ring-1 ring-inset ring-blurple/20'
                  : 'text-muted hover:bg-chat-hover hover:text-norm'
              }`}
            >
              {label}
              {badge !== undefined && badge > 0 && (
                <span
                  aria-hidden
                  className="ml-1.5 flex min-w-[18px] items-center justify-center rounded-full bg-red px-1 text-[11px] font-semibold leading-[18px] text-on-red"
                >
                  {badge > 99 ? '99+' : badge}
                </span>
              )}
            </button>
          ))}
          <button
            type="button"
            aria-current={tab === 'add' ? 'page' : undefined}
            onClick={() => setTab('add')}
            className={`flex h-9 shrink-0 items-center rounded-full px-3 text-sm font-medium transition-colors duration-fast focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-green focus-visible:ring-offset-2 focus-visible:ring-offset-chat ${
              tab === 'add'
                ? 'bg-green/20 text-green'
                : 'bg-green text-on-green hover:brightness-110'
            }`}
          >
            {t.friends.add}
          </button>
        </nav>
      </header>
      {tab === 'add' ? (
        <AddFriend />
      ) : tab === 'invitations' ? (
        <div className="flex-1 overflow-y-auto p-4">
          <div className="mb-2 px-1 text-[11px] font-medium uppercase tracking-wide text-muted">
            {t.invitations.tabLabel} — {pendingInvites.length}
          </div>
          <PendingInvites />
        </div>
      ) : (
        <div className="flex-1 overflow-y-auto p-4">
          <div className="mb-2 px-1 text-[11px] font-medium uppercase tracking-wide text-muted">
            {tabs.find((x) => x.id === tab)?.label} — {byTab[tab].length}
          </div>
          {byTab[tab].length === 0 && <EmptyFriends label={emptyLabel[tab]} />}
          {byTab[tab].map((c) => (
            <FriendRow key={c.pubkey} contact={c} />
          ))}
        </div>
      )}
    </div>
  );
}
