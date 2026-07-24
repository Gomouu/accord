/**
 * Coordination des flux vidéo d'un appel (partage d'écran v5, caméra v6) :
 * relie les captures locales aux API `screen.*`/`camera.*` du nœud, et les
 * trames distantes reçues au rendu sur canevas. Détient les instances uniques
 * de capture et de visionneuse ; le store `stores/calls.ts` n'expose que des
 * booléens d'état.
 */

import { api } from './client';
import {
  bytesToHex,
  hexToBytes,
  VideoCapture,
  VideoPlayback,
  videoSourceSupported,
  type VideoSource,
} from './videoStream';

const captures: Record<VideoSource, VideoCapture> = {
  screen: new VideoCapture(),
  camera: new VideoCapture(),
};

/** Visionneuses des flux distants (le composant leur lie son canevas). */
export const remotePlayback: Record<VideoSource, VideoPlayback> = {
  screen: new VideoPlayback(),
  camera: new VideoPlayback(),
};

export { videoSourceSupported };
export type { VideoSource };

/** Annonce le démarrage d'un flux au pair (best-effort). */
function announceStart(source: VideoSource): Promise<unknown> {
  return source === 'screen' ? api.screenStart() : api.cameraStart();
}

/** Annonce l'arrêt d'un flux au pair (best-effort). */
function announceStop(source: VideoSource): Promise<unknown> {
  return source === 'screen' ? api.screenStop() : api.cameraStop();
}

/** Envoie une trame encodée sur le flux voulu. */
function sendFrame(
  source: VideoSource,
  keyframe: boolean,
  hex: string,
): Promise<unknown> {
  return source === 'screen'
    ? api.screenFrame(keyframe, hex)
    : api.cameraFrame(keyframe, hex);
}

/**
 * Démarre un flux vidéo local : annonce au pair, puis capture → envoi des
 * trames. `onEnded` est invoqué quand la capture s'arrête d'elle-même
 * (utilisateur via l'UI système, débranchement, erreur d'encodage). Rejette si
 * l'utilisateur refuse l'accès ou si le runtime ne supporte pas la source.
 */
export async function startLocalStream(
  source: VideoSource,
  onEnded: () => void,
): Promise<void> {
  await announceStart(source).catch(() => {});
  try {
    await captures[source].start(
      source,
      ({ keyframe, bytes }) => {
        // Fire-and-forget : la contre-pression vient de la file d'encodage.
        void sendFrame(source, keyframe, bytesToHex(bytes)).catch(() => {});
      },
      () => {
        captures[source].stop();
        void announceStop(source).catch(() => {});
        onEnded();
      },
    );
  } catch (error) {
    captures[source].stop();
    await announceStop(source).catch(() => {});
    throw error;
  }
}

/** Arrête un flux vidéo local (capture + annonce d'arrêt au pair). */
export async function stopLocalStream(source: VideoSource): Promise<void> {
  captures[source].stop();
  await announceStop(source).catch(() => {});
}

/** Vrai si la capture locale de `source` est en cours. */
export function isLocalStreamActive(source: VideoSource): boolean {
  return captures[source].active;
}

/**
 * Flux capturé localement, pour l'aperçu (self-view de la caméra). `null` hors
 * capture.
 */
export function localPreviewStream(source: VideoSource): MediaStream | null {
  return captures[source].stream;
}

/** Trame distante reçue : décodée et peinte sur la visionneuse du flux. */
export function pushRemoteFrame(
  source: VideoSource,
  keyframe: boolean,
  dataHex: string,
): void {
  const bytes = hexToBytes(dataHex);
  if (bytes !== null) remotePlayback[source].push(keyframe, bytes);
}

/** Fin d'un flux distant : réinitialise le décodeur concerné. */
export function resetRemote(source: VideoSource): void {
  remotePlayback[source].reset();
}

/** Fin d'appel : réinitialise les deux visionneuses. */
export function resetAllRemote(): void {
  remotePlayback.screen.reset();
  remotePlayback.camera.reset();
}

/** Fin d'appel : coupe toute capture locale encore active. */
export async function stopAllLocal(): Promise<void> {
  await Promise.all([
    captures.screen.active ? stopLocalStream('screen') : Promise.resolve(),
    captures.camera.active ? stopLocalStream('camera') : Promise.resolve(),
  ]);
}
