/** Tests du constructeur et de l'analyseur des liens d'ami partageables. */

import { describe, expect, it } from 'vitest';
import { FRIEND_LINK_PREFIX, buildFriendLink, parseFriendLink } from './friendLink';

const CODE = 'LION-FORET-PLAGE-NUAGE-TIGRE-OCEAN-0042';

describe('buildFriendLink', () => {
  it('préfixe le code avec le schéma accord://friend/', () => {
    // Arrange — un code ami canonique.
    // Act
    const lien = buildFriendLink(CODE);
    // Assert
    expect(lien).toBe(`${FRIEND_LINK_PREFIX}${CODE}`);
  });

  it('borne les espaces autour du code', () => {
    expect(buildFriendLink(`  ${CODE}  `)).toBe(`${FRIEND_LINK_PREFIX}${CODE}`);
  });
});

describe('parseFriendLink', () => {
  it('accepte le lien complet et rend le code', () => {
    expect(parseFriendLink(`accord://friend/${CODE}`)).toBe(CODE);
  });

  it('accepte le lien sans schéma', () => {
    expect(parseFriendLink(`friend/${CODE}`)).toBe(CODE);
  });

  it('accepte le code brut, bornes retirées', () => {
    expect(parseFriendLink(`  ${CODE}  `)).toBe(CODE);
  });

  it('accepte le deep link Rust historique p2papp://add/', () => {
    expect(parseFriendLink(`p2papp://add/${CODE}`)).toBe(CODE);
  });

  it('tolère la casse du schéma et une barre oblique finale', () => {
    expect(parseFriendLink(`ACCORD://friend/${CODE}/`)).toBe(CODE);
  });

  it('normalise les espaces internes d’un code dicté', () => {
    // Le nœud tolère espaces ou tirets ; on réduit les suites d'espaces.
    expect(parseFriendLink('lion  foret   0042')).toBe('lion foret 0042');
  });

  it('accepte les codes accentués (exemple des libellés : accord-lion-forêt-12345)', () => {
    expect(parseFriendLink('accord-lion-forêt-12345')).toBe('accord-lion-forêt-12345');
  });

  it('rejette la saisie vide ou le préfixe seul', () => {
    expect(parseFriendLink('')).toBeNull();
    expect(parseFriendLink('   ')).toBeNull();
    expect(parseFriendLink('accord://friend/')).toBeNull();
  });

  it('rejette les liens étrangers (invitation serveur, https)', () => {
    expect(parseFriendLink('accord://invite/AbCd1234')).toBeNull();
    expect(parseFriendLink('https://example.com/friend/XXXX')).toBeNull();
    expect(parseFriendLink('//friend/XXXX')).toBeNull();
  });

  it('rejette un texte qui ne peut pas être un code', () => {
    expect(parseFriendLink('pas un code !')).toBeNull();
    expect(parseFriendLink('accord://friend/avec/segment')).toBeNull();
  });
});
