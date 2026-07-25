# Point de reprise — 2026-07-25

Fichier de passation entre sessions. Le lire en premier, puis `ROADMAP.md`
pour le détail du lot en cours.

## Où en est le jalon 1 (multi-appareil, 7.0)

| Lot | État |
|---|---|
| 1.A — conception | ✅ `docs/MULTI_DEVICE.md` |
| 1.B — identités compte/appareil | ✅ y compris le choix du PAKE (§4.1) |
| 1.C **phase 1** — savoir résoudre | ✅ publication DHT, résolution, cache, push direct, rattachement appareil → compte |
| 1.C **phase 2** — présenter la clé d'appareil | 🔨 le socle est posé et vert ; le drapeau `device_key_transport` reste à `false` |
| 1.D — appairage | ✅ code, canal, échange, écrans des deux côtés, confirmation d'empreinte, inscription, révocation |
| 1.E — livraison multi-appareils | 🔨 tâche 1 (diffusion) et 2 (boîte par appareil) faites ; 3 à 5 à faire |

## Ce que le drapeau `device_key_transport` attend encore

Il est à `false` et le restera jusqu'à ce que le parc ait la phase 1. Mais tout
ce qui aurait cassé le jour où il bascule est désormais écrit contre la bonne
identité, et se comporte à l'octet près comme avant tant que les deux
coïncident :

