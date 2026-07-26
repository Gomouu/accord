/**
 * Surface vidéo d'un appel ou d'un salon vocal.
 *
 * L'ancienne version empilait les vues verticalement : cela tenait pour un
 * appel à deux avec un écran partagé, pas pour un salon où plusieurs personnes
 * diffusent. La grille s'adapte au nombre de flux, met en avant celui qu'on
 * épingle, et se replie proprement — avatars — quand personne n'a de caméra.
 *
 * Une tuile = un flux (une personne peut en avoir deux : sa caméra ET son
 * écran, comme sur les autres plateformes).
 */

import { useEffect, useMemo, useRef, useState } from 'react';
import {
  playbackFor,
  localPreviewStream,
  videoSourceSupported,
} from '../lib/mediaController';
import type { VideoSource } from '../lib/mediaController';
import { useCalls } from '../stores/calls';
import { displayNameOf, useFriends } from '../stores/friends';
import { useT } from '../stores/ui';
import { useVoice } from '../stores/voice';
import { Avatar } from './Avatar';

/** Icône « moniteur » (partage d'écran). */
function ScreenIcon() {
  return (
    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" aria-hidden="true">
      <rect
        x="3"
        y="4"
        width="18"
        height="12"
        rx="2"
        stroke="currentColor"
        strokeWidth="2"
      />
      <path
        d="M8 20h8M12 16v4"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
      />
    </svg>
  );
}

/** Icône « caméra ». */
function CameraIcon() {
  return (
    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" aria-hidden="true">
      <rect
        x="3"
        y="6"
        width="12"
        height="12"
        rx="2"
        stroke="currentColor"
        strokeWidth="2"
      />
      <path
        d="M15 10.5l6-3.5v10l-6-3.5"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinejoin="round"
      />
    </svg>
  );
}

/** Icône « épingle ». */
function PinIcon() {
  return (
    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" aria-hidden="true">
      <path
        d="M9 3h6l-1 6 4 3v2H6v-2l4-3-1-6Z M12 14v7"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinejoin="round"
      />
    </svg>
  );
}

/** Un flux affichable : qui l'envoie, et de quelle source. */
interface Tile {
  peer: string;
  source: VideoSource;
}

const tileKey = (tile: Tile): string => `${tile.peer}:${tile.source}`;

/**
 * Classes de grille pour `n` tuiles. Une seule tuile occupe toute la largeur ;
 * au-delà, deux colonnes puis trois — au-delà de neuf flux simultanés la
 * lisibilité est perdue de toute façon, et la limite de participants d'un
 * salon (10) borne naturellement le cas.
 */
function gridClass(n: number): string {
  if (n <= 1) return 'grid-cols-1';
  if (n <= 4) return 'grid-cols-2';
  return 'grid-cols-3';
}

/** Canevas d'un flux distant, lié à la visionneuse de son émetteur. */
function RemoteTile({
  tile,
  name,
  avatarHash,
  label,
  pinned,
  onTogglePin,
  pinLabel,
}: {
  tile: Tile;
  name: string;
  avatarHash: string | null;
  label: string;
  pinned: boolean;
  onTogglePin: () => void;
  pinLabel: string;
}) {
  const canvasRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (canvas !== null) playbackFor(tile.peer, tile.source).attach(canvas);
  }, [tile.peer, tile.source]);

  return (
    <div className="group relative overflow-hidden rounded-xl border border-rail/60 bg-black shadow-2">
      <canvas
        ref={canvasRef}
        aria-label={label}
        className="block h-full w-full bg-black object-contain"
      />
      <div className="pointer-events-none absolute inset-x-0 bottom-0 flex items-center gap-2 bg-gradient-to-t from-black/70 to-transparent px-2.5 py-1.5">
        <Avatar
          id={tile.peer}
          name={name}
          size={18}
          avatarHash={avatarHash}
          hint={tile.peer}
        />
        <span className="min-w-0 truncate text-xs font-medium text-white">{label}</span>
      </div>
      <button
        type="button"
        onClick={onTogglePin}
        aria-pressed={pinned}
        title={pinLabel}
        aria-label={pinLabel}
        className="absolute end-1.5 top-1.5 rounded-md bg-black/50 p-1.5 text-white opacity-0 transition-opacity duration-fast focus-visible:opacity-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple group-hover:opacity-100 aria-pressed:opacity-100"
      >
        <PinIcon />
      </button>
    </div>
  );
}

