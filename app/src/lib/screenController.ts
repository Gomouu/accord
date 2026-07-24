/**
 * Coordination du partage d'écran (v5) : relie la capture locale à l'API
 * `screen.*` du nœud, et les trames distantes reçues (`event.screen_frame`) au
 * rendu sur canevas. Détient les instances uniques de capture et de
 * visionneuse ; le store `stores/calls.ts` n'expose que des booléens d'état.
 */

import { api } from './client';
import {
  bytesToHex,
  hexToBytes,
  ScreenCapture,
  ScreenPlayback,
  screenShareSupported,
} from './screenShare';

const capture = new ScreenCapture();

/** Visionneuse du partage distant (le composant lui lie son canevas). */
export const remotePlayback = new ScreenPlayback();

export { screenShareSupported };

/**
 * Démarre le partage d'écran local : annonce au pair, puis capture → envoi des
 * trames. `onEnded` est invoqué quand la capture s'arrête d'elle-même
 * (utilisateur via l'UI système, ou erreur d'encodage). Rejette si
 * l'utilisateur refuse le partage ou si le runtime ne le supporte pas.
 */
export async function startLocalShare(onEnded: () => void): Promise<void> {
  await api.screenStart().catch(() => {});
  try {
    await capture.start(
      ({ keyframe, bytes }) => {
        // Fire-and-forget : la contre-pression vient de la file d'encodage.
        void api.screenFrame(keyframe, bytesToHex(bytes)).catch(() => {});
      },
      () => {
        capture.stop();
        void api.screenStop().catch(() => {});
        onEnded();
      },
    );
  } catch (error) {
    capture.stop();
    await api.screenStop().catch(() => {});
    throw error;
  }
}

/** Arrête le partage d'écran local (capture + annonce d'arrêt au pair). */
export async function stopLocalShare(): Promise<void> {
  capture.stop();
  await api.screenStop().catch(() => {});
}

/** Vrai si une capture locale est en cours. */
export function isLocalShareActive(): boolean {
  return capture.active;
}

/** Trame de partage distant reçue : décodée et peinte sur la visionneuse. */
export function pushRemoteFrame(keyframe: boolean, dataHex: string): void {
  const bytes = hexToBytes(dataHex);
  if (bytes !== null) remotePlayback.push(keyframe, bytes);
}

/** Fin de partage distant (ou d'appel) : réinitialise le décodeur. */
export function resetRemote(): void {
  remotePlayback.reset();
}