- la **présence DHT** est publiée sous la clé de transport (deux machines d'un
  compte se réécrivaient l'adresse l'une de l'autre) ;
- la **boîte aux lettres** hors-ligne est sondée et descellée avec l'identité de
  transport ;
- le **`NodeInfo` DHT local** vient de l'identité de transport, comme la place
  que les autres nous attribuent ;
- **mDNS** annonce la clé de transport (deux machines d'un compte annonçaient le
  même service et s'ignoraient) ;
- la **boucle d'événements** nomme les deux identités : la machine pour le
  carnet, la file et le relais ; la personne pour l'amitié, le profil et
  l'op-log. Le carnet et l'ensemble des vivants indexent l'appareil **et**
  l'aliasent sur le compte, parce que la moitié du nœud demande « cette personne
  est-elle joignable » sans vouloir choisir de machine ;
- **résolution, poinçonnage et circuits relais** itèrent les cibles de
  livraison ; poinçonner vers un compte finirait en `PeerIdentityMismatch`, ce
  qui jette au passage toute la file du pending ;
- la **liste d'appareils se relève dans la DHT**, et plus seulement quand un ami
  la pousse sur une session ouverte. C'était la partie circulaire : un compte
  basculé ne publie plus de présence sous sa clé racine, donc sans sa liste on
  ignore quelle autre clé chercher. Ça se dénoue parce que la clé DHT de la
  liste se calcule depuis la seule clé de compte.

## Ce qui reste au lot 1.E

3. **Appels** : sonnerie sur tous les appareils, décrochage exclusif, arrêt des
   autres sonneries.
4. **Rattrapage** entre ses propres appareils à la reconnexion.
5. **Accusés de lecture** : convention « lu sur au moins un appareil ».

Tâches 1 et 2 faites : `deliver_core` résout le compte en cibles et livre une
fois par cible ; la file hors-ligne est indexée par cible, donc par appareil.

## 🔴 Question ouverte, toujours pas tranchée

**Comment le nouvel appareil joint-il celui qui est autorisé ?** Il n'a ni
session, ni adresse, et le code n'en porte aucune. `docs/MULTI_DEVICE.md` §4
décrit tout le protocole cryptographique et reste muet là-dessus.

`devices.pair_submit` rend donc le message PAKE au lieu de l'envoyer. Trois
pistes, aucune tranchée :

- **découverte LAN** (le mécanisme mDNS existe déjà) — couvre le cas courant
  « mes deux machines sont chez moi », pas le cas nomade ;
- **rendez-vous dérivé du code**, publié dans la DHT — marche partout, mais
  expose qu'un appairage est en cours à qui surveille la clé dérivée ;
- **le QR porte l'adresse** en plus du code — simple, mais inutile quand on
  recopie le code à la main.

## Pièges déjà rencontrés, à ne pas refaire

🔒 **L'ordre du lot 1.C était faux dans la feuille de route.** Commencer par
« le transport utilise la clé d'appareil » coupe toutes les amitiés du réseau :
la clé statique de transport d'un pair **est** sa clé de compte aujourd'hui.
Corrigé en deux phases — voir `docs/MULTI_DEVICE.md` §3.2.1.

🔒 **Une liste d'appareils dit QUI, pas OÙ.** Un appareil est listé longtemps
avant que son transport présente sa propre clé. D'où
`DEVICE_FLAG_TRANSPORT_KEY` : il dit lequel des deux régimes cet appareil
applique *maintenant*. Le cas qui existera réellement pendant des semaines est
le parc **mixte** — les appareils basculés **plus** la clé de compte pour les
autres — et c'est celui qu'on rate en simplifiant.

🔒 **En SPAKE2 symétrique, `finish()` qui réussit ne prouve rien.** Les deux
côtés dérivent une clé même avec des codes différents ; elles diffèrent, voilà
tout. L'offre ne doit donc jamais être consommée sur un échange abouti —
seulement après confirmation d'empreinte par un humain.

🔒 **`install_session` n'a rien à changer pour lever B1.** L'éviction porte
déjà sur `peer_static`. Des clés par appareil la rendent « par appareil »
gratuitement.

🔒 **Une sauvegarde restaurée ne doit pas cloner la clé d'appareil.** L'archive
copie la base, qui contient la graine d'appareil : restaurer sur une seconde
machine y réinstallait la même clé, donc l'éviction mutuelle que ce jalon
supprime. Effacé à l'import, avec la liste du compte — sans quoi la machine
restaurée serait absente de sa propre liste.

## Dette assumée : le budget du chunk initial

Relevé de 140 à 150 ko le 2026-07-25, avec la raison écrite dans
`scripts/check-bundle-budget.mjs`. **Le vrai correctif n'est pas fait.**

Le français est le seul dictionnaire chargé d'emblée : les neuf autres sont des
morceaux paresseux, donc gratuits, mais chaque chaîne française ajoutée
n'importe où tombe dans le chargement initial.

Or `settings` et `decorations.labels` ne sont lus **que** dans la modale de
réglages, qui est déjà un morceau paresseux. Les sortir du dictionnaire
français chargé d'emblée ramènerait le chargement initial vers le bas et
rendrait le plafond de nouveau significatif.

⚠️ La difficulté : `Dict` dérive de `typeof fr`, donc découper `fr` casse le
type de référence. Il faut vraisemblablement deux dictionnaires typés — un
noyau et une extension de réglages — et non un simple `import()`.

## À surveiller : une lecture base par message sortant

`deliver_core` appelle `delivery_targets`, donc une lecture indexée par message
et par destinataire. Sur une diffusion de groupe c'est une lecture par membre.
Mesuré nulle part pour l'instant, et volontairement pas mis en cache — mais
c'est le premier endroit où regarder si un envoi de groupe devient lent.

## Réflexes de cette base de code

- **Une borne de longueur se compte en octets**, jamais en caractères, dès
  qu'elle doit s'accorder avec le fil. « é » pèse deux octets.
- **Un test qui réimplémente la fonction testée ne teste rien.** Paramétrer la
  difficulté de preuve de travail plutôt que dupliquer la vérification.
- **Déployer la tolérance une version avant d'en avoir besoin** (champ de
  capacités, `RecordKind::Unknown`, résolution d'appareil).
- **Un test doit prouver qu'il mord** : muter la production, constater le rouge,
  revenir. Un test vert sur du code cassé coûte plus qu'aucun test.
- Les langues, la détection BCP-47 et le sélecteur dérivent tous de `LANGS` :
  une langue ajoutée ne demande que son dictionnaire et son entrée de chargeur.
- **Jamais `git add -A` dans un checkout partagé** : une autre session y
  travaille, et son travail part dans le commit. Chemins explicites.

## Action qui n'appartient qu'à toi

🔴 **Sauvegarder `~/.tauri/accord-updater.key` hors de cette machine.** Sans
elle, plus aucune mise à jour signée n'est publiable si le disque tombe. Je ne
peux ni la copier ni la vérifier.
