/**
 * Transfert d'historique depuis un autre appareil du compte (§17.4), côté
 * interface : abonnement à l'avancement, et lecture de ce qu'il faut en dire.
 *
 * `devices.transfer_history` ne rend la main qu'à la FIN — une page par
 * aller-retour, donc plusieurs minutes possibles. Tout ce qui s'affiche
 * pendant vient de `event.history_transfer` : l'écran s'abonne AVANT de lancer
 * l'appel. Même forme que `observerProgression` dans `files.ts` (l'abonnement
 * rend son désabonnement), et l'événement porte volontairement les mêmes
 * champs `done`/`total`/`complete` que `event.file_progress`.
 *
 * ⚠️ **L'ambiguïté que ce module refuse de masquer.** Le nœud ne sait pas
 * distinguer « l'appareil d'en face tourne une version qui ignore la demande »
 * de « l'appareil d'en face n'a rien de plus ancien à donner » : les deux se
 * présentent à l'identique, un transfert qui se termine avec zéro page reçue.
 * `conclure` nomme donc les deux causes au lieu d'en inventer une. Un carnet
 * VIDE, lui, explique le zéro à lui seul : il n'y avait rien à demander, et
 * l'annoncer comme ambigu serait un doute fabriqué.
 */

import { rpc } from './client';

/** Charge utile d'`event.history_transfer`. */
export interface AvancementHistorique {
  /** Conversations déjà parcourues. */
  done: number;
  /** Conversations à parcourir — c'est la taille du carnet vue par le nœud. */
  total: number;
  /** Pages d'historique reçues depuis le début du transfert. */
  messages: number;
  /** Vrai sur le dernier événement, celui qui clôt le transfert. */
  complete: boolean;
}

/**
 * Suit l'avancement d'un transfert d'historique : `onProgress` est appelé à
 * chaque `event.history_transfer` du nœud. Rend le désabonnement.
 *
 * ⚠️ L'événement ne nomme PAS l'appareil source. Deux transferts menés en même
 * temps mélangeraient donc leurs avancements dans une seule barre : c'est à
 * l'appelant de n'en mener qu'un à la fois.
 */
export function observerTransfertHistorique(
  onProgress: (avancement: AvancementHistorique) => void,
): () => void {
  return rpc.onEvent((method, params) => {
    if (method !== 'event.history_transfer') return;
    const p = params as Partial<AvancementHistorique>;
    onProgress({
      done: p.done ?? 0,
      total: p.total ?? 0,
      messages: p.messages ?? 0,
      complete: p.complete === true,
    });
  });
}

/**
 * Ce qu'un transfert terminé permet d'affirmer — et rien de plus.
 *
 * - `recu` : des pages sont arrivées, il n'y a rien à interpréter.
 * - `carnet-vide` : aucune conversation à parcourir, donc rien à demander.
 * - `ambigu` : des conversations à parcourir, et zéro page. **Deux causes
 *   indiscernables**, à nommer toutes les deux (voir l'en-tête du module).
 */
export type IssueTransfert = 'recu' | 'carnet-vide' | 'ambigu';

/**
 * Qualifie un transfert terminé à partir de son résumé final
 * (`devices.transfer_history`).
 *
 * `conversations` est le carnet tel que le nœud l'a parcouru : s'en servir
 * plutôt que de relire la liste d'amis côté interface, c'est comparer le zéro
 * de `pages` à la population qui l'a produit, et non à une autre liste chargée
 * ailleurs, à un autre moment, qui pourrait ne pas être la même.
 */
export function conclure(conversations: number, pages: number): IssueTransfert {
  if (pages > 0) return 'recu';
  return conversations > 0 ? 'ambigu' : 'carnet-vide';
}
