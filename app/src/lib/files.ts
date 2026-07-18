/**
 * Lecture de fichiers du magasin (`files.read`) : rend une URL `data:`
 * affichable, avec cache LRU borné hash → URL (les URL `data:` pèsent
 * jusqu'à ~8 Mio de base64 : sans borne, la mémoire croîtrait sans limite).
 * Les URL `blob:` (`URL.createObjectURL`) ne sont pas rendues par la
 * WKWebView de l'app packagée (Tauri/macOS) — images cassées malgré une CSP
 * `img-src blob:` correcte ; les URL `data:` s'affichent partout.
 *
 * `lireMiniature` produit une version réduite (WebP) pour les vignettes du
 * fil : la pleine résolution reste servie par `lireFichier` (Lightbox).
 *
 * Si le contenu n'est pas complet en local, le nœud rend `{ pending: true }`
 * et lance le téléchargement : on attend alors `event.file_progress` avec
 * `complete: true` (délai glissant, réarmé à chaque progression) avant de
 * relire. Un abandon signalé par le nœud (événement `complete: false` sans
 * avancée de `done`) fait échouer l'attente en cours rapidement, mais deux
 * reprises automatiques (backoff court) sont tentées avant de rejeter — le
 * nœud retente lui-même en arrière-plan, et l'abonnement reste vivant à
 * travers les reprises (un `complete: true` tardif résout toujours). Un
 * silence prolongé rejette ; la promesse est alors retirée du cache pour
 * permettre une nouvelle tentative.
 */

import { api, rpc } from './client';
import type { FilesReadResult, FilesStatusResult } from './api';

/** Délai glissant sans progression avant abandon de l'attente. */
export const FILE_WAIT_TIMEOUT_MS = 30_000;

/** Backoffs des reprises automatiques après un abandon signalé par le nœud. */
export const FILE_RETRY_BACKOFF_MS = [2_000, 5_000] as const;

/** Capacité des caches d'URL : borne la mémoire retenue par les `data:`. */
export const FILE_CACHE_MAX = 64;

/**
 * Cache LRU borné. Invariant : une `Map` itère en ordre d'insertion, donc
 * ré-insérer une entrée à chaque accès la marque « plus récente » ; quand la
 * capacité est dépassée, l'éviction retire la première clé de l'itération —
 * la moins récemment utilisée.
 */
class CacheLru<V> {
  private readonly entrees = new Map<string, V>();

  constructor(private readonly capacite: number) {}

  get(cle: string): V | undefined {
    const valeur = this.entrees.get(cle);
    if (valeur === undefined) return undefined;
    // Ré-insertion : l'entrée redevient la plus récente de l'ordre d'itération.
    this.entrees.delete(cle);
    this.entrees.set(cle, valeur);
    return valeur;
  }

  set(cle: string, valeur: V): void {
    this.entrees.delete(cle);
    this.entrees.set(cle, valeur);
    if (this.entrees.size > this.capacite) {
      // Éviction de la plus ancienne : première clé de l'ordre d'insertion.
      const plusAncienne = this.entrees.keys().next().value;
      if (plusAncienne !== undefined) this.entrees.delete(plusAncienne);
    }
  }

  delete(cle: string): void {
    this.entrees.delete(cle);
  }
}

/** Cache module-scope : la promesse est partagée entre appels concurrents. */
const cache = new CacheLru<Promise<string>>(FILE_CACHE_MAX);

/** Octets base64 → URL `data:` affichable (compatible WKWebView). */
function toDataUrl(dataB64: string, mime: string): string {
  return `data:${mime};base64,${dataB64}`;
}

/**
 * Attend la fin du téléchargement de `merkleRoot` (`complete: true`).
 *
 * - Une progression réelle (`done` qui avance) réarme le délai d'inactivité.
 * - Un abandon signalé (`complete: false` sans avancée de `done` — le nœud
 *   ré-émet l'état courant quand il diffère la tentative) fait échouer
 *   l'attente en cours SANS attendre le délai complet : après un court
 *   backoff (`FILE_RETRY_BACKOFF_MS`), l'intention est re-signalée au nœud
 *   (`files.read`) et une nouvelle fenêtre d'attente s'ouvre. L'abonnement
 *   reste vivant pendant le backoff : un `complete: true` tardif résout.
 * - Reprises épuisées (ou silence prolongé) : la promesse rejette.
 */
