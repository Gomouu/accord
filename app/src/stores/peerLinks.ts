/**
 * Qualité du lien réseau vers chaque ami, telle que le nœud la voit.
 *
 * Le diagnostic complet existe depuis la 4.0, mais il fallait ouvrir un
 * panneau pour le lire. Cet état le rend disponible partout — liste d'amis,
 * en-tête de conversation — pour répondre d'un coup d'œil à la seule question
 * qui compte quand un message tarde : « est-ce que je suis vraiment connecté
 * à cette personne ? ».
 *
 * Rafraîchi sur l'événement `event.network` (changement d'état réseau) et par
 * un sondage lent : la latence évolue sans qu'aucun événement ne le signale.
 */

import { create } from 'zustand';
import { api } from '../lib/client';
import type { PeerLink } from '../lib/api';

/**
 * Période du sondage de repli. Assez long pour rester invisible côté charge —
 * l'appel est local et ne touche pas le réseau — assez court pour qu'une
 * bascule direct↔relais ne reste pas affichée à tort très longtemps.
 */
const POLL_MS = 15_000;

/** Qualité affichable d'un lien vers un pair. */
export type LinkQuality = 'direct' | 'relay' | 'offline';

interface PeerLinksState {
  /** Lien courant par clé publique (hex), tel que rendu par `network.peers`. */
  links: Record<string, PeerLink>;
  /** Relit l'état des liens. Silencieux en cas d'échec (diagnostic annexe). */
  refresh: () => Promise<void>;
  /** Vide l'état (déconnexion, changement de compte). */
  reset: () => void;
}

export const usePeerLinks = create<PeerLinksState>((set) => ({
  links: {},

  refresh: async () => {
    try {
      const peers = await api.networkPeers();
      const links: Record<string, PeerLink> = {};
      for (const peer of peers) links[peer.pubkey] = peer;
      set({ links });
    } catch {
      // Un nœud plus ancien ne connaît pas `network.peers` : l'indicateur
      // reste simplement absent, aucune erreur n'est montrée à l'utilisateur.
    }
  },

  reset: () => set({ links: {} }),
}));

/** Période du sondage de repli, exposée pour le câblage et les tests. */
export const PEER_LINKS_POLL_MS = POLL_MS;

/**
 * Qualité affichable du lien vers `pubkey`.
 *
 * `offline` couvre aussi bien « aucune session » que « pair inconnu du
 * diagnostic » : dans les deux cas, rien ne garantit qu'un message parte tout
 * de suite, et c'est ce que l'indicateur doit dire.
 */
export function qualityOf(links: Record<string, PeerLink>, pubkey: string): LinkQuality {
  const link = links[pubkey];
  if (link === undefined || !link.live) return 'offline';
  return link.transport === 'relay' ? 'relay' : 'direct';
}

/** Latence aller-retour du lien vers `pubkey`, si elle a pu être mesurée. */
export function rttOf(links: Record<string, PeerLink>, pubkey: string): number | null {
  return links[pubkey]?.rtt_ms ?? null;
}
