/**
 * Flux vidéo d'un appel : partage d'écran (v5) et caméra (v6). Capture via
 * `getDisplayMedia`/`getUserMedia`, encodage/décodage WebCodecs (VP8),
 * transport par les API `screen.*`/`camera.*` du nœud (session P2P chiffrée,
 * sans serveur). Tout est feature-détecté — sur un runtime sans les API
 * nécessaires, la source se signale indisponible plutôt que d'échouer.
 *
 * Capture : le flux alimente une balise `<video>` hors-DOM ; chaque image est
 * peinte sur un canevas puis encodée (`VideoFrame` → `VideoEncoder`). La boucle
 * suit `requestVideoFrameCallback` quand il existe (cadence réelle de la
 * source), sinon `requestAnimationFrame`. Débit et cadence sont réglés par
 * source (l'écran privilégie la netteté du texte, la caméra la fluidité) et
 * restent modérés pour tenir la fragmentation UDP côté nœud.
 *
 * Rendu : les trames encodées reçues alimentent un `VideoDecoder` dont la
 * sortie est peinte sur le canevas de la visionneuse. Le décodage ne démarre
 * qu'à la première keyframe.
 */

/** Source d'un flux vidéo d'appel. */
export type VideoSource = 'screen' | 'camera';

/** Codec temps réel (largement supporté, pas de description requise). */
const CODEC = 'vp8';

/** Réglages d'encodage propres à chaque source. */
const PROFILES: Record<
  VideoSource,
  { fps: number; bitrate: number; keyframeEvery: number; width: number; height: number }
> = {
  // Écran : cadence modérée, débit plus généreux (le texte doit rester net).
  screen: { fps: 12, bitrate: 1_500_000, keyframeEvery: 60, width: 1280, height: 720 },
  // Caméra : plus fluide, plus petite définition — un visage tolère mal les
  // saccades mais se contente largement du VGA.
  camera: { fps: 24, bitrate: 700_000, keyframeEvery: 48, width: 640, height: 480 },
};

/** Profondeur de file d'encodage tolérée avant de sauter une image (latence). */
const MAX_ENCODE_QUEUE = 2;

/** Trame vidéo encodée, prête à transporter. */
export interface VideoChunk {
  /** Image décodable indépendamment (keyframe). */
  keyframe: boolean;
  /** Octets encodés. */
  bytes: Uint8Array;
}

/** Vrai si le runtime sait encoder ET décoder de la vidéo (WebCodecs). */
function codecsSupported(): boolean {
  return (
    typeof window !== 'undefined' &&
    typeof window.VideoEncoder === 'function' &&
    typeof window.VideoDecoder === 'function' &&
    typeof window.VideoFrame === 'function'
  );
}

/**
 * Vrai si le runtime sait capturer `source` et l'encoder/décoder. Le bouton
 * correspondant se désactive proprement sinon.
 */
export function videoSourceSupported(source: VideoSource): boolean {
  if (!codecsSupported()) return false;
  if (typeof navigator === 'undefined' || navigator.mediaDevices == null) return false;
  return source === 'screen'
    ? typeof navigator.mediaDevices.getDisplayMedia === 'function'
    : typeof navigator.mediaDevices.getUserMedia === 'function';
}

/** Octets → chaîne hexadécimale (transport JSON de l'API locale). */
export function bytesToHex(bytes: Uint8Array): string {
  let out = '';
  for (const b of bytes) out += b.toString(16).padStart(2, '0');
  return out;
}

/** Chaîne hexadécimale → octets (`null` si longueur impaire ou caractère invalide). */
export function hexToBytes(hex: string): Uint8Array | null {
  if (hex.length % 2 !== 0) return null;
  const out = new Uint8Array(hex.length / 2);
  for (let i = 0; i < out.length; i++) {
    const byte = Number.parseInt(hex.slice(i * 2, i * 2 + 2), 16);
    if (Number.isNaN(byte)) return null;
    out[i] = byte;
  }
  return out;
}

/** Arrondit à l'entier pair inférieur (dimensions d'encodage). */
function even(value: number): number {
  const rounded = Math.max(2, Math.floor(value));
  return rounded - (rounded % 2);
}

