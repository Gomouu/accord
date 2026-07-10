/**
 * Panneau utilisateur en bas de la barre latérale : profil + paramètres, et
 * bandeau vocal (mute / raccrocher) au-dessus quand un salon est rejoint.
 * Le clic sur l'avatar ouvre le menu de statut (En ligne / Inactif / Ne pas
 * déranger / Invisible + texte personnalisé, façon Discord).
 */

import type { MouseEvent as ReactMouseEvent } from 'react';
import { useEffect, useRef, useState } from 'react';
import type { OwnPresenceStatus, PresenceStatus } from '../lib/api';
import { useFriends } from '../stores/friends';
import { useGroups } from '../stores/groups';
import { selfDisplayName, useSession } from '../stores/session';
import { useUi, useT } from '../stores/ui';
import { useVoice } from '../stores/voice';
import { Avatar } from './Avatar';
import { PresenceDot } from './PresenceDot';

/** Icône casque, barrée en rouge quand la sortie est coupée (deafen). */
function HeadphonesIcon({ deafened }: { deafened: boolean }) {
  return (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor" aria-hidden>
      <path d="M12 3a9 9 0 0 0-9 9v6a2 2 0 0 0 2 2h2a1 1 0 0 0 1-1v-5a1 1 0 0 0-1-1H5v-1a7 7 0 0 1 14 0v1h-2a1 1 0 0 0-1 1v5a1 1 0 0 0 1 1h2a2 2 0 0 0 2-2v-6a9 9 0 0 0-9-9Z" />
      {deafened && (
        <path
          className="text-red"
          d="M3.3 2.3a1 1 0 0 1 1.4 0l16 16a1 1 0 0 1-1.4 1.4l-16-16a1 1 0 0 1 0-1.4Z"
        />
      )}
    </svg>
  );
}

/** Icône micro, barrée en rouge quand le micro est coupé. */
function MicIcon({ muted }: { muted: boolean }) {
  return (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor" aria-hidden>
      <path d="M12 2a3 3 0 0 0-3 3v6a3 3 0 1 0 6 0V5a3 3 0 0 0-3-3Z" />
      <path d="M6 10a1 1 0 1 0-2 0 8 8 0 0 0 7 7.9V20H8a1 1 0 1 0 0 2h8a1 1 0 1 0 0-2h-3v-2.1a8 8 0 0 0 7-7.9 1 1 0 1 0-2 0 6 6 0 0 1-12 0Z" />
      {muted && (
        <path
          className="text-red"
          d="M3.3 2.3a1 1 0 0 1 1.4 0l16 16a1 1 0 0 1-1.4 1.4l-16-16a1 1 0 0 1 0-1.4Z"
        />
      )}
    </svg>
  );
}

/** Bandeau « Vocal connecté » : nom du groupe, mute micro, raccrocher. */
function VoiceBanner() {
  const t = useT();
  const toast = useUi((s) => s.toast);
  const active = useVoice((s) => s.active);
  const toggleMute = useVoice((s) => s.toggleMute);
  const selfDeafened = useVoice((s) => s.selfDeafened);
  const toggleDeafen = useVoice((s) => s.toggleDeafen);
  const leave = useVoice((s) => s.leave);
  const groupName = useGroups((s) =>
    active === null ? null : (s.states[active.groupId]?.name ?? null),
  );

  if (active === null) return null;

  const onActionError = (): void => toast('error', t.errors.actionFailed);
  const muteLabel = active.muted ? t.voice.unmute : t.voice.mute;
  const deafenLabel = selfDeafened ? t.voice.undeafen : t.voice.deafen;

  return (
    <div className="flex items-center justify-between gap-2 border-b border-rail bg-rail/60 px-2 py-2">
      <div className="min-w-0">
        <div className="truncate text-sm font-semibold text-green">
          {t.voice.connected}
        </div>
        <div className="truncate text-xs text-muted">{groupName ?? '…'}</div>
      </div>
      <div className="flex shrink-0 items-center gap-0.5">
        <button
          type="button"
          aria-label={muteLabel}
          title={muteLabel}
          aria-pressed={active.muted}
          onClick={() => toggleMute().catch(onActionError)}
          className={`rounded p-1.5 hover:bg-chat-hover ${
            active.muted ? 'text-red' : 'text-muted hover:text-norm'
          }`}
        >
          <MicIcon muted={active.muted} />
        </button>
        <button
          type="button"
          aria-label={deafenLabel}
          title={deafenLabel}
          aria-pressed={selfDeafened}
          onClick={() => toggleDeafen().catch(onActionError)}
          className={`rounded p-1.5 hover:bg-chat-hover ${
            selfDeafened ? 'text-red' : 'text-muted hover:text-norm'
          }`}
        >
          <HeadphonesIcon deafened={selfDeafened} />
        </button>
        <button
          type="button"
          aria-label={t.voice.disconnect}
          title={t.voice.disconnect}
          onClick={() => leave().catch(onActionError)}
          className="rounded p-1.5 text-red hover:bg-chat-hover"
        >
          <svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor" aria-hidden>
            <path d="M12 9c-3.9 0-7.5 1.5-10 3.9a2 2 0 0 0-.2 2.7l1.5 1.9a2 2 0 0 0 2.4.6l2.7-1.3a2 2 0 0 0 1.1-1.8v-1.3a10.3 10.3 0 0 1 5 0V15a2 2 0 0 0 1.1 1.8l2.7 1.3a2 2 0 0 0 2.4-.6l1.5-1.9a2 2 0 0 0-.2-2.7A14.6 14.6 0 0 0 12 9Z" />
          </svg>
        </button>
      </div>
    </div>
  );
}

