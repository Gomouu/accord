/**
 * Section « Salons vocaux » de la barre latérale d'un groupe : entrée du salon
 * vocal par défaut (convention UI : channel_id == group_id) et, dessous, la
 * liste des participants connectés — anneau vert autour de l'avatar quand la
 * personne parle, badges micro/son coupé (états diffusés par les pairs) et
 * curseur de volume par participant distant, à la Discord.
 */

import { useEffect, useState } from 'react';
import { interpolate } from '../i18n';
import { rpc } from '../lib/client';
import { displayNameOf, useFriends } from '../stores/friends';
import { selfDisplayName, useSession } from '../stores/session';
import { useUi, useT } from '../stores/ui';
import { useVoice, type ParticipantState } from '../stores/voice';
import { Avatar } from './Avatar';

/** Icône haut-parleur (entrée du salon vocal). */
function SpeakerIcon() {
  return (
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
    >
      <polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5" />
      <path d="M15.54 8.46a5 5 0 0 1 0 7.07" />
      <path d="M19.07 4.93a10 10 0 0 1 0 14.14" />
    </svg>
  );
}

/** Icône micro barré (participant muet), 14 px, à la Discord. */
function MicOffIcon() {
  return (
    <svg
      width="14"
      height="14"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
    >
      <line x1="2" x2="22" y1="2" y2="22" />
      <path d="M18.89 13.23A7.12 7.12 0 0 0 19 12v-2" />
      <path d="M5 10v2a7 7 0 0 0 12 5" />
      <path d="M15 9.34V5a3 3 0 0 0-5.68-1.33" />
      <path d="M9 9v3a3 3 0 0 0 5.12 2.12" />
      <line x1="12" x2="12" y1="19" y2="22" />
    </svg>
  );
}

/** Icône casque barré (participant sourd), 14 px, à la Discord. */
function HeadphonesOffIcon() {
  return (
    <svg
      width="14"
      height="14"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
    >
      <path d="M3 14h3a2 2 0 0 1 2 2v3a2 2 0 0 1-2 2H4a1 1 0 0 1-1-1v-4a9 9 0 0 1 18 0v4a1 1 0 0 1-1 1h-2a2 2 0 0 1-2-2v-3a2 2 0 0 1 2-2h3" />
      <line x1="2" x2="22" y1="2" y2="22" />
    </svg>
  );
}

/**
 * Rangée d'un participant : avatar (anneau vert en parole), pseudo, badges
 * micro/son coupé et — pour les participants distants — un bouton qui déplie
 * un curseur de volume local (0-200 %, persisté côté nœud).
 */
function ParticipantRow({
  pubkey,
  state,
  name,
  isSelf,
}: {
  pubkey: string;
  state: ParticipantState;
  name: string;
  isSelf: boolean;
}) {
  const t = useT();
  const toast = useUi((s) => s.toast);
  const setVolume = useVoice((s) => s.setVolume);
  const [showVolume, setShowVolume] = useState(false);

  const onVolume = (value: number) => {
    setVolume(pubkey, value).catch(() => toast('error', t.errors.actionFailed));
  };

  return (
    <li className="group rounded-md px-2 py-1 text-muted">
      <div className="flex items-center gap-2">
        <div
          className={`shrink-0 rounded-full ${state.speaking ? 'ring-2 ring-green' : ''}`}
        >
          <Avatar id={pubkey} name={name} size={24} />
        </div>
        {state.speaking && <span className="sr-only">{t.voice.speaking}</span>}
        <span className="min-w-0 flex-1 truncate text-sm font-medium">{name}</span>
        {state.serverMuted && (
          <span
            role="img"
            aria-label={t.voice.serverMutedBadge}
            title={t.voice.serverMutedBadge}
            className="shrink-0 text-yellow"
          >
            <MicOffIcon />
          </span>
        )}
        {state.serverDeafened && (
          <span
            role="img"
            aria-label={t.voice.serverDeafenedBadge}
            title={t.voice.serverDeafenedBadge}
            className="shrink-0 text-yellow"
          >
            <HeadphonesOffIcon />
          </span>
        )}
        {state.muted && (
          <span role="img" aria-label={t.voice.mutedBadge} className="shrink-0 text-red">
            <MicOffIcon />
          </span>
        )}
        {state.deafened && (
          <span
            role="img"
            aria-label={t.voice.deafenedBadge}
            className="shrink-0 text-red"
          >
            <HeadphonesOffIcon />
          </span>
        )}
        {!isSelf && (
          <button
            type="button"
            aria-expanded={showVolume}
            aria-label={interpolate(t.voice.adjustVolumeOf, { name })}
            onClick={() => setShowVolume((v) => !v)}
            className={`shrink-0 rounded-xs px-1 text-xs text-faint transition-opacity duration-150 hover:text-norm focus-visible:opacity-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar group-hover:opacity-100 ${
              showVolume ? 'opacity-100 text-norm' : 'opacity-0'
            }`}
          >
            {state.volume}%
          </button>
        )}
      </div>
      {!isSelf && showVolume && (
        <div className="flex items-center gap-2 pb-1 pl-8 pr-1 pt-1">
          <input
            type="range"
            min={0}
            max={200}
            step={1}
            value={state.volume}
            aria-label={interpolate(t.voice.volumeOf, { name })}
            onChange={(e) => onVolume(Number(e.target.value))}
            className="h-1 w-full accent-blurple"
          />
          <span className="w-10 shrink-0 text-right text-xs tabular-nums text-faint">
            {state.volume}%
          </span>
        </div>
      )}
    </li>
  );
}

