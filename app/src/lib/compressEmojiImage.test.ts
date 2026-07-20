/**
 * Tests de la compression d'image d'émoji : détection d'animation GIF
 * (parcours pur des blocs, sans DOM), calcul de dimensions réduites, et
 * pipeline complet (canvas + dégradation qualité/dimensions) avec `Image` et
 * canvas simulés — jsdom ne décode ni ne rend réellement les pixels.
 */

import { afterEach, describe, expect, it, vi } from 'vitest';
import { EMOJI_OCTETS_MAX } from './emoji';
import {
  EmojiCompressionError,
  ajusterDimensions,
  compressEmojiImage,
  estGifAnime,
  estWebpAnime,
} from './compressEmojiImage';

/** Construit un GIF minimal à `nbFrames` images (sans table de couleurs). */
function gifOctets(nbFrames: number): Uint8Array {
  const octets: number[] = [];
  octets.push(...[...'GIF89a'].map((c) => c.charCodeAt(0)));
  octets.push(1, 0, 1, 0, 0x00, 0, 0); // LSD 1×1, pas de table globale.
  for (let i = 0; i < nbFrames; i += 1) {
    octets.push(0x2c, 0, 0, 0, 0, 1, 0, 1, 0, 0x00); // Descripteur d'image.
    octets.push(2); // Taille minimale de code LZW.
    octets.push(1, 0, 0); // Un sous-bloc de donnée + terminateur.
  }
  octets.push(0x3b); // Fin de flux.
  return new Uint8Array(octets);
}

/** Variante avec table de couleurs globale/locale + extension de contrôle graphique. */
function gifOctetsAvecExtensions(nbFrames: number): Uint8Array {
  const octets: number[] = [];
  octets.push(...[...'GIF89a'].map((c) => c.charCodeAt(0)));
  octets.push(2, 0, 2, 0, 0x80, 0, 0); // LSD, table globale de 2 couleurs.
  octets.push(0, 0, 0, 255, 255, 255); // Table de couleurs globale.
  for (let i = 0; i < nbFrames; i += 1) {
    octets.push(0x21, 0xf9, 4, 0x00, 0, 0, 0, 0x00); // Extension de contrôle graphique.
    octets.push(0x2c, 0, 0, 0, 0, 2, 0, 2, 0, 0x80); // Descripteur + table locale.
    octets.push(0, 0, 0, 255, 255, 255); // Table de couleurs locale.
    octets.push(2, 1, 0, 0); // Taille min LZW + sous-bloc + terminateur.
  }
  octets.push(0x3b);
  return new Uint8Array(octets);
}

/**
 * Construit un WebP minimal (conteneur RIFF). `forme` pilote la structure :
 * statique simple (`VP8 `), étendu avec drapeau Animation (`VP8X`), étendu
 * statique, ou animé sans VP8X (chunk `ANIM` seul).
 */
function webpOctets(
  forme: 'simple' | 'vp8x-anime' | 'vp8x-statique' | 'anim-seul',
): Uint8Array {
  const octets: number[] = [];
  const pousserFourcc = (s: string) => octets.push(...[...s].map((c) => c.charCodeAt(0)));
  const pousserTaille = (n: number) =>
    octets.push(n & 0xff, (n >> 8) & 0xff, (n >> 16) & 0xff, (n >> 24) & 0xff);
  pousserFourcc('RIFF');
  pousserTaille(0); // Taille totale : ignorée par le détecteur.
  pousserFourcc('WEBP');
  if (forme === 'vp8x-anime' || forme === 'vp8x-statique') {
    pousserFourcc('VP8X');
    pousserTaille(10);
    octets.push(forme === 'vp8x-anime' ? 0x02 : 0x00); // Drapeaux (bit Animation).
    octets.push(0, 0, 0, 0, 0, 0, 0, 0, 0); // Réservé + dimensions.
  } else if (forme === 'anim-seul') {
    pousserFourcc('ANIM');
    pousserTaille(6);
    octets.push(0, 0, 0, 0, 0, 0);
  } else {
    pousserFourcc('VP8 ');
    pousserTaille(4);
    octets.push(0, 0, 0, 0);
  }
  return new Uint8Array(octets);
}

