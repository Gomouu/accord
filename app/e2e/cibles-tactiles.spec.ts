import { expect, test, type Page } from '@playwright/test';
import { barreDemo, boutonMenuServeur, ouvrirShowcase } from './helpers';

/**
 * Cibles tactiles — WCAG 2.2 SC 2.5.8 « Target Size (Minimum) », niveau AA
 * (feuille de route §9.5 : la dernière case de la passe d'accessibilité, la
 * seule à porter « non audité »).
 *
 * **Pourquoi ce fichier existe.** Les quatre autres cases de la passe se
 * lisent dans les sources ou dans les jetons de thème ; celle-ci non. La
 * taille d'un bouton est le produit d'un `padding`, d'une `line-height`, d'un
 * `gap` et d'une contrainte de parent : elle n'existe qu'après mise en page.
 * En jsdom `getBoundingClientRect()` rend zéro partout — un audit unitaire
 * aurait donc annoncé « rien à signaler » sur une interface entièrement
 * cassée, ce qui est la pire des réponses possibles.
 *
 * **Pourquoi dans un vrai navigateur.** Même raison que `contraste-themes` :
 * seul le moteur de rendu résout la cascade. On mesure ce que la souris
 * touche, pas ce que le CSS déclare.
 *
 * ## Ce qui compte comme cible, et ce qui n'en est pas une
 *
 * Une cible, au sens de la norme, est « une région qui accepte une action de
 * pointage ». Trois filtres découlent de cette définition et sont appliqués
 * ici — ce sont des exclusions, elles sont donc explicites :
 *
 * - **masqué** : `checkVisibility` (opacité, `visibility`, `content-visibility`
 *   sur toute la chaîne d'ancêtres) et `pointer-events: none` ;
 * - **hors écran visuel** : le motif `sr-only` (`clip: rect(0,0,0,0)`), qui
 *   sert aux lecteurs d'écran et au clavier. Le lien d'évitement fait 1 × 1 px
 *   tant qu'il n'a pas le focus ; aucun pointeur ne peut le viser, et lui
 *   demander 24 px reviendrait à le rendre visible ;
 * - **recouvert** : `elementFromPoint` au centre. Sans ce filtre, ouvrir une
 *   modale ferait mesurer toute l'application derrière son voile — on avait
 *   mesuré 149 « cibles » dans les réglages, dont une centaine inatteignables.
 *
 * ## Les exceptions de SC 2.5.8
 *
 * La norme en prévoit quatre. Ce qui est fait de chacune, et pourquoi :
 *
 * - **inline** (« la cible est dans une phrase ») : **implémentée**. Un lien
 *   pris dans un paragraphe a la hauteur de son interligne ; l'élargir
 *   déformerait le texte porteur. Le test exige les deux conditions —
 *   `display: inline*` **et** du texte non-cible dans la même coulée — parce
 *   qu'un `display: inline` isolé dans son parent n'est contraint par aucun
 *   interligne et n'a donc rien à voir avec cette exception.
 *
 * - **spacing** (« un cercle de 24 px centré sur la boîte n'en croise aucun
 *   autre ») : **délibérément non appliquée**, et c'est le seul choix de ce
 *   fichier qui mérite d'être discuté. Elle est écrite pour de petites cibles
 *   carrées isolées ; appliquée à la lettre, elle ne teste qu'**un point**, le
 *   centre de la boîte. Sur les huit manquements trouvés lors de l'audit
 *   initial, elle en excusait sept — dont une poignée de redimensionnement de
 *   6 px de large sur 740 de haut, sauvée parce que son centre tombait dans
 *   un trou de la colonne voisine, et deux curseurs de 636 × 20 px. Ce sont
 *   exactement les géométries où viser est difficile. Les sept étaient
 *   corrigeables en une ligne de CSS chacune ; on a corrigé plutôt qu'exempté.
 *   Un plancher de 24 px sans échappatoire automatique protège ce que
 *   l'exception laisse filer, et le jour où une cible aura vraiment besoin
 *   d'elle, il faudra l'écrire ici, à la main, avec sa raison.
 *
 * - **equivalent** et **essential** : non automatisées, aucune n'est invoquée.
 *   Elles demandent un jugement humain, pas une mesure.
 *
 * ## Une correction sans commentaire, et pourquoi
 *
 * Chaque correction porte sa raison à côté d'elle, sauf une : le bouton du nom
 * d'auteur dans `MessageList.tsx` est passé de `min-w-0` à `min-w-6` — un
 * pseudonyme court comme « Ari » rendait 21,3 px de large — sans une ligne
 * d'explication, parce que le fichier est à sa cote exacte dans le cliquet de
 * `scripts/check-file-size.mjs` et qu'un commentaire de cinq lignes l'aurait
 * fait grossir. Relever un plafond pour loger un commentaire aurait été le
 * plus mauvais des échanges ; la raison vit donc ici.
 *
 * ⚠️ Ce que cette garde NE couvre PAS :
 * - les surfaces absentes du banc de démonstration — la palette de commandes,
 *   le recadreur d'avatar (`AvatarCropper`) et le panneau vocal en session
 *   (`VoiceSection`) n'y sont pas câblés ; leurs curseurs ont été corrigés à
 *   la lecture, sans garde pour les tenir ;
 * - les états qu'aucun clic scripté n'ouvre ici ;
 * - la taille perçue sur un écran tactile réel, qui dépend de la densité
 *   physique ; la norme raisonne en pixels CSS, ce test aussi ;
 * - le seuil de confort 44 × 44 (SC 2.5.5, niveau AAA), qui sert de cible aux
 *   corrections quand la mise en page le permet mais n'est pas exigé ici.
 */