/**
 * Capture et encodage d'une source vidéo locale. `start` ouvre le sélecteur
 * système (écran) ou la caméra, puis émet chaque trame encodée via `onChunk` ;
 * `onEnded` est appelé quand la capture s'arrête d'elle-même (arrêt depuis
 * l'UI du système, débranchement, erreur d'encodage).
 */
export class VideoCapture {
  private mediaStream: MediaStream | null = null;
  private encoder: VideoEncoder | null = null;
  private video: HTMLVideoElement | null = null;
  private canvas: HTMLCanvasElement | null = null;
  private ctx: CanvasRenderingContext2D | null = null;
  private running = false;
  private frameCount = 0;
  private keyframeEvery = 60;
  private onChunk: ((chunk: VideoChunk) => void) | null = null;

  /** Vrai tant qu'une capture est active. */
  get active(): boolean {
    return this.running;
  }

  /**
   * Flux capturé, pour un aperçu local (self-view de la caméra). `null` hors
   * capture.
   */
  get stream(): MediaStream | null {
    return this.mediaStream;
  }

  /**
   * Démarre la capture de `source`. Rejette si le runtime ne la supporte pas,
   * si l'utilisateur refuse l'accès, ou si l'encodeur ne peut s'initialiser.
   */
  async start(
    source: VideoSource,
    onChunk: (chunk: VideoChunk) => void,
    onEnded: () => void,
  ): Promise<void> {
    if (!videoSourceSupported(source)) {
      throw new Error('source vidéo non supportée par ce runtime');
    }
    const profile = PROFILES[source];
    this.keyframeEvery = profile.keyframeEvery;
    this.onChunk = onChunk;

    const stream =
      source === 'screen'
        ? await navigator.mediaDevices.getDisplayMedia({
            video: { frameRate: profile.fps },
            audio: false,
          })
        : await navigator.mediaDevices.getUserMedia({
            video: {
              frameRate: profile.fps,
              width: { ideal: profile.width },
              height: { ideal: profile.height },
            },
            audio: false,
          });
    this.mediaStream = stream;

    const track = stream.getVideoTracks()[0];
    if (track === undefined) {
      this.stop();
      throw new Error('aucune piste vidéo dans le flux capturé');
    }
    track.addEventListener('ended', onEnded, { once: true });

    const encoder = new VideoEncoder({
      output: (chunk) => this.emit(chunk),
      error: () => onEnded(),
    });
    this.encoder = encoder;

    const settings = track.getSettings();
    const width = even(settings.width ?? profile.width);
    const height = even(settings.height ?? profile.height);
    encoder.configure({
      codec: CODEC,
      width,
      height,
      bitrate: profile.bitrate,
      framerate: profile.fps,
      latencyMode: 'realtime',
    });

    const video = document.createElement('video');
    video.srcObject = stream;
    video.muted = true;
    video.playsInline = true;
    await video.play();
    this.video = video;

    const canvas = document.createElement('canvas');
    canvas.width = width;
    canvas.height = height;
    const ctx = canvas.getContext('2d');
    if (ctx === null) {
      this.stop();
      throw new Error('canevas 2D indisponible');
    }
    this.canvas = canvas;
    this.ctx = ctx;

    this.running = true;
    this.schedule();
  }

  /** Arrête la capture et libère la piste, l'encodeur et le flux. */
  stop(): void {
    this.running = false;
    this.onChunk = null;
    if (this.encoder !== null && this.encoder.state !== 'closed') {
      this.encoder.close();
    }
    this.encoder = null;
    if (this.mediaStream !== null) {
      for (const track of this.mediaStream.getTracks()) track.stop();
    }
    this.mediaStream = null;
    if (this.video !== null) {
      this.video.srcObject = null;
    }
    this.video = null;
    this.canvas = null;
    this.ctx = null;
    this.frameCount = 0;
  }