/** Tuile de repli : la personne est là, sans vidéo. */
function AvatarTile({
  peer,
  name,
  avatarHash,
}: {
  peer: string;
  name: string;
  avatarHash: string | null;
}) {
  return (
    <div className="flex flex-col items-center justify-center gap-2 rounded-xl border border-rail/60 bg-sidebar py-6 shadow-1">
      <Avatar id={peer} name={name} size={48} avatarHash={avatarHash} hint={peer} />
      <span className="max-w-full truncate px-2 text-xs font-medium text-muted">
        {name}
      </span>
    </div>
  );
}

/** Aperçu de sa propre caméra (miroir, muet) pendant qu'on la diffuse. */
function SelfView({ label }: { label: string }) {
  const videoRef = useRef<HTMLVideoElement>(null);

  useEffect(() => {
    const el = videoRef.current;
    const stream = localPreviewStream('camera');
    if (el !== null && stream !== null) {
      el.srcObject = stream;
      void el.play().catch(() => {
        // Lecture refusée (autoplay) : l'aperçu reste noir, sans conséquence
        // sur ce que reçoit le pair.
      });
    }
    return () => {
      if (el !== null) el.srcObject = null;
    };
  }, []);

  return (
    <video
      ref={videoRef}
      muted
      playsInline
      aria-label={label}
      className="pointer-events-auto h-[96px] w-[128px] scale-x-[-1] rounded-lg border border-rail/60 bg-black object-cover shadow-2"
    />
  );
}

