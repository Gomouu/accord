import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import type { OwnPresenceStatus, PresenceStatus } from '../lib/api';
import { copyToClipboard } from '../lib/clipboard';
import { profileCardGradient, profileColorCss } from '../lib/color';
import { effectById, frameById } from '../lib/decorations';
import type { AccountMeta } from '../lib/bridge';
import { bouclerTab, focusables } from '../lib/focus';
import { useFriends } from '../stores/friends';
import { selfDisplayName, useSession } from '../stores/session';
import { useUi, useT, type AncrePopover } from '../stores/ui';
import { SecretText } from './SecretText';
import { Avatar } from './Avatar';
import { CheckMenuIcon, CloseIcon, CopyMenuIcon, LeaveMenuIcon } from './ContextMenu';
import { PresenceDot } from './PresenceDot';
import { ProfileBanner } from './ProfileBanner';

export function ownDotStatus(status: OwnPresenceStatus): PresenceStatus {
  return status === 'invisible' ? 'offline' : status;
}

function SwitchAccountIcon() {
  return (
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
      <path d="m16 11 2 2 4-4" />
    </svg>
  );
}

function PencilIcon() {
  return (
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
      <path d="M12 20h9" />
      <path d="M16.5 3.5a2.1 2.1 0 0 1 3 3L8 18l-4 1 1-4Z" />
    </svg>
  );
}

function ChevronRightIcon() {
  return (
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
      <path d="m9 18 6-6-6-6" />
    </svg>
  );
}

function BackIcon() {
  return (
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
      <path d="m15 18-6-6 6-6" />
    </svg>
  );
}

const ACTION_CLASS =
  'flex min-h-12 w-full items-center gap-3 rounded-md px-3 text-left text-sm font-medium text-norm transition-[background-color,color,transform] duration-fast hover:bg-chat-hover hover:text-header focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-blurple active:scale-[0.985]';

const ICON_FRAME_CLASS =
  'flex h-8 w-8 shrink-0 items-center justify-center rounded-md bg-input/70 text-muted';

function accountIsCurrent(account: AccountMeta, pubkey: string, count: number): boolean {
  if (account.pubkey_short !== null) return pubkey.startsWith(account.pubkey_short);
  return count === 1;
}

