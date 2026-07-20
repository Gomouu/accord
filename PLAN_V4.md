# PLAN V4 — Le grand plan d'amélioration d'Accord

> État de départ : **v3.5.0**. ~72 500 lignes Rust (9 crates) + ~46 000 lignes
> front (React/TS/zustand), parité Discord quasi complète, 100 fichiers de tests
> vitest + ~380 tests Rust. La v4 n'est **pas** une course aux fonctionnalités :
> c'est un saut de **qualité, de robustesse, de finition et de différenciation**.
>
> Objectif produit v4 : « la messagerie P2P chiffrée qui donne autant envie
> qu'un produit financé, sans jamais sacrifier la vie privée ni la fiabilité ».
>
> Découpage en 4 lots (~1/4 chacun). **Lot D est confié à l'autre Claude Code**
> (backend/infra, isolable, zéro conflit avec le travail front). Lots A/B/C =
> moi. Chaque point est cochable, la plupart référencent un fichier réel.
>
> Convention : `🟢` prêt / facile · `🟠` gros chantier · `🔴` risque de refonte.
> Chaque lot se termine par sa **porte de sortie** (gate) obligatoire.

---

## LOT A — Design, identité visuelle & motion  *(moi)*

### A.1 Système de design & tokens
- [ ] **A1** Auditer les 10 231 lignes de CSS (`app/src/styles/*.css` : global, chat-polish, liquid-glass, identity-refresh, profile-*, theme-scenes, figurative-themes) et cartographier les doublons de valeurs.
- [ ] **A2** Consolider tous les tokens dans un unique `styles/tokens.css` (couleurs, espacements, rayons, ombres, durées, easings) — plus aucune valeur en dur ailleurs.
- [ ] **A3** Définir une **échelle d'espacement** cohérente (4/8/12/16/24/32/48/64) et remplacer les paddings/margins arbitraires.
- [ ] **A4** Définir une **échelle typographique** fluide (`clamp()`) : caption/body/title/display, ratio constant.
- [ ] **A5** Définir une **échelle de rayons** (sm/md/lg/pill) et supprimer les rayons ponctuels incohérents.
- [ ] **A6** Définir un **système d'élévation** (5 niveaux d'ombre) mappé aux surfaces (rail < sidebar < chat < modal < popover).
- [ ] **A7** Créer un fichier de **primitives de surface** réutilisables (Surface, Card, Panel) plutôt que des `div` stylées ad hoc.
- [ ] **A8** Documenter le design system dans `docs/DESIGN_SYSTEM.md` (tokens, do/don't, captures).
- [ ] **A9** Ajouter une page interne `demo/DesignSystem.tsx` qui rend tous les tokens et composants (référence visuelle vivante).

### A.2 Typographie & iconographie
- [ ] **A10** Choisir une **vraie paire de polices** (display + texte) auto-hébergée, sous-ensemblée, `font-display: swap`, préchargée (règles perf : max 2 familles).
- [ ] **A11** Corriger l'**interlignage** et le `letter-spacing` des messages pour la lisibilité longue durée.
- [x] **A12** Chiffres tabulaires pour horodatages, compteurs, tailles de fichiers (`font-variant-numeric`).
- [ ] **A13** Auditer le **jeu d'icônes** : trait, grille et taille homogènes ; migrer vers un set unique cohérent.
- [ ] **A14** Optimiser les SVG d'icônes (viewBox, currentColor, pas de fill en dur) pour l'héritage de thème.
- [ ] **A15** Rafraîchir `docs/accord-logo.svg` / `accord-icon.svg` et dériver toutes les tailles d'icône d'app depuis une source unique.

### A.3 Thèmes (parité contraste + finition)
- [ ] **A16** Auditer **chaque thème** (`customTheme.ts`, figurative-themes, theme-scenes) au contraste WCAG AA sur texte, texte atténué et horodatages.
- [ ] **A17** Corriger les thèmes qui échouent l'AA (texte muté trop clair, mentions illisibles).
- [ ] **A18** Palette de **coloration syntaxique** dédiée par thème clair/sombre (blocs de code lisibles partout).
- [x] **A19** Styliser les **barres de défilement** par thème (au lieu du natif incohérent).
- [x] **A20** Couleur de **sélection de texte** cohérente par thème.
- [ ] **A21** Anneaux de **focus-visible** unifiés et visibles sur chaque thème.
- [ ] **A22** Uniformiser les **pastilles** (mention, rôle, salon, `@everyone`) : forme, contraste, hover.
- [x] **A23** Export/import de thème utilisateur soigné (fichier `.accordtheme` lisible) — voir aussi C.
- [ ] **A24** Aperçu miniature de thème dans le sélecteur (vignette de conversation réelle, pas juste des pastilles).
- [ ] **A25** Transition douce lors du **changement de thème** (pas de flash brutal).

### A.4 Surfaces de conversation
- [x] **A26** **Regroupement des messages** consécutifs du même auteur (une seule tête d'avatar/pseudo, horodatage au survol) — finition type Discord.
- [ ] **A27** Séparateurs de **date collants** (« Aujourd'hui », « Hier », date) élégants dans `MessageList`.
- [ ] **A28** Soigner le séparateur « **nouveaux messages** » existant (couleur, animation d'apparition).
- [ ] **A29** États **hover/focus/active** designés sur chaque ligne de message (barre d'actions flottante propre).
- [x] **A30** Barre de **réactions** repensée : compteur, tooltip « qui a réagi », bouton ajouter discret.
- [ ] **A31** Rendu des **citations/réponses** (MessageQuote) : ligne de rappel, avatar mini, clic → saut.
- [ ] **A32** Rendu des **embeds/liens** (InviteEmbed, cartes fichier) unifié sous un composant Card commun.
- [ ] **A33** Galerie d'**images multiples** (grille adaptative) + lightbox soignée (zoom, navigation, échap).
- [ ] **A34** Lecteurs **audio/vidéo/message vocal** inline harmonisés (mêmes contrôles, même style).
- [ ] **A35** Cartes de **sondage** (PollCard) et **sticker** (StickerImage) alignées sur le design system.
- [ ] **A36** **Indicateur de frappe** (TypingIndicator) animé proprement (points, pseudos).
- [ ] **A37** **Accusés/états d'envoi** (envoi → remis → lu, échec + réessayer) visuellement clairs et discrets.

### A.5 Navigation & chrome
- [ ] **A38** Refonte visuelle du **rail de serveurs** (ServerRail) : pastilles, dossiers, séparateurs, indicateur de sélection animé.
- [ ] **A39** **Sidebar** (salons/DM) : hiérarchie catégories, salons non lus en gras, badges alignés.
- [ ] **A40** **En-tête de salon** : sujet, membres, boutons d'action, recherche — mise en page soignée.
- [ ] **A41** **UserPanel** (bas de sidebar) : avatar, statut, boutons micro/casque/réglages, états designés.
- [ ] **A42** **Titlebar personnalisée** cohérente macOS/Windows/Linux (drag zone, feux macOS, boutons Windows).
- [ ] **A43** Modes de **densité** (compact/cosy) réellement distincts et testés visuellement.
- [ ] **A44** **ResizeHandle** des panneaux : poignée visible, curseur, feedback.

### A.6 Motion system
- [ ] **A45** Tokens de **durée/easing** (`--duration-fast/normal`, `--ease-out-expo`) et bannir les transitions inline arbitraires.
- [ ] **A46** Animation d'**entrée des messages** (fade/slide léger, uniquement compositor : transform/opacity).
- [ ] **A47** **Ressort d'ouverture des modales** (Modals) + fondu de l'overlay, fermeture symétrique.
- [ ] **A48** Transitions de **changement de vue** (serveur↔DM↔réglages) fluides.
- [ ] **A49** Micro-animations **hover** (boutons, réactions, avatars) subtiles et cohérentes.
- [ ] **A50** **Anneaux de parole** vocaux (VoiceSection) animés en douceur, sans jank.
- [ ] **A51** Respect **strict** de `prefers-reduced-motion` sur TOUTES les animations (audit global, pas seulement MessageList).
- [ ] **A52** Retirer les `will-change` résiduels après animation ; n'animer que transform/opacity/clip-path.

### A.7 États vides, chargement, feedback
- [x] **A53** **États vides** designés : aucun ami, aucun DM, aucun serveur, aucun résultat de recherche, boîte de mentions vide.
- [x] **A54** **Squelettes de chargement** : historique de messages, liste de membres, avatars, liste de serveurs.
- [x] **A55** **Toasts** (Toasts.tsx) repensés : succès/erreur/info, action inline, empilement, auto-dismiss.
- [ ] **A56** **Bannière de mise à jour** (UpdateBanner) soignée + notes de version rendues (déjà Markdown) mises en forme.
- [x] **A57** **Bannière hors-ligne / reconnexion** claire (état réseau visible sans ouvrir NetworkPanel).
- [ ] **A58** **Écran d'onboarding** (Onboarding) : illustrations, progression, moments de confiance (« aucun compte, aucune donnée envoyée »).
- [ ] **A59** **Modal de réglages** : IA revue, en-têtes de section, recherche de réglage (voir B), cohérence des contrôles (controls.tsx).
- [ ] **A60** **Popover/carte de profil** (ProfilePopover, ProfileBanner, ProfileCardPreview) : mise en page premium, bannière, badges, bio.

### A.8 Vitrine web (website/)
- [ ] **A61** Aligner le site sur la **nouvelle identité visuelle** (mêmes polices, tokens, logo).
- [ ] **A62** Nouvelles **captures d'écran** haute résolution des vues clés (les deux thèmes), dans `docs/screenshots`.
- [ ] **A63** Vidéo/GIF de **démo** courte (hero) montrant une vraie conversation.
- [ ] **A64** Vérifier que `site.js` / `releases.js` restent robustes (fallback si l'API GitHub échoue) et rapides (perf CWV).
- [ ] **A65** Page **fonctionnalités** détaillée (chiffrement, P2P, zéro compte, thèmes, vocal) avec visuels.
- [ ] **A66** Audit **CWV** du site (LCP < 2,5 s, CLS < 0,1) + Lighthouse ≥ 95.

### A.9 Finitions transverses
- [ ] **A67** **EmojiPicker** / **StickerImage** / **SoundboardButton** : grilles, catégories, récents, recherche visuellement soignés.
- [ ] **A68** **ContextMenu** : style, séparateurs, icônes, sous-menus, danger rouge cohérents.
- [ ] **A69** **QuickSwitcher** (Ctrl+K) : design de palette (voir B pour l'extension fonctionnelle).
- [ ] **A70** **Avatars** : formes (cercle/carré), anneau de statut, repli initiales colorées déterministes.
- [x] **A71** **PresenceDot** : couleurs de statut distinguables aussi pour daltoniens (forme + couleur).
- [ ] **A72** **Décorations de profil** (decorations*.tsx, 1900+ lignes) : audit qualité/poids, lazy-load, cohérence.
- [ ] **A73** Vérifier le **rendu WKWebView packagé** de tous les nouveaux styles (pas seulement le dev server).

### A.10 Rendu du texte riche
- [ ] **A74** Style des **titres Markdown** (#, ##, ###) dans les messages (hiérarchie sans casser le flux).
- [ ] **A75** Style des **listes** (à puces / numérotées) et **citations** (`>`) avec barre de rappel.
- [ ] **A76** Rendu des **tableaux GFM** (bordures, zébrage, débordement scrollable).
- [x] **A77** **Blocs de code** : fond, gouttière, langue affichée, bouton copier designé.
- [x] **A78** **Code inline** : fond, rayon, contraste par thème.
- [x] **A79** **Spoilers** `||…||` : flou + animation de révélation.
- [ ] **A80** **Liens** : couleur, soulignement au survol, indicateur de lien externe.
- [ ] **A81** **Émojis personnalisés** (CustomEmoji) : taille cohérente inline vs jumbo (message émoji seul).
- [ ] **A82** Surlignage de **mention** (fond `@moi`) distinct et lisible sur tous les thèmes.

### A.11 Badges, indicateurs & panneaux
- [x] **A83** **UnreadBadge** : formes, `99+`, pastille vs compteur, position stable.
- [ ] **A84** **Badge de mention @** rouge distinct du non-lu, aligné (Sidebar/ServerRail).
- [ ] **A85** **MentionInbox** : liste des mentions designée, saut au message, marquage lu.
- [ ] **A86** **Volet des épinglés** (DM + serveur) : cartes, saut, désépinglage.
- [ ] **A87** **ThreadPanel** : en-tête, fil, retour au message racine soignés.
- [ ] **A88** **Bandeau vocal connecté** (barre verte) : latence, participants, boutons.
- [ ] **A89** **SearchBar** + panneau de résultats : puces de filtres, aperçu, surlignage du terme.
- [ ] **A90** **Autocomplétions** (MentionAutocomplete, EmojiAutocomplete, slash) : même popup, navigation clavier, aperçu.
- [ ] **A91** **Progression d'upload** (anneau/barre) sur les pièces jointes et avatars.
- [ ] **A92** **Cartes fichier** : icône par type, taille, bouton télécharger, état.

### A.12 Fenêtre, dock & accessibilité visuelle
- [ ] **A93** **Icône de dock/taskbar** avec badge de non-lus (déjà côté macOS) : cohérence visuelle multi-plateforme.
- [ ] **A94** **Splash/écran de chargement** au démarrage (le temps du déverrouillage du coffre).
- [ ] **A95** **Coins de fenêtre arrondis** + ombre native cohérente par OS.
- [ ] **A96** **Vibrancy/blur** natif macOS derrière le liquid-glass (opt-in, dégradation propre).
- [ ] **A97** **Mode transparence réduite** (accessibilité) qui neutralise le glass.
- [ ] **A98** **Thème très contrasté** intégré (au-delà de l'AA, pour basse vision).
- [ ] **A99** **Puces `kbd`** (raccourcis) et chips `NEW`/`BÊTA` cohérentes.
- [ ] **A100** **Aperçu de reculement au drag** (réordonnancement salons/serveurs) élégant.
- [ ] **A101** **ServerRail folders** : animation de repli/dépli, aperçu des icônes empilées.
- [ ] **A102** **Cropper d'avatar/bannière** (AvatarCropper) : poignées, grille, aperçu live soignés.
- [ ] **A103** **Émojis natifs vs personnalisés** : stratégie de rendu homogène (pas de saut de baseline).
- [ ] **A104** **Sélecteur de couleur de rôle** (ServerRolesTab) : palette + saisie hex accessible.
- [ ] **A105** **Onglets de réglages serveur** (audit/automod/bans/…) : navigation et en-têtes homogènes.

**Porte de sortie Lot A** : capture des 24+ thèmes en clair/sombre aux points de rupture 375/768/1024/1440 ; zéro régression de contraste AA ; `prettier --check` vert ; aucune valeur de design en dur hors `tokens.css`.

---

## LOT B — UX, accessibilité, i18n & qualité frontend  *(moi)*

### B.1 Palette de commandes & clavier
- [ ] **B1** Étendre QuickSwitcher en **palette de commandes** complète (Ctrl+K) : aller à un salon/DM/ami **et** exécuter des actions (créer serveur, changer statut, ouvrir réglages, basculer thème, muet…).
- [ ] **B2** Recherche floue + résultats récents + sections dans la palette.
- [ ] **B3** **Carte des raccourcis** clavier (ShortcutsTab) exhaustive et à jour ; raccourcis configurables au-delà du PTT.
- [ ] **B4** Navigation clavier complète : `Alt+↑/↓` entre salons, `Ctrl+↑/↓` entre serveurs, `Échap` ferme la couche du dessus.
- [ ] **B5** **Pièges de focus** dans toutes les modales (Modals, SettingsModal, ServerSettingsModal, cropper) + restauration du focus à la fermeture.
- [ ] **B6** `Échap` cohérent partout (ferme popover → modal → recherche, un niveau à la fois).
- [ ] **B7** Focus visible géré au **changement de vue** (annonce + focus sur le compositeur ou le titre).

### B.2 Accessibilité (WCAG 2.2 AA)
- [ ] **B8** Audit **ARIA** complet : rôles (list/listitem/dialog/menu), `aria-label` sur boutons-icônes, `aria-expanded/pressed`.
- [x] **B9** **Régions live** : nouveaux messages, toasts, indicateur de frappe, changements de statut annoncés au lecteur d'écran.
- [ ] **B10** **Contraste** : audit AA sur tous les textes/contrôles (couvre A16 côté couleur, ici côté composant).
- [ ] **B11** Contrôles personnalisés (sliders de volume, toggles, sélecteurs de couleur, cropper) accessibles clavier + rôle correct.
- [ ] **B12** **Tooltips** accessibles (déclenchés au focus, `aria-describedby`, pas seulement hover).
- [ ] **B13** Ordre de tabulation logique dans chaque vue ; pas de piège hors modale.
- [ ] **B14** Tests d'accessibilité automatisés (axe-core dans vitest/jsdom ou Playwright) sur les vues clés.
- [ ] **B15** Vérifier la navigation **lecteur d'écran** réelle (VoiceOver macOS) sur l'onboarding et l'envoi de message.

### B.3 Internationalisation
- [ ] **B16** Extraire **toutes** les chaînes en dur restantes vers `i18n/{fr,en}.ts` (grep des littéraux dans les composants).
- [ ] **B17** **Pluralisation** correcte (0/1/n) et **interpolation** partout (pas de concaténation).
- [ ] **B18** **Formatage localisé** des dates/heures/tailles/nombres (`Intl`), pas de format codé en dur.
- [ ] **B19** Ajouter au moins **2 langues** supplémentaires (es, de) avec la même couverture que FR/EN.
- [ ] **B20** Préparer la **RTL** (logical properties CSS : `margin-inline`, `padding-inline`) même sans langue RTL livrée.
- [x] **B21** Test i18n : échouer la CI si une clé existe dans `fr` mais pas dans une autre langue (parité de clés).

### B.4 Flux produit (UX)
- [ ] **B22** **Onboarding** : étape de **vérification de la phrase mnémonique** (re-saisie de quelques mots) avant de continuer.
- [ ] **B23** Onboarding : « **inviter ton premier ami** » guidé (QR + code ami) juste après création d'identité.
- [ ] **B24** Flux d'**ajout d'ami** clarifié (code ami vs QR vs lien) avec états d'attente/erreur explicites.
- [ ] **B25** Flux de **création/rejoint de serveur** (JoinServerForm) : validation, aperçu, erreurs claires.
- [ ] **B26** **Cartes d'invitation** en MP (InviteCard) : corriger l'état « expiré » affiché après redémarrage entre acceptation et admission (limitation connue v3.5).
- [ ] **B27** **Deep-link** notification → salon exact + surlignage du message concerné.
- [x] **B28** **Recherche** : puces de filtres (`from:`/`in:`/`has:`/`before:`), recherches récentes, saut au résultat.
- [ ] **B29** **Réglages** : champ de recherche filtrant les onglets/sections.
- [ ] **B30** **Aperçu Markdown** basculable dans le compositeur (MessageInput/MessageEditor).
- [ ] **B31** **Brouillons** : indicateur visuel de brouillon existant par salon/DM (déjà persistés).
- [ ] **B32** **Glisser-déposer** de fichiers (useDeposeFichiers) : zone de dépôt claire, multi-fichiers, aperçus avant envoi.
- [x] **B33** **Statut personnalisé** : éditeur (texte + émoji + expiration) soigné.
- [ ] **B34** **Mute granulaire** (serveur/salon/DM + durée) : UI cohérente et lisible partout.

### B.5 Décomposition & qualité du code front
- [ ] **B34b** Découper **Sidebar.tsx** (1311 l.) en sous-composants (< 400 l. chacun, règle coding-style).
- [ ] **B35** Découper **MessageInput.tsx** (1257 l.) : compositeur, autocomplétions, pièces jointes, barre d'outils.
- [ ] **B36** Découper **Modals.tsx** (961 l.) : une modale par fichier.
- [ ] **B37** Découper **MessageList.tsx** (915 l.) et **UserMenu.tsx** (760 l.), **AccountTab.tsx** (725 l.), **ContextMenu.tsx** (602 l.).
- [ ] **B38** Audit **état** : dériver au lieu de dupliquer l'état serveur ; supprimer les états redondants dans les stores zustand.
- [ ] **B39** Sélecteurs zustand mémoïsés / `shallow` pour éviter les re-rendus (gros stores : groups, dms, session).
- [ ] **B40** **Vraie virtualisation** de `MessageList` (hauteurs dynamiques) au lieu du fenêtrage manuel `debut/fenetre` actuel — perf sur gros historiques.
- [ ] **B41** Virtualiser **MemberList** et listes longues (bannis, membres serveur).
- [ ] **B42** **Code-splitting** des surfaces lourdes (SettingsModal, ServerSettingsModal, cropper, décorations) via `import()` dynamique.
- [ ] **B43** Vérifier le **budget de bundle** (< 300 kb gz app) et retirer le poids mort (decorations-*).
- [ ] **B44** **ErrorBoundary** par vue majeure (chat, serveur, réglages, vocal) avec récupération.
- [ ] **B45** Éliminer les `console.log`/debug résiduels ; logger via un util contrôlé.
- [ ] **B46** Typage : chasser les `any`/`as` non justifiés ; profiter de `noUncheckedIndexedAccess` déjà actif.

### B.6 Tests front
- [ ] **B47** Porter la **couverture** vers ≥ 80 % (règle testing) — cibler les composants non testés (MessageInput, Sidebar, ServerSettings, UserMenu).
- [ ] **B48** Tests d'**interaction** (Testing Library) sur les flux critiques : envoyer, éditer, réagir, répondre, rejoindre serveur.
- [ ] **B49** Tests de **régression visuelle** Playwright aux 4 points de rupture, thèmes clair/sombre (règle web/testing).
- [ ] **B50** **E2E Playwright** des parcours clés (onboarding → ajout ami → DM → création serveur) contre un backend simulé.
- [ ] **B51** Tests de **reduced-motion** et **accessibilité** (axe) intégrés à la CI front.

### B.7 Confort & actions
- [ ] **B52** **Annuler l'envoi** (courte fenêtre) avant remise effective.
- [ ] **B53** **Confirmation des actions destructrices** (supprimer serveur, quitter, vider, bloquer) — modale claire.
- [ ] **B54** **UI optimiste + rollback** visible sur envoi/édition/réaction (règle patterns).
- [ ] **B55** **File d'attente hors-ligne** visible + « réessayer tout ».
- [x] **B56** **Copier** message / lien / identifiant depuis le menu contextuel (contactMenu, messageMenus).
- [ ] **B57** Audit **clic droit partout** (message, salon, serveur, membre, avatar, lien).
- [ ] **B58** Raccourcis d'action rapides : répondre, éditer (↑ déjà), réagir, épingler, supprimer.
- [ ] **B59** **Aller aux non-lus** + « marquer comme lu » / « tout marquer lu » (markServerRead).
- [ ] **B60** **Persistance de la position de défilement** par salon/DM au retour.
- [ ] **B61** **Restaurer la dernière vue** au lancement (navPersistence) + salon actif.
- [x] **B62** **Zoom** de l'interface (Ctrl +/−) accessible et persistant.
- [ ] **B63** **Correcteur orthographique** dans le compositeur (natif WebView).
- [ ] **B64** **Coller une image** → aperçu avant envoi (déjà partiel) fiabilisé.
- [ ] **B65** **Indicateur de limite** (longueur message, taille pièce jointe) avec feedback avant erreur.
- [ ] **B66** **Boutons occupés** : désactivés + spinner sur toute action async.
- [ ] **B67** **Debounce** de la recherche et des autocomplétions.

### B.8 Système & plateforme (côté UI)
- [ ] **B68** UX de **demande de permission** notifications (NotificationsTab) claire, testable.
- [ ] **B69** UX **démarrage auto** (plugin autostart) + **minimiser dans le tray** (tray.rs) expliquée.
- [x] **B70** **Mode Ne pas déranger** basculable rapidement (barre d'état/palette).
- [x] **B71** **Détection de langue OS** au premier lancement (défaut i18n).
- [ ] **B72** **Import/export des réglages** (hors identité) en fichier.
- [ ] **B73** **Temps relatifs vivants** (« il y a 2 min ») qui se mettent à jour sans re-render coûteux.

### B.9 Perf & hygiène front
- [ ] **B74** Audit **nettoyage des effets** (useEffect/listeners/timers) — pas de fuite mémoire ni d'abonnement orphelin.
- [ ] **B75** **Suspense/lazy** sur les vues secondaires ; frontière de chargement propre.
- [ ] **B76** **Budget de perf en CI** (taille de bundle + Lighthouse sur build) qui échoue au dépassement.
- [ ] **B77** **Mémoïsation** ciblée (React.memo/useMemo) sur les listes chaudes après profilage (pas au hasard).
- [ ] **B78** **Réduire les re-rendus** globaux au changement de statut/présence (isolation de contexte).
- [ ] **B79** **Pages demo** (`app/src/demo`) tenues à jour comme références visuelles/perf.
- [ ] **B80** **Journalisation front** contrôlée (niveau, désactivable) — remplace les console.* épars.

**Porte de sortie Lot B** : `tsc --noEmit` + `eslint` + `vitest run` (≥ 80 % couverture) + `prettier --check` verts ; parité de clés i18n ; zéro violation axe critique sur les vues clés ; aucun composant > 800 lignes.

---

## LOT C — Fonctionnalités produit, différenciation & voix/vidéo  *(moi)*

### C.1 Différenciation « privacy-first » (l'avantage d'Accord)
- [ ] **C1** **Tableau de bord vie privée** : montrer exactement ce qui est stocké localement, chiffré, et ce qui transite (rien vers un serveur). Argument de vente unique.
- [ ] **C2** **Carte de connexion** en direct (NetworkPanel enrichi) : direct vs relais, type de NAT, pairs LAN, latence — pédagogie + confiance.
- [ ] **C3** **Assistant de sauvegarde** : rappel périodique, sauvegarde chiffrée `.accordbackup` en un clic, vérification de restauration.
- [ ] **C4** **Vérification d'identité** entre amis (comparaison d'empreinte / mots de sécurité) façon Signal — anti-MITM visible.
- [ ] **C5** **Messages éphémères / minuteur d'auto-suppression** par conversation (local, honoré par le client).
- [ ] **C6** **Coffre local** : verrouillage à l'inactivité + déverrouillage rapide (Touch ID macOS via keychain, opt-in).
- [ ] **C7** Transparence **hors-ligne** : afficher clairement l'état de remise (mailbox 7 j) et « ton ami est hors-ligne, remise différée ».

### C.2 Messagerie avancée
- [x] **C8** **Messages enregistrés / favoris** (bookmark local) avec vue dédiée.
- [ ] **C9** **Messages programmés** (envoi différé local quand le pair est joignable).
- [ ] **C10** **Rappels** sur un message (« me le rappeler dans 3 h »).
- [ ] **C11** **Traduction** locale d'un message (opt-in, sans service tiers — ou clairement signalé si tiers).
- [ ] **C12** **Épingles** unifiées DM + serveur + volet dédié soigné.
- [ ] **C13** **Recherche globale** locale (tous salons/DM) rapide via l'index HMAC, avec aperçu de résultat.
- [ ] **C14** **GIF/stickers** : bibliothèque **locale** (pas de Tenor/traçage) + import de GIF perso + stickers animés.
- [ ] **C15** **Aperçu de liens** (unfurl) **opt-in** avec avertissement de fuite d'IP, fetch encadré (voir Lot D pour le proxy éventuel).
- [ ] **C16** **Effets/formatage** riches supplémentaires (spoilers `||…||`, sous-titres, sauts de ligne intelligents).
- [ ] **C17** **Réactions** : réactions rapides récentes (déjà amorcé), super-réactions/émojis animés.

### C.3 Serveurs & communauté
- [ ] **C18** **Templates de serveur** (créer depuis un modèle, exporter son serveur en modèle).
- [ ] **C19** **Événements planifiés** (EventsModal) : RSVP, rappels, fuseau, notification à l'heure.
- [ ] **C20** **Salons forum** (ForumView) : tags, tri, résolution, épingle de post.
- [ ] **C21** **Onboarding/règles de serveur** (screening) avant accès.
- [ ] **C22** **AutoMod** (automod.ts) : règles enrichies (anti-spam, anti-lien, seuils) côté client, transparence.
- [ ] **C23** **Statistiques de serveur** locales (activité, membres actifs) sans télémétrie.
- [ ] **C24** **Profils par serveur** (surnom, avatar, bannière) finis et cohérents.
- [ ] **C25** **Rôles** : réordonnancement drag & drop, permissions par salon (overrides) exposées dans l'UI.
- [ ] **C26** **Vue audit** (ServerAuditTab) lisible et filtrable (l'op-log signé est déjà un journal complet).

### C.4 Amis & profils
- [ ] **C27** **Groupes DM multi-personnes** (au-delà du 1:1) — chantier protocole/UI.
- [ ] **C28** **Statuts riches** complets (inactif/DND/invisible + texte + émoji) exposés partout.
- [ ] **C29** **Badges & décorations** de profil : galerie, sens, déblocage local (cosmétique).
- [ ] **C30** **Pronoms / connexions / liens** de profil (champs locaux, partagés aux amis).
- [ ] **C31** **Notes privées** de profil soignées (déjà stockage local).
- [ ] **C32** **Multi-comptes** : sélecteur soigné (AccountPicker), bascule rapide, isolation des vaults.

### C.5 Voix (accord-voice)
- [ ] **C33** **Suppression de bruit rnnoise** (en plus de l'AEC déjà présent, `aec.rs`) — opt-in, réglable.
- [ ] **C34** **AGC** (contrôle de gain automatique) propre au-dessus de `gain.rs`.
- [ ] **C35** **Sensibilité VAD** réglable dans l'UI (VoiceTab) + vumètre en direct (mic_test existe).
- [ ] **C36** **Priority speaker** (déjà amorcé) + **push-to-talk** multi-touches, délai de relâche.
- [ ] **C37** **Soundboard** : finition (catégories, volume, favoris), respect présence vocale.
- [ ] **C38** **Indicateurs de qualité d'appel** (perte, gigue, débit) visibles côté utilisateur.
- [ ] **C39** **Volumes par participant + master** exposés et persistés.
- [ ] **C40** **Mode streamer** (masquer infos sensibles, notifications discrètes).
- [ ] **C41** **Vérifier l'incohérence** UI/backend : plusieurs salons vocaux créés mais session mappée `channel_id==group_id` (une seule pièce vocale réelle ?) — à confirmer/corriger.

### C.6 Vidéo & partage d'écran  *(gros chantier, 🟠)*
- [ ] **C42** **Appel vidéo/caméra 1-à-1** (le wire réserve `media_type`) : capture, encodage, rendu, bascule audio↔vidéo.
- [ ] **C43** **Partage d'écran** (+ audio système) en 1-à-1 puis petit groupe.
- [ ] **C44** Négociation de **débit/résolution** adaptative selon le lien (réutiliser bitrate.rs/loss.rs).
- [ ] **C45** UI d'appel : grille des flux, épinglage, plein écran, contrôles designés (cohérent Lot A).
- [ ] **C46** **PiP / mini-fenêtre** d'appel persistante en changeant de vue.

### C.7 Appels (confort & historique)
- [ ] **C47** **Sonnerie personnalisable** (ringtone.ts) + volume dédié.
- [ ] **C48** **Journal d'appels** (manqués, durée, participants) local.
- [ ] **C49** **Test d'écho / boucle micro** dans les réglages (au-delà du mic_test).
- [ ] **C50** **Calibrage auto** de la sensibilité d'entrée.
- [ ] **C51** **IncomingCall** : plein écran, accepter/refuser, aperçu de l'appelant soignés.

### C.8 Serveurs (gestion fine)
- [ ] **C52** **Gestion des invitations** (usages/expiration/révocation) exposée dans l'UI (ops déjà présentes).
- [ ] **C53** **Recherche/filtre de membres** (ServerMembersTab) + attribution de rôle en masse.
- [ ] **C54** **Message de bienvenue** + niveau de notification par défaut du serveur.
- [ ] **C55** **Gestion des stickers de serveur** (ServerStickersTab) finalisée (ajout/suppression/aperçu).
- [ ] **C56** **Gestion des sons** (ServerSoundsTab) : import, volume, catégories.
- [ ] **C57** **Émojis inter-serveurs** (utiliser ses émojis partout) — équivalent gratuit du Nitro.
- [ ] **C58** **Bans/timeouts** (ServerBansTab) : durée, motif, levée, historique lisible.

### C.9 Personnalisation & données
- [ ] **C59** **Partage de thème** utilisateur via fichier/lien `.accordtheme` (import/export soigné).
- [ ] **C60** **Sauvegarde automatique planifiée** (rappel + `.accordbackup` chiffré).
- [ ] **C61** **Restauration sélective** (choisir quoi restaurer depuis une sauvegarde).
- [ ] **C62** **Déblocage de décorations/badges** de profil (cosmétique local).
- [x] **C63** **Agrégation des non-lus par dossier** (folders) dans le rail.
- [ ] **C64** **Mettre en veille une conversation** (snooze) + « à lire plus tard ».
- [ ] **C65** **Sondages avancés** (PollCard) : choix multiples, anonyme, expiration, résultats en direct.

### C.10 Voix (qualité fine)
- [ ] **C66** **FEC / dissimulation de perte** (loss.rs, PLC) réglés sur pertes réelles.
- [ ] **C67** **Gigue adaptative** (jitter.rs) : cible p95 exposée et ajustable.
- [ ] **C68** **Échelle de débit** (bitrate.rs) pilotée par la qualité du lien, visible.
- [ ] **C69** **Portes de bruit** visuelles (seuil réglable) dans VoiceTab.
- [ ] **C70** **Indicateur « qui parle »** fiable même sous perte (anneaux + liste).

**Porte de sortie Lot C** : chaque nouvelle surface réseau **passée en revue adverse** (droits, forge, amplification), tests Rust + vitest verts, aucune fuite de vie privée non signalée (unfurl/GIF), gate CI complet vert.

---

## LOT D — Robustesse réseau P2P, sécurité, perf backend, observabilité, tests/CI & distribution  *(confié à l'autre Claude Code)*

> **Périmètre = 1/4 du plan, 100 % backend/infra.** Fichiers Rust (`crates/*`,
> `app/src-tauri/src`), CI (`.github/workflows`), `fuzz/`, scripts de release.
> **Ne touche pas** aux fichiers front `app/src/**` ni aux fichiers de suivi
> partagés (`ROADMAP.md`, `PROGRESS.md`, `DECISIONS.md`, `CHANGELOG.md`,
> `PLAN_V4.md`) — c'est le propriétaire du dépôt qui les met à jour.

### D.1 Traversée NAT & connectivité *(déjà implémentée — AUDITER & DURCIR, ne pas reconstruire)*
> ⚠️ État réel au 20/07/2026 : le **tunnel client de relais**, la **sélection de
> relais** (`node/relay.rs` : `select_relays`, `select_home_relays`,
> `prioritize_reachable`, `RELAY_TRY_MAX`), le **repli** hole-punch→relais
> (`PUNCH_FALLBACK_MS`) et la **classification NAT** (`classify_nat`, exposée
> `network.status.nat_kind`) **existent et sont testés** (`relay_tunnel_e2e`).
> Les notes « RESTE : tunnel client » de `PROGRESS.md` (9 juillet) sont périmées.
> Le travail ci-dessous est de **prouver, durcir, et instrumenter**, pas de recoder.
- [ ] **D1** **Prouver la joignabilité** sous conditions adverses : e2e SimNet couvrant NAT symétrique des DEUX côtés, churn de relais, rejet de relais, expiration de circuit, glare — au-delà de `relay_tunnel_e2e` actuel.
- [ ] **D2** **Durcir la sélection/repli** : timing de bascule (`PUNCH_FALLBACK_MS`), santé de relais mesurée (pas seulement « vérifié »), éviction d'un relais défaillant en cours de session, re-tentative sur un autre.
- [x] **D3** **Instrumenter l'orchestration** direct LAN → punch → relais : compteurs de succès/échec par étape, exposés (voir D.5) — c'est ce qui manque vraiment, pas la logique.
- [x] **D4** **Étendre l'exposition NAT/relais par pair** : `nat_kind` global existe ; ajouter par pair « direct vs relayé », relais utilisé, latence — pour la carte de connexion de l'UI (contrat D.5).
- [ ] **D5** Audit **IPv6** de bout en bout (candidats, punch, relais).
- [ ] **D6** **UPnP-IGD / NAT-PMP** : renouvellement du mapping, dégradation propre, re-tentative sur changement de réseau.
- [ ] **D7** **Détection de changement de réseau** (Wi-Fi↔4G, réveil de veille) → re-résolution de présence + reconnexion rapide.
- [ ] **D8** **Découverte du MTU** / bornes de fragmentation (frag.rs) sous chemins réels, tests de robustesse.
- [ ] **D9** **Backoff de reconnexion** ajusté (jitter, plafond) et machine à états de connexion documentée.
- [ ] **D10** **mDNS** : étendre la supervision/relance (déjà faite, `discovery.rs`) au diagnostic et aux autres démons.

### D.2 DHT & remise hors-ligne
- [ ] **D11** **Fiabilité des mailboxes** DHT : TTL configurable, dédup, accusés de remise, purge.
- [ ] **D12** **Backoff de l'outbox** (maintenance.rs) : re-tentatives bornées, priorités, métriques.
- [ ] **D13** **Diversité de la table de routage** (/24, /48) et **anti-Sybil** : re-audit sous churn.
- [ ] **D14** **Republication** d'identité/présence : cadence adaptative selon la joignabilité.
- [ ] **D15** **Résistance à l'analyse de trafic** : padding optionnel, lissage temporel des annonces (étude + prototype).
- [ ] **D16** Tests DHT **60+ nœuds** sous perte/latence/churn (le SimNet existe) — scénarios de partition.

### D.3 Sécurité & vie privée (crypto/hardening)
- [x] **D17** **Rafraîchir le THREAT-MODEL.md** et lancer une revue adverse des surfaces v3.4/3.5 (sauvegarde chiffrée `archive.rs`, invitations MP, backup import).
- [x] **D18** **Élargir le fuzzing** au-delà des 3 cibles (`core_msg`, `group_op_body`, `proto_decode`) : ajouter handshake, session AEAD, décodage d'état de groupe, records DHT, manifests fichiers, **archive de sauvegarde**.
- [x] **D19** **Fuzzing continu en CI** (temps borné par PR + campagne de nuit) avec corpus persistant.
- [ ] **D20** Revue **forward secrecy** : rotation d'epochs, re-keying, gestion des nonces directionnels, fenêtre anti-rejeu.
- [ ] **D21** Audit **temps constant** (subtle) sur toutes les comparaisons de secrets/jetons.
- [ ] **D22** Audit **zeroization** (zeroize) des secrets en mémoire (clés d'epoch, seed, phrases de passe de sauvegarde).
- [x] **D23** **Chasse aux panics** en chemin de production : bannir `unwrap`/`expect`/`panic!` hors tests ; lint clippy dédié (dans la lignée de la régression `debug_assert`).
- [ ] **D24** **Minimisation des métadonnées** : réduire ce que révèlent les records DHT et les entêtes.
- [ ] **D25** **Chaîne d'approvisionnement** : `cargo deny`/`audit` déjà en CI → ajouter génération **SBOM** et politique de licences stricte (deny.toml).
- [ ] **D26** **Rotation de clés** (identité compromise) : chemin de migration documenté et testé.

### D.4 Performance backend
- [ ] **D27** **Perf SQLCipher** (accord-core/src/db) : index manquants, requêtes N+1 (messages.rs 1297 l.), WAL, VACUUM planifié.
- [ ] **D28** **Profilage mémoire** : borner tous les buffers, chasser les copies inutiles, audit des allocations chaudes.
- [ ] **D29** **Temps de démarrage** du nœud (init vault + DHT + réseau) mesuré et optimisé.
- [ ] **D30** **CPU du moteur vocal** (`voice/engine.rs` 2281 l.) : profiler la boucle 20 ms, jitter buffer, mix (mix.rs).
- [ ] **D31** **Passage à l'échelle des gros groupes** : op-log CRDT (`group/state.rs` 4924 l.), listes wire plafonnées à 4096 — mesurer, borner, paginer.
- [ ] **D32** **Débit de transfert de fichiers** (Merkle + Reed-Solomon 10+4, multi-sources) : fenêtre, reprise bitmap, mesures.
- [x] **D33** **Bench criterion** pour crypto (handshake, AEAD), codec voix, décodage proto, requêtes DB — anti-régression perf.

### D.5 Observabilité & diagnostic
- [ ] **D34** **Tracing structuré** : spans cohérents, niveaux, champs (rpc_id, peer, session) — exploiter `ACCORD_LOG_FILE` déjà en place.
- [x] **D35** **Compteurs locaux** (succès de connexion, usage relais, latence de remise, échecs de punch) exposés via une méthode API `diagnostics.*`.
- [x] **D36** **Panneau de diagnostic** (données backend seulement ; l'UI est côté Lot B/C) : auto-test réseau, joignabilité, type de NAT, test de relais.
- [ ] **D37** **Journal de crash local** (aucune télémétrie) : capture panics + contexte, consultable par l'utilisateur.
- [ ] **D38** **Endpoint de santé** interne du nœud (état DHT, sessions actives, mappings NAT).

### D.6 Tests & CI
- [ ] **D39** **Corriger la flakiness e2e** : les suites réseau (`calls_e2e`, `reconnexion_e2e`, `maintenance_e2e`, `tcp_link_e2e`, `profil_reboot_e2e`) échouent par **starvation** sous parallélisme complet → sérialiser ou isoler (groupe de tests dédié `--test-threads`).
- [x] **D40** **Property tests** (proptest) sur les codecs (`accord-proto`), le CRDT de groupe (repli déterministe), les codes amis.
- [ ] **D41** **Couverture Rust** mesurée (llvm-cov/tarpaulin) et publiée ; cibler les modules critiques.
- [ ] **D42** **Étendre les tests en profil release** (la classe de bug `debug_assert` qui avale du code) : couvrir plus de chemins hermétiques SimNet en release.
- [ ] **D43** **Matrice CI** vérifiée sur les 3 OS (macOS arm64, Ubuntu 22.04, Windows) — pas seulement le build, aussi les tests pertinents.
- [ ] **D44** **Chaos réseau déterministe** étendu (SimNet) : partitions, réordonnancement, duplication, horloges décalées.
- [ ] **D45** **Garde-fous CI** existants (clippy `-D warnings`, `-D clippy::debug_assert_with_mut_call`, tests transport release) — vérifier qu'ils restent verts et en ajouter (voir D23).

### D.7 Distribution & mises à jour
- [ ] **D46** **Signature/notarisation** : macOS (notarize + staple), Windows (Authenticode) — supprimer les avertissements d'installation.
- [ ] **D47** **UX de mise à jour côté backend** : robustesse de `latest.json`, vérification de signature (clé updater), reprise sur échec.
- [ ] **D48** **Mises à jour delta** (si supporté par le plugin) pour réduire le poids des updates.
- [ ] **D49** **Rollback** propre si une mise à jour échoue au démarrage.
- [ ] **D50** **Builds reproductibles** (verrouiller toolchain, dépendances) et documentation `DISTRIBUTION.md`.
- [ ] **D51** **Durcir l'automatisation de release** (`.github/workflows/release.yml`) : draft → publish, vérif des 9 clés de plateforme dans `latest.json`, garde anti-release cassée.

### D.8 Liens de transport & relais
- [ ] **D52** **Repli TCP** (tcp.rs) robuste quand l'UDP est bloqué (réseaux d'entreprise) : bascule et tests.
- [ ] **D53** **Relais auto-hébergeable pour ses amis** (différenciateur fort) : un ami peut faire tourner un relais de confiance, sélection prioritaire.
- [ ] **D54** **Plafonds de bande passante / équité** du relais (relay.rs) : anti-abus, quotas par session.
- [ ] **D55** Audit **rate limiting** (ratelimit.rs) : token buckets, seuils, protection DoS revalidés.
- [ ] **D56** **Bornes d'horloge / Lamport** (clock.rs) : anti-forge de `sent_ms`, dérive tolérée documentée.
- [ ] **D57** **Re-keying des sessions longues** : rotation d'epoch sur durée/volume, transparente.

### D.9 Stockage & intégrité
- [ ] **D58** **Framework de migration SQL** robuste + numéro de schéma vérifié au démarrage.
- [ ] **D59** **Récupération après corruption** de la base (SQLCipher) : détection, quarantaine, reconstruction partielle.
- [ ] **D60** **Cohérence de la sauvegarde** (`backup.rs`/`archive.rs`) : snapshot atomique, vérification à l'export.
- [ ] **D61** **Paramètres Argon2** du coffre : équilibrer sécurité/temps de déverrouillage, mesurés par plateforme.
- [ ] **D62** **Files bornées** partout (voix, réseau, outbox) : audit anti-explosion mémoire.
- [ ] **D63** **Arrêt propre** sur panic/kill : aucune perte de données, flush de la base.

### D.10 API & contrats
- [ ] **D64** **Validation d'entrée** systématique à la frontière des ~40 méthodes RPC (accord-api/accord-node/service).
- [ ] **D65** Re-vérifier les défenses locales : **comparaison temps constant** du jeton, **timeout WS**, **contrôle d'Origin**, limite de connexions.
- [x] **D66** **Contrat de diagnostic** (`diagnostics.*`, `network.status` enrichi, événements) documenté dans `API.md` pour l'UI (Lots B/C).
- [ ] **D67** **Versionner le contrat wire** et garantir la compat 3.x/4.x (bandes de kinds, champs optionnels).

### D.11 Tests & fuzz (extension)
- [ ] **D68** **Matrice e2e N-nœuds** (2/3/N) sur SimNet : amitié, groupe, voix, fichiers, relais.
- [ ] **D69** **Graines déterministes** contrôlées pour le SimNet (repro exacte des flakes).
- [ ] **D70** **Cibles de fuzz** supplémentaires : friendcode, mnemonic, vault, link d'invitation, manifest fichier.
- [ ] **D71** **Étendre la suite transport release** (au-delà des 4 suites actuelles) pour couvrir plus de chemins non évalués en debug.
- [x] **D72** **Corpus de fuzz persistant** committé et réutilisé entre campagnes.

### D.12 CI & release (extension)
- [ ] **D73** **Vitesse CI** : cache Rust/cargo affiné, jobs parallélisés, temps de PR mesuré.
- [ ] **D74** **Rétention d'artefacts** + upload **SBOM** attaché à chaque release.
- [ ] **D75** **Vérif d'automatisation du changelog** (échoue si `[Unreleased]` non consommé au tag).
- [ ] **D76** **Packaging Linux** (AppImage/deb) et **quirks Windows** (chemins, notifications) testés.
- [ ] **D77** **Doc de rotation/sauvegarde des clés de signature** (updater + plateformes) — jamais committées.
- [ ] **D78** **Format d'export des métriques** locales (diagnostics) stable pour l'UI et le support.

**Porte de sortie Lot D** : `cargo fmt --all --check` · `cargo clippy --workspace --all-targets -D warnings` · `cargo test --workspace` · tests transport **release** · `cargo deny`/`audit` — tous verts ; suites e2e réseau stables (non flaky) mesurées isolément ; nouvelles cibles de fuzz sans crash sur campagne bornée ; **aucune modification de `app/src/**` ni des fichiers de suivi**.

---

## Coordination des 4 lots

- **Parallélisme sans conflit** : Lot D = Rust/CI/`src-tauri` ; Lots A/B/C = `app/src/**` + `website/**`. Frontière nette, quasi zéro fichier partagé.
- **Interface entre D et C** : les méthodes API que D expose (`diagnostics.*`, `network.status` enrichi, tunnel relais) sont consommées par l'UI C (carte de connexion C2, panneau diagnostic C/B). D livre l'API + le contrat JSON ; C branche l'UI.
- **Cadence** : chaque lot livre par vagues avec sa porte de sortie ; le propriétaire du dépôt merge, met à jour `PROGRESS.md`/`DECISIONS.md`/`CHANGELOG.md`, et coupe les releases.
- **Revue adverse obligatoire** sur toute nouvelle surface réseau (des deux côtés), conformément à l'historique du projet.
- **Cible v4** : quand les 4 portes de sortie sont vertes et les parcours clés validés en app packagée (WKWebView), bump `4.0.0` et release.
