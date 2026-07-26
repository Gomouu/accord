/**
 * Coordination des flux vidéo (partage d'écran v5, caméra v6) : relie les
 * captures locales aux API `screen.*`/`camera.*` du nœud, et les trames
 * distantes reçues au rendu sur canevas.
 *
 * Les visionneuses sont indexées **par pair et par source**. En appel 1-à-1 il
 * n'y en a qu'une de chaque ; en salon vocal de groupe, plusieurs personnes
 * peuvent diffuser en même temps, et une visionneuse unique par source ferait
 * s'écraser leurs images l'une l'autre — le décodeur recevrait des trames de
 * deux flux entrelacées et ne produirait que du bruit. Le nœud, lui, sait
 * depuis toujours qui envoie quoi (`event.*_frame` porte `peer`).
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

/** Visionneuses distantes, indexées `pair:source`. */
const playbacks = new Map<string, VideoPlayback>();

const keyOf = (peer: string, source: VideoSource): string => `${peer}:${source}`;

export { videoSourceSupported };
export type { VideoSource };

/**
 * Visionneuse d'un flux distant, créée à la demande. Le composant lui lie son
 * canevas ; la visionneuse survit au démontage du canevas (changement de mise
 * en page, épinglage) sans perdre son décodeur.
 */
export function playbackFor(peer: string, source: VideoSource): VideoPlayback {
  const key = keyOf(peer, source);
  let playback = playbacks.get(key);
  if (playback === undefined) {
    playback = new VideoPlayback();
    playbacks.set(key, playback);
  }
  return playback;
}

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

/** Trame distante reçue : décodée et peinte sur la visionneuse de ce pair. */
export function pushRemoteFrame(
  peer: string,
  source: VideoSource,
  keyframe: boolean,
  dataHex: string,
): void {
  const bytes = hexToBytes(dataHex);
  if (bytes !== null) playbackFor(peer, source).push(keyframe, bytes);
}

/** Ce qu'un pair nous envoie et que l'on n'affiche pas, tel que déclaré. */
export interface HiddenStreams {
  peer: string;
  streams: VideoSource[];
}

/**
 * Dernière déclaration transmise au nœud, sérialisée. La grille se re-rend à
 * chaque trame reçue (12–24 fois par seconde) : sans ce garde, on enverrait
 * une requête RPC par image, ce qui coûterait plus cher que le trafic
 * économisé. Le nœud filtre lui aussi les non-changements — ceci évite en
 * amont l'aller-retour.
 */
let lastDeclared = '';

const declarationKey = (hidden: readonly HiddenStreams[]): string =>
  hidden
    .map(({ peer, streams }) => `${peer}:${[...streams].sort().join(',')}`)
    .sort()
    .join('|');

/**
 * Vidéo sélective : déclare aux émetteurs ce que l'on n'affiche pas d'eux.
 *
 * Sémantique NÉGATIVE de bout en bout : ce qui n'est pas listé continue
 * d'arriver. Un flux qui vient d'apparaître n'est jamais dans la liste (l'UI ne
 * peut pas masquer ce qu'elle ignore), donc il arrive tout de suite — la
 * première image ne paie aucun aller-retour.
 */
export function declareHidden(hidden: readonly HiddenStreams[]): void {
  const key = declarationKey(hidden);
  if (key === lastDeclared) return;
  lastDeclared = key;
  // Best-effort : un échec laisse l'émetteur envoyer tout, comme avant.
  void api.videoInterest(hidden).catch(() => {
    // La déclaration repartira au prochain changement d'affichage.
    lastDeclared = '';
  });
}

/**
 * Oublie la dernière déclaration (fin de session). Le moteur oublie ses masques
 * en quittant : sans ce reset, l'UI croirait avoir déjà déclaré et le nœud
 * resterait à « rien de masqué ». Sans conséquence visible — c'est l'ancien
 * comportement, tout le monde reçoit tout — mais l'économie serait perdue
 * jusqu'au prochain changement d'affichage.
 */
export function resetDeclaration(): void {
  lastDeclared = '';
}

/** Fin d'un flux distant : ferme et oublie la visionneuse concernée. */
export function resetRemote(peer: string, source: VideoSource): void {
  const key = keyOf(peer, source);
  const playback = playbacks.get(key);
  if (playback === undefined) return;
  playback.close();
  playbacks.delete(key);
}

/** Départ d'un participant : ferme toutes ses visionneuses. */
export function resetPeer(peer: string): void {
  resetRemote(peer, 'screen');
  resetRemote(peer, 'camera');
}

/** Fin d'appel : ferme toutes les visionneuses distantes. */
export function resetAllRemote(): void {
  for (const playback of playbacks.values()) playback.close();
  playbacks.clear();
  resetDeclaration();
}

/** Fin d'appel : coupe toute capture locale encore active. */
export async function stopAllLocal(): Promise<void> {
  await Promise.all([
    captures.screen.active ? stopLocalStream('screen') : Promise.resolve(),
    captures.camera.active ? stopLocalStream('camera') : Promise.resolve(),
  ]);
}