  /** Convertit une trame encodée en octets et la remonte. */
  private emit(chunk: EncodedVideoChunk): void {
    const onChunk = this.onChunk;
    if (onChunk === null) return;
    const bytes = new Uint8Array(chunk.byteLength);
    chunk.copyTo(bytes);
    onChunk({ keyframe: chunk.type === 'key', bytes });
  }

  /** Programme la prochaine capture d'image (cadence réelle si disponible). */
  private schedule(): void {
    const video = this.video;
    if (!this.running || video === null) return;
    if (typeof video.requestVideoFrameCallback === 'function') {
      video.requestVideoFrameCallback(this.tick);
    } else {
      requestAnimationFrame(this.tick);
    }
  }

  /** Peint l'image courante puis l'encode (sauf contre-pression). */
  private tick = (): void => {
    const { video, ctx, canvas, encoder } = this;
    if (
      !this.running ||
      video === null ||
      ctx === null ||
      canvas === null ||
      encoder === null
    ) {
      return;
    }
    if (encoder.state === 'configured') {
      ctx.drawImage(video, 0, 0, canvas.width, canvas.height);
      if (encoder.encodeQueueSize < MAX_ENCODE_QUEUE) {
        const frame = new VideoFrame(canvas, {
          timestamp: Math.round(performance.now() * 1000),
        });
        encoder.encode(frame, { keyFrame: this.frameCount % this.keyframeEvery === 0 });
        frame.close();
        this.frameCount += 1;
      }
    }
    this.schedule();
  };
}

/**
 * Décodage et rendu d'un flux vidéo distant sur un canevas. Alimenté par
 * `push` (une trame encodée à la fois), il ne démarre qu'à la première
 * keyframe et se réinitialise proprement sur erreur de décodage.
 */
export class VideoPlayback {
  private decoder: VideoDecoder | null = null;
  private canvas: HTMLCanvasElement | null = null;
  private ctx: CanvasRenderingContext2D | null = null;
  private configured = false;

  /** Lie la visionneuse à son canevas de rendu. */
  attach(canvas: HTMLCanvasElement): void {
    this.canvas = canvas;
    this.ctx = canvas.getContext('2d');
  }

  /** Ingère une trame encodée reçue (débute au premier keyframe). */
  push(keyframe: boolean, bytes: Uint8Array): void {
    const decoder = this.ensureDecoder();
    if (decoder === null) return;
    if (!this.configured) {
      // On attend une keyframe pour amorcer le décodage.
      if (!keyframe) return;
      decoder.configure({ codec: CODEC, optimizeForLatency: true });
      this.configured = true;
    }
    const chunk = new EncodedVideoChunk({
      type: keyframe ? 'key' : 'delta',
      timestamp: Math.round(performance.now() * 1000),
      data: bytes,
    });
    try {
      decoder.decode(chunk);
    } catch {
      this.reset();
    }
  }

  /** Réinitialise le décodeur (prochaine keyframe requise pour repartir). */
  reset(): void {
    if (this.decoder !== null && this.decoder.state !== 'closed') {
      this.decoder.close();
    }
    this.decoder = null;
    this.configured = false;
  }

  /** Détache et libère tout (fin d'appel / fin de flux distant). */
  close(): void {
    this.reset();
    this.canvas = null;
    this.ctx = null;
  }

  /** Crée le décodeur à la demande (`null` si WebCodecs indisponible). */
  private ensureDecoder(): VideoDecoder | null {
    if (this.decoder !== null) return this.decoder;
    if (typeof window === 'undefined' || typeof window.VideoDecoder !== 'function') {
      return null;
    }
    const decoder = new VideoDecoder({
      output: (frame) => this.render(frame),
      error: () => this.reset(),
    });
    this.decoder = decoder;
    return decoder;
  }

  /** Peint une image décodée sur le canevas puis la libère. */
  private render(frame: VideoFrame): void {
    const canvas = this.canvas;
    const ctx = this.ctx;
    if (canvas !== null && ctx !== null) {
      if (canvas.width !== frame.displayWidth) canvas.width = frame.displayWidth;
      if (canvas.height !== frame.displayHeight) canvas.height = frame.displayHeight;
      ctx.drawImage(frame, 0, 0);
    }
    frame.close();
  }
}