/** Seuil AA de SC 2.5.8, en pixels CSS. */
const MINIMUM = 24;

/**
 * Tolérance de mesure. Les sous-pixels sont réels : un `h-6` posé dans une
 * grille peut rendre 23,99 px après arrondi de mise en page. Refuser à 23,99
 * ferait rougir la garde sur du bruit de rendu, pas sur un défaut.
 */
const TOLERANCE = 0.5;

/**
 * Plancher de cibles mesurées, par défaut. 🔒 Il ne vérifie pas l'interface,
 * il vérifie **la garde** : une liste de manquements vide ne veut rien dire si
 * le balayage n'a rien lu. Un sélecteur cassé, un filtre de visibilité trop
 * large, et toutes les assertions passeraient au vert sur zéro élément — c'est
 * arrivé deux fois pendant l'écriture de ce fichier : `opacity-0` écartait les
 * barres d'action de message, puis une modale en cours d'animation d'entrée
 * rendait ses propres contrôles à `opacity: 0`.
 *
 * Le plancher seul ne suffit pourtant pas, et c'est le rôle du *témoin* (voir
 * `verifier`) : une troisième fois, la vue comptait déjà cinquante cibles au
 * moment où la barre survolée, elle, n'était pas encore apparue. Le compte
 * était bon, la surface visée manquait.
 */
const CIBLES_ATTENDUES_MIN = 12;

type Manquement = {
  readonly designation: string;
  readonly largeur: number;
  readonly hauteur: number;
};

type Releve = {
  /** Combien de cibles pointables ont réellement été mesurées. */
  readonly mesurees: number;
  /** Le témoin demandé figurait-il parmi elles ? Vrai si aucun n'est demandé. */
  readonly temoinVu: boolean;
  readonly manquements: readonly Manquement[];
};

/**
 * Mesure toutes les cibles pointables visibles de la page.
 *
 * Tout le calcul se fait dans la page : `getBoundingClientRect`,
 * `getComputedStyle` et `elementFromPoint` n'existent que là, et un
 * aller-retour par cible en ferait des centaines par surface.
 */
