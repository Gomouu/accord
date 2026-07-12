/**
 * Conversations directes : pages d'historique, fusion incrémentale, envoi,
 * et actions de message (édition, suppression, réactions). Les actions sont
 * confirmées par le nœud puis reflétées immutablement dans le fil local —
 * y compris pour les messages plus anciens que la page récente.
 */

import { create } from 'zustand';
import { api, rpc } from '../lib/client';
import type { DmMessage, FileAttachment } from '../lib/api';
import {
  PAGE_SIZE,
  fetchDmPage,
  mergeOlderPage,
  mergeRecentPage,
  sortAscending,
} from '../lib/history';

interface DmsState {
  /** Messages par clé publique du pair, du plus ancien au plus récent. */
  conversations: Record<string, DmMessage[]>;
  /** Vrai si des messages plus anciens existent probablement côté nœud. */
  hasMore: Record<string, boolean>;
  /** Garde anti-rafale du chargement vers le haut. */
  loadingOlder: Record<string, boolean>;
  /** Identifiants épinglés par pair (vue locale, `dm.pins`). */
  pins: Record<string, string[]>;
  /**
   * Position de lecture du pair par conversation (lamport du dernier de nos
   * messages couvert par son accusé de lecture) ; absente = inconnue.
   */
  peerRead: Record<string, number>;
  /** Avance (jamais ne recule) la position de lecture du pair. */
  applyPeerRead: (peer: string, lamport: number) => void;
  /** Charge (ou rafraîchit) la page récente, fusionnée sans rechargement. */
  refresh: (peer: string) => Promise<void>;
  /** Charge la page précédant le plus ancien message connu. */
  loadOlder: (peer: string) => Promise<void>;
  /**
   * S'assure que `msgId` est chargé (fenêtre `dm.history_around` fusionnée si
   * besoin). Rend `true` si le message est disponible localement au retour,
   * `false` si le nœud l'ignore (fenêtre `found: false`).
   */
  jumpTo: (peer: string, msgId: string) => Promise<boolean>;
  /** Charge les identifiants épinglés de la conversation (`dm.pins`). */
  loadPins: (peer: string) => Promise<void>;
  /** Épingle ou désépingle selon l'état courant `pinned`, puis recharge. */
  togglePin: (peer: string, msgId: string, pinned: boolean) => Promise<void>;
  /** Relance l'envoi d'un message non acquitté (`dm.retry`), puis rafraîchit. */
  retry: (peer: string, msgId: string) => Promise<void>;
  /**
   * Envoie un message, éventuellement en réponse à `replyTo` (msg_id) et
   * avec des pièces jointes déjà publiées (texte vide admis avec pièces).
   */
  send: (
    peer: string,
    text: string,
    replyTo?: string,
    attachments?: FileAttachment[],
  ) => Promise<void>;
  /** Remplace le texte d'un de ses propres messages. */
  edit: (peer: string, msgId: string, text: string) => Promise<void>;
  /** Supprime un de ses propres messages (tombstone). */
  deleteMessage: (peer: string, msgId: string) => Promise<void>;
  /** Ajoute ou retire (bascule) sa réaction `emoji` sur un message. */
  toggleReaction: (
    peer: string,
    msgId: string,
    emoji: string,
    selfPubkey: string,
  ) => Promise<void>;
}

/** Copie d'une conversation où `msgId` est transformé par `patch`. */
function patchConversation(
  conversations: Record<string, DmMessage[]>,
  peer: string,
  msgId: string,
  patch: (message: DmMessage) => DmMessage,
): Record<string, DmMessage[]> {
  const existing = conversations[peer];
  if (existing === undefined) return conversations;
  return {
    ...conversations,
    [peer]: existing.map((m) => (m.msg_id === msgId ? patch(m) : m)),
  };
}

/**
 * Forme complète de la réponse `dm.history` : `fetchDmPage` (contrat gelé)
 * ne déclare que `messages`, le nœud émet aussi `peer_read_lamport`.
 */
interface DmPageWithReceipt {
  messages: DmMessage[];
  peer_read_lamport?: number | null;
}

/**
 * Séquence « dernier gagne » par pair pour `refresh` : plusieurs
 * actualisations peuvent être en vol (une par `event.dm`) et répondre dans le
 * désordre sur la même connexion. On tamponne un numéro monotone au départ ;
 * si une actualisation PLUS récente a démarré avant que celle-ci ne réponde,
 * on ignore la réponse périmée (sinon elle réécraserait une édition/
 * suppression/réaction déjà appliquée par la plus récente).
 */
const refreshSeq = new Map<string, number>();

