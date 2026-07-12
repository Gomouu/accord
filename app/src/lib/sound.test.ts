/** Tests de validation des sons de soundboard (MIME, taille, nom). */

import { describe, expect, it } from 'vitest';
import {
  estMimeSonValide,
  estNomSonValide,
  estTailleSonValide,
  SOUND_OCTETS_MAX,
} from './sound';

describe('estMimeSonValide', () => {
  it('accepte les types audio du contrat', () => {
    for (const mime of ['audio/ogg', 'audio/mpeg', 'audio/mp4', 'audio/webm', 'audio/wav']) {
      expect(estMimeSonValide(mime)).toBe(true);
    }
  });

  it('refuse les types hors contrat', () => {
    expect(estMimeSonValide('audio/flac')).toBe(false);
    expect(estMimeSonValide('image/png')).toBe(false);
    expect(estMimeSonValide('')).toBe(false);
  });
});

describe('estTailleSonValide', () => {
  it('accepte une taille jusqu’à 256 Kio inclus', () => {
    expect(estTailleSonValide(1)).toBe(true);
    expect(estTailleSonValide(SOUND_OCTETS_MAX)).toBe(true);
  });

  it('refuse une taille nulle ou au-delà de la limite', () => {
    expect(estTailleSonValide(0)).toBe(false);
    expect(estTailleSonValide(SOUND_OCTETS_MAX + 1)).toBe(false);
  });
});

describe('estNomSonValide', () => {
  it('suit les bornes de nom des émojis custom', () => {
    expect(estNomSonValide('air_horn')).toBe(true);
    expect(estNomSonValide('bruh2')).toBe(true);
    expect(estNomSonValide('a')).toBe(false);
    expect(estNomSonValide('UPPER')).toBe(false);
    expect(estNomSonValide('bad-name')).toBe(false);
  });
});