describe('estGifAnime', () => {
  it('rend faux pour un GIF statique (une seule image)', () => {
    expect(estGifAnime(gifOctets(1))).toBe(false);
  });

  it('rend vrai pour un GIF de plusieurs images', () => {
    expect(estGifAnime(gifOctets(2))).toBe(true);
    expect(estGifAnime(gifOctets(5))).toBe(true);
  });

  it('ignore correctement table de couleurs et extensions pour détecter l’animation', () => {
    expect(estGifAnime(gifOctetsAvecExtensions(1))).toBe(false);
    expect(estGifAnime(gifOctetsAvecExtensions(3))).toBe(true);
  });

  it('rend faux sur un flux tronqué ou non-GIF', () => {
    expect(estGifAnime(new Uint8Array([0x47, 0x49]))).toBe(false);
    expect(estGifAnime(new Uint8Array(20))).toBe(false);
  });
});

describe('ajusterDimensions', () => {
  it('ne change rien à une image déjà dans les bornes', () => {
    expect(ajusterDimensions(64, 32, 128)).toEqual({ w: 64, h: 32 });
  });

  it('réduit en conservant le ratio (paysage)', () => {
    expect(ajusterDimensions(400, 200, 128)).toEqual({ w: 128, h: 64 });
  });

  it('réduit en conservant le ratio (portrait)', () => {
    expect(ajusterDimensions(200, 400, 128)).toEqual({ w: 64, h: 128 });
  });

  it('n’agrandit jamais une petite image', () => {
    expect(ajusterDimensions(10, 10, 128)).toEqual({ w: 10, h: 10 });
  });
});

/** Simule le chargement d'une image (dimensions fixes), comme AvatarCropper.test.tsx. */
function stubImage(largeur: number, hauteur: number): void {
  vi.spyOn(HTMLImageElement.prototype, 'naturalWidth', 'get').mockReturnValue(largeur);
  vi.spyOn(HTMLImageElement.prototype, 'naturalHeight', 'get').mockReturnValue(hauteur);
  vi.spyOn(HTMLImageElement.prototype, 'src', 'set').mockImplementation(function (
    this: HTMLImageElement,
  ) {
    setTimeout(() => this.onload?.(new Event('load')), 0);
  });
}

/** Simule un canvas 2D minimal (dessin sans effet, jsdom ne rend pas de pixels). */
function stubCanvasContext(): void {
  vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockReturnValue({
    clearRect: vi.fn(),
    drawImage: vi.fn(),
  } as unknown as CanvasRenderingContext2D);
}

describe('compressEmojiImage — pipeline canvas (image statique)', () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('retient le premier palier qui tient sous la limite', async () => {
    stubImage(200, 200);
    stubCanvasContext();
    vi.spyOn(HTMLCanvasElement.prototype, 'toDataURL').mockReturnValue(
      'data:image/webp;base64,QUJD',
    );

    const fichier = new File(['x'], 'icone.png', { type: 'image/png' });
    const resultat = await compressEmojiImage(fichier);

    expect(resultat.mime).toBe('image/webp');
    expect(resultat.dataB64).toBe('QUJD');
  });

  it('recompresse un GIF statique (une seule image) via le canvas', async () => {
    stubImage(200, 200);
    stubCanvasContext();
    vi.spyOn(HTMLCanvasElement.prototype, 'toDataURL').mockReturnValue(
      'data:image/webp;base64,QUJD',
    );

    const fichier = new File([gifOctets(1)], 'icone.gif', { type: 'image/gif' });
    const resultat = await compressEmojiImage(fichier);

    expect(resultat.mime).toBe('image/webp');
  });

  it('replie sur PNG quand l’encodage WebP est indisponible', async () => {
    stubImage(200, 200);
    stubCanvasContext();
    vi.spyOn(HTMLCanvasElement.prototype, 'toDataURL').mockImplementation(
      // Le navigateur ignore le type demandé et rend toujours du PNG.
      () => 'data:image/png;base64,QUJD',
    );

    const fichier = new File(['x'], 'icone.png', { type: 'image/png' });
    const resultat = await compressEmojiImage(fichier);

    expect(resultat.mime).toBe('image/png');
    expect(resultat.dataB64).toBe('QUJD');
  });

  it('dégrade la dimension jusqu’à passer sous la limite', async () => {
    stubImage(500, 500);
    stubCanvasContext();
    vi.spyOn(HTMLCanvasElement.prototype, 'toDataURL').mockImplementation(function (
      this: HTMLCanvasElement,
      type?: string,
    ) {
      if (type !== 'image/webp') return 'data:image/png;base64,QUJD';
      const gros = this.width > 64 ? `A${'A'.repeat(400000)}` : 'QUJD';
      return `data:image/webp;base64,${gros}`;
    });

    const fichier = new File(['x'], 'icone.png', { type: 'image/png' });
    const resultat = await compressEmojiImage(fichier);

    expect(resultat.mime).toBe('image/webp');
    expect(resultat.dataB64).toBe('QUJD');
  });

  it('échoue proprement si même le plus petit palier dépasse la limite', async () => {
    stubImage(500, 500);
    stubCanvasContext();
    vi.spyOn(HTMLCanvasElement.prototype, 'toDataURL').mockReturnValue(
      `data:image/webp;base64,${'A'.repeat(400000)}`,
    );

    const fichier = new File(['x'], 'icone.png', { type: 'image/png' });

    const erreur = await compressEmojiImage(fichier).catch((e: unknown) => e);
    expect(erreur).toBeInstanceOf(EmojiCompressionError);
    expect(erreur).toMatchObject({ raison: 'compression-impossible' });
  });
});

