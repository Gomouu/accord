import { expect, type Page } from '@playwright/test';

export const SHOWCASE = '/ui-showcase.html';

export async function ouvrirShowcase(page: Page): Promise<void> {
  await page.goto(SHOWCASE);
  await expect(boutonMenuServeur(page)).toBeVisible();
}

export function boutonMenuServeur(page: Page) {
  return page
    .getByTestId('server-header')
    .getByRole('button', { name: 'Atelier Cipher' });
}

export function barreDemo(page: Page) {
  const vues = page.getByLabel('Vues de démonstration');
  return {
    salon: vues.getByRole('button', { name: 'Salon', exact: true }),
    mp: vues.getByRole('button', { name: 'MP', exact: true }),
    amis: vues.getByRole('button', { name: 'Amis', exact: true }),
    theme: page.getByLabel('Thème de démonstration'),
  };
}
