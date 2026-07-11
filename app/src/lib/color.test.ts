/** Tests de la conversion sûre des couleurs de profil (`0xRRGGBB` → `#rrggbb`). */

import { describe, expect, it } from 'vitest';
import { profileCardGradient, profileColorCss } from './color';

describe('profileColorCss', () => {
  it('convertit un entier 0xRRGGBB en #rrggbb', () => {
    expect(profileColorCss(0x5865f2)).toBe('#5865f2');
  });

  it('conserve les zéros de tête', () => {
    expect(profileColorCss(0x0000ff)).toBe('#0000ff');
    expect(profileColorCss(0)).toBe('#000000');
  });

  it('rend `null` pour `null`', () => {
    expect(profileColorCss(null)).toBeNull();
  });

  it('rend `null` pour `undefined`', () => {
    expect(profileColorCss(undefined)).toBeNull();
  });

  it('ramène toute valeur hors 24 bits aux bits utiles sans exception', () => {
    expect(profileColorCss(0x1ffffff)).toBe('#ffffff');
    expect(profileColorCss(-1)).toBe('#ffffff');
  });

  it('rend `null` pour une valeur non finie (donnée pair non fiable)', () => {
    expect(profileColorCss(Number.NaN)).toBeNull();
    expect(profileColorCss(Number.POSITIVE_INFINITY)).toBeNull();
  });
});

describe('profileCardGradient', () => {
  it('produit un dégradé rgba clampé bas (16 %) fondant vers transparent', () => {
    expect(profileCardGradient(0x5865f2)).toBe(
      'linear-gradient(to bottom, rgba(88, 101, 242, 0.16) 0%, rgba(88, 101, 242, 0) 100%)',
    );
  });

  it('reste lisible avec un accent blanc (haut) et noir (bas)', () => {
    // Blanc : le point haut ne dépasse jamais l'alpha borné (0.16), jamais
    // un aplat opaque qui casserait le contraste du texte.
    expect(profileCardGradient(0xffffff)).toBe(
      'linear-gradient(to bottom, rgba(255, 255, 255, 0.16) 0%, rgba(255, 255, 255, 0) 100%)',
    );
    // Noir : même alpha borné, jamais un fond qui noircirait toute la carte.
    expect(profileCardGradient(0x000000)).toBe(
      'linear-gradient(to bottom, rgba(0, 0, 0, 0.16) 0%, rgba(0, 0, 0, 0) 100%)',
    );
  });

  it('rend `null` sans couleur de profil (aucun changement visuel)', () => {
    expect(profileCardGradient(null)).toBeNull();
    expect(profileCardGradient(undefined)).toBeNull();
  });

  it('rend `null` pour une valeur non finie', () => {
    expect(profileCardGradient(Number.NaN)).toBeNull();
  });
});
