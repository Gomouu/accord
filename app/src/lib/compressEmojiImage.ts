/**
 * Compression client d'une image façon Discord : quelle que soit la taille de
 * l'original choisi par l'utilisateur, on produit des octets garantis sous la
 * limite demandée sans changer le contrat d'envoi (mêmes types MIME acceptés
 * — voir `lib/emoji.ts`). Paramétrée (`CompressOptions`) plutôt que dupliquée :
 * les émojis de serveur (défaut, 256 Kio / 128 px) et les stickers de serveur
 * (512 Kio / 320 px, voir `lib/sticker.ts`) partagent exactement le même
 * pipeline, seules les bornes numériques diffèrent.
 *
 * - Images statiques (PNG/JPEG/WebP, GIF non animé) : redessinées sur un
 *   canvas hors-écran, réduites pour tenir dans le plafond d'affichage
 *   demandé (ratio conservé, aucun recadrage, fond transparent préservé),
 *   puis encodées en WebP (repli PNG si l'encodage WebP est indisponible) en
 *   dégradant qualité puis dimensions jusqu'à passer sous la limite.
 * - GIF et WebP animés : un ré-encodage canvas ne capturerait qu'une image
 *   fixe (perte de l'animation), donc l'original est transmis tel quel s'il
 *   tient déjà sous la limite ; sinon on échoue avec un message dédié (pas de
 *   dépendance de recompression ajoutée).
 */

import { fichierEnB64, fichierEnDataUrl } from './attachments';
import { EMOJI_OCTETS_MAX } from './emoji';
import { chargerImage, octetsBase64 } from './image';

/** Plafond d'affichage d'un émoji (px) : ceiling de mise à l'échelle. */
export const EMOJI_TAILLE_MAX_PX = 128;

/** Paliers de dimension tentés, du plus grand au plus petit. */
const PALIERS_TAILLE = [EMOJI_TAILLE_MAX_PX, 96, 64] as const;

/** Paliers de qualité WebP tentés à chaque palier de dimension. */
const PALIERS_QUALITE = [0.9, 0.7, 0.5] as const;

/** Raison d'échec de compression, pour un message utilisateur ciblé. */
export type RaisonEchecCompression = 'anime-trop-lourd' | 'compression-impossible';

/** Échec de compression d'une image d'émoji (contrat non atteignable). */
export class EmojiCompressionError extends Error {
  constructor(public readonly raison: RaisonEchecCompression) {
    super(raison);
    this.name = 'EmojiCompressionError';
  }
}

/** Image d'émoji encodée, prête pour l'aperçu et `groups.emoji.add`. */
export interface EmojiImageEncode {
  dataB64: string;
  mime: string;
  dataUrl: string;
}

/**
 * Vrai si les octets `bytes` forment un GIF portant plus d'une image (donc
 * une animation). Marche par un parcours minimal des blocs GIF (en-tête,
 * table de couleurs globale, extensions, descripteurs d'image) sans décoder
 * les pixels — s'arrête dès qu'un deuxième descripteur d'image est rencontré.
 * Rend `false` sur un flux tronqué ou malformé (traité comme statique : au
 * pire on retente un ré-encodage canvas qui échouera proprement).
 */
export function estGifAnime(bytes: Uint8Array): boolean {
  if (bytes.length < 13) return false;
  // Signature « GIF87a » ou « GIF89a ».
  const signature = String.fromCharCode(...bytes.subarray(0, 3));
  if (signature !== 'GIF') return false;

  let offset = 6; // Après la signature + version (6 octets).
  // Descripteur d'écran logique : largeur(2) hauteur(2) empaqueté(1) fond(1) aspect(1).
  const empaquete = bytes[offset + 4] ?? 0;
  offset += 7;
  if ((empaquete & 0x80) !== 0) {
    const tailleTable = 3 * Math.pow(2, (empaquete & 0x07) + 1);
    offset += tailleTable;
  }

  let nbImages = 0;
  while (offset < bytes.length) {
    const marqueur = bytes[offset];
    if (marqueur === 0x21) {
      // Extension : introducteur + étiquette, puis sous-blocs terminés par 0.
      offset += 2;
      offset = sauterSousBlocs(bytes, offset);
    } else if (marqueur === 0x2c) {
      nbImages += 1;
      if (nbImages > 1) return true;
      // Descripteur d'image : gauche/haut/largeur/hauteur (8) + empaqueté (1).
      const empaqueteImg = bytes[offset + 9] ?? 0;
      offset += 10;
      if ((empaqueteImg & 0x80) !== 0) {
        const tailleTable = 3 * Math.pow(2, (empaqueteImg & 0x07) + 1);
        offset += tailleTable;
      }
      offset += 1; // Taille minimale de code LZW.
      offset = sauterSousBlocs(bytes, offset);
    } else {
      // Fin de flux (0x3b) ou octet inattendu : on arrête le parcours.
      break;
    }
  }
  return false;
}

