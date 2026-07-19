import { expect, test } from '@playwright/test';
import { ouvrirShowcase } from './helpers';

test.describe('fil de messages', () => {
  test('séparateur « nouveaux messages » sur le premier non-lu', async ({ page }) => {
    await ouvrirShowcase(page);
    const separateur = page.getByText('Nouveaux messages', { exact: true });
    await expect(separateur).toBeVisible();
  });

  test('bouton « Revenir en bas » après défilement vers le haut', async ({ page }) => {
    await page.setViewportSize({ width: 900, height: 440 });
    await ouvrirShowcase(page);

    const bouton = page.getByRole('button', { name: 'Revenir en bas' });
    await expect(bouton).toHaveCount(0);

    const fil = page
      .getByText('La synchronisation reprend proprement', { exact: false })
      .first();
    await fil.hover();
    await page.mouse.wheel(0, -2000);

    await expect(bouton).toBeVisible();
    await bouton.click();
    await expect(bouton).toHaveCount(0);
  });

  test('copie d’un bloc de code (bouton Copier)', async ({ page, context }) => {
    await context.grantPermissions(['clipboard-read', 'clipboard-write']);
    await ouvrirShowcase(page);

    const bloc = page.locator('pre code');
    const seedSansCode = (await bloc.count()) === 0;
    test.skip(
      seedSansCode,
      'le seed du showcase ne contient aucun bloc de code et l’envoi exige un nœud',
    );

    await bloc.first().hover();
    await page.getByRole('button', { name: 'Copier', exact: true }).first().click();
    const presse = await page.evaluate(() => navigator.clipboard.readText());
    expect(presse.length).toBeGreaterThan(0);
  });
});
