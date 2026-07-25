import { expect, test } from '@playwright/test';
import { ouvrirShowcase } from './helpers';

/**
 * Écriture de droite à gauche (arabe).
 *
 * Le sens vit sur `<html>` : le navigateur en dérive l'ordre visuel des `flex`
 * et `grid`, et le sens des propriétés logiques (`ms-`/`me-`, `start-`/`end-`).
 * On force donc `dir` ici plutôt que de basculer la langue — la vitrine ne
 * charge pas les dictionnaires, et c'est la mise en page qu'on veut mesurer,
 * pas la traduction (celle-là est couverte par le test de parité).
 */
test.describe('écriture de droite à gauche', () => {
  test.beforeEach(async ({ page }) => {
    await ouvrirShowcase(page);
    await page.evaluate(() => {
      document.documentElement.dir = 'rtl';
    });
  });

  test('les colonnes passent de l’autre côté', async ({ page }) => {
    const largeur = page.viewportSize()?.width ?? 0;
    expect(largeur).toBeGreaterThan(0);

    // Le rail des serveurs est la première colonne dans l'ordre de lecture :
    // à gauche en LTR, il doit se retrouver à droite ici. C'est la vérification
    // qui compte — si elle passe, le miroir des `flex` a bien opéré.
    const rail = page.getByRole('navigation', { name: 'Accord' });
    const boite = await rail.boundingBox();
    expect(boite).not.toBeNull();
    expect(boite!.x).toBeGreaterThan(largeur / 2);
  });

  test('la page ne déborde pas horizontalement', async ({ page }) => {
    // Un débordement en RTL trahit une largeur calculée depuis un bord
    // physique — le symptôme classique d'une propriété non miroitée oubliée.
    const debordement = await page.evaluate(
      () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
    );
    expect(debordement).toBeLessThanOrEqual(1);
  });

  test('revenir en ltr remet les colonnes à leur place', async ({ page }) => {
    await page.evaluate(() => {
      document.documentElement.dir = 'ltr';
    });

    const largeur = page.viewportSize()?.width ?? 0;
    const boite = await page.getByRole('navigation', { name: 'Accord' }).boundingBox();
    expect(boite).not.toBeNull();
    expect(boite!.x).toBeLessThan(largeur / 2);
  });
});