function waitForDownload(merkleRoot: string, hint?: string): Promise<void> {
  return new Promise((resolve, reject) => {
    let timer: ReturnType<typeof setTimeout> | null = null;
    let lastDone = -1;
    let retriesLeft: number = FILE_RETRY_BACKOFF_MS.length;
    let settled = false;
    const finish = (err: Error | null): void => {
      if (settled) return;
      settled = true;
      if (timer !== null) clearTimeout(timer);
      off();
      if (err === null) resolve();
      else reject(err);
    };
    const arm = (ms: number, onExpire: () => void): void => {
      if (timer !== null) clearTimeout(timer);
      timer = setTimeout(onExpire, ms);
    };
    const expire = (): void => finish(new Error('téléchargement du fichier interrompu'));
    /** Échec de l'attente en cours : reprise avec backoff, ou rejet. */
    const attemptFailed = (): void => {
      if (retriesLeft === 0) {
        finish(new Error('téléchargement du fichier interrompu'));
        return;
      }
      const backoff =
        FILE_RETRY_BACKOFF_MS[FILE_RETRY_BACKOFF_MS.length - retriesLeft] ?? 0;
      retriesLeft -= 1;
      arm(backoff, () => {
        // Re-signale l'intention au nœud ; s'il a fini entre-temps
        // (événement manqué), on résout tout de suite.
        api
          .filesRead(merkleRoot, hint, true)
          .then((r: FilesReadResult) => {
            if (r.pending !== true) finish(null);
          })
          .catch(() => {
            // Relance impossible : le délai d'inactivité tranchera.
          });
        arm(FILE_WAIT_TIMEOUT_MS, expire);
      });
    };
    const off = rpc.onEvent((method, params) => {
      if (method !== 'event.file_progress') return;
      const p = params as { merkle_root?: string; done?: number; complete?: boolean };
      if (p.merkle_root !== merkleRoot) return;
      if (p.complete === true) {
        finish(null);
        return;
      }
      const done = p.done ?? 0;
      if (done > lastDone) {
        // Progression réelle : on réarme le délai d'inactivité.
        lastDone = done;
        arm(FILE_WAIT_TIMEOUT_MS, expire);
        return;
      }
      // `done` stagnant + `complete: false` : abandon signalé par le nœud.
      attemptFailed();
    });
    arm(FILE_WAIT_TIMEOUT_MS, expire);
  });
}

async function fetchDataUrl(merkleRoot: string, hint?: string): Promise<string> {
  // `media: true` : la lecture en ligne (`lireFichier`) est bornée à 8 Mio,
  // donc le téléchargement déclenché est plafonné d'autant (anti-DoS média).
  const first: FilesReadResult = await api.filesRead(merkleRoot, hint, true);
  if (first.pending !== true) return toDataUrl(first.data_b64, first.mime);
  await waitForDownload(merkleRoot, hint);
  const second: FilesReadResult = await api.filesRead(merkleRoot, hint);
  if (second.pending === true) {
    throw new Error('fichier toujours incomplet après téléchargement');
  }
  return toDataUrl(second.data_b64, second.mime);
}

/**
 * Lit un fichier par sa racine Merkle et rend une URL `data:` réutilisable.
 * `hint` : clé publique d'un pair source probable (expéditeur du message).
 */
/**
 * Télécharge un blob COMPLET (sans plafond — chemin sollicité par un clic,
 * D-055) et rend son chemin disque local, prêt à être servi en streaming via
 * le protocole asset (lecteur vidéo). Un blob déjà complet rend son chemin
 * immédiatement.
 */
export async function telechargerComplet(
  merkleRoot: string,
  hint?: string,
): Promise<string> {
  const cheminLocal = async (): Promise<string | null> => {
    const statut = await api.filesStatus(merkleRoot, hint);
    return statut.complete && typeof statut.path === 'string' && statut.path !== ''
      ? statut.path
      : null;
  };
  const deja = await cheminLocal();
  if (deja !== null) return deja;
  try {
    // Déclenche (ou poursuit) le téléchargement non plafonné.
    await api.filesRead(merkleRoot, hint);
  } catch {
    // « trop volumineux pour une lecture en ligne » : le blob est en fait
    // DÉJÀ complet en local — le statut ci-dessous rend son chemin.
  }
  const direct = await cheminLocal();
  if (direct !== null) return direct;
  await waitForDownload(merkleRoot, hint);
  const fin = await cheminLocal();
  if (fin === null) throw new Error('téléchargement incomplet');
  return fin;
}

export function lireFichier(merkleRoot: string, hint?: string): Promise<string> {
  const cached = cache.get(merkleRoot);
  if (cached !== undefined) return cached;
  const promise = fetchDataUrl(merkleRoot, hint);
  cache.set(merkleRoot, promise);
  // Échec : on libère l'entrée pour qu'une prochaine lecture retente.
  promise.catch(() => cache.delete(merkleRoot));
  return promise;
}

/** Bord maximal (px) des miniatures affichées dans le fil de messages. */
export const MINIATURE_MAX_PX = 512;

/** Qualité WebP des miniatures (compromis taille mémoire / netteté). */
const MINIATURE_QUALITE = 0.82;

