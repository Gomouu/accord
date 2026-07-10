/**
 * Persistance de la mémoire de navigation (façon Discord) : dernier salon
 * consulté par serveur et dernière conversation privée ouverte. Suit le même
 * schéma de lecture/écriture localStorage tolérante que `stores/ui.ts`, sous
 * des clés dédiées pour ne pas entrer en collision avec les préférences.
 */

const STORAGE_KEYS = {
  lastChannelByServer: 'accord.nav.lastChannelByServer',
  lastDm: 'accord.nav.lastDm',
} as const;

function readStored(key: string): string | null {
  try {
    return window.localStorage.getItem(key);
  } catch {
    return null;
  }
}

function writeStored(key: string, value: string): void {
  try {
    window.localStorage.setItem(key, value);
  } catch {
    // Best effort : la mémoire reste valable pour la session en cours.
  }
}

function removeStored(key: string): void {
  try {
    window.localStorage.removeItem(key);
  } catch {
    // Best effort.
  }
}

/**
 * Charge la carte serveur → dernier salon consulté. Une valeur stockée
 * corrompue ou de forme inattendue replie sur une carte vide plutôt que de
 * faire planter le démarrage.
 */
export function loadLastChannelByServer(): Record<string, string> {
  const raw = readStored(STORAGE_KEYS.lastChannelByServer);
  if (raw === null) return {};
  try {
    const parsed: unknown = JSON.parse(raw);
    if (parsed === null || typeof parsed !== 'object' || Array.isArray(parsed)) return {};
    const result: Record<string, string> = {};
    for (const [groupId, channelId] of Object.entries(
      parsed as Record<string, unknown>,
    )) {
      if (typeof channelId === 'string') result[groupId] = channelId;
    }
    return result;
  } catch {
    return {};
  }
}

export function saveLastChannelByServer(map: Readonly<Record<string, string>>): void {
  writeStored(STORAGE_KEYS.lastChannelByServer, JSON.stringify(map));
}

/** Dernier pair de conversation privée ouvert, ou `null` si aucun connu. */
export function loadLastDm(): string | null {
  return readStored(STORAGE_KEYS.lastDm);
}

export function saveLastDm(peer: string | null): void {
  if (peer === null) removeStored(STORAGE_KEYS.lastDm);
  else writeStored(STORAGE_KEYS.lastDm, peer);
}
