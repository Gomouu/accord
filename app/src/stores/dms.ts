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

export const useDms = create<DmsState>((set, get) => ({
  conversations: {},
  hasMore: {},
  loadingOlder: {},
  peerRead: {},

  applyPeerRead: (peer, lamport) => {
    set((s) => {
      const known = s.peerRead[peer];
      if (known !== undefined && known >= lamport) return s;
      return { peerRead: { ...s.peerRead, [peer]: lamport } };
    });
  },

  refresh: async (peer) => {
    const page = (await fetchDmPage(rpc, peer)) as DmPageWithReceipt;
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
      conversations: patchConversation(s.conversations, peer, msgId, (m) => ({
        ...m,
        reactions: already
          ? (m.reactions ?? []).filter(
              (r) => !(r.emoji === emoji && r.author === selfPubkey),
            )
          : [...(m.reactions ?? []), { emoji, author: selfPubkey }],
      })),
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
