/**
 * Garde d'accessibilité : aucun contrôle atteignable ne doit être anonyme.
 *
 * **Pourquoi cette garde plutôt qu'un audit.** L'audit a été fait : 319
 * éléments `<button>` dans les sources, 319 avec un nom accessible. Le
 * problème n'est donc pas l'état — c'est qu'il n'existait rien pour le
 * maintenir. Un bouton à icône seule ajouté demain sans `aria-label` serait
 * invisible au lecteur d'écran et personne ne le verrait, parce que rien ne
 * regarde.
 *
 * **Pourquoi ici et pas en test unitaire.** Une première version balayait les
 * sources à l'expression régulière. Elle a produit trois faux positifs en deux
 * itérations — un `{q}` pris pour du vide, un `<span aria-hidden />`
 * auto-fermant dont le `</span>` suivant appartenait au libellé. Un nom
 * accessible se calcule à partir de l'arbre rendu (contenu, `aria-label`,
 * `aria-labelledby`, `title`, `alt`, éléments masqués écartés), pas du texte
 * source. Ici c'est le navigateur qui le calcule, avec les règles ARIA : zéro
 * heuristique, donc zéro faux positif à maintenir.
 *
 * Ce que cette garde NE couvre PAS : les surfaces absentes du banc de
 * démonstration. La palette de commandes n'y est pas câblée — elle a ses
 * propres tests (`QuickSwitcher.test.tsx`), qui vérifient le motif combobox
 * complet.
 */

import { expect, test, type Page } from '@playwright/test';
import { barreDemo, boutonMenuServeur, ouvrirShowcase } from './helpers';

/**
 * Rôles qui se prennent au clavier ou à la souris. Un contrôle anonyme dans
 * cette liste est annoncé « bouton », « lien », sans plus — inutilisable sans
 * voir l'écran.
 */
const ROLES_INTERACTIFS = [
  'button',
  'link',
  'checkbox',
  'switch',
  'tab',
  'menuitem',
  'menuitemcheckbox',
  'menuitemradio',
  'radio',
  'combobox',
  'textbox',
  'slider',
] as const;

const LIGNE_INTERACTIVE = new RegExp(`^\\s*- (${ROLES_INTERACTIFS.join('|')})\\b`);

/**
 * Contrôles atteignables dépourvus de nom accessible.
 *
 * `ariaSnapshot` rend une ligne par nœud, le nom entre guillemets quand il y
 * en a un : `- button "Ajouter un ami"`. Sans nom, la ligne s'arrête au rôle.
 */
async function controlesAnonymes(page: Page): Promise<string[]> {
  const snapshot = await page.locator('body').ariaSnapshot();
  return snapshot
    .split('\n')
    .filter((ligne) => LIGNE_INTERACTIVE.test(ligne) && !ligne.includes('"'))
    .map((ligne) => ligne.trim());
}

async function attendreControles(page: Page): Promise<void> {
  await expect(page.getByRole('button').first()).toBeVisible();
}

test.describe('accessibilité — aucun contrôle anonyme', () => {
  test('sur les vues salon, MP et amis', async ({ page }) => {
    await ouvrirShowcase(page);
    const barre = barreDemo(page);

    for (const [nom, aller] of [
      ['salon', async () => {}],
      ['MP', async () => barre.mp.click()],
      ['amis', async () => barre.amis.click()],
    ] as const) {
      await aller();
      await attendreControles(page);
      expect(await controlesAnonymes(page), `vue ${nom}`).toEqual([]);
    }
  });

  test('dans le menu de serveur ouvert', async ({ page }) => {
    await ouvrirShowcase(page);
    await boutonMenuServeur(page).click();
    await expect(page.getByRole('menu')).toBeVisible();
    expect(await controlesAnonymes(page)).toEqual([]);
  });

  test('dans les réglages', async ({ page }) => {
    await ouvrirShowcase(page);
    await page.getByRole('button', { name: 'Réglages' }).click();
    await expect(page.getByRole('dialog')).toBeVisible();
    expect(await controlesAnonymes(page)).toEqual([]);
  });

  test('et la garde elle-même attrape un contrôle anonyme', async ({ page }) => {
    // 🔒 Sans ce test, la garde deviendrait muette sans que personne le voie :
    // il suffirait que `ariaSnapshot` change de format pour qu'aucune ligne ne
    // corresponde plus au motif, et les quatre tests ci-dessus passeraient sur
    // une liste vide en rapportant « aucun contrôle anonyme ». On injecte donc
    // un bouton à icône seule, sans étiquette — exactement la faute cherchée —
    // et on exige de la garde qu'elle le désigne.
    await ouvrirShowcase(page);
    expect(await controlesAnonymes(page)).toEqual([]);

    await page.evaluate(() => {
      const bouton = document.createElement('button');
      bouton.type = 'button';
      const icone = document.createElement('span');
      icone.setAttribute('aria-hidden', 'true');
      icone.textContent = '★';
      bouton.append(icone);
      document.body.append(bouton);
    });

    // On exige la DÉTECTION, pas une chaîne exacte : le format de
    // `ariaSnapshot` appartient à Playwright et peut bouger. Si un jour il
    // bouge au point que plus rien ne corresponde, c'est ici que ça se voit —
    // la liste redevient vide et ce test tombe, au lieu que les quatre autres
    // se mettent à passer sur du vide.
    const anonymes = await controlesAnonymes(page);
    expect(anonymes).toHaveLength(1);
    expect(anonymes[0]).toContain('button');
  });

  test('dans la grille vidéo d’appel', async ({ page }) => {
    // 🔒 Surface nommée par la feuille de route (§9.5) : la grille est
    // récente, ses tuiles portent des boutons d'épinglage à icône seule, et
    // c'est exactement le genre de contrôle qui part sans étiquette.
    await ouvrirShowcase(page);
    await page.getByRole('button', { name: 'Appel vidéo' }).click();
    await attendreControles(page);
    expect(await controlesAnonymes(page)).toEqual([]);
  });
});
