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
| 1.D — appairage | 🔨 cœur crypto fait, transport à écrire |
| 1.E — livraison multi-appareils | ⬜ pas commencé |

## La prochaine tâche, précisément

**Lot 1.D — le transport de l'appairage.** Le cœur cryptographique est prêt
dans `crates/accord-crypto/src/pairing.rs` (11 tests) : code de 8 caractères,
canal SPAKE2 symétrique, empreinte à six chiffres.

Reste à écrire :

1. Une variante `CoreMsg` pour l'échange PAKE (deux messages, un par côté).
   ⚠️ Vérifié en 1.C : un `CoreMsg` inconnu fait jeter le datagramme et la
   session survit — ajouter une variante est donc sûr.
2. L'état d'appairage côté nœud : code à usage **unique**, expiration à 5 min
   (`CODE_TTL_MS`), cadence limitée sur les tentatives.
3. La confirmation d'empreinte **des deux côtés** avant toute signature.
4. L'ajout de l'appareil à la liste, signature en version *n+1*, publication.
5. Les écrans : « Ajouter un appareil » (code + QR) côté autorisé, saisie côté
   nouveau, confirmation d'empreinte des deux côtés.

Tests exigés par la feuille de route (§6.4, lot 1.D) : appairage nominal ; code
expiré ; code réutilisé ; empreinte non confirmée ; **tentative d'appairage par
un tiers qui a intercepté le code** — doit échouer sans la confirmation.

## Deux pièges déjà rencontrés, à ne pas refaire

🔒 **L'ordre du lot 1.C était faux dans la feuille de route.** Commencer par
« le transport utilise la clé d'appareil » coupe toutes les amitiés du réseau :
la clé statique de transport d'un pair **est** sa clé de compte aujourd'hui.
Corrigé en deux phases — voir `docs/MULTI_DEVICE.md` §3.2.1.

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