export const useDms = create<DmsState>((set, get) => ({
  conversations: {},
  hasMore: {},
  loadingOlder: {},
  pins: {},
  peerRead: {},

  applyPeerRead: (peer, lamport) => {
    set((s) => {
      const known = s.peerRead[peer];
      if (known !== undefined && known >= lamport) return s;
      return { peerRead: { ...s.peerRead, [peer]: lamport } };
    });
  },

  refresh: async (peer) => {
    const seq = (refreshSeq.get(peer) ?? 0) + 1;
    refreshSeq.set(peer, seq);
    const page = (await fetchDmPage(rpc, peer)) as DmPageWithReceipt;
    // Réponse périmée (une actualisation plus récente a démarré depuis) :
    // l'appliquer réécraserait des données plus fraîches — on l'ignore.
    if (refreshSeq.get(peer) !== seq) return;
    const { messages } = page;
    if (typeof page.peer_read_lamport === 'number') {
      get().applyPeerRead(peer, page.peer_read_lamport);
    }
    const pageFull = messages.length === PAGE_SIZE;
    set((s) => {
      const existing = s.conversations[peer];
      if (existing === undefined || existing.length === 0) {
        return {
          conversations: { ...s.conversations, [peer]: sortAscending(messages) },
          hasMore: { ...s.hasMore, [peer]: pageFull },
        };
      }
      const merged = mergeRecentPage(existing, messages, pageFull);
      return {
        conversations: { ...s.conversations, [peer]: merged.messages },
        // Trou détecté : l'existant est remplacé par la page récente,
        // le défilement vers le haut re-remontera le fil.
        hasMore: merged.gapDetected ? { ...s.hasMore, [peer]: pageFull } : s.hasMore,
      };
    });
  },

  loadOlder: async (peer) => {
    const state = get();
    const oldest = (state.conversations[peer] ?? [])[0];
    if (
      oldest === undefined ||
      state.loadingOlder[peer] === true ||
      state.hasMore[peer] !== true
    ) {
      return;
    }
    set((s) => ({ loadingOlder: { ...s.loadingOlder, [peer]: true } }));
    try {
      const { messages } = await fetchDmPage(rpc, peer, oldest.lamport);
      set((s) => ({
        conversations: {
          ...s.conversations,
          [peer]: mergeOlderPage(s.conversations[peer] ?? [], messages),
        },
        hasMore: { ...s.hasMore, [peer]: messages.length === PAGE_SIZE },
      }));
    } finally {
      set((s) => ({ loadingOlder: { ...s.loadingOlder, [peer]: false } }));
    }
  },

  jumpTo: async (peer, msgId) => {
    const existing = get().conversations[peer] ?? [];
    if (existing.some((m) => m.msg_id === msgId)) return true;
    const res = await api.dmHistoryAround(peer, msgId);
    if (!res.found) return false;
    if (typeof res.peer_read_lamport === 'number') {
      get().applyPeerRead(peer, res.peer_read_lamport);
    }
    set((s) => {
      const merged = mergeOlderPage(s.conversations[peer] ?? [], res.messages);
      // Une fenêtre pleine laisse supposer d'autres messages plus anciens :
      // on active la remontée si l'état ne la connaissait pas encore.
      const knownHasMore = s.hasMore[peer];
      return {
        conversations: { ...s.conversations, [peer]: merged },
        hasMore: {
          ...s.hasMore,
          [peer]:
            knownHasMore === undefined
              ? res.messages.length >= PAGE_SIZE
              : knownHasMore,
        },
      };
    });
    return true;
  },

  loadPins: async (peer) => {
    const { msg_ids } = await api.dmPins(peer);
    set((s) => ({ pins: { ...s.pins, [peer]: msg_ids } }));
  },

  togglePin: async (peer, msgId, pinned) => {
    if (pinned) await api.dmUnpin(peer, msgId);
    else await api.dmPin(peer, msgId);
    await get().loadPins(peer);
  },

  retry: async (peer, msgId) => {
    await api.dmRetry(peer, msgId);
    await get().refresh(peer);
  },

  send: async (peer, text, replyTo, attachments) => {
    await api.dmSend(peer, text, replyTo, attachments);
    await get().refresh(peer);
  },

  edit: async (peer, msgId, text) => {
    await api.dmEdit(peer, msgId, text);
    set((s) => ({
      conversations: patchConversation(s.conversations, peer, msgId, (m) => ({
        ...m,
        edited: text,
      })),
    }));
  },

  deleteMessage: async (peer, msgId) => {
    await api.dmDelete(peer, msgId);
    set((s) => ({
      conversations: patchConversation(s.conversations, peer, msgId, (m) => ({
        ...m,
        deleted: true,
      })),
    }));
  },

  toggleReaction: async (peer, msgId, emoji, selfPubkey) => {
    const message = (get().conversations[peer] ?? []).find((m) => m.msg_id === msgId);
    if (message === undefined) return;
    const already = (message.reactions ?? []).some(
      (r) => r.emoji === emoji && r.author === selfPubkey,
    );
    await api.dmReact(peer, msgId, emoji, already);
    set((s) => ({
      conversations: patchConversation(s.conversations, peer, msgId, (m) => {
        // Idempotent : deux clics rapides lisent tous deux `already=false` puis
        // ajoutent ; on garde contre le doublon en relisant l'état COURANT.
        const has = (m.reactions ?? []).some(
          (r) => r.emoji === emoji && r.author === selfPubkey,
        );
        return {
          ...m,
          reactions: already
            ? (m.reactions ?? []).filter(
                (r) => !(r.emoji === emoji && r.author === selfPubkey),
              )
            : has
              ? (m.reactions ?? [])
              : [...(m.reactions ?? []), { emoji, author: selfPubkey }],
        };
      }),
    }));
  },
}));

/**
 * Événement `event.dm_read` : le pair a lu nos messages jusqu'à `lamport`.
 * Exporté pour les tests ; câblé au chargement du module (client singleton).
 */
export function handleDmsNodeEvent(method: string, params: unknown): void {
  if (method !== 'event.dm_read') return;
  const p = params as { peer?: string; lamport?: number };
  if (typeof p.peer !== 'string' || typeof p.lamport !== 'number') return;
  useDms.getState().applyPeerRead(p.peer, p.lamport);
}

// Garde d'environnement : les tests unitaires qui simulent `../lib/client`
// avec un `rpc` réduit à `call` doivent pouvoir importer ce module.
try {
  rpc.onEvent(handleDmsNodeEvent);
} catch {
  // Client simulé (tests) : pas d'événements à câbler.
}