/**
 * Délai au-delà duquel le décodage canvas d'une miniature est abandonné au
 * profit de la pleine résolution. Certaines WKWebView (app packagée macOS) ne
 * déclenchent ni `onload` ni `onerror` sur un `data:` volumineux : sans cette
 * borne, la vignette resterait en chargement perpétuel (« l'image ne charge pas »).
 */
const MINIATURE_TIMEOUT_MS = 4_000;

/** Cache LRU des miniatures, séparé du cache pleine taille. Clé : `maxPx:hash`. */
const cacheMiniatures = new CacheLru<Promise<string>>(FILE_CACHE_MAX);

/**
 * Réduit une image (URL `data:`) à `maxPx` de bord au plus, ré-encodée en
 * WebP. Best effort : si l'API Image/canvas est indisponible (jsdom des
 * tests, `getContext('2d')` rend null) ou si le décodage échoue, rend l'URL
 * source telle quelle — l'appelant reste fonctionnel, seule l'économie
 * mémoire est perdue. La promesse rendue ne rejette jamais.
 */
function reduireImage(url: string, maxPx: number): Promise<string> {
  if (typeof Image === 'undefined' || typeof document === 'undefined') {
    return Promise.resolve(url);
  }
  const canvas = document.createElement('canvas');
  const contexte = canvas.getContext('2d');
  if (contexte === null) return Promise.resolve(url);
  return new Promise((resolve) => {
    const image = new Image();
    let regle = false;
    const finir = (resultat: string): void => {
      if (regle) return;
      regle = true;
      clearTimeout(minuteur);
      resolve(resultat);
    };
    // Filet de sécurité : décodage qui ne se termine jamais (WKWebView) →
    // pleine résolution, jamais de vignette bloquée en chargement. `finir`
    // n'est appelée que de façon asynchrone : `minuteur` est toujours
    // initialisé au moment où elle s'exécute.
    const minuteur = setTimeout(() => finir(url), MINIATURE_TIMEOUT_MS);
    image.onload = () => {
      const bord = Math.max(image.naturalWidth, image.naturalHeight);
      if (bord <= maxPx) {
        // Déjà assez petite (ou dimensions inconnues) : rien à réduire.
        finir(url);
        return;
      }
      try {
        const facteur = maxPx / bord;
        canvas.width = Math.max(1, Math.round(image.naturalWidth * facteur));
        canvas.height = Math.max(1, Math.round(image.naturalHeight * facteur));
        contexte.drawImage(image, 0, 0, canvas.width, canvas.height);
        const reduit = canvas.toDataURL('image/webp', MINIATURE_QUALITE);
        // Certaines WKWebView rendent un `data:` inexploitable pour un type
        // non pris en charge (WebP) sans lever d'exception : on ne propage la
        // miniature que si elle est bien une image `data:`, sinon pleine taille.
        finir(reduit.startsWith('data:image/') ? reduit : url);
      } catch {
        // Encodage impossible : on retombe sur la pleine taille.
        finir(url);
      }
    };
    // Décodage impossible : pleine taille (l'<img> aval signalera l'erreur).
    image.onerror = () => finir(url);
    image.src = url;
  });
}

/**
 * Lit une image et rend une URL `data:` miniature (bord ≤ `maxPx`, WebP)
 * pour l'affichage en vignette. La lecture source passe par `lireFichier`
 * (téléchargement, attente, cache pleine taille partagés) ; la Lightbox
 * garde `lireFichier` pour la pleine résolution.
 */
export function lireMiniature(
  merkleRoot: string,
  hint?: string,
  maxPx: number = MINIATURE_MAX_PX,
): Promise<string> {
  const cle = `${maxPx}:${merkleRoot}`;
  const cachee = cacheMiniatures.get(cle);
  if (cachee !== undefined) return cachee;
  const promesse = lireFichier(merkleRoot, hint).then((url) => reduireImage(url, maxPx));
  cacheMiniatures.set(cle, promesse);
  // Échec (lecture source) : on libère l'entrée pour qu'une lecture retente.
  promesse.catch(() => cacheMiniatures.delete(cle));
  return promesse;
}

/** Métadonnées et progression locales d'un fichier (`files.status`). */
export function statutFichier(
  merkleRoot: string,
  hint?: string,
): Promise<FilesStatusResult> {
  return api.filesStatus(merkleRoot, hint);
}

/**
 * Suit la progression du téléchargement de `merkleRoot` : `onProgress` est
 * appelé à chaque `event.file_progress` du nœud. Rend le désabonnement.
 */
export function observerProgression(
  merkleRoot: string,
  onProgress: (done: number, total: number, complete: boolean) => void,
): () => void {
  return rpc.onEvent((method, params) => {
    if (method !== 'event.file_progress') return;
    const p = params as {
      merkle_root?: string;
      done?: number;
      total?: number;
      complete?: boolean;
    };
    if (p.merkle_root !== merkleRoot) return;
    onProgress(p.done ?? 0, p.total ?? 0, p.complete === true);
  });
}
