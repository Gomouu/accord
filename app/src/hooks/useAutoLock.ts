/**
 * Verrouillage automatique après inactivité.
 *
 * Le coffre se referme tout seul au bout de N minutes sans activité, comme le
 * ferait un gestionnaire de mots de passe. Utile quand on quitte son bureau :
 * l'application reste ouverte, mais les messages ne sont plus lisibles sans la
 * phrase de passe.
 *
 * La logique de décision (`createIdleController`) est pure et testée en
 * isolation ; le hook ne fait que la câbler aux événements du navigateur, au
 * store d'interface et au verrouillage réel.
 */

import { useEffect } from 'react';
import { useSession } from '../stores/session';
import { useUi } from '../stores/ui';
import { useVoice } from '../stores/voice';

/** Effets requis par le contrôleur (injectables pour les tests). */
export interface IdleEffects {
  /** Verrouille le coffre. */
  lock: () => void;
  /** Horloge murale en millisecondes. */
  now: () => number;
}

/** Contrôleur d'inactivité, indépendant du DOM. */
export interface IdleController {
  /** Signale une activité de l'utilisateur (frappe, clic, molette). */
  activity: () => void;
  /**
   * Vérifie l'échéance. Rend `true` si le verrouillage a été déclenché.
   *
   * `blocked` couvre tout ce qui doit surseoir : coffre déjà fermé, salon
   * vocal en cours. Verrouiller pendant un appel serait hostile — l'utilisateur
   * est présent, il ne touche simplement pas au clavier.
   */
  tick: (blocked: boolean) => boolean;
}

/**
 * Crée un contrôleur qui verrouille après `timeoutMs` d'inactivité.
 * `timeoutMs <= 0` désactive le mécanisme.
 */
export function createIdleController(
  timeoutMs: number,
  effects: IdleEffects,
): IdleController {
  let last = effects.now();
  return {
    activity: () => {
      last = effects.now();
    },
    tick: (blocked) => {
      if (timeoutMs <= 0) return false;
      if (blocked) {
        // Le compteur repart de zéro : à la fin de l'appel, l'utilisateur
        // dispose du délai complet, il ne se fait pas éjecter aussitôt.
        last = effects.now();
        return false;
      }
      if (effects.now() - last < timeoutMs) return false;
      last = effects.now();
      effects.lock();
      return true;
    },
  };
}

/** Fréquence de vérification de l'échéance. */
const CHECK_INTERVAL_MS = 15_000;

/** Événements comptant comme une activité de l'utilisateur. */
const ACTIVITY_EVENTS = ['pointerdown', 'keydown', 'wheel', 'touchstart'] as const;

/**
 * Câble le verrouillage automatique. Sans effet quand le réglage vaut 0
 * (désactivé, valeur par défaut).
 */
export function useAutoLock(): void {
  const minutes = useUi((s) => s.autoLockMinutes);

  useEffect(() => {
    if (minutes <= 0) return;
    const controller = createIdleController(minutes * 60_000, {
      lock: () => {
        void useSession.getState().lock();
      },
      now: () => Date.now(),
    });
    const onActivity = (): void => controller.activity();
    for (const name of ACTIVITY_EVENTS) {
      window.addEventListener(name, onActivity, { passive: true });
    }
    const timer = setInterval(() => {
      const phase = useSession.getState().phase;
      const inVoice = useVoice.getState().active !== null;
      // Rien à verrouiller hors session ouverte ; rien à interrompre en appel.
      const blocked = (phase !== 'ready' && phase !== 'offline') || inVoice;
      controller.tick(blocked);
    }, CHECK_INTERVAL_MS);
    return () => {
      for (const name of ACTIVITY_EVENTS) {
        window.removeEventListener(name, onActivity);
      }
      clearInterval(timer);
    };
  }, [minutes]);
}
