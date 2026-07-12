/**
 * Brouillons de composeur : dérivation de la clé par type de cible, aller-retour
 * lecture/écriture, effacement quand le texte devient vide, et borne de taille.
 */

import { beforeEach, describe, expect, it } from 'vitest';
import { draftKey, readDraft, writeDraft, MAX_DRAFT_LEN } from './drafts';

beforeEach(() => {
  window.localStorage.clear();
});

describe('draftKey', () => {
  it('dérive une clé distincte par MP (peer)', () => {
    expect(draftKey({ kind: 'dm', peer: 'pk_alice' })).toBe('draft:dm:pk_alice');
  });

  it('dérive une clé de groupe au format channelKey (groupId/channelId)', () => {
    expect(draftKey({ kind: 'group', groupId: 'g1', channelId: 'c1' })).toBe(
      'draft:grp:g1/c1',
    );
  });

  it('renvoie null sans cible (pas de persistance)', () => {
    expect(draftKey(undefined)).toBeNull();
  });
});

describe('readDraft / writeDraft', () => {
  it('fait un aller-retour du texte sous la clé', () => {
    const key = draftKey({ kind: 'dm', peer: 'pk_bob' });
    writeDraft(key, 'salut, ceci est un brouillon');
    expect(readDraft(key)).toBe('salut, ceci est un brouillon');
  });

  it('efface la clé quand le texte devient vide', () => {
    const key = draftKey({ kind: 'group', groupId: 'g1', channelId: 'c1' });
    writeDraft(key, 'en cours');
    expect(readDraft(key)).toBe('en cours');
    writeDraft(key, '');
    expect(readDraft(key)).toBeNull();
  });

  it('n’écrit rien quand la clé est nulle', () => {
    writeDraft(null, 'ignoré');
    expect(window.localStorage.length).toBe(0);
  });

  it('lit null pour une clé absente ou nulle', () => {
    expect(readDraft(null)).toBeNull();
    expect(readDraft('draft:dm:inconnu')).toBeNull();
  });

  it('ignore un brouillon au-delà de la borne, sans écraser le dernier valide', () => {
    const key = draftKey({ kind: 'dm', peer: 'pk_carol' });
    writeDraft(key, 'a'.repeat(MAX_DRAFT_LEN));
    expect(readDraft(key)).toBe('a'.repeat(MAX_DRAFT_LEN));
    // Au-delà de la borne : écriture ignorée, le brouillon valide subsiste.
    writeDraft(key, 'b'.repeat(MAX_DRAFT_LEN + 1));
    expect(readDraft(key)).toBe('a'.repeat(MAX_DRAFT_LEN));
  });
});
