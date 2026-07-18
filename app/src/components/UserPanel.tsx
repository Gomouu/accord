/**
 * Panneau utilisateur en bas de la barre latérale : profil + paramètres, et
 * bandeau vocal (mute / raccrocher) au-dessus quand un salon est rejoint.
 * Le clic sur l'avatar/pseudo ouvre le menu utilisateur rapide, façon
 * Discord : statut (En ligne / Inactif / Ne pas déranger / Invisible + texte
 * personnalisé), changement de compte et déconnexion rapide sans passer par
 * les Paramètres (voir `UserMenu`).
 */

import { useEffect, useState } from 'react';
import type { PresenceStatus } from '../lib/api';
import { formatDuration } from '../lib/format';
import { useCalls } from '../stores/calls';
import {
  avatarDecorationOf,
  avatarOf,
  displayNameOf,
  useFriends,
} from '../stores/friends';
import { useContextMenu } from '../stores/contextMenu';
import { useGroups } from '../stores/groups';
import { selfDisplayName, useSession } from '../stores/session';
import { useUi, useT } from '../stores/ui';
import { useVoice } from '../stores/voice';
import { Avatar } from './Avatar';
import { buildOwnUserMenu } from './contactMenu';
import { PhoneOffIcon } from './ContextMenu';
import { PresenceDot } from './PresenceDot';
import { SoundboardButton } from './SoundboardButton';
import { ownDotStatus } from './UserMenu';

/** Icône casque, barrée en rouge quand la sortie est coupée (deafen). */
function HeadphonesIcon({ deafened }: { deafened: boolean }) {
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
      <path d="M3 14h3a2 2 0 0 1 2 2v3a2 2 0 0 1-2 2H4a1 1 0 0 1-1-1v-4a9 9 0 0 1 18 0v4a1 1 0 0 1-1 1h-2a2 2 0 0 1-2-2v-3a2 2 0 0 1 2-2h3" />
      {deafened && <line className="text-red" x1="3" y1="3" x2="21" y2="21" />}
    </svg>
  );
}

/** Icône micro, barrée en rouge quand le micro est coupé. */
function MicIcon({ muted }: { muted: boolean }) {
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
      <path d="M12 2a3 3 0 0 0-3 3v6a3 3 0 0 0 6 0V5a3 3 0 0 0-3-3Z" />
      <path d="M19 10v1a7 7 0 0 1-14 0v-1" />
      <line x1="12" x2="12" y1="18" y2="22" />
      <line x1="8" x2="16" y1="22" y2="22" />
      {muted && <line className="text-red" x1="3" y1="3" x2="21" y2="21" />}
    </svg>
  );
}

/** Bouton d'action carré du bandeau vocal. */
const ICON_BUTTON_CLASS =
  'flex h-9 w-9 shrink-0 items-center justify-center rounded-md transition-colors duration-fast hover:bg-chat-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-rail active:scale-95';

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
    <div className="flex items-center justify-between gap-2 border-b border-[color:var(--glass-border)] bg-rail/60 px-2 py-2">
      <div className="min-w-0">
        <div className="truncate text-sm font-medium text-green">{t.voice.connected}</div>
        <div className="truncate text-xs text-muted">{groupName ?? '…'}</div>
      </div>
      <div className="flex shrink-0 items-center gap-0.5">
        <button
          type="button"
          aria-label={muteLabel}
          title={muteLabel}
          aria-pressed={active.muted}
          onClick={() => toggleMute().catch(onActionError)}
          className={`${ICON_BUTTON_CLASS} ${active.muted ? 'text-red' : 'text-muted hover:text-norm'}`}
        >
          <MicIcon muted={active.muted} />
        </button>
        <button
          type="button"
          aria-label={deafenLabel}
          title={deafenLabel}
          aria-pressed={selfDeafened}
          onClick={() => toggleDeafen().catch(onActionError)}
          className={`${ICON_BUTTON_CLASS} ${selfDeafened ? 'text-red' : 'text-muted hover:text-norm'}`}
        >
          <HeadphonesIcon deafened={selfDeafened} />
        </button>
        <SoundboardButton className={ICON_BUTTON_CLASS} />
        <button
          type="button"
          aria-label={t.voice.disconnect}
          title={t.voice.disconnect}
          onClick={() => leave().catch(onActionError)}
          className={`${ICON_BUTTON_CLASS} text-red`}
        >
          <PhoneOffIcon />
        </button>
      </div>
    </div>
  );
}

