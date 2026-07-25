# Point de reprise — 2026-07-25

Fichier de passation entre sessions. Le lire en premier, puis `ROADMAP.md`
pour le détail du lot en cours.

## Où en est le jalon 1 (multi-appareil, 7.0)

| Lot | État |
|---|---|
| 1.A — conception | ✅ `docs/MULTI_DEVICE.md` |
| 1.B — identités compte/appareil | ✅ y compris le choix du PAKE (§4.1) |
| 1.C **phase 1** — savoir résoudre | ✅ publication DHT, résolution, cache, push direct, rattachement appareil → compte |
| 1.C **phase 2** — présenter la clé d'appareil | ⏳ **bloqué** : attend que le parc ait la phase 1 |
| 1.D — appairage | 🔨 crypto + machine à états + `pair_start`/`pair_cancel` faits ; routage et écrans à écrire |
| 1.E — livraison multi-appareils | ⬜ pas commencé |

## La prochaine tâche, précisément

**Lot 1.D — le transport de l'appairage.** Le cœur cryptographique est prêt
dans `crates/accord-crypto/src/pairing.rs` (11 tests) : code de 8 caractères,
canal SPAKE2 symétrique, empreinte à six chiffres.

Fait : `crates/accord-crypto/src/pairing.rs` (11 tests), `CoreMsg::PairingHello`
(0x18, borné à 256 o au décodage), `crates/accord-node/src/pairing.rs` —
machine à états pure, 10 tests — et `devices.pair_start` / `devices.pair_cancel`.

Reste à écrire :

1. **Le routage** de `PairingHello` dans `runtime.rs` vers l'offre en cours, et
   la réponse avec notre propre message PAKE.
2. **La confirmation d'empreinte des deux côtés** : `devices.pair_confirm`,
   qui appelle `PairingOffer::confirm`. L'empreinte du canal candidat doit
   remonter jusqu'à l'écran (elle n'est pas encore exposée — volontairement,
   voir le commit `8afc8e6`).
3. **L'ajout de l'appareil à la liste**, signature en version *n+1*,
   publication.
4. **Les écrans** : « Ajouter un appareil » (code + QR) côté autorisé, saisie
   côté nouveau, confirmation d'empreinte des deux côtés.

Tests exigés par la feuille de route (§6.4, lot 1.D) : appairage nominal ; code
expiré ; code réutilisé ; empreinte non confirmée ; **tentative d'appairage par
un tiers qui a intercepté le code** — doit échouer sans la confirmation.

## 🔴 Question ouverte, apparue à l'implémentation

**Comment le nouvel appareil joint-il celui qui est autorisé ?** Il n'a ni
session, ni adresse, et le code n'en porte aucune. `docs/MULTI_DEVICE.md` §4
décrit tout le protocole cryptographique et reste muet là-dessus.

`devices.pair_submit` rend donc le message PAKE au lieu de l'envoyer, et son
acheminement reste à concevoir. Trois pistes, aucune tranchée :

- **découverte LAN** (le mécanisme mDNS existe déjà) — couvre le cas courant
  « mes deux machines sont chez moi », pas le cas nomade ;
- **rendez-vous dérivé du code**, publié dans la DHT — marche partout, mais
  expose qu'un appairage est en cours à qui surveille la clé dérivée ;
- **le QR porte l'adresse** en plus du code — simple, mais inutile quand on
  recopie le code à la main.

À trancher avant de finir le lot 1.D.

## Deux pièges déjà rencontrés, à ne pas refaire

🔒 **L'ordre du lot 1.C était faux dans la feuille de route.** Commencer par
« le transport utilise la clé d'appareil » coupe toutes les amitiés du réseau :
la clé statique de transport d'un pair **est** sa clé de compte aujourd'hui.
Corrigé en deux phases — voir `docs/MULTI_DEVICE.md` §3.2.1.

🔒 **En SPAKE2 symétrique, `finish()` qui réussit ne prouve rien.** Les deux
côtés dérivent une clé même avec des codes différents ; elles diffèrent, voilà
tout. Une erreur ne signale qu'un message mal formé. L'offre d'appairage ne
doit donc **jamais** être consommée sur un échange abouti — seulement après la
confirmation d'empreinte par un humain. Sinon n'importe qui la détruit à
distance avec un datagramme bien formé.

🔒 **`install_session` n'a rien à changer pour lever B1.** L'éviction porte
déjà sur `peer_static`. Des clés par appareil la rendent « par appareil »
gratuitement.

## Réflexes de cette base de code

- **Une borne de longueur se compte en octets**, jamais en caractères, dès
  qu'elle doit s'accorder avec le fil. « é » pèse deux octets.
- **Un test qui réimplémente la fonction testée ne teste rien.** Paramétrer la
  difficulté de preuve de travail plutôt que dupliquer la vérification.
- **Déployer la tolérance une version avant d'en avoir besoin** (champ de
  capacités, `RecordKind::Unknown`, résolution d'appareil).
- Les langues, la détection BCP-47 et le sélecteur dérivent tous de `LANGS` :
  une langue ajoutée ne demande que son dictionnaire et son entrée de chargeur.

## Action qui n'appartient qu'à toi

🔴 **Sauvegarder `~/.tauri/accord-updater.key` hors de cette machine.** Sans
elle, plus aucune mise à jour signée n'est publiable si le disque tombe. Je ne
peux ni la copier ni la vérifier.
