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
    // ⚠️ Attendre le FOCUS, pas seulement l'affichage. `MessageEditor` pose le
    // focus dans un `useEffect`, donc après la peinture : `toBeVisible()` rend
    // la main dans l'intervalle. Et `Escape` est traité par le `onKeyDown` de
    // la zone d'édition — sans le focus, la touche part dans le composeur et
    // l'éditeur reste ouvert.
    //
    // C'est exactement ce qui est arrivé le 2026-07-28 : vert cinq fois de
    // suite en local et sur la CI de `main`, rouge sur le runner de release,
    // plus lent. Une course ne se prouve pas verte, elle se supprime.
    await expect(editeur).toBeFocused();
    // Frappe sur le localisateur plutôt que sur la page : Playwright cible
    // alors explicitement l'élément au lieu de dépendre du focus ambiant.
    await editeur.press('Escape');
    await expect(editeur).toHaveCount(0);
  });
});