/** Avance `offset` après une suite de sous-blocs `[taille][octets]…[0]`. */
function sauterSousBlocs(bytes: Uint8Array, depart: number): number {
  let offset = depart;
  while (offset < bytes.length) {
    const taille = bytes[offset];
    offset += 1;
    if (taille === undefined || taille === 0) break;
    offset += taille;
  }
  return offset;
}

/**
 * Vrai si les octets `bytes` forment un WebP animé. Parcourt les chunks du
 * conteneur RIFF sans décoder les pixels : le drapeau Animation (bit 0x02) du
 * chunk `VP8X` fait foi, et la présence d'un chunk `ANIM`/`ANMF` est acceptée
 * en repli (fichiers étendus sans VP8X conformes a minima). Rend `false` sur
 * un flux tronqué ou malformé — traité comme statique, comme pour les GIF.
 */
export function estWebpAnime(bytes: Uint8Array): boolean {
  if (bytes.length < 16) return false;
  const fourcc = (o: number) =>
    String.fromCharCode(bytes[o] ?? 0, bytes[o + 1] ?? 0, bytes[o + 2] ?? 0, bytes[o + 3] ?? 0);
  if (fourcc(0) !== 'RIFF' || fourcc(8) !== 'WEBP') return false;

  let offset = 12;
  while (offset + 8 <= bytes.length) {
    const nom = fourcc(offset);
    const taille =
      ((bytes[offset + 4] ?? 0) |
        ((bytes[offset + 5] ?? 0) << 8) |
        ((bytes[offset + 6] ?? 0) << 16) |
        ((bytes[offset + 7] ?? 0) << 24)) >>>
      0;
    if (nom === 'VP8X') return ((bytes[offset + 8] ?? 0) & 0x02) !== 0;
    if (nom === 'ANIM' || nom === 'ANMF') return true;
    // Les chunks RIFF sont alignés sur 2 octets (octet de bourrage si impair).
    offset += 8 + taille + (taille % 2);
  }
  return false;
}

/** Dimensions réduites (ratio conservé, jamais agrandies) tenant dans `maxCote`. */
export function ajusterDimensions(
  largeur: number,
  hauteur: number,
  maxCote: number,
): { w: number; h: number } {
  const l = Math.max(1, largeur);
  const h = Math.max(1, hauteur);
  const echelle = Math.min(1, maxCote / Math.max(l, h));
  return {
    w: Math.max(1, Math.round(l * echelle)),
    h: Math.max(1, Math.round(h * echelle)),
  };
}

/** Dessine `source` réduite dans `w`×`h` sur un canvas transparent hors-écran. */
function dessiner(source: CanvasImageSource, w: number, h: number): HTMLCanvasElement {
  const canvas = document.createElement('canvas');
  canvas.width = w;
  canvas.height = h;
  const contexte = canvas.getContext('2d');
  if (contexte === null) throw new Error('canvas indisponible');
  contexte.clearRect(0, 0, w, h);
  contexte.drawImage(source, 0, 0, w, h);
  return canvas;
}

/** Encode `canvas` en WebP (qualité `quality`), repli PNG si WebP indisponible. */
function encoder(canvas: HTMLCanvasElement, quality: number): EmojiImageEncode {
  const webp = canvas.toDataURL('image/webp', quality);
  const dataUrl = webp.startsWith('data:image/webp')
    ? webp
    : canvas.toDataURL('image/png');
  const mime = dataUrl.startsWith('data:image/webp') ? 'image/webp' : 'image/png';
  return { dataUrl, mime, dataB64: dataUrl.slice(dataUrl.indexOf(',') + 1) };
}

