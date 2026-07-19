/** Tests du thème personnalisé : conversions, dérivations, application DOM. */

import { describe, expect, it } from 'vitest';
import {
  PERSO_DEFAUT,
  ajusterLuminosite,
  appliquerThemePerso,
  deriverVariables,
  exporterTheme,
  hexVersRgb,
  hexVersTriplet,
  importerTheme,
} from './customTheme';

describe('hexVersRgb / hexVersTriplet', () => {
  it('décompose un hex valide', () => {
    expect(hexVersRgb('#5865f2')).toEqual([88, 101, 242]);
    expect(hexVersTriplet('#5865F2')).toBe('88 101 242');
  });

  it('rejette un hex invalide', () => {
    expect(hexVersRgb('5865f2')).toBeNull();
    expect(hexVersRgb('#fff')).toBeNull();
    expect(hexVersRgb('#gg0000')).toBeNull();
    expect(hexVersTriplet('')).toBeNull();
  });
});

describe('ajusterLuminosite', () => {
  it('éclaircit et assombrit avec bornage', () => {
    expect(ajusterLuminosite('#000000', 16)).toBe('#101010');
    expect(ajusterLuminosite('#ffffff', 16)).toBe('#ffffff');
    expect(ajusterLuminosite('#101010', -32)).toBe('#000000');
  });

  it('laisse un hex invalide inchangé', () => {
    expect(ajusterLuminosite('oops', 10)).toBe('oops');
  });
});

describe('deriverVariables', () => {
  it('dérive les neuf variables en base sombre', () => {
    const vars = deriverVariables(PERSO_DEFAUT);
    expect(Object.keys(vars)).toHaveLength(9);
    expect(vars['--color-chat']).toBe('49 51 56');
    expect(vars['--color-blurple']).toBe('88 101 242');
    const chat = vars['--color-chat']!.split(' ').map(Number);
    const survol = vars['--color-chat-hover']!.split(' ').map(Number);
    expect(survol[0]!).toBeLessThan(chat[0]!);
  });

  it('inverse le sens des dérivations en base claire', () => {
    const vars = deriverVariables({
      fond: '#e0e0e0',
      panneaux: '#d0d0d0',
      accent: '#5865f2',
      base: 'light',
    });
    const chat = vars['--color-chat']!.split(' ').map(Number);
    const survol = vars['--color-chat-hover']!.split(' ').map(Number);
    expect(survol[0]!).toBeGreaterThan(chat[0]!);
  });

  it('omet les variables issues de couleurs invalides', () => {
    const vars = deriverVariables({ ...PERSO_DEFAUT, accent: 'nope' });
    expect(vars['--color-blurple']).toBeUndefined();
    expect(vars['--color-chat']).toBeDefined();
  });
});

describe('appliquerThemePerso', () => {
  it('pose puis retire les variables inline sur la racine', () => {
    appliquerThemePerso(PERSO_DEFAUT);
    const style = document.documentElement.style;
    expect(style.getPropertyValue('--color-chat')).toBe('49 51 56');
    expect(style.getPropertyValue('--color-blurple')).toBe('88 101 242');
    appliquerThemePerso(null);
    expect(style.getPropertyValue('--color-chat')).toBe('');
  });
});

describe('exporterTheme / importerTheme', () => {
  it('fait un aller-retour fidèle', () => {
    const theme = {
      fond: '#101020',
      panneaux: '#181828',
      accent: '#ff0080',
      base: 'light' as const,
    };
    const code = exporterTheme(theme);
    expect(code.startsWith('accord-theme:')).toBe(true);
    expect(importerTheme(code)).toEqual(theme);
  });

  it('accepte un code sans préfixe et avec espaces', () => {
    const code = exporterTheme(PERSO_DEFAUT).slice('accord-theme:'.length);
    expect(importerTheme(`  ${code}  `)).toEqual(PERSO_DEFAUT);
  });

  it('rejette un code corrompu', () => {
    expect(importerTheme('n’importe quoi')).toBeNull();
    expect(importerTheme('accord-theme:@@@')).toBeNull();
  });

  it('rejette un thème aux couleurs invalides', () => {
    const mauvais = btoa(
      JSON.stringify({ f: 'x', p: '#fff000', a: '#fff000', b: 'dark' }),
    );
    expect(importerTheme(`accord-theme:${mauvais}`)).toBeNull();
  });

  it('replie une base inconnue sur sombre', () => {
    const code = btoa(
      JSON.stringify({ f: '#101020', p: '#181828', a: '#ff0080', b: 'bizarre' }),
    )
      .replace(/\+/g, '-')
      .replace(/\//g, '_')
      .replace(/=+$/, '');
    expect(importerTheme(`accord-theme:${code}`)?.base).toBe('dark');
  });
});
