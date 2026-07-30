import { expect, test } from '@playwright/test';
import { THEME_IDS } from '../src/stores/uiConstants';
import { contrastRatio, type Rgb } from '../src/lib/themeContrast';
import { SHOWCASE } from './helpers';

/**
 * Contraste WCAG AA sur **tous** les thèmes (jalon 8, « les 24 thèmes »).
 *
 * **Pourquoi ce fichier existe.** `lib/themeContrast.ts` — luminance relative,
 * rapport de contraste, composition alpha — était écrit et **importé par rien**.
 * Un balayage de code mort l'a signalé comme export inutilisé ; il ne l'était
 * pas, il était en avance sur son usage. L'outil de la passe d'accessibilité
 * existait, la passe n'avait jamais été faite.
 *
 * **Pourquoi dans un vrai navigateur.** Les variables de thème se composent en
 * cascade (`:root` puis `[data-theme='…']`, plus les fichiers de scène). Lire
 * les fichiers CSS à la main obligerait à réimplémenter cette cascade — et
 * c'est exactement le genre de réimplémentation qui diverge en silence. Ici
 * c'est le moteur de rendu qui la résout.
 *
 * ⚠️ Ce que cette garde NE couvre PAS : le texte sur image (bannières, effets
 * de profil), où le fond n'est pas une couleur unie, et le rendu réel d'un
 * lecteur d'écran. Le premier demande une analyse d'image, le second une
 * machine et des oreilles — voir §1.4 de la feuille de route.
 */

/** Seuil AA pour du texte de corps. */
const AA_TEXTE = 4.5;
/** Seuil AA pour du texte large et les éléments d'interface. */
const AA_LARGE = 3.0;

/**
 * Paires (premier plan, arrière-plan) à vérifier, par nom de variable CSS.
 * `faint` est du texte secondaire de petite taille : il tombe sous le seuil de
 * corps, pas sous celui des grands éléments.
 */
const PAIRES: ReadonlyArray<{ avant: string; arriere: string; seuil: number }> = [
  { avant: '--color-norm', arriere: '--color-chat', seuil: AA_TEXTE },
  { avant: '--color-muted', arriere: '--color-chat', seuil: AA_TEXTE },
  { avant: '--color-faint', arriere: '--color-chat', seuil: AA_LARGE },
  { avant: '--color-link-text', arriere: '--color-chat', seuil: AA_TEXTE },
  { avant: '--color-danger-text', arriere: '--color-chat', seuil: AA_LARGE },
  { avant: '--color-success-text', arriere: '--color-chat', seuil: AA_LARGE },
];

/**
 * Valeur d'une variable de thème → triplet.
 *
 * Le projet suit la convention Tailwind : les variables portent « 49 51 56 »,
 * trois canaux nus, que les feuilles composent en `rgb(var(--x) / <alpha>)`.
 * Une propriété personnalisée n'est pas résolue par le moteur — `getPropertyValue`
 * rend le jeton littéral — donc c'est cette forme-là qu'on lit, et `rgb(...)`
 * en repli si une variable venait à être écrite autrement.
 */
function versRgb(css: string): Rgb | null {
  const nus = /^\s*([\d.]+)\s+([\d.]+)\s+([\d.]+)\s*$/.exec(css);
  if (nus !== null) return [Number(nus[1]), Number(nus[2]), Number(nus[3])] as const;
  const fonction = /rgba?\(\s*([\d.]+)[,\s]+([\d.]+)[,\s]+([\d.]+)/.exec(css);
  if (fonction === null) return null;
  return [Number(fonction[1]), Number(fonction[2]), Number(fonction[3])] as const;
}

test.describe('contraste des thèmes (jalon 8)', () => {
  test('les 25 thèmes tiennent le seuil AA sur les paires de texte courantes', async ({
    page,
  }) => {
    await page.goto(SHOWCASE);

    const echecs: string[] = [];
    for (const theme of THEME_IDS) {
      await page.evaluate((id) => {
        document.documentElement.setAttribute('data-theme', id);
      }, theme);

      // Une seule traversée du DOM par thème : `getComputedStyle` est cher, et
      // 25 thèmes x 6 paires x un aller-retour ferait une minute de test.
      const mesures = await page.evaluate(
        (noms: string[]) => {
          const style = getComputedStyle(document.documentElement);
          return noms.map((n) => style.getPropertyValue(n).trim());
        },
        [...new Set(PAIRES.flatMap((p) => [p.avant, p.arriere]))],
      );

      const noms = [...new Set(PAIRES.flatMap((p) => [p.avant, p.arriere]))];
      const table = new Map(noms.map((n, i) => [n, mesures[i]]));

      for (const { avant, arriere, seuil } of PAIRES) {
        const fg = versRgb(table.get(avant) ?? '');
        const bg = versRgb(table.get(arriere) ?? '');
        if (fg === null || bg === null) {
          echecs.push(`${theme} : ${avant} ou ${arriere} illisible`);
          continue;
        }
        const ratio = contrastRatio(fg, bg);
        if (ratio < seuil) {
          echecs.push(
            `${theme} : ${avant} sur ${arriere} = ${ratio.toFixed(2)} (seuil ${seuil})`,
          );
        }
      }
    }

    // Tous les manquements d'un coup : corriger un thème pour découvrir le
    // suivant à l'exécution d'après ferait vingt-cinq allers-retours.
    expect(echecs, `contraste insuffisant :\n${echecs.join('\n')}`).toEqual([]);
  });
});
