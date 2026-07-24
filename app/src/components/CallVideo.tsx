/**
 * Vidéo d'appel (v5 partage d'écran, v6 caméra) : panneau flottant affiché
 * pendant un appel 1-à-1 actif. Deux boutons (caméra, écran), la vue du flux
 * distant sur canevas (alimentée par les décodeurs WebCodecs), et l'aperçu
 * local de sa propre caméra en incrustation. Chaque bouton se désactive
 * proprement quand le runtime ne supporte pas la source correspondante.
 */

import { useEffect, useMemo, useRef } from 'react';
import {
  localPreviewStream,
  remotePlayback,
  videoSourceSupported,
  type VideoSource,
} from '../lib/mediaController';
import { useCalls } from '../stores/calls';
import { useT } from '../stores/ui';

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

/** Canevas d'un flux distant, lié à sa visionneuse tant qu'il est actif. */
function RemoteView({
  source,
  label,
  badge,
}: {
  source: VideoSource;
  label: string;
  badge: string;
}) {
  const canvasRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (canvas !== null) remotePlayback[source].attach(canvas);
  }, [source]);

  return (
    <div className="pointer-events-auto overflow-hidden rounded-xl border border-rail/60 bg-black shadow-3">
      <div className="flex items-center justify-between gap-4 bg-modal px-3 py-1.5 text-xs font-medium text-muted">
        <span>{label}</span>
        <span className="flex items-center gap-1.5 text-red">
          <span className="h-2 w-2 rounded-full bg-red" aria-hidden="true" />
          {badge}
        </span>
      </div>
      <canvas
        ref={canvasRef}
        aria-label={label}
        className="block h-auto max-h-[42vh] w-[min(640px,80vw)] bg-black"
      />
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
  const localSharing = useCalls((s) => s.localSharing);
  const remoteSharing = useCalls((s) => s.remoteSharing);
  const localCamera = useCalls((s) => s.localCamera);
  const remoteCamera = useCalls((s) => s.remoteCamera);
  const startScreenShare = useCalls((s) => s.startScreenShare);
  const stopScreenShare = useCalls((s) => s.stopScreenShare);
  const startCamera = useCalls((s) => s.startCamera);
  const stopCamera = useCalls((s) => s.stopCamera);

  const screenOk = useMemo(() => videoSourceSupported('screen'), []);
  const cameraOk = useMemo(() => videoSourceSupported('camera'), []);

  if (phase !== 'active') return null;

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

  const buttonClass = (on: boolean): string =>
    `pointer-events-auto inline-flex min-h-9 items-center gap-2 rounded-full px-4 py-2 text-sm font-medium shadow-2 transition-colors duration-fast focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-chat disabled:cursor-not-allowed disabled:opacity-50 ${
      on
        ? 'bg-red text-white hover:bg-red/90'
        : 'bg-blurple text-white hover:bg-blurple/90'
    }`;

  return (
    <div className="pointer-events-none fixed inset-x-0 bottom-24 z-30 flex flex-col items-center gap-3">
      {remoteCamera && (
        <RemoteView
          source="camera"
          label={t.callVideo.remoteCameraLabel}
          badge={t.callVideo.peerCamera}
        />
      )}
      {remoteSharing && (
        <RemoteView
          source="screen"
          label={t.screenShare.viewerLabel}
          badge={t.screenShare.peerSharing}
        />
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
    </div>
  );
}