async function relever(page: Page, temoin: string | null): Promise<Releve> {
  return page.evaluate(
    ({ min, tolerance, temoin }) => {
      const SELECTEUR = [
        'button',
        'a[href]',
        'input:not([type="hidden"])',
        'select',
        'textarea',
        'summary',
        '[role="button"]',
        '[role="link"]',
        '[role="checkbox"]',
        '[role="switch"]',
        '[role="tab"]',
        '[role="radio"]',
        '[role="menuitem"]',
        '[role="menuitemcheckbox"]',
        '[role="menuitemradio"]',
        '[role="option"]',
        '[role="slider"]',
        '[tabindex]:not([tabindex="-1"])',
      ].join(',');

      /** Motif `sr-only` : présent pour les lecteurs d'écran, invisible au pointeur. */
      const masqueVisuellement = (el: Element): boolean => {
        const style = getComputedStyle(el);
        return (
          style.clip === 'rect(0px, 0px, 0px, 0px)' || style.clipPath === 'inset(50%)'
        );
      };

      /** Recouvert par une autre couche (voile de modale, popover…). */
      const recouvert = (el: Element, r: DOMRect): boolean => {
        const cx = (r.left + r.right) / 2;
        const cy = (r.top + r.bottom) / 2;
        // Hors du cadre visible : `elementFromPoint` ne répond rien d'utile.
        // La géométrie reste valide, on garde la cible.
        if (cx < 0 || cy < 0 || cx > innerWidth || cy > innerHeight) return false;
        const dessus = document.elementFromPoint(cx, cy);
        if (dessus === null) return true;
        return !(dessus === el || el.contains(dessus) || dessus.contains(el));
      };

      const estCible = (el: Element): DOMRect | null => {
        if (
          !el.checkVisibility({
            contentVisibilityAuto: true,
            opacityProperty: true,
            visibilityProperty: true,
          })
        ) {
          return null;
        }
        if (getComputedStyle(el).pointerEvents === 'none') return null;
        if (masqueVisuellement(el)) return null;
        const rect = el.getBoundingClientRect();
        if (rect.width <= 0 || rect.height <= 0) return null;
        if (recouvert(el, rect)) return null;
        return rect;
      };

      /** Exception *inline* : cible en flux de texte partageant sa coulée. */
      const estEnLigne = (el: Element): boolean => {
        if (!getComputedStyle(el).display.startsWith('inline')) return false;
        const parent = el.parentElement;
        if (parent === null) return false;
        for (const noeud of parent.childNodes) {
          if (
            noeud.nodeType === Node.TEXT_NODE &&
            (noeud.textContent ?? '').trim() !== ''
          ) {
            return true;
          }
        }
        return false;
      };

      /** De quoi retrouver l'élément dans les sources sans le chercher. */
      const designer = (el: Element): string => {
        const nom = (
          el.getAttribute('aria-label') ??
          el.getAttribute('title') ??
          (el.textContent ?? '').trim()
        ).slice(0, 40);
        const role = el.getAttribute('role');
        const classes = typeof el.className === 'string' ? el.className.trim() : '';
        const indice =
          classes === '' ? '' : ` — ${classes.split(/\s+/).slice(0, 4).join(' ')}`;
        return `<${el.tagName.toLowerCase()}${role === null ? '' : ` role="${role}"`}> « ${nom} »${indice}`;
      };

      /** Nom accessible approché, pour reconnaître le témoin. */
      const nommer = (el: Element): string =>
        el.getAttribute('aria-label') ??
        el.getAttribute('title') ??
        (el.textContent ?? '').trim();

      let mesurees = 0;
      let temoinVu = temoin === null;
      const manquements: Manquement[] = [];
      for (const el of document.querySelectorAll(SELECTEUR)) {
        const rect = estCible(el);
        if (rect === null) continue;
        mesurees += 1;
        if (temoin !== null && !temoinVu && nommer(el).includes(temoin)) temoinVu = true;
        if (rect.width >= min - tolerance && rect.height >= min - tolerance) continue;
        if (estEnLigne(el)) continue;
        manquements.push({
          designation: designer(el),
          largeur: Math.round(rect.width * 10) / 10,
          hauteur: Math.round(rect.height * 10) / 10,
        });
      }
      return { mesurees, temoinVu, manquements };
    },
    { min: MINIMUM, tolerance: TOLERANCE, temoin },
  );
}

/**
 * Vérifie une surface : aucune cible sous le seuil, et un balayage qui a
 * réellement lu quelque chose.
 *
 * Tous les manquements de la surface sont rapportés d'un coup — corriger le
 * premier pour découvrir le second à l'exécution suivante ferait autant
 * d'allers-retours que de boutons.
 */
