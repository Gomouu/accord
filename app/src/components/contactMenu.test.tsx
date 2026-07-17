/**
 * Tests des menus contextuels « contact » et « utilisateur local » : structure
 * des items selon l'état de la relation (ami, bloqué, demande reçue) et
 * câblage des actions sur les stores de domaine (copie, statut, blocage).
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fr } from '../i18n/fr';
import type { Contact, SelfProfile } from '../lib/api';
import { useFriends } from '../stores/friends';
import { useUi } from '../stores/ui';
import { buildContactMenu, buildOwnUserMenu } from './contactMenu';

function contact(over: Partial<Contact> = {}): Contact {
  return {
    node_id: 'n-bob',
    pubkey: 'bob',
    friend_code: 'accord-bob-12345',
    display_name: 'Bobby',
    bio: null,
    avatar: null,
    banner: null,
    state: 'friend',
    last_seen_ms: 0,
    ...over,
  };
}

const SELF: SelfProfile = {
  node_id: 'n-moi',
  pubkey: 'moi',
  friend_code: 'accord-moi-99999',
  name: 'Moi',
  bio: null,
  avatar: null,
  banner: null,
  pronouns: null,
  accent_color: null,
  banner_color: null,
  avatar_decoration: null,
  profile_effect: null,
  profile_frame: null,
};

const labels = (items: { label: string }[]): string[] => items.map((i) => i.label);
const target = (): HTMLElement => document.createElement('div');

beforeEach(() => {
  useUi.setState({ toasts: [] });
});

describe('buildContactMenu', () => {
  it('offre profil, message, appel et blocage pour un ami', () => {
    const items = buildContactMenu(fr, contact(), target());
    const l = labels(items);
    expect(l).toContain(fr.contextMenu.viewProfile);
    expect(l).toContain(fr.friends.sendDm);
    expect(l).toContain(fr.calls.startCall);
    expect(l).toContain(fr.contextMenu.copyFriendCode);
    expect(l).toContain(fr.friends.remove);
    expect(l).toContain(fr.friends.block);
    // Pas de « marquer comme lu » sans message non lu.
    expect(l).not.toContain(fr.contextMenu.markAsRead);
  });

  it('ajoute « marquer comme lu » quand des messages sont non lus', () => {
    const items = buildContactMenu(fr, contact({ unread: 3 }), target());
    expect(labels(items)).toContain(fr.contextMenu.markAsRead);
  });

  it('propose accepter / refuser pour une demande reçue', () => {
    const items = buildContactMenu(fr, contact({ state: 'pending_in' }), target());
    const l = labels(items);
    expect(l).toContain(fr.friends.accept);
    expect(l).toContain(fr.friends.decline);
    expect(l).not.toContain(fr.friends.remove);
  });

  it('propose débloquer (pas retrait) pour un contact bloqué', () => {
    const items = buildContactMenu(fr, contact({ state: 'blocked' }), target());
    const l = labels(items);
    expect(l).toContain(fr.friends.unblock);
    expect(l).not.toContain(fr.friends.block);
    expect(l).not.toContain(fr.friends.remove);
  });

  it('copie le code ami dans le presse-papiers', async () => {
    const writeText = vi.fn((_t: string) => Promise.resolve());
    Object.assign(navigator, { clipboard: { writeText } });
    const items = buildContactMenu(fr, contact(), target());
    items.find((i) => i.label === fr.contextMenu.copyFriendCode)?.onClick();
    expect(writeText).toHaveBeenCalledWith('accord-bob-12345');
  });

  it('bloque via l’action du store', () => {
    const block = vi.fn(async () => {});
    useFriends.setState({ block });
    const items = buildContactMenu(fr, contact(), target());
    items.find((i) => i.label === fr.friends.block)?.onClick();
    expect(block).toHaveBeenCalledWith('bob');
  });
});

describe('buildOwnUserMenu', () => {
  it('coche le statut courant parmi les quatre radios', () => {
    const items = buildOwnUserMenu(fr, SELF, 'dnd');
    const dnd = items.find((i) => i.label === fr.profil.dnd);
    const online = items.find((i) => i.label === fr.profil.online);
    expect(dnd?.checked).toBe(true);
    expect(online?.checked).toBe(false);
  });

  it('change le statut via setOwnStatus', () => {
    const setOwnStatus = vi.fn(async () => {});
    useFriends.setState({ setOwnStatus });
    const items = buildOwnUserMenu(fr, SELF, 'online');
    items.find((i) => i.label === fr.profil.invisible)?.onClick();
    expect(setOwnStatus).toHaveBeenCalledWith('invisible');
  });

  it('donne accès à la copie du code, de l’ID et aux paramètres', () => {
    const l = labels(buildOwnUserMenu(fr, SELF, 'online'));
    expect(l).toContain(fr.profil.copyFriendCode);
    expect(l).toContain(fr.contextMenu.copyUserId);
    expect(l).toContain(fr.settings.title);
  });
});