/**
 * Bandeau d'appel 1-à-1 : sonnerie sortante (annuler) ou appel actif (pair,
 * durée, mute/deafen/raccrocher — la session réutilise le moteur vocal
 * existant une fois `event.call_accepted` traité, voir `AppShell`). Un appel
 * et un salon de groupe ne coexistent jamais (contrat voix, voir
 * VOICE_CALLS.md §1.3) : `UserPanel` n'affiche celui-ci qu'à la place de
 * `VoiceBanner`, jamais les deux.
 */
function CallBanner() {
  const t = useT();
  const toast = useUi((s) => s.toast);
  const phase = useCalls((s) => s.phase);
  const peer = useCalls((s) => s.peer);
  const sincePhaseMs = useCalls((s) => s.sincePhaseMs);
  const hangup = useCalls((s) => s.hangup);
  const contacts = useFriends((s) => s.contacts);
  const voiceActive = useVoice((s) => s.active);
  const toggleMute = useVoice((s) => s.toggleMute);
  const selfDeafened = useVoice((s) => s.selfDeafened);
  const toggleDeafen = useVoice((s) => s.toggleDeafen);

  // Fait vivre le chronomètre de l'appel actif (aucun intérêt à re-rendre
  // pendant la sonnerie : le libellé ne change pas).
  const [, forceTick] = useState(0);
  useEffect(() => {
    if (phase !== 'active') return undefined;
    const id = setInterval(() => forceTick((n) => n + 1), 1000);
    return () => clearInterval(id);
  }, [phase]);

  if (peer === null || (phase !== 'outgoing_ringing' && phase !== 'active')) return null;

  const name = displayNameOf(contacts, peer);
  const onActionError = (): void => toast('error', t.errors.actionFailed);
  const voiceReady = phase === 'active' && voiceActive !== null && voiceActive.isCall;
  const elapsed =
    phase === 'active' && sincePhaseMs !== null
      ? formatDuration((Date.now() - sincePhaseMs) / 1000)
      : null;
  const muteLabel = voiceActive?.muted === true ? t.voice.unmute : t.voice.mute;
  const deafenLabel = selfDeafened ? t.voice.undeafen : t.voice.deafen;
  const hangupLabel = phase === 'outgoing_ringing' ? t.calls.cancel : t.calls.hangup;

  return (
    <div className="flex items-center justify-between gap-2 border-b border-[color:var(--glass-border)] bg-rail/60 px-2 py-2">
      <div className="flex min-w-0 items-center gap-2">
        <Avatar
          id={peer}
          name={name}
          size={28}
          avatarHash={avatarOf(contacts, peer)}
          hint={peer}
          decoration={avatarDecorationOf(contacts, peer)}
        />
        <div className="min-w-0">
          <div className="truncate text-sm font-medium text-green">{name}</div>
          <div className="truncate text-xs text-muted">
            {phase === 'outgoing_ringing'
              ? t.calls.outgoingRinging
              : (elapsed ?? t.voice.connected)}
          </div>
        </div>
      </div>
      <div className="flex shrink-0 items-center gap-0.5">
        {voiceReady && (
          <>
            <button
              type="button"
              aria-label={muteLabel}
              title={muteLabel}
              aria-pressed={voiceActive?.muted === true}
              onClick={() => toggleMute().catch(onActionError)}
              className={`${ICON_BUTTON_CLASS} ${
                voiceActive?.muted === true ? 'text-red' : 'text-muted hover:text-norm'
              }`}
            >
              <MicIcon muted={voiceActive?.muted === true} />
            </button>
            <button
              type="button"
              aria-label={deafenLabel}
              title={deafenLabel}
              aria-pressed={selfDeafened}
              onClick={() => toggleDeafen().catch(onActionError)}
              className={`${ICON_BUTTON_CLASS} ${
                selfDeafened ? 'text-red' : 'text-muted hover:text-norm'
              }`}
            >
              <HeadphonesIcon deafened={selfDeafened} />
            </button>
          </>
        )}
        <button
          type="button"
          aria-label={hangupLabel}
          title={hangupLabel}
          onClick={() => hangup().catch(onActionError)}
          className={`${ICON_BUTTON_CLASS} text-red`}
        >
          <PhoneOffIcon />
        </button>
      </div>
    </div>
  );
}

