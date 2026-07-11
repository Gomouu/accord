/**
 * Salon vocal : salon actif (un seul à la fois, contrat voix), participants
 * connectés, bascule du micro, deafen local et volumes de sortie. Les
 * événements `event.voice_*` du nœud sont appliqués ici ; `sync`
 * resynchronise l'état complet via `voice.status`.
 */

import { create } from 'zustand';
import { api } from '../lib/client';

/** Volume de sortie par défaut (100 % = gain neutre). */
const VOLUME_DEFAULT = 100;
/** Volume de sortie maximal accepté par le nœud (200 % ≈ +6 dB). */
const VOLUME_MAX = 200;

/** Salon vocal rejoint, vu de l'interface (clés en camelCase). */
export interface ActiveVoice {
  groupId: string;
  channelId: string;
  muted: boolean;
  /**
   * Session d'appel 1-à-1 (`group_id` sentinelle, 32 zéros) plutôt qu'un
   * salon de groupe — distingue l'affichage (pair vs salon) sans jamais
   * indexer `useGroups.states` par la sentinelle.
   */
  isCall: boolean;
}

/** État d'un participant connecté au salon actif. */
export interface ParticipantState {
  speaking: boolean;
  /** Micro coupé, tel que diffusé par le participant. */
  muted: boolean;
  /** Sortie coupée (deafen), tel que diffusé par le participant. */
  deafened: boolean;
  /** Volume de sortie local de ce participant (0-200 %, persisté au nœud). */
  volume: number;
  /** Micro forcé coupé par un modérateur de groupe (toujours `false` en appel). */
  serverMuted: boolean;
  /** Sortie forcée coupée par un modérateur (toujours `false` en appel). */
  serverDeafened: boolean;
  /** Porteur de la permission PRIORITY_SPEAKER en train de parler. */
  prioritySpeaker: boolean;
}

/** Réglages DSP de capture, reflétés depuis `voice.status.dsp`. */
export interface VoiceDspState {
  noiseSuppression: boolean;
  agc: boolean;
}

interface VoiceState {
  active: ActiveVoice | null;
  /**
   * Sortie locale coupée (deafen, sémantique Discord : force le micro coupé ;
   * le rétablissement restaure l'état de micro demandé avant). Portée à la
   * session : réinitialisé en rejoignant ou quittant un salon.
   */
  selfDeafened: boolean;
  /** Volume de sortie principal (0-200 %, persisté au nœud). */
  masterVolume: number;
  /** Participants du salon actif, indexés par clé publique (hex). */
  participants: Map<string, ParticipantState>;
  /** Réglages DSP de capture (suppression de bruit, AGC), persistés au nœud. */
  dsp: VoiceDspState;
  /** Rejoint un salon vocal ; le nœud quitte l'ancien implicitement. */
  join: (groupId: string, channelId: string) => Promise<void>;
  /** Quitte le salon actif et vide la liste des participants. */
  leave: () => Promise<void>;
  /** Coupe/rétablit le micro (voice.mute) tout en restant dans le salon. */
  toggleMute: () => Promise<void>;
  /** Force l'état du micro (utilisé par l'appui-pour-parler). */
  setMuted: (muted: boolean) => Promise<void>;
  /** Coupe/rétablit toute la voix entrante localement (voice.deafen). */
  toggleDeafen: () => Promise<void>;
  /** Force l'état du deafen (voice.deafen) ; force aussi le micro coupé. */
  setDeafened: (on: boolean) => Promise<void>;
  /**
   * Volume de sortie en pourcentage (0-200, arrondi et borné) : principal
   * quand `peer` est `null`, sinon celui du participant (voice.set_volume).
   */
  setVolume: (peer: string | null, volume: number) => Promise<void>;
  /** Resynchronise l'état local depuis `voice.status` (reprise de session). */
  sync: () => Promise<void>;
  /** Recharge le volume principal et les réglages DSP persistés (onglet Voix). */
  loadMasterVolume: () => Promise<void>;
  /** Active/désactive la suppression de bruit (voice.set_noise_suppression). */
  setNoiseSuppression: (enabled: boolean) => Promise<void>;
  /** Active/désactive le contrôle automatique de gain (voice.set_agc). */
  setAgc: (enabled: boolean) => Promise<void>;
  /** Applique `event.voice_joined` (ignoré hors du salon actif). */
  applyJoined: (params: { group_id: string; channel_id: string; pubkey: string }) => void;
  /** Applique `event.voice_left` (ignoré hors du salon actif). */
  applyLeft: (params: { group_id: string; channel_id: string; pubkey: string }) => void;
  /** Applique `event.voice_speaking` sur un participant connu. */
  applySpeaking: (params: { pubkey: string; speaking: boolean }) => void;
  /** Applique `event.voice_mute` (micro/deafen) sur un participant connu. */
  applyMuteState: (params: { pubkey: string; muted: boolean; deafened: boolean }) => void;
  /**
   * Applique `event.voice_moderate` (modération serveur) sur un participant
   * du salon de groupe actif — jamais émis pour une session d'appel 1-à-1
   * (toujours `false` côté nœud). Ignoré hors du salon concerné.
   */
  applyVoiceModerate: (params: {
    group_id: string;
    pubkey: string;
    server_muted: boolean;
    server_deafened: boolean;
    priority_speaker: boolean;
  }) => void;
}

