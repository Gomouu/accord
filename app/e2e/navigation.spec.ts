import { expect, test } from '@playwright/test';
import { barreDemo, ouvrirShowcase } from './helpers';

test.describe('navigation entre les vues', () => {
  test('salon → MP → amis → retour, salon actif surligné', async ({ page }) => {
    await ouvrirShowcase(page);
    const barre = barreDemo(page);

    await expect(page.getByLabel('Écrire dans #général')).toBeVisible();
    const salonActif = page.locator('[aria-current="page"]', {
      hasText: 'général',
    });
    await expect(salonActif).toBeVisible();

    await barre.mp.click();
    await expect(page.getByLabel('Écrire à @Noa Chen')).toBeVisible();

    await barre.amis.click();
    await expect(page.getByRole('button', { name: 'Ajouter un ami' })).toBeVisible();

    await barre.salon.click();
    await expect(page.getByLabel('Écrire dans #général')).toBeVisible();
    await expect(salonActif).toBeVisible();

    const autreSalon = page.getByRole('button', { name: 'design-lab' });
    await autreSalon.click();
    await expect(page.getByLabel('Écrire dans #design-lab')).toBeVisible();
    await expect(
      page.locator('[aria-current="page"]', { hasText: 'design-lab' }),
    ).toBeVisible();
    await expect(salonActif).toHaveCount(0);
  });
});
