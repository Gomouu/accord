/**
 * Soundboard : lecture des clips audio. Deux chemins convergent vers le même
 * `playSound` best-effort :
 * - l'émetteur joue le son localement dès le clic (feedback immédiat) ;
 * - les récepteurs présents dans le vocal reçoivent `event.soundboard_play`
 *   et jouent le clip désigné par sa racine Merkle.
 *
 * La lecture est best-effort : un autoplay refusé par la WKWebView ou un
 * fichier indisponible est silencieusement ignoré (jamais d'erreur remontée).
 * Câblé au chargement du module comme `stores/groups.ts` (garde d'environnement
 * pour les tests qui simulent `../lib/client` sans `rpc.onEvent`).
 */

import { rpc } from '../lib/client';
import { lireFichier } from '../lib/files';
import { useSession } from './session';
import { useVoice } from './voice';

/**
 * Joue un clip de soundboard par sa racine Merkle (best-effort, silencieux en
 * cas d'échec). `hint` : clé publique d'un pair source probable (l'émetteur),
 * utilisée pour amorcer le téléchargement si le clip n'est pas encore local.
 */
export function playSound(merkleRoot: string, hint?: string): void {
  lireFichier(merkleRoot, hint)
    .then((url) => new Audio(url).play())
    .catch(() => {
      // Best effort : autoplay refusé, fichier indisponible ou lecture avortée.
    });
}

/**
 * Applique `event.soundboard_play` : joue le clip reçu si — et seulement si —
 * on est bien dans le salon vocal concerné. Le nœud filtre déjà côté réseau ;
 * cette vérification légère évite toute lecture parasite si un événement
 * traîne. L'émetteur ayant déjà joué le son localement, on ignore l'écho de
 * sa propre émission pour ne pas le jouer deux fois.
 */
export function handleSoundboardEvent(method: string, params: unknown): void {
  if (method !== 'event.soundboard_play') return;
  const p = params as {
    group_id?: unknown;
    channel_id?: unknown;
    sound?: unknown;
    from?: unknown;
  };
  if (
    typeof p.group_id !== 'string' ||
    typeof p.channel_id !== 'string' ||
    typeof p.sound !== 'string'
  ) {
    return;
  }
  const active = useVoice.getState().active;
  if (active === null || active.groupId !== p.group_id || active.channelId !== p.channel_id) {
    return;
  }
  const self = useSession.getState().self;
  if (self !== null && p.from === self.pubkey) return;
  playSound(p.sound, typeof p.from === 'string' ? p.from : undefined);
}

// Garde d'environnement : les tests qui simulent `../lib/client` sans
// `rpc.onEvent` doivent pouvoir importer ce module sans câblage.
try {
  rpc.onEvent(handleSoundboardEvent);
} catch {
  // Client simulé (tests) : pas d'événements à câbler.
}
