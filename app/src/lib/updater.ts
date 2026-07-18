/**
 * Mise à jour intégrée (D-049) : enveloppe fine de @tauri-apps/plugin-updater.
 * Le manifeste `latest.json` de la dernière release GitHub est consulté côté
 * hôte (hors CSP webview) et chaque artefact est authentifié par signature
 * minisign avant installation. Imports dynamiques : le plugin ne pèse pas sur
 * le chargement initial, et hors Tauri (dev navigateur) tout est neutre.
 */

import type { Update } from '@tauri-apps/plugin-updater';
import { isTauri } from './bridge';

/** Page des releases — repli manuel affiché quand l'installation échoue. */
export const RELEASES_URL = 'https://github.com/Gomouu/accord/releases/latest';

export interface UpdateInfo {
  version: string;
  /** Notes de version du manifeste (corps de la release), si fournies. */
  notes: string | null;
}

/** Avancement du téléchargement ; `total` inconnu si le serveur le tait. */
export type OnProgress = (downloaded: number, total: number | null) => void;

/**
 * Mise à jour trouvée par le dernier `checkForUpdate`, conservée pour que
 * `downloadAndInstall` réutilise la même réponse signée (pas de re-check).
 */
let pendingUpdate: Update | null = null;

/**
 * Interroge le point de terminaison de mise à jour. `null` : aucune version
 * plus récente (ou hors Tauri). Les erreurs réseau/manifeste sont propagées.
 */
export async function checkForUpdate(): Promise<UpdateInfo | null> {
  if (!isTauri()) return null;
  const { check } = await import('@tauri-apps/plugin-updater');
  const update = await check();
  pendingUpdate = update;
  if (update === null) return null;
  return { version: update.version, notes: update.body ?? null };
}

/**
 * Télécharge puis installe la mise à jour trouvée par `checkForUpdate`.
 * Sous Windows, l'installateur ferme l'application lui-même ; ailleurs,
 * l'installation est effective au prochain démarrage (`restartApp`).
 */
export async function downloadAndInstall(onProgress: OnProgress): Promise<void> {
  const update = pendingUpdate;
  if (update === null) {
    throw new Error('aucune mise à jour en attente — lancer checkForUpdate d’abord');
  }
  let downloaded = 0;
  let total: number | null = null;
  await update.downloadAndInstall((event) => {
    if (event.event === 'Started') {
      total = event.data.contentLength ?? null;
      onProgress(0, total);
    } else if (event.event === 'Progress') {
      downloaded += event.data.chunkLength;
      onProgress(downloaded, total);
    }
  });
}

/** Redémarre l'application pour appliquer la mise à jour installée. */
export async function restartApp(): Promise<void> {
  const { relaunch } = await import('@tauri-apps/plugin-process');
  await relaunch();
}