export function VoiceSection({ groupId }: { groupId: string }) {
  const t = useT();
  const toast = useUi((s) => s.toast);
  const active = useVoice((s) => s.active);
  const participants = useVoice((s) => s.participants);
  const join = useVoice((s) => s.join);
  const contacts = useFriends((s) => s.contacts);
  const self = useSession((s) => s.self);

  // Convention UI : un salon vocal par groupe, channel_id == group_id.
  const isConnectedHere =
    active !== null && active.groupId === groupId && active.channelId === groupId;
  const connected = isConnectedHere ? [...participants.entries()] : [];

  // Les états micro/son des pairs changent sans re-jointure : on applique
  // `event.voice_mute` au store tant que la section est montée…
  useEffect(() => {
    return rpc.onEvent((method, params) => {
      if (method !== 'event.voice_mute') return;
      const p = params as { pubkey?: unknown; muted?: unknown; deafened?: unknown };
      if (
        typeof p.pubkey !== 'string' ||
        typeof p.muted !== 'boolean' ||
        typeof p.deafened !== 'boolean'
      ) {
        return;
      }
      useVoice
        .getState()
        .applyMuteState({ pubkey: p.pubkey, muted: p.muted, deafened: p.deafened });
    });
  }, []);

  // … et on resynchronise à la connexion (volumes persistés, états courants),
  // au cas où des événements auraient été manqués section démontée.
  useEffect(() => {
    if (!isConnectedHere) return;
    useVoice
      .getState()
      .sync()
      .catch(() => {
        // Best effort : l'affichage se corrige aux prochains événements.
      });
  }, [isConnectedHere]);

  const nameOf = (pubkey: string): string =>
    self !== null && pubkey === self.pubkey
      ? selfDisplayName(self)
      : displayNameOf(contacts, pubkey);

  const onJoin = () => {
    if (isConnectedHere) return;
    join(groupId, groupId).catch(() => toast('error', t.errors.actionFailed));
  };

  return (
    <section aria-label={t.voice.channels}>
      <div className="px-2 pb-1 pt-4 text-xs font-medium uppercase tracking-wide text-faint">
        {t.voice.channels}
      </div>
      <button
        type="button"
        onClick={onJoin}
        className={`flex w-full items-center gap-1.5 rounded-md px-2 py-1.5 font-medium focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar ${
          isConnectedHere
            ? 'bg-chat-hover text-header'
            : 'text-muted hover:bg-chat-hover hover:text-norm'
        }`}
      >
        <span aria-hidden className="text-faint">
          <SpeakerIcon />
        </span>
        <span className="truncate">{t.voice.defaultChannel}</span>
      </button>
      {connected.length > 0 && (
        <ul className="space-y-0.5 pb-1 pl-6 pr-1 pt-0.5">
          {connected.map(([pubkey, state]) => (
            <ParticipantRow
              key={pubkey}
              pubkey={pubkey}
              state={state}
              name={nameOf(pubkey)}
              isSelf={self !== null && pubkey === self.pubkey}
            />
          ))}
        </ul>
      )}
    </section>
  );
}