export function CallVideo() {
  const t = useT();
  const phase = useCalls((s) => s.phase);
  const callPeer = useCalls((s) => s.peer);
  const remoteVideo = useCalls((s) => s.remoteVideo);
  const localSharing = useCalls((s) => s.localSharing);
  const localCamera = useCalls((s) => s.localCamera);
  const startScreenShare = useCalls((s) => s.startScreenShare);
  const stopScreenShare = useCalls((s) => s.stopScreenShare);
  const startCamera = useCalls((s) => s.startCamera);
  const stopCamera = useCalls((s) => s.stopCamera);
  const contacts = useFriends((s) => s.contacts);

  const active = useVoice((s) => s.active);
  const participants = useVoice((s) => s.participants);
  const inVoiceRoom = active !== null && !active.isCall;

  const [pinned, setPinned] = useState<string | null>(null);
  const screenOk = useMemo(() => videoSourceSupported('screen'), []);
  const cameraOk = useMemo(() => videoSourceSupported('camera'), []);

  const tiles = useMemo<Tile[]>(() => {
    const out: Tile[] = [];
    for (const [peer, streams] of Object.entries(remoteVideo)) {
      if (streams.camera) out.push({ peer, source: 'camera' });
      if (streams.screen) out.push({ peer, source: 'screen' });
    }
    // Ordre stable : sans tri, la grille se réorganiserait au gré de l'ordre
    // d'arrivée des annonces, ce qui déplace les visages sous le curseur.
    return out.sort((a, b) => tileKey(a).localeCompare(tileKey(b)));
  }, [remoteVideo]);

  // Un épinglage sur un flux qui s'est arrêté ne doit pas figer une tuile
  // morte : on retombe sur la grille complète.
  const pinnedTile = tiles.find((tile) => tileKey(tile) === pinned) ?? null;
  const shown = pinnedTile === null ? tiles : [pinnedTile];

  /** Participants présents sans aucun flux vidéo (repli en avatars). */
  const silent = useMemo(() => {
    if (!inVoiceRoom) return [] as string[];
    return [...participants.keys()].filter((pubkey) => remoteVideo[pubkey] === undefined);
  }, [inVoiceRoom, participants, remoteVideo]);

  if (phase !== 'active' && !inVoiceRoom) return null;

  const nameOf = (pubkey: string): string => displayNameOf(contacts, pubkey);
  const avatarOf = (pubkey: string): string | null =>
    contacts.find((c) => c.pubkey === pubkey)?.avatar ?? null;

  const labelOf = (tile: Tile): string =>
    tile.source === 'screen'
      ? `${nameOf(tile.peer)} — ${t.screenShare.peerSharing}`
      : nameOf(tile.peer);

  const toggleScreen = (): void => {
    if (localSharing) {
      void stopScreenShare();
    } else {
      // L'utilisateur peut refuser le sélecteur système : échec silencieux.
      void startScreenShare().catch(() => {});
    }
  };

  const toggleCamera = (): void => {
    if (localCamera) {
      void stopCamera();
    } else {
      void startCamera().catch(() => {});
    }
  };

  // Caméra et partage d'écran : deux boutons isolés qui flottent au-dessus de
  // la grille d'appel, sans voisin à bousculer. 36 → 44 px de haut, c'est le
  // seul endroit du projet où la cible peut grandir franchement sans coûter
  // un pixel à quoi que ce soit d'autre.
  const buttonClass = (on: boolean): string =>
    `pointer-events-auto inline-flex min-h-11 items-center gap-2 rounded-full px-4 py-2 text-sm font-medium shadow-2 transition-colors duration-fast focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-chat disabled:cursor-not-allowed disabled:opacity-50 ${
      on
        ? 'bg-red text-white hover:bg-red/90'
        : 'bg-blurple text-white hover:bg-blurple/90'
    }`;

  // Un appel 1-à-1 sans vidéo n'a personne à représenter en avatar : la
  // surface se réduit alors aux deux boutons.
  const avatarFallback = shown.length === 0 && silent.length > 0;

  return (
    <div className="pointer-events-none fixed inset-x-0 bottom-24 z-30 flex flex-col items-center gap-3">
      {shown.length > 0 && (
        <div
          role="group"
          aria-label={t.callVideo.gridLabel}
          className={`pointer-events-auto grid w-[min(960px,92vw)] gap-2 ${gridClass(shown.length)}`}
        >
          {shown.map((tile) => (
            <RemoteTile
              key={tileKey(tile)}
              tile={tile}
              name={nameOf(tile.peer)}
              avatarHash={avatarOf(tile.peer)}
              label={labelOf(tile)}
              pinned={pinned === tileKey(tile)}
              pinLabel={pinned === tileKey(tile) ? t.callVideo.unpin : t.callVideo.pin}
              onTogglePin={() =>
                setPinned((current) => (current === tileKey(tile) ? null : tileKey(tile)))
              }
            />
          ))}
        </div>
      )}

      {avatarFallback && (
        <div
          role="group"
          aria-label={t.callVideo.gridLabel}
          className={`pointer-events-auto grid w-[min(640px,88vw)] gap-2 ${gridClass(silent.length)}`}
        >
          {silent.map((pubkey) => (
            <AvatarTile
              key={pubkey}
              peer={pubkey}
              name={nameOf(pubkey)}
              avatarHash={avatarOf(pubkey)}
            />
          ))}
        </div>
      )}

      {localCamera && <SelfView label={t.callVideo.selfLabel} />}

      <div className="flex items-center gap-2">
        <button
          type="button"
          onClick={toggleCamera}
          disabled={!cameraOk}
          aria-pressed={localCamera}
          {...(cameraOk ? {} : { title: t.callVideo.cameraUnsupported })}
          className={buttonClass(localCamera)}
        >
          <CameraIcon />
          {localCamera ? t.callVideo.stopCamera : t.callVideo.startCamera}
        </button>
        <button
          type="button"
          onClick={toggleScreen}
          disabled={!screenOk}
          aria-pressed={localSharing}
          {...(screenOk ? {} : { title: t.screenShare.unsupported })}
          className={buttonClass(localSharing)}
        >
          <ScreenIcon />
          {localSharing ? t.screenShare.stop : t.screenShare.start}
        </button>
      </div>

      {/* En appel 1-à-1, le pair reste identifiable même sans vidéo. */}
      {callPeer !== null && !inVoiceRoom && shown.length === 0 && (
        <span className="sr-only">{nameOf(callPeer)}</span>
      )}
    </div>
  );
}
