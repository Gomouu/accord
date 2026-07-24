/**
 * Grille vidéo d'appel — vérification à l'écran.
 *
 * Une mise en page ne se juge pas en test unitaire : « deux tuiles existent »
 * ne dit rien sur le fait qu'elles soient côte à côte, visibles, et qu'elles
 * ne débordent pas. Ces tests ouvrent la vraie interface dans un navigateur et
 * regardent.
 *
 * Les canevas restent noirs (aucun flux réel sans backend) : c'est la
 * composition qu'on vérifie, pas l'image. Le rendu vidéo effectif reste dans
 * la liste des choses non vérifiables en headless.
 */

import { expect, test } from '@playwright/test';
import { ouvrirShowcase } from './helpers';

/** Ouvre l'aperçu d'appel de la barre de démonstration. */
async function ouvrirAppel(page: import('@playwright/test').Page): Promise<void> {
  await ouvrirShowcase(page);
  await page.getByRole('button', { name: 'Appel vidéo', exact: true }).click();
  await expect(page.getByRole('group', { name: 'Participants en vidéo' })).toBeVisible();
}

test.describe('grille vidéo d’appel', () => {
  test('affiche une tuile par flux, nommée d’après son émetteur', async ({ page }) => {
    await ouvrirAppel(page);
    const grille = page.getByRole('group', { name: 'Participants en vidéo' });

    // Trois flux : la caméra de Noa, la caméra ET l'écran de Mina.
    await expect(grille.getByLabel('Noa Chen', { exact: true })).toBeVisible();
    await expect(grille.getByLabel('Mina Sol', { exact: true })).toBeVisible();
    await expect(grille.getByLabel(/^Mina Sol — /)).toBeVisible();
  });

  test('ne déborde pas de la fenêtre', async ({ page }) => {
    await ouvrirAppel(page);
    const grille = page.getByRole('group', { name: 'Participants en vidéo' });
    const boite = await grille.boundingBox();
    const largeur = page.viewportSize()?.width ?? 0;

    expect(boite).not.toBeNull();
    expect(boite!.x).toBeGreaterThanOrEqual(0);
    expect(boite!.x + boite!.width).toBeLessThanOrEqual(largeur);
  });

  test('épingler ne laisse qu’un flux, retirer l’épingle les rend tous', async ({
    page,
  }) => {
    await ouvrirAppel(page);
    const grille = page.getByRole('group', { name: 'Participants en vidéo' });
    // Une tuile = un canevas ; c'est le repère le plus direct de leur nombre.
    const tuiles = grille.locator('canvas');

    await grille.getByRole('button', { name: 'Épingler' }).first().click();
    await expect(grille.getByRole('button', { name: 'Retirer l’épingle' })).toBeVisible();
    await expect(tuiles).toHaveCount(1);

    await grille.getByRole('button', { name: 'Retirer l’épingle' }).click();
    await expect(tuiles).toHaveCount(3);
  });

  test('les boutons caméra et écran restent atteignables', async ({ page }) => {
    // La grille est un panneau flottant : elle ne doit jamais recouvrir ses
    // propres commandes.
    await ouvrirAppel(page);
    await expect(page.getByRole('button', { name: 'Activer la caméra' })).toBeVisible();
    await expect(page.getByRole('button', { name: 'Partager l’écran' })).toBeVisible();
  });

  test('tient dans une fenêtre étroite', async ({ page }) => {
    await page.setViewportSize({ width: 900, height: 700 });
    await ouvrirAppel(page);
    const grille = page.getByRole('group', { name: 'Participants en vidéo' });
    const boite = await grille.boundingBox();

    expect(boite).not.toBeNull();
    expect(boite!.x + boite!.width).toBeLessThanOrEqual(900);
    await expect(page.getByRole('button', { name: 'Activer la caméra' })).toBeVisible();
  });
});
