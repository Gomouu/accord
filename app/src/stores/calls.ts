/**
 * Appel 1-à-1 : miroir de la machine à états `calls.*` (idle /
 * outgoing_ringing / incoming_ringing / active, voir VOICE_CALLS.md §1.3).
 * Les événements `event.call_*` (câblés dans AppShell) font foi ; nos propres
 * actions (`start`/`accept`/`decline`/`hangup`) appliquent un état optimiste
 * après succès du nœud — le même schéma que `stores/voice.ts` — et les
 * événements qui suivent sont idempotents sur ce même état.
 *
 * `sincePhaseMs` est une ancre murale **locale** (`Date.now()`), pas le
 * `since_ms` du nœud (horloge interne du moteur, sans repère mural
 * exploitable côté UI — voir VOICE_CALLS.md §1.1) : elle est reposée à chaque
 * transition de phase et sert uniquement à afficher une durée relative
 * (sonnerie, appel actif).
 */

import { create } from 'zustand';
import { api } from '../lib/client';
import type { CallEndedReason, CallState } from '../lib/api';

export type { CallEndedReason, CallState } from '../lib/api';

interface CallsState {
  phase: CallState;
  peer: string | null;
  callId: string | null;
  sincePhaseMs: number | null;
  /** Pairs ayant un appel manqué non consulté (badge DM). */
  missedPeers: Set<string>;
  /** Démarre un appel vers `peer` (ami confirmé requis côté nœud). */
  start: (peer: string) => Promise<void>;
  /** Accepte l'appel entrant en sonnerie. */
  accept: () => Promise<void>;
  /** Refuse l'appel entrant en sonnerie. */
  decline: () => Promise<void>;
  /** Annule une sonnerie sortante ou raccroche un appel actif. */
  hangup: () => Promise<void>;
  /** Resynchronise l'état depuis `calls.status` (connexion/reprise). */
  sync: () => Promise<void>;
  /** Applique `event.call_outgoing` (idempotent sur le même `call_id`). */
  applyOutgoing: (params: { peer: string; call_id: string }) => void;
  /** Applique `event.call_incoming` (idempotent sur le même `call_id`). */
  applyIncoming: (params: { peer: string; call_id: string }) => void;
  /**
   * Applique `event.call_accepted` : toujours autoritaire, y compris avec un
   * `call_id` différent de celui suivi localement (appels croisés —
   * `reason: "superseded"` suivi de cet événement pour l'appel retenu).
   */
  applyAccepted: (params: { peer: string; call_id: string }) => void;
  /**
   * Applique `event.call_ended` : n'a d'effet que si `call_id` correspond à
   * l'appel suivi localement (ignore un événement tardif d'un appel déjà
   * remplacé ou déjà résolu localement par `decline`/`hangup`). Rend `true`
   * si l'état a bien été réinitialisé — l'appelant (AppShell) ne notifie
   * (toast) que dans ce cas, ce qui évite aussi un toast redondant pour une
   * fin d'appel qu'on a soi-même déclenchée.
   */
  applyEnded: (params: { peer: string; call_id: string; reason: CallEndedReason }) => boolean;
  /** Marque `peer` comme ayant un appel manqué (badge DM). */
  markMissed: (peer: string) => void;
  /** Efface le badge d'appel manqué de `peer` (ouverture de la conversation). */
  clearMissed: (peer: string) => void;
}

export const useCalls = create<CallsState>((set, get) => ({
  phase: 'idle',
  peer: null,
  callId: null,
  sincePhaseMs: null,
  missedPeers: new Set(),

  start: async (peer) => {
    const { call_id: callId } = await api.callsStart(peer);
    // N'adopte l'état « sortant » QUE si rien d'événementiel (plus autoritaire)
    // n'a bougé la machine à états pendant l'appel RPC — p. ex. un appel
    // ENTRANT arrivé entre-temps ne doit pas être écrasé. `applyOutgoing`
    // (événement de notre propre appel) a déjà posé le bon état le cas échéant.
    set((s) =>
      s.phase === 'idle'
        ? { phase: 'outgoing_ringing', peer, callId, sincePhaseMs: Date.now() }
        : s,
    );
  },

  accept: async () => {
    const callId = get().callId;
    if (get().phase !== 'incoming_ringing' || callId === null) return;
    await api.callsAccept(callId);
    set({ phase: 'active', sincePhaseMs: Date.now() });
  },

  decline: async () => {
    const callId = get().callId;
    if (get().phase !== 'incoming_ringing' || callId === null) return;
    await api.callsDecline(callId);
    set({ phase: 'idle', peer: null, callId: null, sincePhaseMs: null });
  },

  hangup: async () => {
    if (get().phase === 'idle') return;
    await api.callsHangup();
    set({ phase: 'idle', peer: null, callId: null, sincePhaseMs: null });
  },

  sync: async () => {
    const status = await api.callsStatus();
    set({
      phase: status.state,
      peer: status.peer,
      callId: status.call_id,
      // Repère mural remis à zéro : `since_ms` (horloge du moteur) n'est pas
      // convertible en temps mural sans second point de repère (voir l'en-tête).
      sincePhaseMs: status.state === 'idle' ? null : Date.now(),
    });
  },

  applyOutgoing: ({ peer, call_id: callId }) => {
    set((s) => {
      if (s.phase === 'outgoing_ringing' && s.callId === callId) return s;
      return { phase: 'outgoing_ringing', peer, callId, sincePhaseMs: Date.now() };
    });
  },

  applyIncoming: ({ peer, call_id: callId }) => {
    set((s) => {
      if (s.phase === 'incoming_ringing' && s.callId === callId) return s;
      return { phase: 'incoming_ringing', peer, callId, sincePhaseMs: Date.now() };
    });
  },

  applyAccepted: ({ peer, call_id: callId }) => {
    set({ phase: 'active', peer, callId, sincePhaseMs: Date.now() });
  },

  applyEnded: ({ call_id: callId }) => {
    if (get().callId !== callId) return false;
    set({ phase: 'idle', peer: null, callId: null, sincePhaseMs: null });
    return true;
  },

  markMissed: (peer) =>
    set((s) => {
      if (s.missedPeers.has(peer)) return s;
      const missedPeers = new Set(s.missedPeers);
      missedPeers.add(peer);
      return { missedPeers };
    }),

  clearMissed: (peer) =>
    set((s) => {
      if (!s.missedPeers.has(peer)) return s;
      const missedPeers = new Set(s.missedPeers);
      missedPeers.delete(peer);
      return { missedPeers };
    }),
}));