describe('estWebpAnime', () => {
  it('rend false pour un WebP simple et un VP8X sans drapeau Animation', () => {
    expect(estWebpAnime(webpOctets('simple'))).toBe(false);
    expect(estWebpAnime(webpOctets('vp8x-statique'))).toBe(false);
  });

  it('détecte le drapeau Animation de VP8X et le chunk ANIM en repli', () => {
    expect(estWebpAnime(webpOctets('vp8x-anime'))).toBe(true);
    expect(estWebpAnime(webpOctets('anim-seul'))).toBe(true);
  });

  it('rend false sur un flux tronqué ou non WebP', () => {
    expect(estWebpAnime(new Uint8Array([0x52, 0x49]))).toBe(false);
    expect(estWebpAnime(gifOctets(2))).toBe(false);
  });
});

describe('compressEmojiImage — WebP animé', () => {
  it('transmet tel quel un WebP animé sous la limite (animation préservée)', async () => {
    const fichier = new File([webpOctets('vp8x-anime')], 'anim.webp', {
      type: 'image/webp',
    });

    const resultat = await compressEmojiImage(fichier);

    expect(resultat.mime).toBe('image/webp');
    expect(resultat.dataUrl.startsWith('data:image/webp;base64,')).toBe(true);
  });

  it('échoue avec la raison dédiée si le WebP animé dépasse la limite', async () => {
    const entete = webpOctets('vp8x-anime');
    const remplissage = new Uint8Array(EMOJI_OCTETS_MAX).fill(0x41);
    const gros = new Uint8Array(entete.length + remplissage.length);
    gros.set(entete, 0);
    gros.set(remplissage, entete.length);
    const fichier = new File([gros], 'anim.webp', { type: 'image/webp' });

    await expect(compressEmojiImage(fichier)).rejects.toMatchObject({
      raison: 'anime-trop-lourd',
    });
  });
});

describe('compressEmojiImage — GIF animé', () => {
  it('transmet tel quel un GIF animé sous la limite', async () => {
    const fichier = new File([gifOctets(3)], 'anim.gif', { type: 'image/gif' });

    const resultat = await compressEmojiImage(fichier);

    expect(resultat.mime).toBe('image/gif');
    expect(resultat.dataUrl.startsWith('data:image/gif;base64,')).toBe(true);
  });

  it('échoue avec la raison dédiée si le GIF animé dépasse la limite', async () => {
    const octetsAnimes = gifOctets(2);
    const remplissage = new Uint8Array(EMOJI_OCTETS_MAX).fill(0x41);
    const gros = new Uint8Array(octetsAnimes.length + remplissage.length);
    gros.set(octetsAnimes, 0);
    gros.set(remplissage, octetsAnimes.length);
    const fichier = new File([gros], 'anim.gif', { type: 'image/gif' });

    await expect(compressEmojiImage(fichier)).rejects.toMatchObject({
      raison: 'anime-trop-lourd',
    });
  });
});