/** Borne un volume en entier 0-200 % (frontière utilisateur : curseurs). */
export function clampVolume(volume: number): number {
  if (!Number.isFinite(volume)) return VOLUME_DEFAULT;
  return Math.min(VOLUME_MAX, Math.max(0, Math.round(volume)));
}

/** Participant fraîchement rejoint : aucun état particulier. */
function idleParticipant(): ParticipantState {
  return {
    speaking: false,
    muted: false,
    deafened: false,
    volume: VOLUME_DEFAULT,
    serverMuted: false,
    serverDeafened: false,
    prioritySpeaker: false,
  };
}

/** Construit la table des participants (personne ne parle au départ). */
function participantsFrom(pubkeys: string[]): Map<string, ParticipantState> {
  return new Map(pubkeys.map((pubkey) => [pubkey, idleParticipant()]));
}

/** Vrai si l'événement concerne le salon actuellement rejoint. */
function matchesActive(
  active: ActiveVoice | null,
  groupId: string,
  channelId: string,
): active is ActiveVoice {
  return active !== null && active.groupId === groupId && active.channelId === channelId;
}

const DSP_DEFAULT: VoiceDspState = { noiseSuppression: false, agc: false };

export const useVoice = create<VoiceState>((set, get) => ({
  active: null,
  selfDeafened: false,
  masterVolume: VOLUME_DEFAULT,
  participants: new Map(),
  dsp: DSP_DEFAULT,

  join: async (groupId, channelId) => {
    const { participants } = await api.voiceJoin(groupId, channelId);
    set({
      active: { groupId, channelId, muted: false, isCall: false },
      selfDeafened: false,
      participants: participantsFrom(participants),
    });
  },

  leave: async () => {
    await api.voiceLeave();
    set({ active: null, selfDeafened: false, participants: new Map() });
  },

  toggleMute: async () => {
    const active = get().active;
    if (active === null) return;
    await get().setMuted(!active.muted);
  },

  setMuted: async (muted) => {
    const active = get().active;
    if (active === null || active.muted === muted) return;
    await api.voiceMute(muted);
    const current = get().active;
    if (current === null) return;
    // Sourd : le nœud garde le micro coupé et mémorise l'état demandé.
    set({ active: { ...current, muted: get().selfDeafened ? true : muted } });
  },

  toggleDeafen: async () => {
    await get().setDeafened(!get().selfDeafened);
  },

  setDeafened: async (on) => {
    const { active, selfDeafened } = get();
    if (active === null || selfDeafened === on) return;
    await api.voiceDeafen(on);
    if (on) {
      const current = get().active;
      if (current === null) return;
      // Le deafen force le micro coupé (sémantique Discord).
      set({ selfDeafened: true, active: { ...current, muted: true } });
      return;
    }
    // Rétablissement : le nœud restaure l'état de micro demandé avant le
    // deafen ; on relit l'état complet plutôt que de le dupliquer ici.
    set({ selfDeafened: false });
    await get().sync();
  },

  setVolume: async (peer, volume) => {
    const clamped = clampVolume(volume);
    await api.voiceSetVolume(peer, clamped);
    if (peer === null) {
      set({ masterVolume: clamped });
      return;
    }
    set((s) => {
      const current = s.participants.get(peer);
      if (current === undefined || current.volume === clamped) return s;
      const participants = new Map(s.participants);
      participants.set(peer, { ...current, volume: clamped });
      return { participants };
    });
  },

  sync: async () => {
    const { active, master_volume: masterVolume, dsp } = await api.voiceStatus();
    const nextDsp: VoiceDspState = {
      noiseSuppression: dsp?.noise_suppression ?? DSP_DEFAULT.noiseSuppression,
      agc: dsp?.agc ?? DSP_DEFAULT.agc,
    };
    if (active === null) {
      set({
        active: null,
        selfDeafened: false,
        masterVolume,
        participants: new Map(),
        dsp: nextDsp,
      });
      return;
    }
    set({
      active: {
        groupId: active.group_id,
        channelId: active.channel_id,
        muted: active.muted,
        isCall: active.is_call ?? false,
      },
      selfDeafened: active.deafened,
      masterVolume,
      dsp: nextDsp,
      participants: new Map(
        active.participants.map((p) => [
          p.pubkey,
          {
            speaking: p.speaking,
            muted: p.muted,
            deafened: p.deafened,
            volume: p.volume,
            serverMuted: p.server_muted ?? false,
            serverDeafened: p.server_deafened ?? false,
            prioritySpeaker: p.priority_speaker ?? false,
          },
        ]),
      ),
    });
  },

  loadMasterVolume: async () => {
    const { master_volume: masterVolume, dsp } = await api.voiceStatus();
    set({
      masterVolume,
      dsp: {
        noiseSuppression: dsp?.noise_suppression ?? DSP_DEFAULT.noiseSuppression,
        agc: dsp?.agc ?? DSP_DEFAULT.agc,
      },
    });
  },

  setNoiseSuppression: async (enabled) => {
    await api.voiceSetNoiseSuppression(enabled);
    set((s) => ({ dsp: { ...s.dsp, noiseSuppression: enabled } }));
  },

  setAgc: async (enabled) => {
    await api.voiceSetAgc(enabled);
    set((s) => ({ dsp: { ...s.dsp, agc: enabled } }));
  },

  applyJoined: ({ group_id, channel_id, pubkey }) => {
    set((s) => {
      if (!matchesActive(s.active, group_id, channel_id)) return s;
      if (s.participants.has(pubkey)) return s;
      const participants = new Map(s.participants);
      participants.set(pubkey, idleParticipant());
      return { participants };
    });
  },

  applyLeft: ({ group_id, channel_id, pubkey }) => {
    set((s) => {
      if (!matchesActive(s.active, group_id, channel_id)) return s;
      if (!s.participants.has(pubkey)) return s;
      const participants = new Map(s.participants);
      participants.delete(pubkey);
      return { participants };
    });
  },

  applySpeaking: ({ pubkey, speaking }) => {
    set((s) => {
      const current = s.participants.get(pubkey);
      if (current === undefined || current.speaking === speaking) return s;
      const participants = new Map(s.participants);
      participants.set(pubkey, { ...current, speaking });
      return { participants };
    });
  },

  applyMuteState: ({ pubkey, muted, deafened }) => {
    set((s) => {
      const current = s.participants.get(pubkey);
      if (current === undefined || (current.muted === muted && current.deafened === deafened)) {
        return s;
      }
      const participants = new Map(s.participants);
      participants.set(pubkey, { ...current, muted, deafened });
      return { participants };
    });
  },

  applyVoiceModerate: ({ group_id, pubkey, server_muted, server_deafened, priority_speaker }) => {
    set((s) => {
      // Jamais émis pour une session d'appel (group_id sentinelle) : la
      // comparaison directe suffit à ignorer l'événement sans jamais indexer
      // `useGroups.states` par la sentinelle.
      if (s.active === null || s.active.groupId !== group_id) return s;
      const current = s.participants.get(pubkey);
      if (
        current === undefined ||
        (current.serverMuted === server_muted &&
          current.serverDeafened === server_deafened &&
          current.prioritySpeaker === priority_speaker)
      ) {
        return s;
      }
      const participants = new Map(s.participants);
      participants.set(pubkey, {
        ...current,
        serverMuted: server_muted,
        serverDeafened: server_deafened,
        prioritySpeaker: priority_speaker,
      });
      return { participants };
    });
  },
}));