/** Statut affichable de son propre nœud (invisible = pastille hors ligne). */
function ownDotStatus(status: OwnPresenceStatus): PresenceStatus {
  return status === 'invisible' ? 'offline' : status;
}

/** Menu de statut : les quatre statuts + champ de texte personnalisé. */
function StatusMenu({ onClose }: { onClose: () => void }) {
  const t = useT();
  const toast = useUi((s) => s.toast);
  const ownStatus = useFriends((s) => s.ownStatus);
  const ownStatusText = useFriends((s) => s.ownStatusText);
  const setOwnStatus = useFriends((s) => s.setOwnStatus);
  const [draft, setDraft] = useState(ownStatusText ?? '');
  const ref = useRef<HTMLDivElement>(null);

  // Fermeture au clic extérieur et à Échap (même approche que ProfilePopover).
  useEffect(() => {
    const onKey = (e: KeyboardEvent): void => {
      if (e.key === 'Escape') onClose();
    };
    const onDown = (e: MouseEvent): void => {
      if (ref.current !== null && !ref.current.contains(e.target as Node)) onClose();
    };
    window.addEventListener('keydown', onKey);
    document.addEventListener('mousedown', onDown);
    return () => {
      window.removeEventListener('keydown', onKey);
      document.removeEventListener('mousedown', onDown);
    };
  }, [onClose]);

  const apply = (status: OwnPresenceStatus, custom?: string): void => {
    setOwnStatus(status, custom).catch(() => toast('error', t.errors.actionFailed));
  };

  const options: { status: OwnPresenceStatus; label: string }[] = [
    { status: 'online', label: t.profil.online },
    { status: 'idle', label: t.profil.idle },
    { status: 'dnd', label: t.profil.dnd },
    { status: 'invisible', label: t.profil.invisible },
  ];

  return (
    <div
      ref={ref}
      role="menu"
      aria-label={t.profil.setStatus}
      className="absolute bottom-full left-2 z-50 mb-2 w-56 rounded-lg bg-modal p-2 shadow-modal"
    >
      {options.map(({ status, label }) => (
        <button
          key={status}
          type="button"
          role="menuitemradio"
          aria-checked={ownStatus === status}
          onClick={() => {
            apply(status);
            onClose();
          }}
          className={`flex w-full items-center gap-2.5 rounded px-2 py-1.5 text-sm ${
            ownStatus === status
              ? 'bg-chat-hover text-header'
              : 'text-norm hover:bg-chat-hover'
          }`}
        >
          <PresenceDot status={ownDotStatus(status)} />
          {label}
        </button>
      ))}
      <div className="my-2 h-px bg-input" role="separator" />
      <div className="flex items-center gap-1.5 px-1 pb-1">
        <input
          aria-label={t.profil.customStatusPlaceholder}
          placeholder={t.profil.customStatusPlaceholder}
          value={draft}
          maxLength={128}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') {
              apply(ownStatus, draft);
              onClose();
            }
          }}
          className="min-w-0 flex-1 rounded bg-input px-2 py-1.5 text-sm text-norm placeholder-faint outline-none focus-visible:ring-2 focus-visible:ring-blurple"
        />
        {(ownStatusText ?? '') !== '' && (
          <button
            type="button"
            aria-label={t.profil.clearCustomStatus}
            title={t.profil.clearCustomStatus}
            onClick={() => {
              apply(ownStatus, '');
              onClose();
            }}
            className="rounded p-1.5 text-muted hover:bg-chat-hover hover:text-norm"
          >
            <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor" aria-hidden>
              <path d="M5.3 5.3a1 1 0 0 1 1.4 0L12 10.6l5.3-5.3a1 1 0 1 1 1.4 1.4L13.4 12l5.3 5.3a1 1 0 0 1-1.4 1.4L12 13.4l-5.3 5.3a1 1 0 0 1-1.4-1.4L10.6 12 5.3 6.7a1 1 0 0 1 0-1.4Z" />
            </svg>
          </button>
        )}
      </div>
      <div className="px-1 pb-0.5 text-[10px] text-faint">{t.profil.customStatusHint}</div>
    </div>
  );
}

