/**
 * Tests du masquage AutoMod au rendu : correspondance par mot entier
 * insensible à la casse ET aux accents, masque de longueur bornée [3, 8],
 * mots vides ignorés, aucun masquage au milieu d'un autre mot.
 *
 * 🔗 Les cas marqués « jumeau » existent à l'identique dans
 * `crates/accord-core/src/automod.rs` : les deux implémentations décident du
 * même masquage, l'une pour le rendu, l'autre pour le compteur de non-lus.
 */

import { describe, expect, it } from 'vitest';
import {
  containsFiltered,
  maskFiltered,
  MAX_AUTOMOD_WORDS,
  MAX_AUTOMOD_WORD_CHARS,
} from './automod';

describe('maskFiltered', () => {
  it('rend le texte inchangé sans mot filtré', () => {
    expect(maskFiltered('bonjour tout le monde', [])).toBe('bonjour tout le monde');
    expect(maskFiltered('bonjour', ['zut'])).toBe('bonjour');
  });

  it('ignore les mots vides ou blancs de la liste', () => {
    expect(maskFiltered('bonjour', ['', '   '])).toBe('bonjour');
  });

  it('masque une occurrence avec des █ de même longueur', () => {
    expect(maskFiltered('quel idiot celui-là', ['idiot'])).toBe('quel █████ celui-là');
  });

  it('borne le masque à 3 minimum et 8 maximum', () => {
    expect(maskFiltered('oh zut', ['zut'])).toBe('oh ███');
    expect(maskFiltered('un ah', ['ah'])).toBe('un ███');
    expect(
      maskFiltered('anticonstitutionnellement !', ['anticonstitutionnellement']),
    ).toBe('████████ !');
  });

  it('est insensible à la casse', () => {
    expect(maskFiltered('IDIOT et Idiot et idiot', ['idiot'])).toBe(
      '█████ et █████ et █████',
    );
  });

  it('gère les accents (casse Unicode)', () => {
    expect(maskFiltered('Espèce de crétin', ['crétin'])).toBe('Espèce de ██████');
    expect(maskFiltered('CRÉTIN va', ['crétin'])).toBe('██████ va');
  });

  // jumeau: matches_ignores_accents_in_both_directions
  it('masque un mot accentué filtré sans accent, et l’inverse', () => {
    // Le mot filtré est saisi sans accent : le message accentué doit tomber.
    expect(maskFiltered('espèce de crétin', ['cretin'])).toBe('espèce de ██████');
    // ... et le mot filtré accentué doit attraper le message sans accent.
    expect(maskFiltered('espece de cretin', ['crétin'])).toBe('espece de ██████');
  });

  // jumeau: matches_precomposed_and_decomposed_alike
  it('masque quel que soit le codage Unicode de l’accent (NFC ou NFD)', () => {
    const nfc = 'crétin';
    const nfd = 'crétin';
    expect(maskFiltered(`quel ${nfd} !`, [nfc])).toBe('quel ██████ !');
    expect(maskFiltered(`quel ${nfc} !`, [nfd])).toBe('quel ██████ !');
    // Le masque compte les caractères VISIBLES : la forme décomposée occupe
    // sept unités de code mais n'en montre que six.
    expect(maskFiltered(nfd, [nfc])).toBe('██████');
  });

  // jumeau: matches_whole_words_only
  it('ne masque pas un mot filtré au milieu d’un autre mot', () => {
    expect(maskFiltered('le chaton dort', ['chat'])).toBe('le chaton dort');
    expect(maskFiltered('achat en ligne', ['chat'])).toBe('achat en ligne');
    // Le mot isolé reste masqué dans la même phrase.
    expect(maskFiltered('le chat et le chaton', ['chat'])).toBe('le ████ et le chaton');
    // Le cas qui donne son sens à la frontière de mot.
    expect(maskFiltered('on va au concert', ['con'])).toBe('on va au concert');
    expect(maskFiltered('quel con', ['con'])).toBe('quel ███');
  });

  it('respecte les frontières Unicode (lettre accentuée collée = même mot)', () => {
    expect(maskFiltered('idiotè', ['idiot'])).toBe('idiotè');
  });

  it('masque plusieurs occurrences et plusieurs mots', () => {
    expect(maskFiltered('zut, zut et flûte', ['zut', 'flûte'])).toBe('███, ███ et █████');
  });

  it('masque en bord de ponctuation et de chaîne', () => {
    expect(maskFiltered('idiot', ['idiot'])).toBe('█████');
    expect(maskFiltered('(idiot)', ['idiot'])).toBe('(█████)');
  });

  it('neutralise les métacaractères regex des mots filtrés', () => {
    expect(maskFiltered('a.b partout', ['a.b'])).toBe('███ partout');
    expect(maskFiltered('aXb partout', ['a.b'])).toBe('aXb partout');
  });

  it('ne coupe pas un caractère hors BMP situé avant le mot filtré', () => {
    // Un emoji occupe deux unités de code : masquer aux offsets du texte
    // replié le trancherait en deux moitiés de paire de substitution.
    expect(maskFiltered('🙂 idiot', ['idiot'])).toBe('🙂 █████');
  });
});

describe('containsFiltered', () => {
  it('détecte un mot filtré (même règle que maskFiltered)', () => {
    expect(containsFiltered('quel idiot', ['idiot'])).toBe(true);
    expect(containsFiltered('quel IDIOT', ['idiot'])).toBe(true);
    expect(containsFiltered('le chaton', ['chat'])).toBe(false);
    expect(containsFiltered('on va au concert', ['con'])).toBe(false);
    expect(containsFiltered('rien à signaler', ['idiot'])).toBe(false);
    expect(containsFiltered('peu importe', [])).toBe(false);
    expect(containsFiltered('peu importe', ['', ' '])).toBe(false);
  });

  it('détecte à travers accents et formes Unicode', () => {
    expect(containsFiltered('espece de cretin', ['crétin'])).toBe(true);
    expect(containsFiltered('espèce de crétin', ['cretin'])).toBe(true);
    expect(containsFiltered('quel crétin', ['crétin'])).toBe(true);
  });
});

describe('bornes partagées avec le nœud', () => {
  it('reprend les bornes réellement appliquées côté Rust', () => {
    // 🔒 Ces deux nombres viennent du nœud (`MAX_AUTOMOD_WORDS` d'accord-proto
    // et `MAX_AUTOMOD_WORD_CHARS` d'accord-core). Les voir changer ici sans
    // les changer là-bas est exactement la dérive qui faisait échouer
    // « Enregistrer » au 51ᵉ mot.
    expect(MAX_AUTOMOD_WORDS).toBe(50);
    expect(MAX_AUTOMOD_WORD_CHARS).toBe(32);
  });
});
