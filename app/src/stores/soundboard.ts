/**
 * Soundboard : lecture des clips audio. Deux chemins convergent vers le même
 * `playSound` :
 * - l'émetteur joue le son localement dès le clic (feedback immédiat) ;
 * - les récepteurs présents dans le vocal reçoivent `event.soundboard_play`
 *   et jouent le clip désigné par sa racine Merkle.
 *
 * La lecture passe par le contexte Web Audio partagé (`lib/audio.playClip`)
 * plutôt que par un élément `Audio` : un contexte déverrouillé une fois joue
 * sans nouveau geste (les récepteurs n'ont aucun geste au moment de
 * l'événement) et le décodage ne dépend pas de la CSP `media-src`. Un échec
 * (fichier indisponible, contexte encore verrouillé) est signalé par un
 * toast — plus jamais avalé en silence.
 *
 * En rejoignant un salon vocal de groupe, tous les clips du serveur sont
 * préchargés (`lireFichier` met la promesse en cache) : la première lecture
 * distante est instantanée. Câblé au chargement du module comme
 * `stores/groups.ts` (garde d'environnement pour les tests qui simulent
 * `../lib/client` sans `rpc.onEvent` ou `./voice` sans `subscribe`).
 */

import { dictionary } from '../i18n';
import { playClip } from '../lib/audio';
import { rpc } from '../lib/client';
import { lireFichier } from '../lib/files';
import { useGroups } from './groups';
import { useSession } from './session';
import { useUi } from './ui';
import { useVoice } from './voice';

/**
 * Joue un clip de soundboard par sa racine Merkle. `hint` : clé publique d'un
 * pair source probable (l'émetteur), utilisée pour amorcer le téléchargement
 * si le clip n'est pas encore local. Rend `true` si la lecture a démarré ;
 * `false` en échec (déjà signalé à l'utilisateur par un toast d'erreur).
 */
export async function playSound(merkleRoot: string, hint?: string): Promise<boolean> {
  try {
    const url = await lireFichier(merkleRoot, hint);
    await playClip(url);
    return true;
  } catch {
    const ui = useUi.getState();
    ui.toast('error', dictionary(ui.lang).soundboard.playbackFailed);
    return false;
  }
}

/**
 * Précharge tous les clips de soundboard d'un groupe (best effort) : les
 * promesses entrent dans le cache de `lireFichier`, la première lecture —
 * locale ou distante — est alors immédiate au lieu d'attendre un
 * téléchargement pair-à-pair.
 */
export function prefetchGroupSounds(groupId: string): void {
  const sounds = useGroups.getState().states[groupId]?.sounds ?? [];
  for (const sound of sounds) {
    lireFichier(sound.merkle_root).catch(() => {
      // Best effort : le clip retentera sa chance à la lecture réelle.
    });
  }
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
  if (
    active === null ||
    active.groupId !== p.group_id ||
    active.channelId !== p.channel_id
  ) {
    return;
  }
  const self = useSession.getState().self;
  if (self !== null && p.from === self.pubkey) return;
  void playSound(p.sound, typeof p.from === 'string' ? p.from : undefined);
}

/**
 * Précharge les sons du groupe à chaque entrée dans un salon vocal de groupe
 * (jamais pour la session d'appel 1-à-1, `group_id` sentinelle sans sons).
 */
function wirePrefetchOnVoiceJoin(): void {
  let previousGroupId: string | null = null;
  useVoice.subscribe((s) => {
    const next = s.active !== null && !s.active.isCall ? s.active.groupId : null;
    if (next !== null && next !== previousGroupId) prefetchGroupSounds(next);
    previousGroupId = next;
  });
}

// Garde d'environnement : les tests qui simulent `../lib/client` sans
// `rpc.onEvent` (ou `./voice` sans `subscribe`) doivent pouvoir importer ce
// module sans câblage.
try {
  rpc.onEvent(handleSoundboardEvent);
  wirePrefetchOnVoiceJoin();
} catch {
  // Client simulé (tests) : pas d'événements à câbler.
}