export function UserPanel() {
  const t = useT();
  const self = useSession((s) => s.self);
  const phase = useSession((s) => s.phase);
  const openModal = useUi((s) => s.openModal);
  const openProfile = useUi((s) => s.openProfile);
  const ownStatus = useFriends((s) => s.ownStatus);
  const ownStatusText = useFriends((s) => s.ownStatusText);
  const loadOwnStatus = useFriends((s) => s.loadOwnStatus);
  const [statusMenuOpen, setStatusMenuOpen] = useState(false);

  useEffect(() => {
    loadOwnStatus().catch(() => {
      // Best effort : le statut par défaut (en ligne) reste affiché.
    });
  }, [loadOwnStatus]);

  if (!self) return null;

  // Ouvre sa propre carte de profil (façon Discord), ancrée sur le bouton
  // cliqué (pseudo + code ami).
  const ouvrirProfil = (e: ReactMouseEvent<HTMLButtonElement>): void => {
    const r = e.currentTarget.getBoundingClientRect();
    openProfile(self.pubkey, {
      top: r.top,
      left: r.left,
      bottom: r.bottom,
      right: r.right,
    });
  };

  const displayName = selfDisplayName(self);
  const dotStatus: PresenceStatus =
    phase === 'ready' ? ownDotStatus(ownStatus) : 'offline';

  return (
    <div className="relative">
      <VoiceBanner />
      {statusMenuOpen && <StatusMenu onClose={() => setStatusMenuOpen(false)} />}
      <div className="flex items-center gap-2 bg-rail/60 px-2 py-2">
        <button
          type="button"
          onClick={() => setStatusMenuOpen((open) => !open)}
          title={t.profil.setStatus}
          aria-label={t.profil.setStatus}
          aria-expanded={statusMenuOpen}
          className="relative shrink-0 rounded-full focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple"
        >
          <Avatar
            id={self.pubkey}
            name={displayName}
            size={32}
            avatarHash={self.avatar}
            hint={self.pubkey}
          />
          <PresenceDot
            status={dotStatus}
            label={t.profil[dotStatus]}
            className="absolute -bottom-0.5 -right-0.5 rounded-full ring-2 ring-rail"
          />
        </button>
        <button
          type="button"
          onClick={ouvrirProfil}
          title={t.profil.title}
          aria-label={t.profil.title}
          className="flex min-w-0 flex-1 items-center gap-2 rounded px-1 py-0.5 text-left hover:bg-chat-hover"
        >
          <div className="min-w-0 flex-1">
            <div className="truncate text-sm font-medium text-header">{displayName}</div>
            <div className="truncate text-xs text-faint">
              {(ownStatusText ?? '') !== '' ? ownStatusText : self.friend_code}
            </div>
          </div>
        </button>
        <button
          type="button"
          aria-label={t.settings.title}
          title={t.settings.title}
          onClick={() => openModal({ kind: 'settings' })}
          className="rounded p-1.5 text-muted hover:bg-chat-hover hover:text-norm"
        >
          <svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor" aria-hidden>
            <path d="M10.3 3.6a2 2 0 0 1 3.4 0l.6 1a2 2 0 0 0 2.2.9l1.1-.3a2 2 0 0 1 2.4 2.4l-.3 1.1a2 2 0 0 0 .9 2.2l1 .6a2 2 0 0 1 0 3.4l-1 .6a2 2 0 0 0-.9 2.2l.3 1.1a2 2 0 0 1-2.4 2.4l-1.1-.3a2 2 0 0 0-2.2.9l-.6 1a2 2 0 0 1-3.4 0l-.6-1a2 2 0 0 0-2.2-.9l-1.1.3a2 2 0 0 1-2.4-2.4l.3-1.1a2 2 0 0 0-.9-2.2l-1-.6a2 2 0 0 1 0-3.4l1-.6a2 2 0 0 0 .9-2.2l-.3-1.1a2 2 0 0 1 2.4-2.4l1.1.3a2 2 0 0 0 2.2-.9l.6-1ZM12 15.5a3.5 3.5 0 1 0 0-7 3.5 3.5 0 0 0 0 7Z" />
          </svg>
        </button>
      </div>
    </div>
  );
}