async function verifier(
  page: Page,
  surface: string,
  temoin: string | null = null,
  attendues: number = CIBLES_ATTENDUES_MIN,
): Promise<void> {
  let releve: Releve = { mesurees: 0, temoinVu: false, manquements: [] };
  // Sondage plutôt que mesure unique, pour deux raisons qui ont chacune produit
  // un faux « rien à signaler » :
  //   — les panneaux entrent en fondu (`modal-panel-enter`, `popover-enter`) et
  //     pendant ce fondu leurs contrôles sont à `opacity: 0`, donc invisibles
  //     au sens de la norme, donc absents du relevé ;
  //   — `toBeVisible()` ne regarde pas l'opacité : il rend la main avant que la
  //     surface qu'on vient d'ouvrir soit mesurable.
  // Le témoin est le contrôle pour lequel on a ouvert la surface. Tant qu'il
  // n'est pas DANS le relevé, il n'y a rien à conclure de ce relevé.
  await expect
    .poll(async () => {
      releve = await relever(page, temoin);
      const trop_peu =
        releve.mesurees < attendues ? `${releve.mesurees} cible(s) seulement` : '';
      const absent = releve.temoinVu ? '' : `témoin « ${temoin ?? ''} » hors du relevé`;
      return [trop_peu, absent].filter((raison) => raison !== '').join(', ') || 'prêt';
    })
    .toBe('prêt');

  const detail = releve.manquements
    .map((m) => `  ${m.largeur}×${m.hauteur} px — ${m.designation}`)
    .join('\n');
  expect(
    releve.manquements,
    `${surface} : ${releve.manquements.length} cible(s) sous ${MINIMUM}×${MINIMUM} px (${releve.mesurees} mesurées) :\n${detail}`,
  ).toEqual([]);
}

