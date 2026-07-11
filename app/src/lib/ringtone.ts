/**
 * Sonnerie d'appel entrant : deux notes courtes répétées toutes les ~2 s tant
 * que `stopRingtone` n'est pas appelé — même schéma Web Audio que
 * `notificationSound.ts` (contexte créé au premier besoin, jamais au chargement
 * du module, réactivation au premier geste utilisateur si démarré suspendu).
 * Contrairement au blip de notification, jouée en boucle explicite (pas de
 * limitation de fréquence — elle DOIT sonner tant que l'appel sonne) et gardée
 * par la préférence de son ET l'absence de Ne pas déranger (l'appelant voit
 * toujours l'overlay d'appel entrant en DND, seul le son est coupé — voir
 * `IncomingCall.tsx`). Toute défaillance est silencieuse : une sonnerie
 * manquée ne doit jamais empêcher de répondre à l'appel.
 */

import { useFriends } from '../stores/friends';
import { useUi } from '../stores/ui';

const RING_INTERVAL_MS = 2000;
const NOTE_DURATION_S = 0.32;
const NOTE_GAP_S = 0.4;

let ctx: AudioContext | null = null;
let unlockArmed = false;
let ringTimer: ReturnType<typeof setInterval> | null = null;

/** Constructeur `AudioContext` disponible (préfixé Safari inclus), ou `null`. */
function resolveAudioContextCtor(): typeof AudioContext | null {
  const w = window as unknown as {
    AudioContext?: typeof AudioContext;
    webkitAudioContext?: typeof AudioContext;
  };
  return w.AudioContext ?? w.webkitAudioContext ?? null;
}

/** Contexte partagé, créé au premier besoin ; `null` si l'API est indisponible. */
function ensureContext(): AudioContext | null {
  if (ctx !== null) return ctx;
  const Ctor = resolveAudioContextCtor();
  if (Ctor === null) return null;
  try {
    ctx = new Ctor();
  } catch {
    ctx = null;
  }
  return ctx;
}

/** Réarme la reprise du contexte suspendu au prochain geste utilisateur. */
function armAutoplayUnlock(context: AudioContext): void {
  if (unlockArmed) return;
  unlockArmed = true;
  const resume = (): void => {
    unlockArmed = false;
    context.resume().catch(() => {
      // Best effort : reste suspendu jusqu'au prochain geste.
    });
  };
  window.addEventListener('pointerdown', resume, { once: true });
  window.addEventListener('keydown', resume, { once: true });
}

/** Joue une note (sinusoïde, enveloppe attaque/chute courte) à `startAt`. */
function playTone(context: AudioContext, freq: number, startAt: number): void {
  const osc = context.createOscillator();
  const gain = context.createGain();
  osc.type = 'sine';
  osc.frequency.value = freq;
  gain.gain.setValueAtTime(0, startAt);
  gain.gain.linearRampToValueAtTime(0.22, startAt + 0.02);
  gain.gain.exponentialRampToValueAtTime(0.0001, startAt + NOTE_DURATION_S);
  osc.connect(gain);
  gain.connect(context.destination);
  osc.start(startAt);
  osc.stop(startAt + NOTE_DURATION_S + 0.02);
}

/** Un cycle de sonnerie : deux notes (façon téléphone), C5 puis E5. */
function playRingOnce(context: AudioContext): void {
  const start = context.currentTime;
  playTone(context, 523.25, start);
  playTone(context, 659.25, start + NOTE_GAP_S);
}

/** Vrai si le son est autorisé : préférence utilisateur ET pas de Ne pas déranger. */
function ringtoneAllowed(): boolean {
  return useUi.getState().notifySoundEnabled && useFriends.getState().ownStatus !== 'dnd';
}

/**
 * Démarre la sonnerie en boucle (no-op si déjà en cours, si le son de
 * notification est coupé, ou en Ne pas déranger). Idempotent : plusieurs
 * appels successifs ne cumulent pas les minuteurs.
 */
export function startRingtone(): void {
  if (ringTimer !== null) return;
  if (!ringtoneAllowed()) return;
  try {
    const context = ensureContext();
    if (context === null) return;
    if (context.state === 'suspended') {
      armAutoplayUnlock(context);
      void context.resume().catch(() => {
        // Best effort : armAutoplayUnlock couvre le prochain geste.
      });
    }
    playRingOnce(context);
    ringTimer = setInterval(() => {
      const running = ensureContext();
      if (running !== null) playRingOnce(running);
    }, RING_INTERVAL_MS);
  } catch {
    // Best effort : une sonnerie manquée ne doit jamais bloquer l'appel.
  }
}

/** Arrête la sonnerie immédiatement (no-op si déjà arrêtée). */
export function stopRingtone(): void {
  if (ringTimer === null) return;
  clearInterval(ringTimer);
  ringTimer = null;
}
