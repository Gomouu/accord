/**
 * Tests du menu contextuel « utilisateur » (`buildUserItems`) : composition
 * selon la relation d'amitié (ami, bloqué, inconnu, soi-même). Fonction pure —
 * on inspecte les libellés produits sans monter de vue.
 */

import { describe, expect, it } from 'vitest';
import { dictionaries, interpolate } from '../i18n';
import type { Contact } from '../lib/api';
import { buildUserItems, type MessageMenuDeps } from './messageMenus';

const t = dictionaries.fr;

function contact(pubkey: string, state: Contact['state']): Contact {
  return {
    node_id: 'n',
    pubkey,
    friend_code: 'accord-lion-foret-12345',
    display_name: pubkey,
    bio: null,
    avatar: null,
    banner: null,
    state,
    last_seen_ms: 0,
  };
}

function deps(over: Partial<MessageMenuDeps> = {}): MessageMenuDeps {
  return {
    t,
    selfPubkey: 'moi',
    contacts: [],
    actions: undefined,
    nameOf: (a) => a,
    copyWithToast: () => {},
    requestMentionInsert: () => {},
    openProfile: () => {},
    onForward: () => {},
    onEditInPlace: () => {},
    ...over,
  };
}

/** Libellés des items produits pour `author`. */
function labels(over: Partial<MessageMenuDeps>, author: string): string[] {
  const target = document.createElement('div');
  return buildUserItems(deps(over), author, target).map((i) => i.label);
}

describe('buildUserItems', () => {
  it('pour un ami : profil, mention, message, appel, retrait, blocage, copie', () => {
    const out = labels({ contacts: [contact('alice', 'friend')] }, 'alice');

    expect(out).toEqual([
      t.contextMenu.viewProfile,
      interpolate(t.contextMenu.mention, { name: 'alice' }),
      t.friends.sendDm,
      t.calls.startCall,
      t.friends.remove,
      t.friends.block,
      t.contextMenu.copyUserId,
    ]);
  });

  it('pour soi-même : ni message, ni appel, ni action de relation', () => {
    const out = labels({ contacts: [] }, 'moi');

    expect(out).toEqual([
      t.contextMenu.viewProfile,
      interpolate(t.contextMenu.mention, { name: 'moi' }),
      t.contextMenu.copyUserId,
    ]);
  });

  it('pour un contact bloqué : propose « Débloquer », pas « Bloquer »', () => {
    const out = labels({ contacts: [contact('spam', 'blocked')] }, 'spam');

    expect(out).toContain(t.friends.unblock);
    expect(out).not.toContain(t.friends.block);
    expect(out).not.toContain(t.friends.sendDm);
    expect(out).not.toContain(t.friends.remove);
  });

  it('pour un inconnu : blocage possible, sans message ni retrait', () => {
    const out = labels({ contacts: [] }, 'inconnu');

    expect(out).toContain(t.friends.block);
    expect(out).not.toContain(t.friends.sendDm);
    expect(out).not.toContain(t.friends.remove);
  });
});
