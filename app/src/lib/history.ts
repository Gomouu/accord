/**
 * Pagination des historiques (contrat API.md : `before_lamport`) et fusion
 * incrémentale des pages dans les listes déjà chargées.
 *
 * Ces enveloppes vivent hors de `api.ts` pour ne pas toucher au contrat gelé
 * des méthodes existantes ; le client RPC est passé en argument pour rester
 * injectable dans les tests.
 */

import type { RpcClient } from './rpc';
import type { DmMessage, GroupMessage } from './api';

/** Taille de page des historiques (le nœud borne `limit` à [1, 200]). */
export const PAGE_SIZE = 50;

/** Surface minimale du client RPC utilisée ici (injectable en test). */
export type RpcCaller = Pick<RpcClient, 'call'>;

/** Page d'historique direct, éventuellement bornée par `before_lamport`. */
export function fetchDmPage(
  rpc: RpcCaller,
  pubkey: string,
  beforeLamport?: number,
  limit: number = PAGE_SIZE,
): Promise<{ messages: DmMessage[] }> {
  return rpc.call('dm.history', {
    pubkey,
    limit,
    ...(beforeLamport !== undefined ? { before_lamport: beforeLamport } : {}),
  });
}

/** Page d'historique de salon, éventuellement bornée par `before_lamport`. */
export function fetchGroupPage(
  rpc: RpcCaller,
  groupId: string,
  channelId: string,
  beforeLamport?: number,
  limit: number = PAGE_SIZE,
): Promise<{ messages: GroupMessage[] }> {
  return rpc.call('groups.history', {
    group_id: groupId,
    channel_id: channelId,
    limit,
    ...(beforeLamport !== undefined ? { before_lamport: beforeLamport } : {}),
  });
}

/** Sous-ensemble ordonnable d'un message (direct ou de groupe). */
export interface Sequenced {
  msg_id: string;
  lamport: number;
}

/** Tri stable pour l'affichage : lamport croissant, départage par msg_id. */
export function sortAscending<T extends Sequenced>(messages: readonly T[]): T[] {
  return [...messages].sort(
    (a, b) => a.lamport - b.lamport || a.msg_id.localeCompare(b.msg_id),
  );
}

/** Résultat d'une fusion de page récente. */
export interface MergeResult<T> {
  messages: T[];
  /** Vrai si un trou est possible entre la page et l'existant (remplacement). */
  gapDetected: boolean;
}

/**
 * Égalité structurelle « suffisante » : `base` et `fresh` proviennent du même
 * sérialiseur du nœud (ordre des clés stable), donc une comparaison JSON
 * détecte fidèlement un message inchangé. Bon marché sur une page (≤ 50).
 */
function sameContent<T>(a: T, b: T): boolean {
  return a === b || JSON.stringify(a) === JSON.stringify(b);
}

/**
 * Union par msg_id : les entrées de `fresh` remplacent celles de `base`.
 * Quand le contenu n'a pas changé, on CONSERVE l'objet de `base` (même
 * référence) : les rangées mémoïsées (`BodyText`/`MarkdownText`) sautent alors
 * leur ré-rendu au lieu de re-parser tout le fil à chaque rafraîchissement.
 */
function mergeById<T extends Sequenced>(base: readonly T[], fresh: readonly T[]): T[] {
  const byId = new Map<string, T>();
  for (const m of base) byId.set(m.msg_id, m);
  for (const m of fresh) {
    const prev = byId.get(m.msg_id);
    byId.set(m.msg_id, prev !== undefined && sameContent(prev, m) ? prev : m);
  }
  return sortAscending([...byId.values()]);
}

/**
 * Fusionne la page la plus récente dans la liste croissante existante.
 *
 * Si la page est pleine, sans recouvrement avec l'existant et strictement
 * plus récente, des messages ont pu être manqués entre les deux : la page
 * remplace alors l'existant (`gapDetected`) plutôt que d'afficher un fil
 * troué. Sinon, union par msg_id (la page, plus fraîche, gagne : éditions,
 * suppressions et acquittements sont ainsi rafraîchis).
 */
export function mergeRecentPage<T extends Sequenced>(
  existing: readonly T[],
  page: readonly T[],
  pageFull: boolean,
): MergeResult<T> {
  const sortedPage = sortAscending(page);
  if (existing.length === 0) {
    return { messages: sortedPage, gapDetected: false };
  }
  const known = new Set(existing.map((m) => m.msg_id));
  const overlaps = sortedPage.some((m) => known.has(m.msg_id));
  const newestKnown = existing[existing.length - 1]?.lamport ?? 0;
  const oldestOfPage = sortedPage[0]?.lamport ?? 0;
  if (pageFull && !overlaps && oldestOfPage > newestKnown) {
    return { messages: sortedPage, gapDetected: true };
  }
  return { messages: mergeById(existing, sortedPage), gapDetected: false };
}

/**
 * Fusionne une page plus ancienne (chargée au défilement vers le haut) dans
 * la liste croissante existante. La page vient d'être lue : ses copies
 * remplacent les éventuels doublons.
 */
export function mergeOlderPage<T extends Sequenced>(
  existing: readonly T[],
  page: readonly T[],
): T[] {
  return mergeById(existing, page);
}