export function UserPanel() {
  const t = useT();
  const self = useSession((s) => s.self);
  const phase = useSession((s) => s.phase);
  const openModal = useUi((s) => s.openModal);
  const profile = useUi((s) => s.profile);
  const openProfile = useUi((s) => s.openProfile);
  const ownStatus = useFriends((s) => s.ownStatus);
  const ownStatusText = useFriends((s) => s.ownStatusText);
  const loadOwnStatus = useFriends((s) => s.loadOwnStatus);
  const callPhase = useCalls((s) => s.phase);

  useEffect(() => {
    loadOwnStatus().catch(() => {
      // Best effort : le statut par défaut (en ligne) reste affiché.
    });
  }, [loadOwnStatus]);

  if (!self) return null;

  const displayName = selfDisplayName(self);
  const dotStatus: PresenceStatus =
    phase === 'ready' ? ownDotStatus(ownStatus) : 'offline';
  // Un appel et un salon de groupe ne coexistent jamais (voir CallBanner) :
  // le bandeau d'appel prime, jamais les deux affichés ensemble.
  const inCallPhase = callPhase === 'outgoing_ringing' || callPhase === 'active';
  const userMenuOpen = profile?.pubkey === self.pubkey && profile.surface === 'user-menu';

  return (
    <div className="accord-user-panel relative border-t border-[color:var(--glass-border)]">
      {inCallPhase ? <CallBanner /> : <VoiceBanner />}
      <div className="flex items-center gap-1.5 bg-rail/70 p-2">
        <button
          type="button"
          data-user-menu-trigger
          onMouseDown={(e) => e.stopPropagation()}
          onClick={(e) => {
            const r = e.currentTarget.getBoundingClientRect();
            openProfile(
              self.pubkey,
              {
                top: r.top,
                left: r.left,
                bottom: r.bottom,
                right: r.right,
              },
              null,
              'user-menu',
            );
          }}
          onContextMenu={(e) => {
            e.preventDefault();
            useContextMenu
              .getState()
              .openMenu(e.clientX, e.clientY, buildOwnUserMenu(t, self, ownStatus), {
                preferredSide: 'top',
              });
          }}
          title={t.profil.userMenu}
          aria-label={t.profil.userMenu}
          aria-haspopup="dialog"
          aria-expanded={userMenuOpen}
          className="flex min-h-11 min-w-0 flex-1 items-center gap-2.5 rounded-md px-1.5 py-1 text-left transition-colors duration-fast hover:bg-chat-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-rail active:scale-[0.99]"
        >
          <span className="relative shrink-0 rounded-full">
            <Avatar
              id={self.pubkey}
              name={displayName}
              size={36}
              avatarHash={self.avatar}
              hint={self.pubkey}
              decoration={self.avatar_decoration}
            />
            <PresenceDot
              status={dotStatus}
              label={t.profil[dotStatus]}
              className="absolute -bottom-0.5 -right-0.5 rounded-full ring-2 ring-rail"
            />
          </span>
          <div className="min-w-0 flex-1">
            <div className="truncate text-sm font-semibold text-header">
              {displayName}
            </div>
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
          className="flex h-10 w-10 shrink-0 items-center justify-center rounded-md text-muted transition-colors duration-fast hover:bg-chat-hover hover:text-norm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-rail active:scale-95"
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
            <path d="M10.3 3.6a2 2 0 0 1 3.4 0l.4.7a2 2 0 0 0 2.2.9l.8-.2a2 2 0 0 1 2.4 2.4l-.2.8a2 2 0 0 0 .9 2.2l.7.4a2 2 0 0 1 0 3.4l-.7.4a2 2 0 0 0-.9 2.2l.2.8a2 2 0 0 1-2.4 2.4l-.8-.2a2 2 0 0 0-2.2.9l-.4.7a2 2 0 0 1-3.4 0l-.4-.7a2 2 0 0 0-2.2-.9l-.8.2a2 2 0 0 1-2.4-2.4l.2-.8a2 2 0 0 0-.9-2.2l-.7-.4a2 2 0 0 1 0-3.4l.7-.4a2 2 0 0 0 .9-2.2l-.2-.8a2 2 0 0 1 2.4-2.4l.8.2a2 2 0 0 0 2.2-.9l.4-.7Z" />
            <circle cx="12" cy="12" r="3" />
          </svg>
        </button>
      </div>
    </div>
  );
}
