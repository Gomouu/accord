/**
 * Comparaison d'un numéro de sécurité scanné (§17.4).
 *
 * Ce que ces tests protègent avant tout : `match` ne doit avoir qu'une seule
 * origine possible. Un QR illisible, un QR étranger, une charge utile tronquée
 * ou rallongée doivent tous sortir par une porte *autre* que « identique » —
 * une vérification qui répond « c'est bon » quand elle n'a rien pu lire est
 * pire que pas de vérification du tout.
 */

import { describe, expect, it } from 'vitest';
import {
  buildSafetyQrPayload,
  parseSafetyQrPayload,
  SAFETY_DIGITS_LENGTH,
  verdictForScan,
} from './safetyQr';

/** Numéro local de référence : 60 chiffres, comme en sort le nœud. */
const LOCAL = '123450987612345098761234509876123450987612345098761234509876';

/** Le même, avec un seul chiffre changé (le dernier). */
const OFF_BY_ONE = `${LOCAL.slice(0, -1)}5`;

describe('buildSafetyQrPayload / parseSafetyQrPayload', () => {
  it('fait un aller-retour exact sur un numéro bien formé', () => {
    // Arrange / Act
    const payload = buildSafetyQrPayload(LOCAL);

    // Assert
    expect(payload).toBe(`accord://safety/${LOCAL}`);
    expect(parseSafetyQrPayload(payload)).toBe(LOCAL);
  });

  it('tolère les espaces autour de la charge utile décodée', () => {
    expect(parseSafetyQrPayload(`  ${buildSafetyQrPayload(LOCAL)}\n`)).toBe(LOCAL);
  });

  it('rejette tout ce qui n’est pas exactement un numéro de sécurité', () => {
    const refuses = [
      '',
      'bonjour',
      LOCAL, // les chiffres seuls, sans schéma
      'accord://friend/LION-FORET-PLAGE-NUAGE-TIGRE-OCEAN-0042', // lien d'ami
      `accord://safety/${LOCAL.slice(1)}`, // 59 chiffres
      `accord://safety/${LOCAL}7`, // 61 chiffres
      `accord://safety/${LOCAL.slice(0, -1)}x`, // caractère non décimal
      `accord://safety/${LOCAL} extra`, // suffixe après les chiffres
      `prefix accord://safety/${LOCAL}`, // texte avant le schéma
      `accord://safety/${'١'.repeat(SAFETY_DIGITS_LENGTH)}`, // chiffres arabo-indiens
    ];

    for (const texte of refuses) {
      expect(parseSafetyQrPayload(texte), texte).toBeNull();
    }
  });
});

describe('verdictForScan', () => {
  it('rend « match » pour deux numéros identiques', () => {
    // Arrange
    const scanne = buildSafetyQrPayload(LOCAL);

    // Act
    const verdict = verdictForScan(scanne, LOCAL);

    // Assert
    expect(verdict).toBe('match');
  });

  it('rend « mismatch » quand un seul chiffre diffère', () => {
    // Arrange — un chiffre sur soixante : exactement ce qu'une lecture à voix
    // haute rate le plus souvent.
    const scanne = buildSafetyQrPayload(OFF_BY_ONE);
    expect(OFF_BY_ONE).not.toBe(LOCAL);
    expect(OFF_BY_ONE).toHaveLength(SAFETY_DIGITS_LENGTH);

    // Act
    const verdict = verdictForScan(scanne, LOCAL);

    // Assert
    expect(verdict).toBe('mismatch');
  });

  it('rend « null » quand aucun QR n’a été trouvé dans l’image', () => {
    // Une image sans QR n'est pas un verdict : on continue à scanner.
    expect(verdictForScan(null, LOCAL)).toBeNull();
  });

  it('🔒 ne rend jamais « match » sur un décodage qui n’a pas abouti', () => {
    // Arrange — tout ce qui peut sortir d'une caméra sans être un numéro de
    // sécurité : rien lu, autre QR, charge utile tronquée, chiffres locaux
    // absents, valeurs vides des deux côtés.
    const echecs: [string | null, string][] = [
      [null, LOCAL],
      ['', LOCAL],
      ['', ''],
      ['accord://safety/', ''],
      [`accord://safety/${LOCAL.slice(1)}`, LOCAL.slice(1)],
      ['accord://friend/LION-FORET-PLAGE-NUAGE-TIGRE-OCEAN-0042', LOCAL],
      ['https://exemple.test/', LOCAL],
      [buildSafetyQrPayload(LOCAL), ''],
      [buildSafetyQrPayload(LOCAL), LOCAL.slice(0, -1)],
    ];

    // Act / Assert
    for (const [decode, local] of echecs) {
      expect(verdictForScan(decode, local), `${decode} / ${local}`).not.toBe('match');
    }
  });

  it('🔒 un numéro local vide ou tronqué ne peut pas devenir « match »', () => {
    // Garde-fou contre le piège classique : `'' === ''`. La charge utile ne
    // se décode qu'en 60 chiffres exactement, donc l'égalité stricte ne peut
    // pas se satisfaire d'un numéro local dégénéré.
    expect(verdictForScan(buildSafetyQrPayload(''), '')).toBe('foreign');
    expect(verdictForScan('accord://safety/', '')).toBe('foreign');
  });
});
