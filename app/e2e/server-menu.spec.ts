import { expect, test } from '@playwright/test';
import { boutonMenuServeur, ouvrirShowcase } from './helpers';

test.describe('menu du serveur', () => {
  test('ouverture au clic, marquer comme lu en tête, profil de serveur', async ({
    page,
  }) => {
    await ouvrirShowcase(page);
    await boutonMenuServeur(page).click();

    const menu = page.getByRole('menu', { name: 'Menu du serveur' });
    await expect(menu).toBeVisible();

    const items = menu.getByRole('menuitem');
    await expect(items.first()).toHaveText('Marquer comme lu');
    await expect(
      menu.getByRole('menuitem', { name: 'Modifier mon profil de serveur' }),
    ).toBeVisible();
  });

  test('navigation clavier : flèches puis Échap', async ({ page }) => {
    await ouvrirShowcase(page);
    const declencheur = boutonMenuServeur(page);
    await declencheur.click();

    const menu = page.getByRole('menu', { name: 'Menu du serveur' });
    await expect(menu).toBeVisible();

    await page.keyboard.press('ArrowDown');
    const premier = menu.getByRole('menuitem', { name: 'Marquer comme lu' });
    const focalise = menu.locator(':focus');
    await expect(focalise).toHaveCount(1);
    const avantFleche = await focalise.textContent();
    await page.keyboard.press('ArrowDown');
    await expect(focalise).toHaveCount(1);
    const apresFleche = await focalise.textContent();
    expect(apresFleche).not.toBe(avantFleche);
    await expect(premier).toBeVisible();

    await page.keyboard.press('Escape');
    await expect(menu).toHaveCount(0);
    await expect(declencheur).toHaveAttribute('aria-expanded', 'false');
  });
});
