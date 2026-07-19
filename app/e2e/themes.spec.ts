import { expect, test } from '@playwright/test';
import { barreDemo, ouvrirShowcase } from './helpers';

const THEMES = ['dark', 'light', 'wisteria'] as const;
const TAILLES = [
  { width: 1280, height: 800 },
  { width: 1440, height: 900 },
] as const;

test.describe('thèmes et régression visuelle', () => {
  test('la bascule de thème est appliquée au document', async ({ page }) => {
    await ouvrirShowcase(page);
    const barre = barreDemo(page);
    for (const theme of THEMES) {
      await barre.theme.selectOption(theme);
      await expect(page.locator('html')).toHaveAttribute('data-theme', theme);
    }
  });

  for (const taille of TAILLES) {
    for (const theme of THEMES) {
      test(`capture ${theme} en ${taille.width}×${taille.height}`, async ({ page }) => {
        await page.clock.install({ time: new Date('2026-01-01T12:00:00') });
        await page.setViewportSize(taille);
        await ouvrirShowcase(page);
        await barreDemo(page).theme.selectOption(theme);
        await expect(page.locator('html')).toHaveAttribute('data-theme', theme);
        await page.waitForTimeout(300);
        await expect(page).toHaveScreenshot(
          `showcase-${theme}-${taille.width}x${taille.height}.png`,
          { fullPage: false },
        );
      });
    }
  }
});
