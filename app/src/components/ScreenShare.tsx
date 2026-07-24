/**
 * Partage d'écran (v5) : panneau flottant affiché pendant un appel 1-à-1
 * actif. Bouton de partage (démarrer/arrêter), visionneuse du partage distant
 * (canevas alimenté par le décodeur WebCodecs), et indicateurs d'état. Le
 * bouton se désactive proprement quand le runtime ne supporte pas la capture.
 */

import { useEffect, useMemo, useRef } from 'react';
import { remotePlayback, screenShareSupported } from '../lib/screenController';
import { useCalls } from '../stores/calls';
import { useT } from '../stores/ui';

/** Petite icône « écran » (moniteur). */
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

export function ScreenShare() {
  const t = useT();
  const phase = useCalls((s) => s.phase);
  const localSharing = useCalls((s) => s.localSharing);
  const remoteSharing = useCalls((s) => s.remoteSharing);
  const startScreenShare = useCalls((s) => s.startScreenShare);
  const stopScreenShare = useCalls((s) => s.stopScreenShare);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const supported = useMemo(() => screenShareSupported(), []);

  // Lie le canevas de la visionneuse au décodeur dès qu'un partage distant
  // apparaît (le décodeur peint dessus les trames reçues).
  useEffect(() => {
    const canvas = canvasRef.current;
    if (remoteSharing && canvas !== null) {
      remotePlayback.attach(canvas);
    }
  }, [remoteSharing]);

  if (phase !== 'active') return null;

  const toggle = (): void => {
    if (localSharing) {
      void stopScreenShare();
    } else {
      // L'utilisateur peut refuser le sélecteur système : échec silencieux.
      void startScreenShare().catch(() => {});
    }
  };

  return (
    <div className="pointer-events-none fixed inset-x-0 bottom-24 z-30 flex flex-col items-center gap-3">
      {remoteSharing && (
        <div className="pointer-events-auto overflow-hidden rounded-xl border border-rail/60 bg-black shadow-3">
          <div className="flex items-center justify-between gap-4 bg-modal px-3 py-1.5 text-xs font-medium text-muted">
            <span>{t.screenShare.viewerLabel}</span>
            <span className="flex items-center gap-1.5 text-red">
              <span className="h-2 w-2 rounded-full bg-red" aria-hidden="true" />
              {t.screenShare.peerSharing}
            </span>
          </div>
          <canvas
            ref={canvasRef}
            aria-label={t.screenShare.viewerLabel}
            className="block h-auto max-h-[42vh] w-[min(640px,80vw)] bg-black"
          />
        </div>
      )}
      <button
        type="button"
        onClick={toggle}
        disabled={!supported}
        aria-pressed={localSharing}
        {...(supported ? {} : { title: t.screenShare.unsupported })}
        className={`pointer-events-auto inline-flex min-h-9 items-center gap-2 rounded-full px-4 py-2 text-sm font-medium shadow-2 transition-colors duration-fast focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-chat disabled:cursor-not-allowed disabled:opacity-50 ${
          localSharing
            ? 'bg-red text-white hover:bg-red/90'
            : 'bg-blurple text-white hover:bg-blurple/90'
        }`}
      >
        <ScreenIcon />
        {localSharing ? t.screenShare.stop : t.screenShare.start}
      </button>
    </div>
  );
}
