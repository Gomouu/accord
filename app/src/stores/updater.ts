/**
 * État de la mise à jour intégrée (D-049) : cycle vérification →
 * téléchargement → redémarrage, partagé entre la bannière (App) et la
 * section « Mises à jour » des paramètres (onglet Système).
 *
 * La vérification automatique (démarrage + périodique) et la vérification
 * manuelle passent par la même action `check` ; seul le traitement des
 * échecs diffère (silencieux en automatique, visible en manuel).
 */

import { create } from 'zustand';
import { checkForUpdate, downloadAndInstall, restartApp } from '../lib/updater';

export type UpdaterStatus =
  'idle' | 'checking' | 'upToDate' | 'available' | 'downloading' | 'ready' | 'error';

interface UpdaterState {
  status: UpdaterStatus;
  /** Version proposée par le manifeste (`null` tant que rien n'est trouvé). */
  version: string | null;
  /** Notes de version fournies par la release, si présentes. */
  notes: string | null;
  /** Avancement du téléchargement, 0..1 (`null` : taille totale inconnue). */
  progress: number | null;
  /** Message de la dernière erreur (réseau, signature, installation). */
  error: string | null;
  /** Version écartée d'un « Plus tard » : la bannière ne revient pas dessus. */
  dismissedVersion: string | null;
  /** Vérifie le manifeste. `manual` : les échecs deviennent visibles. */
  check: (manual?: boolean) => Promise<void>;
  /** Télécharge et installe la version proposée. */
  install: () => Promise<void>;
  /** Redémarre l'application pour appliquer la mise à jour. */
  restart: () => Promise<void>;
  /** Masque la bannière pour la version proposée (paramètres inchangés). */
  dismissBanner: () => void;
}

function messageOf(cause: unknown): string {
  if (cause instanceof Error) return cause.message;
  return String(cause);
}

export const useUpdater = create<UpdaterState>((set, get) => ({
  status: 'idle',
  version: null,
  notes: null,
  progress: null,
  error: null,
  dismissedVersion: null,

  check: async (manual = false) => {
    const { status } = get();
    // Un téléchargement ou une installation en cours n'est jamais interrompu
    // par une vérification (périodique ou manuelle).
    if (status === 'checking' || status === 'downloading' || status === 'ready') {
      return;
    }
    set({ status: 'checking', error: null });
    try {
      const info = await checkForUpdate();
      if (info === null) {
        set({ status: 'upToDate', version: null, notes: null });
        return;
      }
      set({ status: 'available', version: info.version, notes: info.notes });
    } catch (cause) {
      // Échec silencieux en automatique (réseau coupé au démarrage, etc.) :
      // l'état revient à `idle` et la prochaine vérification retentera.
      if (manual) {
        set({ status: 'error', error: messageOf(cause) });
      } else {
        set({ status: 'idle' });
      }
    }
  },

  install: async () => {
    const { status } = get();
    if (status !== 'available' && status !== 'error') return;
    set({ status: 'downloading', progress: 0, error: null });
    try {
      await downloadAndInstall((downloaded, total) => {
        set({
          progress: total !== null && total > 0 ? Math.min(downloaded / total, 1) : null,
        });
      });
      set({ status: 'ready', progress: 1 });
    } catch (cause) {
      set({ status: 'error', error: messageOf(cause), progress: null });
    }
  },

  restart: async () => {
    try {
      await restartApp();
    } catch (cause) {
      set({ status: 'error', error: messageOf(cause) });
    }
  },

  dismissBanner: () => {
    set((s) => ({ dismissedVersion: s.version ?? s.dismissedVersion }));
  },
}));
