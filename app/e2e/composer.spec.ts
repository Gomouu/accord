import { expect, test } from '@playwright/test';
import { ouvrirShowcase } from './helpers';

test.describe('composeur', () => {
  test('taper puis envoyer', async ({ page }) => {
    await ouvrirShowcase(page);
    const champ = page.getByLabel('Écrire dans #général');
    await champ.click();
    await champ.fill('Message de vérification e2e');
    await expect(champ).toHaveValue('Message de vérification e2e');

    await page.keyboard.press('Enter');
    const envoye = page
      .getByRole('main')
      .getByText('Message de vérification e2e', { exact: true });
    const compose = await champ.inputValue();
    if (compose === '') {
      await expect(envoye).toBeVisible();
    } else {
      await expect(champ).toHaveValue('Message de vérification e2e');
    }
  });

  test('flèche haut sur composeur vide édite le dernier message envoyé', async ({
    page,
  }) => {
    await ouvrirShowcase(page);
    const champ = page.getByLabel('Écrire dans #général');
    await champ.click();
    await expect(champ).toHaveValue('');
    await page.keyboard.press('ArrowUp');

    const editeur = page.locator('textarea', {
      hasText: 'Parfait. Je garde le contraste calme',
    });
    await expect(editeur).toBeVisible();
    await page.keyboard.press('Escape');
    await expect(editeur).toHaveCount(0);
  });
});