export function UserMenu({
  onClose,
  anchor,
}: {
  onClose: () => void;
  anchor?: AncrePopover;
}) {
  const t = useT();
  const toast = useUi((s) => s.toast);
  const openModal = useUi((s) => s.openModal);
  const closeModal = useUi((s) => s.closeModal);
  const self = useSession((s) => s.self);
  const accounts = useSession((s) => s.accounts);
  const accountError = useSession((s) => s.error);
  const loadAccounts = useSession((s) => s.loadAccounts);
  const activateAccount = useSession((s) => s.activateAccount);
  const lock = useSession((s) => s.lock);
  const switchAccount = useSession((s) => s.switchAccount);
  const ownStatus = useFriends((s) => s.ownStatus);
  const ownStatusText = useFriends((s) => s.ownStatusText);
  const setOwnStatus = useFriends((s) => s.setOwnStatus);
  const [view, setView] = useState<'profile' | 'status' | 'accounts'>('profile');
  const [draft, setDraft] = useState(ownStatusText ?? '');
  const [confirmingLogout, setConfirmingLogout] = useState(false);
  const [selectedAccount, setSelectedAccount] = useState<string | null>(null);
  const [passphrase, setPassphrase] = useState('');
  const [switching, setSwitching] = useState(false);
  const [position, setPosition] = useState<{ left: number; top: number } | null>(null);
  const ref = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLElement | null>(null);

  useLayoutEffect(() => {
    const candidate =
      document.activeElement instanceof HTMLElement &&
      document.activeElement !== document.body &&
      document.activeElement !== document.documentElement &&
      document.activeElement.isConnected
        ? document.activeElement
        : null;
    const candidateRect = candidate?.getBoundingClientRect();
    const trigger =
      candidate !== null &&
      (anchor === undefined ||
        (candidateRect !== undefined &&
          Math.abs(candidateRect.top - anchor.top) <= 2 &&
          Math.abs(candidateRect.left - anchor.left) <= 2 &&
          Math.abs(candidateRect.bottom - anchor.bottom) <= 2 &&
          Math.abs(candidateRect.right - anchor.right) <= 2))
        ? candidate
        : null;
    triggerRef.current = trigger;
    return () => {
      if (trigger !== null && trigger.isConnected) trigger.focus();
    };
  }, [anchor]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent): void => {
      if (e.key !== 'Escape') return;
      if (view !== 'profile') setView('profile');
      else onClose();
    };
    const onDown = (e: MouseEvent): void => {
      if (
        e.target instanceof Element &&
        e.target.closest('[data-user-menu-trigger]') !== null
      ) {
        return;
      }
      const liveAnchor = triggerRef.current?.getBoundingClientRect() ?? anchor ?? null;
      if (
        liveAnchor !== null &&
        e.clientX >= liveAnchor.left &&
        e.clientX <= liveAnchor.right &&
        e.clientY >= liveAnchor.top &&
        e.clientY <= liveAnchor.bottom
      ) {
        return;
      }
      if (ref.current !== null && !ref.current.contains(e.target as Node)) onClose();
    };
    window.addEventListener('keydown', onKey);
    document.addEventListener('mousedown', onDown);
    return () => {
      window.removeEventListener('keydown', onKey);
      document.removeEventListener('mousedown', onDown);
    };
  }, [anchor, onClose, view]);

  useEffect(() => {
    ref.current?.focus();
  }, [view]);

  useLayoutEffect(() => {
    if (anchor === undefined || ref.current === null) {
      setPosition(null);
      return undefined;
    }
    const card = ref.current;
    const update = (): void => {
      const liveAnchor = triggerRef.current?.getBoundingClientRect() ?? anchor;
      const width = card.offsetWidth;
      const height = card.offsetHeight;
      const gap = 10;
      const maxLeft = Math.max(8, window.innerWidth - width - 8);
      const nearBottom = liveAnchor.bottom >= window.innerHeight * 0.72;
      const preferredLeft = nearBottom
        ? liveAnchor.left
        : window.innerWidth - liveAnchor.right >= width + gap
          ? liveAnchor.right + gap
          : liveAnchor.left - width - gap;
      const left = Math.max(8, Math.min(preferredLeft, maxLeft));
      const preferredTop = nearBottom
        ? liveAnchor.top - height - gap
        : liveAnchor.top - 24;
      const top = Math.max(8, Math.min(preferredTop, window.innerHeight - height - 8));
      setPosition({ left, top });
    };
    update();
    const observer =
      typeof ResizeObserver === 'undefined' ? null : new ResizeObserver(update);
    observer?.observe(card);
    window.addEventListener('resize', update);
    window.addEventListener('scroll', update, true);
    return () => {
      observer?.disconnect();
      window.removeEventListener('resize', update);
      window.removeEventListener('scroll', update, true);
    };
  }, [anchor]);

  if (self === null) return null;

  const options: { status: OwnPresenceStatus; label: string }[] = [
    { status: 'online', label: t.profil.online },
    { status: 'idle', label: t.profil.idle },
    { status: 'dnd', label: t.profil.dnd },
    { status: 'invisible', label: t.profil.invisible },
  ];
  const displayName = selfDisplayName(self);
  const accentHex = profileColorCss(self.accent_color);
  const cardGradient = profileCardGradient(self.banner_color ?? self.accent_color);
  const effect = effectById(self.profile_effect);
  const frame = frameById(self.profile_frame);
  const currentStatusLabel =
    options.find(({ status }) => status === ownStatus)?.label ?? t.profil.online;

  const applyStatus = (status: OwnPresenceStatus, custom?: string): void => {
    setOwnStatus(status, custom).catch(() => toast('error', t.errors.actionFailed));
  };

  const copyFriendCode = (): void => {
    copyToClipboard(
      self.friend_code,
      () => toast('success', t.app.copied),
      () => toast('error', t.errors.actionFailed),
    );
  };

  const editProfile = (): void => {
    onClose();
    openModal({ kind: 'settings' });
  };

  const openAccounts = (): void => {
    setSelectedAccount(null);
    setPassphrase('');
    setView('accounts');
    void loadAccounts();
  };

  const activateSelectedAccount = async (): Promise<void> => {
    if (selectedAccount === null || passphrase === '' || switching) return;
    setSwitching(true);
    try {
      await activateAccount(selectedAccount, passphrase);
      onClose();
    } catch {
      setSwitching(false);
    }
  };

  const addAccount = (): void => {
    onClose();
    closeModal();
    void switchAccount();
  };

  const confirmLogout = (): void => {
    onClose();
    closeModal();
    void lock();
  };

  return (
    <div
      ref={ref}
      role="dialog"
      aria-label={t.profil.userMenu}
      tabIndex={-1}
      onKeyDown={(e) => {
        if (e.key === 'Tab' && document.activeElement === ref.current) {
          const targets = focusables(ref.current);
          const target = e.shiftKey ? targets.at(-1) : targets[0];
          if (target !== undefined) {
            e.preventDefault();
            target.focus();
          }
        } else bouclerTab(e, ref.current);
      }}
      style={{
        width: 360,
        maxWidth: 'calc(100vw - 16px)',
        ...(anchor === undefined
          ? undefined
          : {
              position: 'fixed',
              left: position?.left ?? anchor.left,
              top: position?.top ?? anchor.bottom,
              visibility: position === null ? 'hidden' : 'visible',
            }),
      }}
      className={`context-menu-enter z-50 focus:outline-none ${
        anchor === undefined
          ? 'absolute bottom-[calc(100%+10px)] left-2 origin-bottom-left'
          : 'origin-top-left'
      }`}
    >
      {/* Même enveloppe que la carte de profil (`ProfilePopover`) : le cadre
          déborde de la surface, qui doit donc rester `overflow: visible` et
          porter l'arrondi/rognage sur la couche intérieure. Sans elle, le
          profil ouvert depuis le panneau utilisateur perdait son cadre. */}
      <div className="profile-card-shell">
        {frame?.render()}
        <div
          className="profile-card-shell__surface glass-strong overflow-y-auto overscroll-contain rounded-xl"
          style={{
            maxHeight:
              anchor === undefined
                ? 'min(720px, calc(100vh - 80px))'
                : 'calc(100vh - 16px)',
          }}
        >
          <div className="profile-card-canvas min-h-full">
            {effect?.render()}
            {cardGradient !== null && (
              <span
                aria-hidden
                className="profile-card-tint"
                style={{ backgroundImage: cardGradient }}
              />
            )}

            <div className="profile-card-content">
              {view === 'profile' ? (
                <>
                  <ProfileBanner
                    hash={self.banner}
                    hint={self.pubkey}
                    color={self.banner_color}
                    heightClassName="h-28"
                  />

                  <div className="-mt-11 px-4 pb-4">
                    <div className="flex items-end justify-between gap-3">
                      <div className="relative z-10 rounded-full bg-modal p-1 shadow-2">
                        <Avatar
                          id={self.pubkey}
                          name={displayName}
                          size={80}
                          avatarHash={self.avatar}
                          hint={self.pubkey}
                          decoration={self.avatar_decoration}
                        />
                        <PresenceDot
                          status={ownDotStatus(ownStatus)}
                          label={currentStatusLabel}
                          className="absolute bottom-1 right-1 rounded-full ring-[3px] ring-modal"
                        />
                      </div>
                      <span className="mb-1 flex min-w-0 items-center gap-1.5 rounded-full border border-[color:var(--glass-border)] bg-modal/75 px-2.5 py-1 text-xs font-medium text-muted shadow-1">
                        <PresenceDot status={ownDotStatus(ownStatus)} />
                        <span className="truncate">{currentStatusLabel}</span>
                      </span>
                    </div>

                    <div className="mt-2">
                      <h2
                        className="truncate text-xl font-semibold tracking-[-0.02em] text-header"
                        style={accentHex !== null ? { color: accentHex } : undefined}
                      >
                        {displayName}
                      </h2>
                      {self.pronouns !== null && self.pronouns !== '' && (
                        <p className="mt-0.5 text-xs text-muted">{self.pronouns}</p>
                      )}

                      <div className="mt-1.5 flex items-center gap-1.5">
                        <span className="min-w-0 truncate font-mono text-xs text-faint">
                          <SecretText value={self.friend_code} />
                        </span>
                        <button
                          type="button"
                          aria-label={t.profil.copyFriendCode}
                          title={t.profil.copyFriendCode}
                          onClick={copyFriendCode}
                          className="flex h-7 w-7 shrink-0 items-center justify-center rounded-md text-faint transition-[background-color,color,transform] duration-fast hover:bg-chat-hover hover:text-norm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple active:scale-95"
                        >
                          <CopyMenuIcon />
                        </button>
                      </div>

                      {ownStatusText !== null && ownStatusText !== '' && (
                        <div className="mt-3 flex items-start gap-2 rounded-lg border border-[color:var(--glass-border)] bg-modal/55 px-3 py-2.5 text-sm text-norm shadow-1">
                          <span className="mt-0.5 text-blurple">✦</span>
                          <p className="min-w-0 break-words text-pretty">
                            {ownStatusText}
                          </p>
                        </div>
                      )}

                      {self.bio !== null && self.bio !== '' && (
                        <p className="mt-3 whitespace-pre-wrap break-words text-pretty text-sm leading-relaxed text-norm">
                          {self.bio}
                        </p>
                      )}
                    </div>
                  </div>

                  <div className="space-y-2 px-3 pb-3">
                    <div className="rounded-lg border border-[color:var(--glass-border)] bg-sidebar/80 p-1 shadow-1">
                      <button
                        type="button"
                        onClick={editProfile}
                        className={ACTION_CLASS}
                      >
                        <span className={ICON_FRAME_CLASS}>
                          <PencilIcon />
                        </span>
                        <span className="min-w-0 flex-1 truncate">
                          {t.profil.editProfile}
                        </span>
                      </button>
                      <div className="mx-3 h-px bg-input/70" role="separator" />
                      <button
                        type="button"
                        aria-label={`${t.profil.setStatus} — ${currentStatusLabel}`}
                        onClick={() => setView('status')}
                        className={ACTION_CLASS}
                      >
                        <span className={ICON_FRAME_CLASS}>
                          <PresenceDot status={ownDotStatus(ownStatus)} />
                        </span>
                        <span className="min-w-0 flex-1 truncate">
                          {currentStatusLabel}
                        </span>
                        <span className="text-faint">
                          <ChevronRightIcon />
                        </span>
                      </button>
                    </div>

                    <div className="rounded-lg border border-[color:var(--glass-border)] bg-sidebar/80 p-1 shadow-1">
                      <button
                        type="button"
                        onClick={openAccounts}
                        className={ACTION_CLASS}
                      >
                        <span className={ICON_FRAME_CLASS}>
                          <SwitchAccountIcon />
                        </span>
                        <span className="min-w-0 flex-1 truncate">
                          {t.profil.switchAccount}
                        </span>
                        <span className="text-faint">
                          <ChevronRightIcon />
                        </span>
                      </button>
                    </div>

                    {!confirmingLogout ? (
                      <button
                        type="button"
                        onClick={() => setConfirmingLogout(true)}
                        className={`${ACTION_CLASS} border border-red/20 bg-red/10 text-red hover:bg-red/15 hover:text-red`}
                      >
                        <span className={`${ICON_FRAME_CLASS} bg-red/10 text-red`}>
                          <LeaveMenuIcon />
                        </span>
                        <span className="min-w-0 flex-1 truncate">
                          {t.settings.logout}
                        </span>
                      </button>
                    ) : (
                      <div className="rounded-lg border border-red/20 bg-sidebar/85 p-3 shadow-1">
                        <p className="text-pretty text-sm text-norm">
                          {t.settings.logoutConfirmText}
                        </p>
                        <div className="mt-3 flex gap-2">
                          <button
                            type="button"
                            onClick={confirmLogout}
                            className="min-h-10 flex-1 rounded-md bg-red px-3 text-sm font-medium text-on-red transition-[filter,transform] duration-fast hover:brightness-110 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-red active:scale-[0.98]"
                          >
                            {t.settings.logoutConfirm}
                          </button>
                          <button
                            type="button"
                            onClick={() => setConfirmingLogout(false)}
                            className="min-h-10 rounded-md bg-input px-3 text-sm font-medium text-norm transition-[background-color,transform] duration-fast hover:bg-chat-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple active:scale-[0.98]"
                          >
                            {t.app.cancel}
                          </button>
                        </div>
                      </div>
                    )}
                  </div>
                </>
              ) : view === 'status' ? (
                <div className="p-3">
                  <div className="flex items-center gap-2 px-1 pb-3">
                    <button
                      type="button"
                      aria-label={t.profil.backToProfile}
                      onClick={() => setView('profile')}
                      className="flex h-10 w-10 shrink-0 items-center justify-center rounded-md text-muted transition-[background-color,color,transform] duration-fast hover:bg-chat-hover hover:text-header focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple active:scale-95"
                    >
                      <BackIcon />
                    </button>
                    <div>
                      <h2 className="text-lg font-semibold text-header">
                        {t.profil.setStatus}
                      </h2>
                      <p className="text-xs text-muted">{currentStatusLabel}</p>
                    </div>
                  </div>

                  <div
                    role="radiogroup"
                    aria-label={t.profil.setStatus}
                    className="rounded-lg border border-[color:var(--glass-border)] bg-sidebar/80 p-1 shadow-1"
                  >
                    {options.map(({ status, label }, index) => {
                      const checked = ownStatus === status;
                      return (
                        <div key={status}>
                          {index > 0 && <div className="mx-3 h-px bg-input/70" />}
                          <button
                            type="button"
                            role="radio"
                            aria-checked={checked}
                            onClick={() => {
                              applyStatus(status);
                              onClose();
                            }}
                            className={`${ACTION_CLASS} ${checked ? 'bg-chat-hover text-header' : ''}`}
                          >
                            <span className={ICON_FRAME_CLASS}>
                              <PresenceDot status={ownDotStatus(status)} />
                            </span>
                            <span className="min-w-0 flex-1 truncate">{label}</span>
                            {checked && (
                              <span className="flex h-6 w-6 items-center justify-center text-header">
                                <CheckMenuIcon />
                              </span>
                            )}
                          </button>
                        </div>
                      );
                    })}
                  </div>

                  <div className="mt-3 rounded-lg border border-[color:var(--glass-border)] bg-sidebar/80 p-3 shadow-1">
                    <label
                      htmlFor="user-custom-status"
                      className="text-xs font-medium text-muted"
                    >
                      {t.profil.customStatusPlaceholder}
                    </label>
                    <div className="mt-2">
                      <input
                        id="user-custom-status"
                        value={draft}
                        maxLength={128}
                        onChange={(e) => setDraft(e.target.value)}
                        onKeyDown={(e) => {
                          if (e.key === 'Enter') {
                            applyStatus(ownStatus, draft);
                            onClose();
                          }
                        }}
                        className="min-h-10 w-full rounded-md border border-transparent bg-input px-3 text-sm text-norm placeholder-faint outline-none transition-[border-color,box-shadow] duration-fast focus:border-blurple/50 focus:ring-1 focus:ring-blurple/25"
                      />
                      <div className="mt-2 flex items-center gap-2">
                        <p className="min-w-0 flex-1 truncate text-xs text-faint">
                          {t.profil.customStatusHint}
                        </p>
                        {(ownStatusText ?? '') !== '' && (
                          <button
                            type="button"
                            aria-label={t.profil.clearCustomStatus}
                            title={t.profil.clearCustomStatus}
                            onClick={() => {
                              applyStatus(ownStatus, '');
                              onClose();
                            }}
                            className="flex h-10 w-10 shrink-0 items-center justify-center rounded-md text-muted transition-[background-color,color,transform] duration-fast hover:bg-chat-hover hover:text-norm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple active:scale-95"
                          >
                            <CloseIcon size={15} />
                          </button>
                        )}
                        <button
                          type="button"
                          onClick={() => {
                            applyStatus(ownStatus, draft);
                            onClose();
                          }}
                          className="min-h-10 rounded-md bg-blurple px-3 text-sm font-medium text-white transition-[background-color,transform] duration-fast hover:bg-blurple-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple active:scale-[0.98]"
                        >
                          {t.profil.saveStatus}
                        </button>
                      </div>
                    </div>
                  </div>
                </div>
              ) : (
                <div className="p-3">
                  <div className="flex items-center gap-2 px-1 pb-3">
                    <button
                      type="button"
                      aria-label={t.profil.backToProfile}
                      onClick={() => setView('profile')}
                      className="flex h-10 w-10 shrink-0 items-center justify-center rounded-md text-muted transition-[background-color,color,transform] duration-fast hover:bg-chat-hover hover:text-header focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple active:scale-95"
                    >
                      <BackIcon />
                    </button>
                    <div className="min-w-0">
                      <h2 className="truncate text-lg font-semibold text-header">
                        {t.profil.chooseAccount}
                      </h2>
                      <p className="text-xs text-muted">{t.profil.chooseAccountHint}</p>
                    </div>
                  </div>

                  <div className="space-y-2">
                    {accounts.map((account) => {
                      const current = accountIsCurrent(
                        account,
                        self.pubkey,
                        accounts.length,
                      );
                      const selected = selectedAccount === account.id;
                      return (
                        <div
                          key={account.id}
                          className={`overflow-hidden rounded-lg border bg-sidebar/80 shadow-1 ${
                            selected
                              ? 'border-blurple/60'
                              : 'border-[color:var(--glass-border)]'
                          }`}
                        >
                          <button
                            type="button"
                            disabled={current || switching}
                            aria-expanded={selected}
                            onClick={() => {
                              setSelectedAccount(selected ? null : account.id);
                              setPassphrase('');
                            }}
                            className="flex min-h-14 w-full items-center gap-3 px-3 text-left transition-[background-color,opacity] duration-fast hover:bg-chat-hover disabled:cursor-default disabled:opacity-70 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-blurple"
                          >
                            <Avatar
                              id={account.id}
                              name={account.name}
                              size={38}
                              avatarHash={null}
                            />
                            <span className="min-w-0 flex-1">
                              <span className="block truncate text-sm font-semibold text-header">
                                {account.name}
                              </span>
                              <span className="block truncate font-mono text-[11px] text-faint">
                                {current
                                  ? t.profil.currentAccount
                                  : (account.pubkey_short ?? t.profil.localAccount)}
                              </span>
                            </span>
                            {!current && (
                              <span className="text-faint">
                                <ChevronRightIcon />
                              </span>
                            )}
                          </button>
                          {selected && !current && (
                            <form
                              className="border-t border-[color:var(--glass-border)] p-3"
                              onSubmit={(e) => {
                                e.preventDefault();
                                void activateSelectedAccount();
                              }}
                            >
                              <label
                                htmlFor={`account-passphrase-${account.id}`}
                                className="text-xs font-medium text-muted"
                              >
                                {t.onboarding.passphrase}
                              </label>
                              <input
                                id={`account-passphrase-${account.id}`}
                                type="password"
                                autoFocus
                                value={passphrase}
                                onChange={(e) => setPassphrase(e.target.value)}
                                className="mt-2 min-h-10 w-full rounded-md border border-transparent bg-input px-3 text-sm text-norm outline-none transition-[border-color,box-shadow] duration-fast focus:border-blurple/50 focus:ring-1 focus:ring-blurple/25"
                              />
                              {accountError !== null && (
                                <p
                                  role="alert"
                                  className="mt-2 text-pretty text-xs text-red"
                                >
                                  {accountError}
                                </p>
                              )}
                              <button
                                type="submit"
                                disabled={passphrase === '' || switching}
                                className="mt-3 min-h-10 w-full rounded-md bg-blurple px-3 text-sm font-medium text-white transition-[background-color,transform,opacity] duration-fast hover:bg-blurple-hover disabled:pointer-events-none disabled:opacity-50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple active:scale-[0.98]"
                              >
                                {switching
                                  ? t.app.loading
                                  : t.profil.confirmAccountSwitch}
                              </button>
                            </form>
                          )}
                        </div>
                      );
                    })}
                  </div>

                  <button
                    type="button"
                    onClick={addAccount}
                    className="mt-3 min-h-11 w-full rounded-md border border-[color:var(--glass-border)] bg-input/70 px-3 text-sm font-medium text-norm transition-[background-color,transform] duration-fast hover:bg-chat-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple active:scale-[0.98]"
                  >
                    {t.onboarding.addAccount}
                  </button>
                </div>
              )}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