/** Redessine `img` en dégradant dimension/qualité jusqu'à tenir sous `maxBytes`. */
function compresserStatique(
  img: HTMLImageElement,
  maxBytes: number,
  sizes: readonly number[],
): EmojiImageEncode {
  const largeur = img.naturalWidth || img.width;
  const hauteur = img.naturalHeight || img.height;
  for (const palier of sizes) {
    const { w, h } = ajusterDimensions(largeur, hauteur, palier);
    const canvas = dessiner(img, w, h);
    for (const quality of PALIERS_QUALITE) {
      const resultat = encoder(canvas, quality);
      if (octetsBase64(resultat.dataB64) <= maxBytes) return resultat;
      // PNG ignore la qualité : inutile de retenter la même sortie.
      if (resultat.mime === 'image/png') break;
    }
  }
  throw new EmojiCompressionError('compression-impossible');
}

/**
 * Octets décodés d'une chaîne base64 (sans préfixe `data:`). Passe par
 * `atob` plutôt que `Blob.arrayBuffer()` : la WKWebView packagée et jsdom (en
 * test) n'exposent pas systématiquement cette méthode sur `File`, alors que
 * `fichierEnB64` (via `FileReader`) est déjà la voie éprouvée du projet.
 */
function octetsDepuisBase64(b64: string): Uint8Array {
  const binaire = atob(b64);
  const bytes = new Uint8Array(binaire.length);
  for (let i = 0; i < binaire.length; i += 1) bytes[i] = binaire.charCodeAt(i);
  return bytes;
}

/** Image animée sous la limite : transmise telle quelle, sinon échec dédié. */
function passerAnime(
  dataB64: string,
  mime: 'image/gif' | 'image/webp',
  tailleOctets: number,
  maxBytes: number,
): EmojiImageEncode {
  if (tailleOctets > maxBytes) {
    throw new EmojiCompressionError('anime-trop-lourd');
  }
  return { dataB64, mime, dataUrl: `data:${mime};base64,${dataB64}` };
}

/** Bornes de compression paramétrables (défauts : émoji de serveur). */
export interface CompressOptions {
  /** Taille max en octets décodés (défaut `EMOJI_OCTETS_MAX`, 256 Kio). */
  maxBytes?: number;
  /** Paliers de dimension tentés, du plus grand au plus petit (défaut `PALIERS_TAILLE`). */
  sizes?: readonly number[];
}

/**
 * Compresse une image choisie par l'utilisateur pour tenir sous
 * `options.maxBytes` (défaut `EMOJI_OCTETS_MAX`) une fois décodée, mise à
 * l'échelle par paliers de `options.sizes` (défaut `PALIERS_TAILLE`). Ne
 * touche jamais aux GIF ni aux WebP animés au-delà d'une vérification de
 * taille (voir en-tête de fichier) ; toute autre image (y compris un GIF ou
 * un WebP statique) passe par le pipeline canvas. Les stickers de serveur
 * (`lib/sticker.ts`) réutilisent ce même pipeline avec des bornes plus larges
 * plutôt que de le dupliquer.
 */
export async function compressEmojiImage(
  fichier: File,
  options: CompressOptions = {},
): Promise<EmojiImageEncode> {
  const maxBytes = options.maxBytes ?? EMOJI_OCTETS_MAX;
  const sizes = options.sizes ?? PALIERS_TAILLE;
  if (fichier.type === 'image/gif' || fichier.type === 'image/webp') {
    const dataB64 = await fichierEnB64(fichier);
    const bytes = octetsDepuisBase64(dataB64);
    const anime =
      fichier.type === 'image/gif' ? estGifAnime(bytes) : estWebpAnime(bytes);
    if (anime) {
      return passerAnime(dataB64, fichier.type, bytes.byteLength, maxBytes);
    }
  }
  const url = await fichierEnDataUrl(fichier);
  const img = await chargerImage(url);
  return compresserStatique(img, maxBytes, sizes);
}
