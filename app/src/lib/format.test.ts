/** Tests des aides d'affichage : horodatages, initiales, couleurs, libellés. */

import { describe, expect, it } from 'vitest';
import {
  avatarColor,
  formatDay,
  formatDuration,
  formatTimestamp,
  formatTimestampCompact,
  initials,
  shortId,
  tailleLisible,
} from './format';

// Points fixes locaux (sans fuseau explicite : interprétés en heure locale).
const NOW = new Date('2026-07-08T15:00:00').getTime();
const SAME_DAY = new Date('2026-07-08T09:05:00').getTime();
const CHRISTMAS = new Date('2025-12-25T10:00:00').getTime();

describe('formatTimestamp', () => {
  it('affiche seulement l’heure pour un message du jour', () => {
    expect(formatTimestamp(SAME_DAY, 'fr', NOW)).toBe('09:05');
  });

  it('affiche la date (format local) pour un autre jour', () => {
    expect(formatTimestamp(CHRISTMAS, 'fr', NOW)).toBe('25/12/2025');
    expect(formatTimestamp(CHRISTMAS, 'en', NOW)).toBe('12/25/2025');
  });

  it('suit la convention de la locale par défaut (« auto »)', () => {
    // fr-FR : 24 h par défaut ; en-US : 12 h par défaut.
    expect(formatTimestamp(SAME_DAY, 'fr', NOW)).toBe('09:05');
    expect(formatTimestamp(SAME_DAY, 'en', NOW)).toMatch(/AM$/);
  });

  it('force le format 12 h indépendamment de la locale', () => {
    expect(formatTimestamp(SAME_DAY, 'fr', NOW, '12h')).toMatch(/AM$/);
    expect(formatTimestamp(SAME_DAY, 'en', NOW, '12h')).toMatch(/AM$/);
  });

  it('force le format 24 h indépendamment de la locale', () => {
    expect(formatTimestamp(SAME_DAY, 'fr', NOW, '24h')).toBe('09:05');
    expect(formatTimestamp(SAME_DAY, 'en', NOW, '24h')).not.toMatch(/AM$|PM$/);
  });

  it('ne change pas l’affichage d’une autre date (heure absente)', () => {
    expect(formatTimestamp(CHRISTMAS, 'fr', NOW, '12h')).toBe('25/12/2025');
  });
});

describe('formatDay', () => {
  it('rend un séparateur de jour complet dans la langue demandée', () => {
    expect(formatDay(CHRISTMAS, 'fr')).toContain('décembre 2025');
    expect(formatDay(CHRISTMAS, 'en')).toContain('December');
  });
});

describe('initials', () => {
  it('prend la première lettre des deux premiers mots, en majuscules', () => {
    expect(initials('alice')).toBe('A');
    expect(initials('alice bob')).toBe('AB');
    expect(initials('alice bob charlie')).toBe('AB');
  });

  it('tolère les espaces superflus et les noms vides', () => {
    expect(initials('  alice   bob  ')).toBe('AB');
    expect(initials('')).toBe('?');
    expect(initials('   ')).toBe('?');
  });
});

describe('avatarColor', () => {
  it('rend une couleur hexadécimale stable pour un même identifiant', () => {
    const color = avatarColor('a1b2c3');
    expect(color).toMatch(/^#[0-9a-f]{6}$/);
    expect(avatarColor('a1b2c3')).toBe(color);
  });

  it('rend une couleur même pour une chaîne vide', () => {
    expect(avatarColor('')).toMatch(/^#[0-9a-f]{6}$/);
  });
});

describe('shortId', () => {
  it('tronque une clé hexadécimale à 6 caractères', () => {
    expect(shortId('abcdef0123456789')).toBe('abcdef');
    expect(shortId('ab')).toBe('ab');
  });
});

describe('tailleLisible', () => {
  it('affiche les octets tels quels sous 1 Kio', () => {
    expect(tailleLisible(0, 'fr')).toBe('0 o');
    expect(tailleLisible(1023, 'fr')).toBe('1023 o');
    expect(tailleLisible(512, 'en')).toBe('512 B');
  });

  it('monte d’unité par paliers de 1024 avec une décimale au plus', () => {
    expect(tailleLisible(1024, 'fr')).toBe('1 Ko');
    expect(tailleLisible(1536, 'fr')).toBe('1,5 Ko');
    expect(tailleLisible(1536, 'en')).toBe('1.5 KB');
    expect(tailleLisible(8 * 1024 * 1024, 'fr')).toBe('8 Mo');
    expect(tailleLisible(3 * 1024 * 1024 * 1024, 'en')).toBe('3 GB');
  });

  it('plafonne à l’unité la plus grande et tolère les négatifs', () => {
    expect(tailleLisible(5 * 1024 ** 4, 'fr')).toBe('5120 Go');
    expect(tailleLisible(-42, 'fr')).toBe('0 o');
  });
});

describe('formatTimestampCompact', () => {
  const noon = new Date('2026-07-11T00:05:00').getTime();
  const now = new Date('2026-07-11T15:00:00').getTime();

  it('colle et minusculise le méridien en 12 h (tient dans la gouttière)', () => {
    expect(formatTimestampCompact(noon, 'en', now, '12h')).toBe('12:05am');
  });

  it('ne change rien en 24 h', () => {
    expect(formatTimestampCompact(noon, 'en', now, '24h')).toBe('00:05');
  });
});

describe('formatDuration', () => {
  it('formate en mm:ss sous l’heure', () => {
    expect(formatDuration(0)).toBe('0:00');
    expect(formatDuration(5)).toBe('0:05');
    expect(formatDuration(65)).toBe('1:05');
    expect(formatDuration(599)).toBe('9:59');
  });

  it('bascule en h:mm:ss au-delà d’une heure', () => {
    expect(formatDuration(3600)).toBe('1:00:00');
    expect(formatDuration(3665)).toBe('1:01:05');
  });

  it('tronque les fractions de seconde et borne les durées négatives à 0', () => {
    expect(formatDuration(59.9)).toBe('0:59');
    expect(formatDuration(-10)).toBe('0:00');
  });
});