test.describe('cibles tactiles — WCAG 2.2 SC 2.5.8 (jalon 9)', () => {
  test('sur les quatre vues de conversation', async ({ page }) => {
    await ouvrirShowcase(page);
    const barre = barreDemo(page);

    for (const [nom, aller, temoin] of [
      ['salon', async () => {}, 'Envoyer'],
      ['MP', async () => barre.mp.click(), 'Envoyer'],
      ['groupe de MP', async () => barre.groupeMp.click(), 'Envoyer'],
      ['amis', async () => barre.amis.click(), 'Envoyer un message'],
    ] as const) {
      await aller();
      await verifier(page, `vue ${nom}`, temoin);
    }
  });

  test('sur les surfaces révélées au survol d’un message', async ({ page }) => {
    // 🔒 Les barres d'action de message vivent en `opacity-0` jusqu'au survol.
    // Sans ce test elles ne seraient jamais mesurées — et ce sont justement
    // des grappes de boutons à icône seule, la géométrie que SC 2.5.8 vise.
    await ouvrirShowcase(page);
    await page.getByText('La synchronisation reprend').first().hover();
    await verifier(page, 'salon, message survolé', 'Répondre');

    await page.getByRole('button', { name: 'Ajouter une réaction' }).first().click();
    // « Réagir avec ❤️ » n'existe que dans ce volet : aucune pastille de
    // réaction du banc ne porte ce cœur.
    await verifier(page, 'volet de réaction rapide', 'Réagir avec ❤️');
    await page.keyboard.press('Escape');

    await page.getByText('La synchronisation reprend').first().click({ button: 'right' });
    await verifier(page, 'menu contextuel de message', 'Copier le texte');
  });

  test('dans les menus et panneaux latéraux', async ({ page }) => {
    await ouvrirShowcase(page);

    await boutonMenuServeur(page).click();
    await verifier(page, 'menu de serveur', 'Marquer comme lu');
    await page.keyboard.press('Escape');

    // Le panneau utilisateur est un `dialog`, pas un `menu` : il porte une
    // carte de profil complète, pas une liste d'items.
    await page.getByRole('button', { name: 'Menu utilisateur' }).first().click();
    await verifier(page, 'menu utilisateur', 'Copier le code ami');
    await page.keyboard.press('Escape');

    await page.getByRole('button', { name: 'Messages épinglés' }).first().click();
    await verifier(page, 'panneau des messages épinglés', 'Désépingler');
    await page.keyboard.press('Escape');

    await page.getByRole('button', { name: 'Fils' }).first().click();
    await verifier(page, 'panneau des fils', 'Rendre les panneaux plus souples');
    await page.keyboard.press('Escape');

    await page.getByRole('button', { name: 'Émoji' }).first().click();
    await verifier(page, 'sélecteur d’émojis du compositeur', 'Rechercher un émoji');
  });

  test('dans les quatorze onglets de réglages', async ({ page }) => {
    await ouvrirShowcase(page);
    await page.getByRole('button', { name: 'Réglages' }).click();
    const dialogue = page.getByRole('dialog');
    await expect(dialogue).toBeVisible();

    const onglets = await dialogue.locator('nav button').all();
    expect(onglets.length, 'les onglets de réglages ont disparu').toBeGreaterThanOrEqual(
      10,
    );
    for (const onglet of onglets) {
      const nom = ((await onglet.textContent()) ?? '?').trim();
      await onglet.click();
      // `level: 2` et `exact` : un onglet peut porter le même mot qu'un de ses
      // intertitres (« Notifications » titre l'onglet ET une de ses sections).
      await expect(
        dialogue.getByRole('heading', { level: 2, name: nom, exact: true }),
      ).toBeVisible();
      // Pas de témoin : le contenu d'un onglet n'entre pas en fondu, et le
      // titre de niveau 2 ci-dessus atteste déjà qu'il a été échangé.
      await verifier(page, `réglages / ${nom}`);
    }
  });

  test('dans la grille vidéo et les modales de groupe de MP', async ({ page }) => {
    await ouvrirShowcase(page);
    await page.getByRole('button', { name: 'Appel vidéo' }).click();
    await verifier(page, 'grille vidéo', 'Raccrocher');
    await page.getByRole('button', { name: 'Appel vidéo' }).click();

    const barre = barreDemo(page);
    await barre.mp.click();
    await page.getByRole('button', { name: 'Créer un groupe privé' }).click();
    await expect(
      page.getByRole('dialog', { name: 'Nouveau groupe privé' }),
    ).toBeVisible();
    // Plancher plus bas : cette modale n'expose que sa recherche, ses cases de
    // membres et ses deux boutons.
    await verifier(page, 'création de groupe privé', 'Créer', 6);
    await page.keyboard.press('Escape');

    await barre.groupeMp.click();
    await page.getByRole('button', { name: 'Paramètres du groupe' }).click();
    await expect(
      page.getByRole('dialog', { name: 'Paramètres du groupe' }),
    ).toBeVisible();
    await verifier(page, 'réglages de groupe privé', null, 6);
  });

  test('sur l’écran de premier lancement', async ({ page }) => {
    // Page à part : `Onboarding` n'est pas monté par le banc principal, et
    // c'est la toute première surface qu'un utilisateur touche.
    await page.goto('/onboarding-showcase.html');
    await expect(page.getByRole('button').first()).toBeVisible();
    // Trois cibles seulement : créer, importer une sauvegarde, restaurer.
    await verifier(page, 'premier lancement', null, 3);
  });

  test('et la garde attrape une cible rétrécie sans confondre un lien en ligne', async ({
    page,
  }) => {
    // 🔒 Sans ce test, la garde deviendrait muette sans que personne le voie.
    // On lui soumet donc les deux cas d'un coup, sur la même page :
    //   — un bouton hors flux de texte, trop petit : elle DOIT le désigner ;
    //   — un lien de la même taille pris dans une phrase : elle NE DOIT PAS,
    //     sinon l'exception *inline* serait décorative.
    // Sans le second, il suffirait de supprimer l'exception pour que le
    // premier passe encore, et personne ne saurait qu'elle a disparu.
    await ouvrirShowcase(page);
    await verifier(page, 'état initial');

    await page.evaluate(() => {
      const bac = document.createElement('div');
      bac.style.cssText =
        'position:fixed;inset-inline-start:0;bottom:0;z-index:9999;background:#fff;color:#000;font-size:12px;padding:0';
      bac.innerHTML =
        '<p style="margin:0">Du texte porteur <a href="#ancre-de-test">un lien</a> et la suite.</p>' +
        '<button type="button" style="display:block;height:12px;width:12px;padding:0">x</button>';
      document.body.append(bac);
    });

    const { manquements } = await relever(page);
    expect(manquements, 'la garde ne voit plus rien rétrécir').toHaveLength(1);
    expect(manquements[0].designation).toContain('button');
    expect(manquements[0].hauteur).toBeLessThan(MINIMUM);
  });
});
