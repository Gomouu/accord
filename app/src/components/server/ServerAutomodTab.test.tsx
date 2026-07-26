/**
 * Tests de l'onglet AutoMod : la borne affichée et la borne appliquée sont
 * celles du NŒUD (50 mots), et le compteur les annonce.
 *
 * 🔒 Ce que ces tests protègent : l'écran laissait ajouter jusqu'à 100 mots
 * alors que le format filaire en refuse plus de 50. Passé le cinquantième,
 * « Enregistrer » échouait sur une erreur venue du nœud, sans que rien n'ait
 * prévenu — une limite qui ne vit que dans l'interface n'est pas une limite.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import type { GroupStateJson } from '../../lib/api';
import { MAX_AUTOMOD_WORDS } from '../../lib/automod';
import { PERMISSIONS, useGroups } from '../../stores/groups';
import { useUi } from '../../stores/ui';
import { ServerAutomodTab } from './ServerAutomodTab';

function groupState(words: string[]): GroupStateJson {
  return {
    group_id: 'g1',
    name: 'Guilde',
    icon: null,
    banner: null,
    founder: null,
    members: [],
    bans: [],
    channels: [],
    categories: [],
    roles: [],
    invites: [],
    my_permissions: PERMISSIONS.VIEW | PERMISSIONS.SEND | PERMISSIONS.MANAGE_CHANNELS,
    automod_words: words,
  };
}

function seed(words: string[]): { setAutomodWords: ReturnType<typeof vi.fn> } {
  const setAutomodWords = vi.fn(async () => {});
  useGroups.setState({ states: { g1: groupState(words) }, setAutomodWords });
  return { setAutomodWords };
}

/** Saisit un mot dans le champ d'ajout et valide par Entrée. */
function addWord(word: string): void {
  const input = screen.getByLabelText('Ajouter un mot à filtrer (Entrée)');
  fireEvent.change(input, { target: { value: word } });
  fireEvent.keyDown(input, { key: 'Enter' });
}

beforeEach(() => {
  useUi.setState({ lang: 'fr', toasts: [] });
});

describe('ServerAutomodTab', () => {
  it('annonce la borne du nœud dans le compteur, pas 100', () => {
    seed(['spam']);
    render(<ServerAutomodTab groupId="g1" />);
    expect(screen.getByText(`Mots filtrés — 1/${MAX_AUTOMOD_WORDS}`)).toBeTruthy();
    expect(screen.queryByText('Mots filtrés — 1/100')).toBeNull();
  });

  it('refuse le mot au-delà de la borne du nœud, avec un message explicite', () => {
    // Liste pleine AU SENS DU NŒUD : le 51ᵉ mot ne doit pas entrer dans la
    // liste éditée, sinon « Enregistrer » partirait vers un refus du nœud.
    const pleine = Array.from({ length: MAX_AUTOMOD_WORDS }, (_, i) => `mot${i}`);
    seed(pleine);
    render(<ServerAutomodTab groupId="g1" />);

    addWord('unmotdetrop');

    expect(screen.getByRole('alert').textContent).toBe(
      `${MAX_AUTOMOD_WORDS} mots au maximum.`,
    );
    expect(
      screen.getByText(`Mots filtrés — ${MAX_AUTOMOD_WORDS}/${MAX_AUTOMOD_WORDS}`),
    ).toBeTruthy();
    expect(screen.queryByText('unmotdetrop')).toBeNull();
  });

  it('accepte un mot tant que la borne n’est pas atteinte', () => {
    const presque = Array.from({ length: MAX_AUTOMOD_WORDS - 1 }, (_, i) => `mot${i}`);
    seed(presque);
    render(<ServerAutomodTab groupId="g1" />);

    addWord('dernier');

    expect(screen.queryByRole('alert')).toBeNull();
    expect(
      screen.getByText(`Mots filtrés — ${MAX_AUTOMOD_WORDS}/${MAX_AUTOMOD_WORDS}`),
    ).toBeTruthy();
  });

  it('compte les CARACTÈRES et non les unités de code pour la longueur', () => {
    seed([]);
    render(<ServerAutomodTab groupId="g1" />);

    // 32 emojis = 32 caractères mais 64 unités de code : la borne du nœud
    // compte des caractères Unicode, l'écran doit compter pareil.
    addWord('🙂'.repeat(32));

    expect(screen.queryByRole('alert')).toBeNull();
    expect(screen.getByText('Mots filtrés — 1/50')).toBeTruthy();

    // Un caractère de plus est bien refusé.
    addWord('a'.repeat(33));
    expect(screen.getByRole('alert').textContent).toBe(
      'Mot trop long (32 caractères au plus).',
    );
  });
});
