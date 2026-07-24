/**
 * Partage d'écran — helpers purs : encodage hexadécimal (transport JSON de
 * l'API locale) et détection de support (WebCodecs + getDisplayMedia, absents
 * de jsdom).
 */

import { describe, expect, it } from 'vitest';
import { bytesToHex, hexToBytes, screenShareSupported } from './screenShare';

describe('screenShare — helpers hexadécimaux', () => {
  it('fait un aller-retour octets ↔ hexadécimal', () => {
    const bytes = new Uint8Array([0, 1, 15, 16, 255, 128]);
    const hex = bytesToHex(bytes);
    expect(hex).toBe('00010f10ff80');
    expect(hexToBytes(hex)).toEqual(bytes);
  });

  it('encode chaque octet sur deux caractères', () => {
    expect(bytesToHex(new Uint8Array([0, 5, 10]))).toBe('00050a');
  });

  it('gère la chaîne vide', () => {
    expect(bytesToHex(new Uint8Array([]))).toBe('');
    expect(hexToBytes('')).toEqual(new Uint8Array([]));
  });

  it('rejette une longueur impaire', () => {
    expect(hexToBytes('abc')).toBeNull();
  });

  it('rejette un caractère non hexadécimal', () => {
    expect(hexToBytes('zz')).toBeNull();
  });
});

describe('screenShare — détection de support', () => {
  it('rend faux sans WebCodecs ni getDisplayMedia (jsdom)', () => {
    expect(screenShareSupported()).toBe(false);
  });
});
