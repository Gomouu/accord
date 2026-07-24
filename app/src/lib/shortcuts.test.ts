/**
 * Le catalogue de raccourcis est une promesse faite à l'utilisateur : ce qui y
 * figure doit exister, et ce qui existe doit y figurer. Ces tests attrapent les
 * deux dérives — un raccourci ajouté au code sans être documenté, et un libellé
 * documenté qui ne correspond plus à rien.
 */

import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import { dictionaries } from '../i18n/all';
import { SHORTCUTS, SHORTCUT_SECTIONS, renderCombo } from './shortcuts';

describe('catalogue des raccourcis', () => {
  it('a un libellé traduit et non vide dans chaque langue', () => {
    for (const [lang, dict] of Object.entries(dictionaries)) {
      const manquants = SHORTCUTS.filter(
        (s) => (dict.shortcuts[s.key] ?? '').trim() === '',
      ).map((s) => s.key);
      expect(manquants, `libellés manquants en ${lang}`).toEqual([]);
    }
  });

  it('a un titre traduit pour chaque section', () => {
    for (const [lang, dict] of Object.entries(dictionaries)) {
      for (const { titleKey } of SHORTCUT_SECTIONS) {
        expect(dict.shortcuts[titleKey], `${lang}.${String(titleKey)}`).toBeTruthy();
      }
    }
  });

  it('range chaque raccourci dans une section connue', () => {
    const connues = new Set(SHORTCUT_SECTIONS.map((s) => s.id));
    const orphelins = SHORTCUTS.filter((s) => !connues.has(s.section));
    expect(orphelins).toEqual([]);
  });

  it('ne liste pas deux fois le même libellé', () => {
    const cles = SHORTCUTS.map((s) => s.key);
    expect(new Set(cles).size).toBe(cles.length);
  });

  it('n’a aucune section vide', () => {
    // Une section sans ligne s'afficherait comme un titre suivi de rien.
    // `voice` fait exception : elle porte aussi l'appui-pour-parler, ajouté au
    // rendu parce que sa touche est configurable.
    for (const { id } of SHORTCUT_SECTIONS) {
      expect(
        SHORTCUTS.some((s) => s.section === id),
        `section ${id}`,
      ).toBe(true);
    }
  });
});

describe('renderCombo', () => {
  it('substitue les jetons dépendant de la plateforme et de la langue', () => {
    expect(
      renderCombo(['mod', 'K'], { mod: '⌘', enter: 'Entrée', esc: 'Échap' }),
    ).toEqual(['⌘', 'K']);
    expect(
      renderCombo(['⇧', 'enter'], { mod: 'Ctrl', enter: 'Entrée', esc: 'Échap' }),
    ).toEqual(['⇧', 'Entrée']);
    expect(renderCombo(['esc'], { mod: 'Ctrl', enter: 'Enter', esc: 'Esc' })).toEqual([
      'Esc',
    ]);
  });

  it('laisse intacte une touche littérale qui ressemble à un jeton', () => {
    // `M`, `0`, `+` ne doivent jamais être réécrits.
    expect(renderCombo(['mod', '⇧', 'M'], { mod: 'Ctrl', enter: 'e', esc: 'x' })).toEqual(
      ['Ctrl', '⇧', 'M'],
    );
  });
});

describe('correspondance avec les raccourcis réellement câblés', () => {
  /**
   * `AppShell` est la seule source des raccourcis GLOBAUX. On lit son code
   * plutôt que de simuler des frappes : ce qu'on veut détecter, c'est un
   * raccourci ajouté là-bas et oublié ici — un test de comportement ne le
   * verrait pas, il ne teste que ce qu'on pense à tester.
   */
  const source = readFileSync(resolve(__dirname, '../components/AppShell.tsx'), 'utf8');

  it('documente chaque raccourci global implémenté', () => {
    const documentes = new Set(SHORTCUTS.map((s) => s.combo.join('|')));
    const attendus: [string, string][] = [
      ['mod|K', 'Ctrl/Cmd+K — sélecteur rapide'],
      ['mod|+', 'Ctrl/Cmd+= — zoom avant'],
      ['mod|-', 'Ctrl/Cmd+- — zoom arrière'],
      ['mod|0', 'Ctrl/Cmd+0 — zoom par défaut'],
      ['mod|⇧|M', 'Ctrl/Cmd+Maj+M — micro'],
      ['Alt|↑', 'Alt+Haut — conversation précédente'],
      ['Alt|↓', 'Alt+Bas — conversation suivante'],
    ];
    const absents = attendus
      .filter(([combo]) => !documentes.has(combo))
      .map(([, quoi]) => quoi);
    expect(absents).toEqual([]);
  });

  it('ne rate aucune touche interceptée globalement', () => {
    // Garde-fou grossier mais efficace : si `AppShell` se met à intercepter une
    // touche dont on n'a pas parlé, ce test le signale au lieu de laisser le
    // raccourci vivre sans documentation.
    const interceptees = new Set(
      [...source.matchAll(/e\.key(?:\.toLowerCase\(\))? === '([^']+)'/g)].map(
        (m) => m[1],
      ),
    );
    const connues = new Set(['k', '=', '+', '-', '0', 'm', 'ArrowUp', 'ArrowDown']);
    const nouvelles = [...interceptees].filter((k) => !connues.has(k ?? ''));
    expect(nouvelles, 'touche interceptée sans entrée au catalogue').toEqual([]);
  });
});
