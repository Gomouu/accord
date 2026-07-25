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
import {
  resetAllRemote,
  resetPeer,
  resetRemote,
  startLocalStream,
  stopAllLocal,
  stopLocalStream,
  type VideoSource,
} from '../lib/mediaController';
import type { CallEndedReason, CallState } from '../lib/api';

export type { CallEndedReason, CallState } from '../lib/api';

/** Flux vidéo reçus d'un participant. */
export interface RemoteVideo {
  /** Ce participant partage son écran. */
  screen: boolean;
  /** Ce participant a sa caméra allumée. */
  camera: boolean;
}

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
   *
   * Le motif n'entre volontairement pas en jeu ici : toute fin remet la
   * machine au repos, et c'est cette remise au repos (`phase`/`peer`) qui
   * retire l'overlay de sonnerie et coupe la sonnerie côté `IncomingCall`.
   * Un nouveau motif est donc pris en charge sans rien changer.
   */
  applyEnded: (params: {
    peer: string;
    call_id: string;
    reason: CallEndedReason;
  }) => boolean;
  /** Marque `peer` comme ayant un appel manqué (badge DM). */
  markMissed: (peer: string) => void;
  /** Efface le badge d'appel manqué de `peer` (ouverture de la conversation). */
  clearMissed: (peer: string) => void;
  /** Vrai si l'on partage son écran (v5). */
  localSharing: boolean;
  /** Vrai si notre caméra est allumée (v6). */
  localCamera: boolean;
  /**
   * Flux vidéo reçus, par clé publique de l'émetteur. Un salon de groupe peut
   * en compter plusieurs simultanés : un booléen unique par source ne saurait
   * pas dire QUI diffuse, et la grille ne pourrait pas s'afficher.
   */
  remoteVideo: Record<string, RemoteVideo>;
  /**
   * Démarre le partage de son écran (appel actif requis). Rejette si
   * l'utilisateur refuse le partage ou si le runtime ne le supporte pas.
   */
  startScreenShare: () => Promise<void>;
  /** Arrête le partage de son écran. */
  stopScreenShare: () => Promise<void>;
  /**
   * Allume sa caméra (appel actif requis). Rejette si l'utilisateur refuse
   * l'accès ou si le runtime ne le supporte pas.
   */
  startCamera: () => Promise<void>;
  /** Éteint sa caméra. */
  stopCamera: () => Promise<void>;
  /** Applique `event.screen_state` (un pair a démarré/arrêté son partage). */
  applyScreenState: (params: { peer: string; sharing: boolean }) => void;
  /** Applique `event.camera_state` (un pair a allumé/éteint sa caméra). */
  applyCameraState: (params: { peer: string; on: boolean }) => void;
  /**
   * Marque un flux distant actif dès la première trame reçue. L'annonce
   * d'état peut se perdre (elle ne voyage pas en fiable) ; une trame, elle,
   * prouve que le flux existe.
   */
  noteRemoteFrame: (peer: string, source: VideoSource) => void;
  /**
   * Oublie tous les flux d'un participant qui s'en va. Un départ brutal ne
   * s'accompagne d'aucune annonce d'arrêt : sans ce nettoyage, sa tuile
   * resterait à l'écran, figée sur sa dernière image.
   */
  forgetPeerVideo: (peer: string) => void;
}

/**
 * Met à jour un flux d'un participant sans muter la table. Rend `{}` — soit
 * aucun changement — quand l'état est déjà celui demandé : les trames arrivent
 * à 12–24 par seconde, et re-créer la table à chaque trame ferait re-rendre
 * toute la grille en continu.
 */
function setRemote(
  table: Record<string, RemoteVideo>,
  peer: string,
  source: VideoSource,
  on: boolean,
): Partial<CallsState> {
  const current = table[peer] ?? { screen: false, camera: false };
  if (current[source] === on) return {};
  const next = { ...current, [source]: on };
  // Un participant sans aucun flux quitte la table : la grille n'affiche que
  // ce qui existe, et une entrée fantôme y laisserait une tuile vide.
  if (!next.screen && !next.camera) {
    const reste = { ...table };
    delete reste[peer];
    return { remoteVideo: reste };
  }
  return { remoteVideo: { ...table, [peer]: next } };
}

export const useCalls = create<CallsState>((set, get) => ({
  phase: 'idle',
  peer: null,
  callId: null,
  sincePhaseMs: null,
  missedPeers: new Set(),
  localSharing: false,
  localCamera: false,
  remoteVideo: {},

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
    resetAllRemote();
    set({
      phase: 'idle',
      peer: null,
      callId: null,
      sincePhaseMs: null,
      localSharing: false,
      localCamera: false,
      remoteVideo: {},
    });
  },

  hangup: async () => {
    if (get().phase === 'idle') return;
    // Coupe tout flux vidéo en cours avant de raccrocher.
    await stopAllLocal();
    resetAllRemote();
    await api.callsHangup();
    set({
      phase: 'idle',
      peer: null,
      callId: null,
      sincePhaseMs: null,
      localSharing: false,
      localCamera: false,
      remoteVideo: {},
    });
  },

  sync: async () => {
    const status = await api.callsStatus();
    const idle = status.state === 'idle';
    if (idle) resetAllRemote();
    set({
      phase: status.state,
      peer: status.peer,
      callId: status.call_id,
      // Repère mural remis à zéro : `since_ms` (horloge du moteur) n'est pas
      // convertible en temps mural sans second point de repère (voir l'en-tête).
      sincePhaseMs: idle ? null : Date.now(),
      ...(idle
        ? {
            localSharing: false,
            localCamera: false,
            remoteVideo: {},
          }
        : {}),
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
    // Fin d'appel : coupe tous les flux vidéo (locaux et distants).
    void stopAllLocal();
    resetAllRemote();
    set({
      phase: 'idle',
      peer: null,
      callId: null,
      sincePhaseMs: null,
      localSharing: false,
      localCamera: false,
      remoteVideo: {},
    });
    return true;
  },

  startScreenShare: async () => {
    // Autorisé en appel 1-à-1 comme en salon vocal de groupe (v6.1) : le nœud
    // ignore la demande s'il n'y a aucune session média active.
    if (get().localSharing) return;
    await startLocalStream('screen', () => set({ localSharing: false }));
    set({ localSharing: true });
  },

  stopScreenShare: async () => {
    if (!get().localSharing) return;
    await stopLocalStream('screen');
    set({ localSharing: false });
  },

  startCamera: async () => {
    if (get().localCamera) return;
    await startLocalStream('camera', () => set({ localCamera: false }));
    set({ localCamera: true });
  },

  stopCamera: async () => {
    if (!get().localCamera) return;
    await stopLocalStream('camera');
    set({ localCamera: false });
  },

  applyScreenState: ({ peer, sharing }) => {
    if (!sharing) resetRemote(peer, 'screen');
    set((s) => setRemote(s.remoteVideo, peer, 'screen', sharing));
  },

  applyCameraState: ({ peer, on }) => {
    if (!on) resetRemote(peer, 'camera');
    set((s) => setRemote(s.remoteVideo, peer, 'camera', on));
  },

  noteRemoteFrame: (peer, source) => {
    set((s) => setRemote(s.remoteVideo, peer, source, true));
  },

  forgetPeerVideo: (peer) => {
    if (get().remoteVideo[peer] === undefined) return;
    resetPeer(peer);
    set((s) => {
      const remoteVideo = { ...s.remoteVideo };
      delete remoteVideo[peer];
      return { remoteVideo };
    });
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
