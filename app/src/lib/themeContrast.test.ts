import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import postcss from 'postcss';
import { THEME_IDS } from '../stores/ui';
import { compositeColor, contrastRatio, type Rgb } from './themeContrast';

const globalCss = readFileSync(resolve('src/styles/global.css'), 'utf8');
const sceneCss = readFileSync(resolve('src/styles/theme-scenes.css'), 'utf8');
const figurativeCss = readFileSync(resolve('src/styles/figurative-themes.css'), 'utf8');

const BUILT_IN_THEMES = THEME_IDS.filter((theme) => theme !== 'custom');
const SURFACES = ['rail', 'sidebar', 'chat', 'chat-hover', 'input', 'modal'] as const;
const TEXT_TOKENS = ['norm', 'muted', 'faint', 'link-text', 'accent-text'] as const;
const SYNTAX_TOKENS = [
  'syntax-keyword',
  'syntax-string',
  'syntax-comment',
  'syntax-number',
] as const;

function collectPalettes(): Map<string, Map<string, string>> {
  const base = new Map<string, string>();
  const overrides = new Map(
    BUILT_IN_THEMES.map((theme) => [theme, new Map<string, string>()]),
  );
  for (const css of [globalCss, sceneCss, figurativeCss]) {
    postcss.parse(css).walkRules((rule) => {
      const selectors = rule.selectors ?? [];
      const targets = BUILT_IN_THEMES.filter((theme) =>
        selectors.includes(`[data-theme='${theme}']`),
      );
      rule.walkDecls(/^--color-/, (declaration) => {
        if (selectors.includes(':root')) base.set(declaration.prop, declaration.value);
        for (const target of targets) {
          overrides.get(target)?.set(declaration.prop, declaration.value);
        }
      });
    });
  }
  return new Map(
    BUILT_IN_THEMES.map((theme) => [theme, new Map([...base, ...overrides.get(theme)!])]),
  );
}

function color(palette: Map<string, string>, token: string): Rgb {
  const value = palette.get(`--color-${token}`);
  if (value === undefined || !/^\d+\s+\d+\s+\d+$/.test(value.trim())) {
    throw new Error(`Missing RGB token --color-${token}`);
  }
  return value.trim().split(/\s+/).map(Number) as unknown as Rgb;
}

function expectAa(foreground: Rgb, background: Rgb, label: string): void {
  expect(contrastRatio(foreground, background), label).toBeGreaterThanOrEqual(4.5);
}

describe('contrast utilities', () => {
  it('matches the WCAG black and white ratio', () => {
    expect(contrastRatio([0, 0, 0], [255, 255, 255])).toBe(21);
  });

  it('composites translucent colors', () => {
    expect(compositeColor([255, 255, 255], 0.5, [0, 0, 0])).toEqual([128, 128, 128]);
  });
});

describe('built-in theme contrast', () => {
  const palettes = collectPalettes();

  it.each(BUILT_IN_THEMES)('%s keeps text and links AA on every surface', (theme) => {
    const palette = palettes.get(theme)!;
    for (const surface of SURFACES) {
      const background = color(palette, surface);
      for (const token of TEXT_TOKENS) {
        expectAa(color(palette, token), background, `${theme}:${token}/${surface}`);
      }
    }
  });

  it.each(BUILT_IN_THEMES)('%s keeps text AA on liquid glass', (theme) => {
    const palette = palettes.get(theme)!;
    const modal = color(palette, 'modal');
    for (const surface of SURFACES) {
      const glass = compositeColor(modal, 0.62, color(palette, surface));
      for (const token of TEXT_TOKENS) {
        expectAa(color(palette, token), glass, `${theme}:${token}/glass-${surface}`);
      }
    }
  });

  it.each(BUILT_IN_THEMES)('%s keeps mentions and semantic pills AA', (theme) => {
    const palette = palettes.get(theme)!;
    const pairs = [
      ['accent-text', 'blurple', 0.2],
      ['warning-text', 'yellow', 0.2],
      ['success-text', 'green', 0.15],
      ['danger-text', 'red', 0.15],
    ] as const;
    for (const surface of SURFACES) {
      const background = color(palette, surface);
      for (const [foregroundToken, tintToken, alpha] of pairs) {
        const tinted = compositeColor(color(palette, tintToken), alpha, background);
        expectAa(
          color(palette, foregroundToken),
          tinted,
          `${theme}:${foregroundToken}/${tintToken}-${surface}`,
        );
      }
    }
  });

  it.each(BUILT_IN_THEMES)('%s keeps badge labels AA', (theme) => {
    const palette = palettes.get(theme)!;
    expectAa(color(palette, 'on-green'), color(palette, 'green'), `${theme}:green badge`);
    expectAa(color(palette, 'on-red'), color(palette, 'red'), `${theme}:red badge`);
    expectAa([255, 255, 255], color(palette, 'blurple'), `${theme}:primary action`);
  });

  it.each(BUILT_IN_THEMES)('%s keeps focus indicators visible', (theme) => {
    const palette = palettes.get(theme)!;
    for (const surface of SURFACES) {
      expect(
        contrastRatio(color(palette, 'focus'), color(palette, surface)),
        `${theme}:focus/${surface}`,
      ).toBeGreaterThanOrEqual(3);
    }
  });

  it.each(BUILT_IN_THEMES)('%s keeps syntax highlighting AA', (theme) => {
    const palette = palettes.get(theme)!;
    for (const token of SYNTAX_TOKENS) {
      expectAa(color(palette, token), color(palette, 'input'), `${theme}:${token}`);
    }
  });
});
