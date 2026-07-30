# Feuille de route Accord — 6.2 → 13.0

**Horizon : 12 mois.** Point de départ : v6.1.0, publiée le 2026-07-24.

> Versionné depuis la 8.0. L'analyse d'écarts du 10 juillet — largement
> réalisée depuis — est conservée dans `ROADMAP-archive-2026-07-10.md`.

---

## Sommaire

**Cadre**
- [Partie 0 — Comment lire ce document](#partie-0--comment-lire-ce-document)
- [Partie 1 — État des lieux](#partie-1--état-des-lieux)
- [Partie 2 — Principes non négociables](#partie-2--principes-non-négociables)
- [Partie 3 — Les trois bloqueurs structurels](#partie-3--les-trois-bloqueurs-structurels)
- [Partie 4 — Vue d'ensemble des six premiers mois](#partie-4--vue-densemble-des-six-premiers-mois)

**Premier semestre (mois 1 à 6)**
- [Partie 5 — Jalon 0 : assainissement (6.2)](#partie-5--jalon-0--assainissement-62)
- [Partie 6 — Jalon 1 : multi-appareil (7.0)](#partie-6--jalon-1--multi-appareil-70)
- [Partie 7 — Jalon 2 : post-quantique (8.0)](#partie-7--jalon-2--post-quantique-80)
- [Partie 8 — Jalon 3 : mobile — 🔴 ABANDONNÉ](#partie-8--jalon-3--mobile--🔴-abandonné-2026-07-26)
- [Partie 9 — Jalon 4 : échelle et robustesse (10.0)](#partie-9--jalon-4--échelle-et-robustesse-100)

**Second semestre (mois 7 à 12)**
- [Partie 18 — Mois 7 à 12 : jalons 5 à 8](#partie-18--second-semestre--mois-7-à-12)

**Transverse et méthode**
- [Partie 10 — Chantiers transverses](#partie-10--chantiers-transverses)
- [Partie 11 — Méthode de travail](#partie-11--méthode-de-travail)
- [Partie 12 — Registre des risques](#partie-12--registre-des-risques)
- [Partie 13 — Parcours à ne jamais casser](#partie-13--parcours-à-ne-jamais-casser)

**Référence**
- [Partie 14 — Backlog de fonctionnalités](#partie-14--backlog-de-fonctionnalités)
- [Partie 15 — Stratégie de test](#partie-15--stratégie-de-test)
- [Partie 16 — Inventaire du protocole filaire](#partie-16--inventaire-du-protocole-filaire)
- [Partie 17 — Plans de version](#partie-17--plans-de-version)
- [Partie 19 — Annexes](#partie-19--annexes)

> **Par où commencer ?** Si vous reprenez ce document après une pause :
> la partie 19.5 (prochaines actions immédiates) dit quoi faire maintenant,
> la partie 3 rappelle ce qui bloque, et la partie 2 ce qu'il ne faut pas casser.

## Partie 0 — Comment lire ce document

### 0.1 À quoi il sert

C'est un plan de travail personnel : ce qu'il y a à faire, dans quel ordre, pourquoi cet ordre, et ce qu'il ne faut surtout pas casser en chemin.

Il sert trois usages :

1. **Décider quoi faire ensuite** sans repartir de zéro à chaque session.
2. **Garder la mémoire des raisons** — les principes de la partie 2 et le journal des décisions (19.3) existent pour qu'un choix pris un jour ne soit pas défait le lendemain par oubli.
3. **Détecter les dérives** : quand une estimation double, c'est un signal, pas une fatalité.

Le travail se fait **seul**. Pas de répartition entre exécutants : l'ordre des lots est donc purement séquentiel, et le seul parallélisme possible est celui des tâches de fond (CI, builds, campagnes de fuzzing) pendant qu'on code autre chose.

### 0.2 Conventions

- **Jalon** : une version publiée sur GitHub. Un jalon se termine toujours par une release complète (gate vert, tag, binaires signés, `latest.json` vérifié).
- **Lot** : ensemble cohérent de tâches, livrable en une branche.
- **Tâche** : unité atomique, testable, avec critère de fin explicite.
- **Estimation** : en *sessions de travail* (≈ une journée d'attention soutenue), pas en jours calendaires.
- 🔴 **Bloquant** — rien n'avance tant que ce n'est pas résolu.
- ⚠️ **Risque** — peut faire dérailler le jalon.
- 🔒 **Contrainte dure** — non négociable, même sous pression.

### 0.3 Ce que ce document n'est pas

Ce n'est **pas une promesse de dates**. Les estimations servent à ordonner et à détecter les dérives, pas à s'engager.

Ce n'est **pas figé**. Trois choses le feront évoluer : ce qu'on découvre en codant (comme le bloqueur multi-appareil, découvert en ouvrant le dossier), ce que les utilisateurs signalent, et ce que l'usage réel révèle.

### 0.4 Règle d'or

> **Un jalon n'est jamais « presque fini ».** Il est publié et vérifié, ou il est en cours.

Les demi-livraisons coûtent plus cher que le retard. Le précédent qui fonde cette règle : la régression 3.0 → 3.3 (un `debug_assert` avalait `install_session` en release) a causé une **panne totale de la messagerie pendant quatre versions**, parce qu'un chemin n'était vérifié qu'en debug.

---

## Partie 1 — État des lieux

### 1.1 Ce qui existe aujourd'hui (v6.1.0)

#### Identité et comptes

- Identité = **une seule graine Ed25519**, scellée localement par une phrase de passe (Argon2id).
- Preuve de travail à la création (`IDENTITY_POW_BITS = 16`) — anti-spam d'identités.
- Phrase de récupération 12 mots ; restauration complète depuis la phrase.
- **Multi-comptes sur une même machine** : profils isolés, sélecteur au démarrage, registre local.
- Sauvegarde chiffrée complète (`.accordbackup`), protégée par phrase de passe.
- ❌ **Pas de multi-appareil** (bloqueur B1).

#### Réseau et transport

- Transport UDP maison, sessions chiffrées bout en bout (handshake type Noise, X25519 + Ed25519).
- Fragmentation/réassemblage transparents au-delà de la MTU (`UDP_MTU = 1200`).
- Re-keying périodique (forward secrecy) : `REKEY_FRAME_LIMIT`, `REKEY_MAX_AGE_S`.
- **DHT Kademlia** pour la résolution des codes d'ami (`DHT_K = 20`, `DHT_ALPHA = 3`).
- **Traversée de NAT** : observation d'adresse, poinçonnage coordonné, repli relais.
- **Découverte LAN** par mDNS, démon supervisé (l'amont `mdns-sd` panique — issue #483).
- Nœuds d'amorçage par défaut + configurables in-app.
- **Fiabilité de reconnexion** (Lot G, v4.5) : régénération de handshake bornée à nonce frais, récupération d'un WELCOME perdu, éviction de session cadavre, store-and-forward hors ligne.

#### Messagerie

- Messages directs chiffrés ; boîte aux lettres chiffrée pour les pairs hors ligne (7 jours).
- Serveurs avec op-log répliqué, `op_id` adressé par contenu.
- Salons **texte, vocal, annonces, forum** (avec fils).
- Rôles et permissions granulaires, hiérarchie, fondateur intouchable.
- Réactions, réponses, édition, suppression, épinglage, transfert, liens de message.
- Markdown complet (GFM, tables), coloration de code, autocomplétion émoji.
- Fils, sondages, événements de serveur.
- Invitations : tickets signés nominatifs, liens partageables, consentement explicite.
- Messages éphémères (suppression locale après durée choisie).
- Brouillons, non-lus (cap 99+), mentions, recherche, recherches récentes, favoris.

#### Voix et vidéo

- Voix **full mesh jusqu'à 10 participants** (`VOICE_MAX_PARTICIPANTS`).
- Opus, FEC en bande, DTX ; tampon de gigue ; débit adaptatif.
- DSP : suppression de bruit (nnnoiseless), AGC, **annulation d'écho** (FFT réelle).
- Mixage avant sortie, limiteur doux, declick.
- VAD à hystérésis, indicateur « parle », orateur prioritaire (ducking).
- Modération vocale serveur (sourdine/surdité forcées, op 0x1F).
- Appels 1-à-1 : sonnerie, occupé, timeout, appels croisés arbitrés.
- Soundboard (gate d'émission **et** de réception sur le salon).
- **Partage d'écran** (5.0), **caméra** (6.0), les deux en appel **et en salon de groupe** (6.1).

#### Interface

- 24 thèmes, dont des thèmes immersifs à scènes animées.
- Décorations de profil : 63 décorations d'avatar, effets, cadres.
- Palette de commandes (Ctrl/⌘-K) contextuelle, actions permissionnées.
- Recherche dans les réglages (13 onglets).
- Densité, zoom d'interface, taille de police, réduction d'animations, saturation.
- Squelettes de chargement, états vides, badge Dock macOS.
- Barre latérale redimensionnable, dossiers de serveurs, MP épinglés.
- **Trois langues** : français, anglais, espagnol.

#### Qualité et outillage

- Gate `./ci.sh` : fmt, clippy `-D warnings`, clippy anti-panic (libs **et** bins), tests workspace, e2e transport en release, `cargo deny`, `cargo audit`, puis tsc/eslint/prettier/vitest/build.
- CI GitHub, miroir exact du gate local.
- Release multi-plateformes automatisée (macOS, Windows, Ubuntu), artefacts signés, `latest.json` à 9 clés.
- Mise à jour in-app, notes en Markdown.
- Fuzzing (8 cibles), campagnes nocturnes.
- **~1925 tests frontend** (132 fichiers), **~870 tests Rust unitaires**, plus les e2e.

### 1.2 Métriques de référence

| Métrique | Valeur au 2026-07-25 | Seuil d'alerte |
|---|---|---|
| Tests frontend | 1971 (136 fichiers) + 20 e2e Playwright | ne doit jamais baisser |
| Tests Rust unitaires | ~925 | ne doit jamais baisser |
| Bundle JS principal | 532 ko / **138 ko gzip** | 140 ko gzip (vérifié par la CI) |
| CSS principal | 187 ko / 34,5 ko gzip | 50 ko gzip |
| Durée CI | ~5–7 min | 15 min |
| Durée release complète | ~20–25 min | 45 min |
| Crates du workspace | 9 + 1 app | — |
| Langues | 4 (fr, en, es, pt) | — |

**Règle** : toute livraison qui franchit un seuil doit corriger ou justifier explicitement.

### 1.3 Dette technique

| # | Dette | Impact si ignorée | Effort |
|---|---|---|---|
| ~~D1~~ | ~~Bundle JS monolithique~~ — **payée (6.2)** : 207 → 138 ko gzip, budget vérifié par la CI | — | — |
| ~~D2~~ | ~~E2e d'interface hors du gate~~ — **payée (6.3)** : Playwright entre dans `ci.sh` et la CI, + 5 spécs sur la grille vidéo. La suite existait mais n'était lancée par personne : elle échouait dans son coin depuis la 4.5.0 | — | — |
| D3 | Fichiers au-delà de la limite de 800 lignes — **30 fichiers**, remesurés le 2026-07-27 au soir. L'entrée ne nommait que du TypeScript ; les pires sont en Rust : `group/state.rs` **5288**, `service/tests.rs` 4295, `core_msg.rs` 3918, `node/tests.rs` 3466, `lib/api.ts` 2738. Un cliquet (`scripts/check-file-size.mjs`) empêche désormais toute croissance, en local ET en CI | Modifier le chat devient risqué | 2 |
| ~~D4~~ | ~~Pas de télémétrie locale agrégée de santé réseau~~ — **payée, et l'entrée était périmée** : `NetCounters` (`node/diagnostics.rs`) agrège douze compteurs, `diagnostics.counters/selftest/report` les exposent, `NetworkPanel.tsx` les affiche | — | — |
| ~~D5~~ | ~~Décorations en dictionnaire dur~~ — **payée (6.2)** : libellés dans les dictionnaires, couverts par le test de parité | — | — |
| ~~D6~~ | ~~Pas de migration de schéma versionnée~~ — **payée (6.2)** : étapes numérotées transactionnelles, sauvegarde, refus de rétrograder | — | — |
| ~~D7~~ | ~~Worktrees et branches obsolètes~~ — **payée (6.2)**, ⚠️ **repoussée depuis** : 22 branches fusionnées et libres de tout worktree supprimées le 2026-07-27. La convention est écrite mais rien ne l'applique — l'état repousse à chaque série de chantiers | — | — |
| ~~D8~~ | ~~`dist/` accumule les DMG~~ — **payée (6.2)**, ⚠️ **repoussée depuis** : trois DMG au lieu de deux le 2026-07-27, le 4.5.0 retiré. Même remarque qu'en D7 : une règle sans garde-fou | — | — |

### 1.3.1 Ce que le ménage a révélé (2026-07-25)

Deux entrées de dette étaient fausses, dans les deux sens, et il a fallu mesurer
pour s'en apercevoir :

- **D2 disait « pas d'e2e d'interface ».** Faux : la suite existait (composeur,
  navigation, menu serveur, thèmes, fil). Le vrai problème était pire — elle
  n'était lancée **ni par `ci.sh` ni par la CI**. Elle échouait donc dans son
  coin depuis la refonte 4.5.0, sans destinataire.
- **Ce qu'elle signalait était une vraie régression publiée** : l'entrée
  « Marquer comme lu » du menu serveur, présente dans l'ancien menu de la barre
  latérale, a été perdue à la refonte du menu d'en-tête. Elle est restée absente
  pendant plusieurs versions.
- **Un test unitaire affirmait la régression** au lieu de l'attraper (il
  attendait « Inviter des personnes » en tête même avec des non-lus). Réécrit,
  pas supprimé.

🔒 **Leçon à retenir** : avant de conclure qu'une suite est verte, vérifier
qu'elle est **exécutée**. Une suite hors du gate ne rapporte à personne.

### 1.4 Ce qui n'est pas vérifiable en headless

Point d'honnêteté : certaines choses ne peuvent **pas** être prouvées par les tests automatiques.

- **Capture et rendu vidéo réels** (`getDisplayMedia`, `getUserMedia`, WebCodecs dans le WKWebView macOS). Le code est feature-détecté et dégrade proprement, mais l'image effective n'est visible que sur machine.
- **Qualité audio perçue** : l'annulation d'écho, le mixage et le limiteur ont des tests numériques ; « est-ce que ça sonne bien » ne se teste pas en CI.
- **Traversée de NAT réelle** : les e2e couvrent des scénarios simulés ; les NAT symétriques d'opérateurs réservent des surprises.
- **Permissions système** (micro, caméra, enregistrement d'écran) : les dialogues ne se déclenchent qu'en usage réel.
- **Fluidité perçue** : les budgets de performance sont des proxys.

**Conséquence** : chaque jalon touchant ces domaines se termine par une **passe de vérification sur appareil**, documentée comme telle. On ne prétend jamais avoir vérifié ce qu'on n'a pas vu.

---

## Partie 2 — Principes non négociables

Chaque règle a un incident derrière elle.

### 2.1 🔒 Compatibilité filaire

**Un client déjà installé ne doit jamais cesser de fonctionner à cause d'une mise à jour de l'autre côté.**

1. **Toujours ajouter, jamais modifier** : une fonctionnalité protocolaire prend une **variante neuve** (nouveau discriminant), pas un champ ajouté à une variante existante.
2. Un genre inconnu est **rejeté proprement** au décodage (datagramme jeté, trace debug), jamais mal interprété ni source de panique.
3. Toute rupture exige une **négociation de version** explicite, et une période de coexistence.
4. Le diff filaire est **vérifié à chaque livraison** : `proto`, `crypto`, `dht`, `frag`, `relay` ne bougent pas sans décision consciente.

*Précédent* : la 6.0 a ajouté la caméra via `CameraFrame`/`CameraControl` (variantes neuves) plutôt qu'un drapeau sur `ScreenFrame`. Un client 5.0 rejette proprement le genre inconnu au lieu d'afficher une caméra dans sa visionneuse d'écran.

### 2.2 🔒 Interdiction de paniquer en production

`unwrap`, `expect`, `panic!`, `todo!`, `unimplemented!` sont **interdits par clippy** dans les bibliothèques **et** les binaires. Le lint est dans le gate et dans la CI.

**Ne jamais le retirer, même temporairement.** Un nœud P2P qui panique, c'est un utilisateur déconnecté sans message d'erreur.

Corollaire : mutex empoisonnés récupérés (`unwrap_or_else(|e| e.into_inner())`), index vérifiés, conversions bornées.

### 2.3 🔒 Pas de `debug_assert` sur un chemin à effet de bord

*Précédent* : un `debug_assert!` contenant l'appel à `install_session` — en release la macro disparaît, **la messagerie tombe en panne totale**. Quatre versions affectées.

Le lint `clippy::debug_assert_with_mut_call` est dans le gate. Il reste.

### 2.4 🔒 Le gate est le gate

Aucune livraison sans `./ci.sh` vert — ou, si la machine locale est empêchée, sans la **CI GitHub verte**, qui exécute exactement les mêmes commandes.

**Piège documenté** : ne jamais valider vitest avec `tail`. Toujours lire la ligne `Test Files … / Tests … passed`. Une sortie tronquée a déjà masqué un échec.

### 2.5 🔒 Zéro serveur

Accord n'a pas de serveur central et n'en aura pas.

Les nœuds d'amorçage sont un **annuaire de rendez-vous**, pas un serveur : aucun contenu, aucun message stocké, et l'application fonctionne sans eux dès que les pairs se connaissent.

Quand une fonctionnalité semble exiger un serveur (recherche globale, notifications push, synchronisation d'historique), la bonne question est : *« comment le fait-on entre pairs — ou pas du tout ? »*

### 2.6 🔒 Vérifier à l'écran ce qui se voit à l'écran

*Précédent* : trois itérations en aveugle sur le menu déroulant de serveur, trois échecs. La mise en place d'une vraie boucle de prévisualisation a révélé le défaut en quelques minutes — la surface était translucide, la liste des salons transparaissait.

Pour tout travail d'interface : **regarder le résultat**, pas seulement faire passer les tests.

### 2.7 🔒 Un chantier parallèle a son propre dossier

Dès qu'on veut mener une expérimentation sans perturber le travail en cours — tester une approche risquée, préparer un lot pendant qu'une release tourne — il faut un **worktree**, pas seulement une branche.

*Précédent* : deux sessions travaillant dans le même arbre se sont écrasées mutuellement. L'une a perdu un fichier de traduction, l'autre a vu ses libellés effacés. `git checkout` change l'arbre **partagé** ; le travail non commité voyage avec.

Même seul, la règle vaut : une branche protège l'historique, pas l'arbre de travail.

```bash
git worktree add ../accord-<lot> -b feat/<lot> origin/main
```

🔒 Et son corollaire : **ne jamais supprimer un worktree sans avoir vérifié qu'il ne contient pas de travail non commité.**

### 2.8 🔒 Rituel de release, sans raccourci

1. Bump : `Cargo.toml` (workspace), `Cargo.lock` (les 10 crates `accord-*`), `app/package.json`, `app/package-lock.json`, `app/src-tauri/tauri.conf.json`
2. CHANGELOG daté, écrit **pour l'utilisateur** (ce qui change pour lui, pas la liste des commits)
3. Gate vert
4. Merge `main` en fast-forward
5. Push `main`, attendre la CI verte
6. Tag `vX.Y.Z`, push du tag
7. `release.yml` : 4 jobs (validate + macOS + Windows + Ubuntu)
8. Vérifier `latest.json` : 9 clés, **toutes signées**, URLs https
9. `gh release edit vX.Y.Z --draft=false --latest`

La clé de signature (`~/.tauri/accord-updater.key`) **ne doit jamais être commitée**, affichée, ni copiée ailleurs.

### 2.9 Langue

**Corrigé le 2026-07-27.** Cette section affirmait « tout le dépôt, commentaires
compris, en anglais ». C'était faux, et `docs/DEV.md` §5 l'a tranché le
2026-07-26 : les **commentaires de code sont en français** — c'est ce que le
code fait réellement, y compris tout ce qui a été écrit depuis la 7.0, et
prétendre à une migration anglaise qui n'a jamais eu lieu ne produisait qu'une
troisième convention.

| Quoi | Langue |
|---|---|
| Commentaires de code | **français** |
| Noms de symboles | français ou anglais, selon le fichier voisin |
| Documentation, README, CHANGELOG, SPEC, notes de version | **anglais** |
| Messages de commit | **anglais** |
| Échanges de travail | français |

⚠️ Deux documents n'ont pas rattrapé : `docs/COMMUNITY.md` et
`docs/VOICE_CALLS.md` sont encore en français. Dit plutôt que passé sous
silence.

---

## Partie 3 — Les trois bloqueurs structurels

### 3.1 🔴 B1 — Le multi-appareil est bloqué par un invariant du transport

#### Le constat

L'identité Accord est **une seule graine Ed25519**. `public_key`, `node_id`, la clé X25519 d'échange et toutes les signatures en dérivent (`accord-crypto/src/identity.rs`).

L'approche naïve — copier la graine sur le second appareil — se heurte à un invariant explicite du transport, dans `install_session` :

> *« au plus une session directe par identité, livraison déterministe »*

À l'établissement d'une session, **toutes les autres sessions directes portant la même clé statique sont évincées**.

#### Pourquoi cet invariant existe

Ce n'est pas un détail d'implémentation : c'est le correctif de la **cause 4 du flake de reconnexion** (Lot G). Après le redémarrage silencieux d'un pair, une session périmée subsistait à l'ancienne adresse et créait un trou noir de livraison jusqu'à expiration d'inactivité.

**Le retirer réintroduirait le bug qu'un chantier entier a éliminé.**

#### Ce que ça implique

Deux appareils avec la même graine se **chassent mutuellement** chez chaque ami. Seul le dernier connecté reçoit. C'est pire qu'une fonctionnalité manquante : c'est **silencieusement cassé**.

#### La seule sortie propre

Séparer **identité de compte** et **identité d'appareil** :

- chaque appareil a sa **propre paire de clés** (donc sa propre session, sans éviction) ;
- un **compte** est une identité racine qui signe la **liste de ses appareils** ;
- les contacts s'ajoutent au niveau **compte** ; les messages sont diffusés à **tous les appareils** du destinataire ;
- l'historique se synchronise entre ses propres appareils.

C'est le modèle de Signal et de Matrix. Ce n'est pas une fonctionnalité : c'est une **refonte du modèle d'identité** touchant identité, transport, DHT, amis, groupes, chiffrement, boîte aux lettres, sauvegardes.

**→ Jalon 1 (7.0), avec une phase de conception avant toute ligne de code.**

### 3.2 ⚠️ B2 — Le post-quantique casse le format filaire

#### Le constat

Le handshake repose sur X25519. Un chiffrement hybride post-quantique (X25519 + ML-KEM, façon PQXDH) modifie la structure du HELLO/WELCOME : **rupture filaire**, exactement ce que le principe 2.1 interdit sans négociation.

#### Le vrai risque

Une négociation de version dans le handshake est délicate : c'est le seul endroit où l'on parle à un pair **avant** de savoir ce qu'il sait faire. Mal conçue, elle ouvre une **attaque par repli** — un attaquant force les deux pairs à l'ancien format, annulant tout le bénéfice.

#### Ce qu'il faut

1. Un champ de version/capacités dans le HELLO, **déjà présent dans les clients déployés** avant d'introduire le nouveau format → préparer le terrain **une version à l'avance**.
2. Une période de coexistence des deux formats.
3. Une protection anti-repli : la version négociée doit être **authentifiée dans le transcript** du handshake.
4. Un basculement en deux temps : accepter les deux, puis exiger le nouveau.

**→ La préparation (champ de capacités) est glissée dans le jalon 0. Le chiffrement est le jalon 2.**

### 3.3 ✅ B3 — Le build local macOS est empêché — **levé le 2026-07-27**

> **Le DMG universel 7.1.0 a été produit en local.** Le blocage décrit ci-dessous
> n'est plus actif : `tauri-build` recopie ses fichiers sans erreur, l'attribut
> `com.apple.macl` ne gêne plus rien.
>
> ⚠️ **Mais ce n'est pas la seule chose qui empêchait ce build.** Le 2026-07-27,
> il échouait encore, pour une cause entièrement différente : CMake 4 a supprimé
> la compatibilité avec `cmake_minimum_required < 3.5`, que déclare le Opus
> embarqué par `audiopus_sys`. Le contournement (`CMAKE_POLICY_VERSION_MINIMUM`)
> existait dans `ci.yml` depuis des semaines, mais pas dans
> `scripts/build-macos.sh` — et **pkg-config n'est pas une échappatoire ici** :
> Homebrew ne fournit libopus que pour l'architecture native, donc la seconde
> tranche d'un binaire universel passe forcément par le Opus embarqué, donc par
> CMake. Corrigé dans le script.
>
> La leçon de la troisième puce ci-dessous tient toujours, et elle vient d'être
> vérifiée deux fois : **ce bloqueur a changé de cause sans changer de symptôme**
> (« le build local ne passe pas »). C'est exactement pourquoi une release ne doit
> jamais dépendre de la machine locale.

#### Le constat historique (2026-07-24)

Depuis le 2026-07-24, le build du crate Tauri échoue en local avec `Operation not permitted (os error 1)` dans le build script, au moment où `tauri-build` copie les fichiers de configuration.

Cause identifiée : macOS a apposé l'attribut étendu `com.apple.macl` (jeton d'accès sandbox) sur les fichiers source. Quand `copyfile()` tente de le recopier vers la destination, l'opération est refusée.

Vérifications faites :
- échoue aussi **hors bac à sable** ;
- échoue aussi **hors du dépôt** (répertoire de build temporaire) ;
- `xattr -c` ne peut pas retirer l'attribut (macOS le réapplique aussitôt) ;
- le même build **passait le matin même** (le DMG 5.0.0 a été produit ainsi) ;
- **la CI GitHub compile tout sans problème** — ce n'est pas le code.

#### Ce qu'il faut faire

- **Immédiat** : un redémarrage de la machine efface généralement ces attributs.
- **Durable** : `scripts/clean-xattrs.sh` au dépôt, à lancer si le symptôme réapparaît.
- **Structurel** : ne jamais faire dépendre une release de la seule machine locale. La CI doit rester capable de tout produire seule — c'est déjà le cas, il faut le préserver.

**→ Tâche T0.3.**

---

## Partie 4 — Vue d'ensemble des six premiers mois

### 4.1 Les cinq premiers jalons

| Jalon | Version | Thème | Durée | Risque |
|---|---|---|---|---|
| **0** | 6.2 | Assainissement et préparation | 1 mois | Faible |
| **1** | 7.0 | **Multi-appareil** | 2 mois | Élevé |
| **2** | 8.0 | **Confidentialité de demain** (post-quantique) | 1 mois | Moyen |
| ~~**3**~~ | ~~9.0~~ | ~~**Mobile** (iOS/Android)~~ | — | 🔴 **abandonné**, voir partie 8 |
| **4** | 10.0 | Échelle et robustesse | continu | Moyen |

Les jalons 5 à 8 (mois 7 à 12) sont décrits en partie 18.

Le jalon 4 n'est pas séquentiel : c'est un fil de fond qui absorbe correctifs, optimisation et dette, et donne les versions mineures intermédiaires.

### 4.2 Pourquoi cet ordre

**Le multi-appareil d'abord**, malgré son coût :

1. C'est le **manque le plus ressenti** au quotidien (on a un portable *et* un fixe).
2. Il touche le **modèle d'identité**, la fondation. Plus on attend, plus il y a de code à migrer.
3. ~~Le mobile (jalon 3) est **inutilisable sans lui**~~ — le jalon 3 est abandonné (partie 8). Le multi-appareil garde tout son sens sans lui : deux ordinateurs sont le cas quotidien.

**Le post-quantique ensuite** parce qu'il touche le handshake, déjà remué par les clés d'appareil. Enchaîner évite de payer deux fois la compréhension du transport.

~~**Le mobile en dernier**~~ — abandonné (partie 8). Son prix d'entrée est un serveur de réveil, ce que le principe 2.5 refuse.

### 4.3 Séquencement

Le travail étant séquentiel, chaque mois a un **thème principal** et une **poche de tâches courtes** à glisser quand le chantier principal bloque (attente de CI, de build, de release) ou quand il faut souffler après une zone à risque.

```
Mois 1    │ Principal : Jalon 0 — assainissement, capacités de version,
          │             migrations de schéma
          │ Poches    : découpage du bundle (D1), décorations i18n (D5),
          │             refonte de la surface d'appel
──────────┼──────────────────────────────────────────────────────────────
Mois 2-3  │ Principal : Jalon 1 — multi-appareil
          │             (conception complète AVANT toute implémentation)
          │ Poches    : les 10 langues, e2e d'interface (D2),
          │             écrans de gestion des appareils
──────────┼──────────────────────────────────────────────────────────────
Mois 4    │ Principal : Jalon 2 — post-quantique
          │ Poches    : télémétrie locale de santé réseau (D4),
          │             onglet Sécurité
──────────┼──────────────────────────────────────────────────────────────
Mois 5-6  │ Principal : Jalon 4 — échelle et robustesse (le mobile est abandonné)
          │ Poches    : accessibilité, contrastes sur les 24 thèmes,
          │             navigation clavier complète
```

🔒 **Règle de discipline** : ne jamais ouvrir un second chantier de fond en parallèle du chantier principal. Les poches sont des tâches **courtes et fermées** (une session au plus), pas des projets. Deux chantiers longs menés de front, c'est deux chantiers inachevés.

### 4.4 Critères d'arrêt

Un jalon peut être **abandonné ou reporté sans honte** si :

- une découverte technique invalide l'approche (comme B1 l'a fait pour le multi-appareil naïf) ;
- le coût estimé double sans que la valeur augmente ;
- un problème de fiabilité en production devient prioritaire.

Dans ce cas : documenter la découverte, publier ce qui est stable, re-planifier. **Ne jamais livrer une demi-fonctionnalité pour « tenir » le jalon.**

---

## Partie 5 — Jalon 0 : assainissement (6.2)

**Durée estimée : 1 mois.** Risque faible. Objectif : payer la dette qui rendrait le multi-appareil plus douloureux, et **poser le champ de capacités** dont le post-quantique aura besoin dans deux jalons.

Ce jalon n'apporte presque rien de visible à l'utilisateur. C'est assumé : il rend les deux jalons suivants possibles.

### 5.1 Lot 0.A — Fondations protocolaires

#### T0.1 — Champ de capacités dans le HELLO

**Pourquoi maintenant** : un champ de négociation n'est utile que s'il est **déjà déployé chez tout le monde** au moment où on en a besoin. Si on l'ajoute en même temps que le post-quantique, aucun client ancien ne saura négocier — on aura une rupture nette.

**Conception**

Ajouter au HELLO (et symétriquement au WELCOME) un champ `capabilities: u32`, bitmask de capacités du pair :

| Bit | Nom | Signification |
|---|---|---|
| 0 | `DEVICE_KEYS` | Comprend les identités d'appareil (jalon 1) |
| 1 | `PQ_HYBRID` | Sait faire le handshake hybride post-quantique (jalon 2) |
| 2 | `GROUP_VIDEO_N` | Sait recevoir de la vidéo de plusieurs émetteurs simultanés |
| 3–31 | réservés | doivent être ignorés s'ils sont inconnus |

🔒 **Contrainte** : les bits inconnus sont **ignorés**, jamais une erreur. C'est ce qui permettra d'ajouter des capacités sans casser les anciens.

🔒 **Contrainte anti-repli** : `capabilities` doit entrer dans le **transcript authentifié** du handshake, pour qu'un attaquant ne puisse pas le réécrire en vol. C'est le point le plus important de cette tâche — un champ de capacités non authentifié est pire qu'aucun champ, parce qu'il donne une fausse impression de sécurité.

**Compatibilité** : un client 6.1 qui reçoit un HELLO avec ce champ doit continuer à fonctionner. Deux approches possibles, à trancher en implémentation :

- *(a)* Nouveau genre de HELLO (discriminant neuf) — le client ancien le rejette et… ne se connecte pas. ❌ inacceptable.
- *(b)* Champ ajouté **en fin de structure**, décodage tolérant au manque : un pair ancien envoie un HELLO plus court, on considère `capabilities = 0`. ✅ **retenu**.

L'approche (b) exige que le décodeur actuel tolère les octets en trop — **à vérifier avant de coder** : si `Reader::finish()` échoue sur des octets restants, il faut d'abord assouplir ce point précis pour le HELLO uniquement.

> ✅ **Vérifié le 2026-07-25, et la réponse change le plan.** `WireDecode::from_bytes`
> appelle bien `finish()`, qui rejette tout octet excédentaire. Un client 6.1
> ne se contente donc pas d'ignorer le champ : il **rejette le HELLO entier**,
> et la session ne s'établit pas du tout. Émettre le champ dès la 6.2 aurait
> coupé toute communication avec le parc installé.
>
> D'où un **déploiement en deux temps**, seul moyen d'y arriver sans rupture :
> la 6.2 sait *lire* le champ (et l'authentifie), mais ne l'*émet* pas
> (`EndpointConfig::capabilities = None`). Un répondeur renvoie ses capacités
> uniquement à un initiateur qui en a annoncé — preuve qu'il sait les décoder.
> L'émission s'allumera dans une version ultérieure, quand le parc 6.2 sera
> répandu ; c'est un changement d'une ligne.
>
> C'est exactement la raison d'être de cette tâche : un champ de négociation
> ne sert à rien s'il n'est pas déjà déployé au moment où on en a besoin.

**Tâches**
1. Vérifier le comportement de `finish()` sur octets excédentaires dans le chemin HELLO.
2. Ajouter le champ, décodage tolérant, valeur par défaut 0.
3. L'inclure dans le transcript authentifié.
4. Exposer les capacités du pair dans l'état de session (pour que les couches hautes décident).
5. Tests : ancien HELLO → `capabilities = 0` ; nouveau HELLO → bits lus ; bits inconnus ignorés ; **tentative de réécriture des capacités → handshake rejeté**.

**Critère de fin** : un nœud 6.2 et un nœud 6.1 se parlent normalement, dans les deux sens, avec les tests qui le prouvent.

**Effort** : 2 sessions. ⚠️ **Risque** : toucher au handshake est la zone la plus dangereuse du code. Cette tâche mérite une revue adversariale dédiée (« qu'est-ce qu'un attaquant peut faire de ce champ ? »).

#### T0.2 — Migrations de schéma versionnées (dette D6)

**Problème** : la base locale évolue sans version explicite. Le multi-appareil va ajouter des tables (appareils, vecteurs de synchro) ; sans mécanisme de migration, chaque évolution devient un risque de corruption chez l'utilisateur.

**Conception**
- Table `schema_version` (entier unique).
- Migrations numérotées, appliquées en séquence dans une transaction, avec point de non-retour explicite.
- Au démarrage : si la version en base est **supérieure** à celle du binaire (l'utilisateur a rétrogradé), refuser de démarrer avec un message clair plutôt que corrompre.
- Sauvegarde automatique du fichier avant toute migration.

**Tâches**
1. Table de version + lecture/écriture.
2. Registre de migrations, application transactionnelle.
3. Garde-fou anti-rétrogradation, message utilisateur explicite.
4. Sauvegarde pré-migration (fichier `.bak` horodaté, purge des anciens).
5. Tests : montée de version, migration échouée → rollback propre, rétrogradation refusée.

**Effort** : 2 sessions.

#### T0.3 — Script de nettoyage des attributs étendus (bloqueur B3)

**Tâches**
1. `scripts/clean-xattrs.sh` : retire les attributs étendus du dépôt, avec message clair si l'OS refuse.
2. Documenter le symptôme et le remède dans `docs/DEVELOPMENT.md`.
3. Vérifier que la CI n'a aucune dépendance à une capacité locale.

**Effort** : 0,5 session.

### 5.2 Lot 0.B — Dette technique

#### T0.4 — Découpage du bundle (dette D1)

**Problème** : 738 ko en un seul chunk (189 ko gzip). Sous le seuil d'alerte, mais la trajectoire est mauvaise : chaque fonctionnalité l'alourdit.

**Cibles de découpage** (par ordre de gain) :
- Les **thèmes immersifs** et leurs images (déjà partiellement séparés) — chargés à la demande du thème choisi.
- Les **décorations de profil** (déjà en chunks séparés — vérifier qu'ils sont bien paresseux).
- La **coloration syntaxique** — souvent la plus grosse dépendance d'un chat ; charger le langage à la demande.
- Le **lecteur vidéo** et le **QR code** — déjà séparés, vérifier.
- Les **écrans de réglages** — 13 onglets dont la plupart ne sont jamais ouverts.

**Critère de fin** : chunk initial **< 140 ko gzip**, sans régression de test, et démarrage vérifié à l'écran (pas seulement mesuré).

**Effort** : 1 session.

#### T0.5 — Décorations de profil internationalisées (dette D5)

Les 73 libellés de décorations sont en dur (fr/en) dans `lib/decorations.tsx`. Chaque langue ajoutée demande une passe manuelle — le lot des 10 langues va rendre ça pénible.

**Tâches** : déplacer les libellés dans les dictionnaires ; le test de parité couvre alors automatiquement toute langue future.

**Effort** : 1 session.

#### T0.6 — Ménage des branches et worktrees (dette D7, D8)

**Tâches**
1. Inventorier les branches locales : celles mergées dans `main` sont supprimées ; les autres sont documentées ou supprimées après vérification de leur contenu.
2. Supprimer les worktrees obsolètes (`/private/tmp/accord-*`) **après avoir vérifié qu'ils ne contiennent pas de travail non commité**.
3. Purger `dist/macos` des DMG antérieurs à la version courante — 1.
4. Documenter la convention de nommage des branches.

🔒 **Contrainte** : ne jamais supprimer un worktree ou une branche contenant du travail non commité sans le signaler d'abord.

**Effort** : 0,5 session.

### 5.3 Lot 0.C — Confort utilisateur

Un jalon purement technique est démoralisant. Trois petites choses visibles, à faible risque.

#### T0.7 — Indicateur de qualité de connexion par pair

Le diagnostic réseau existe déjà (type de NAT, direct/relais, latence). Le rendre **visible en un coup d'œil** : une pastille discrète dans la liste d'amis et l'en-tête de conversation (vert direct / orange relais / gris hors ligne), infobulle avec la latence.

**Effort** : 1 session.

#### T0.8 — Refonte de la surface d'appel

L'interface d'appel a grossi par ajouts successifs : audio (v2), écran (v5), caméra (v6), groupe (v6.1). Le panneau flottant actuel empile les vues verticalement — ça ne tiendra pas à 10 participants avec caméras.

**Brief design** : une **grille vidéo** qui s'adapte au nombre d'émetteurs, avec orateur actif mis en avant, épinglage manuel, mode plein écran, et dégradation propre quand personne n'a de caméra (avatars). Cohabitation caméra + écran d'un même participant.

**Effort** : 2 sessions de conception + 2 d'intégration.

#### T0.9 — Raccourcis clavier documentés et complets

Un écran de référence des raccourcis existe. Le compléter (navigation, appels, palette) et vérifier que **tout** est atteignable au clavier.

**Effort** : 1 session.

### 5.4 Définition de fin du jalon 0

- [x] Un nœud 6.2 et un nœud 6.1 s'interconnectent dans les deux sens, prouvé par test
      (`nouveau_et_ancien_noeud_sinterconnectent`, transport e2e).
- [x] Le champ de capacités est authentifié dans le transcript ; retirer le champ,
      en abaisser les bits ou l'injecter dans un handshake qui n'en avait pas fait
      échouer la vérification de signature (4 tests dédiés).
- [x] Les migrations de schéma s'appliquent en séquence, se rollbackent
      intégralement sur échec, et une base écrite par une version plus récente
      est refusée au lieu d'être corrompue.
- [x] Chunk initial **138 ko gzip** (207 avant), budget vérifié par la CI.
- [x] Gate vert, release 6.2.0 publiée, `latest.json` à 9 clés signées — publiée
      le 2026-07-24, 14 fichiers. La case était restée vide après coup.
- [x] Aucune branche ni worktree orphelin non documenté (47 branches et
      3 worktrees retirés ; les deux worktrees contenant du travail non commité
      ont été conservés et signalés).

**Ce qui a changé en route** : T0.1 s'est révélée plus contrainte que prévu (voir
l'encadré de vérification), et T0.8 cachait un vrai défaut sous une question de
mise en page — les visionneuses vidéo étaient indexées par source et non par
émetteur, donc deux personnes diffusant en même temps dans un salon de groupe
se mélangeaient. La grille n'était pas qu'un habillage.

---

## Partie 6 — Jalon 1 : multi-appareil (7.0)

**Durée estimée : 2 mois.** 🔴 **Risque élevé.** C'est le chantier le plus lourd de la feuille de route.

### 6.1 Le problème utilisateur

> « J'ai un portable et un fixe. Je veux lire et écrire depuis les deux, avec le même compte, et retrouver mon historique. »

Aujourd'hui c'est impossible : une identité = une machine. Le contournement (restaurer la phrase de récupération sur la seconde machine) produit deux appareils qui **se chassent mutuellement** (bloqueur B1) — c'est-à-dire un résultat cassé, pas dégradé.

### 6.2 Conception : comptes et appareils

#### 6.2.1 Le modèle

On introduit deux niveaux :

```
        Compte (identité racine Ed25519)
        │  ← c'est ce que voient vos amis : le "code ami" pointe ici
        │
        ├── Appareil A  (clé Ed25519 propre)  "Portable"
        ├── Appareil B  (clé Ed25519 propre)  "Fixe"
        └── Appareil C  (clé Ed25519 propre)  "Téléphone"
```

- La **clé racine du compte** signe une **liste d'appareils**. Elle ne sert qu'à ça : autoriser et révoquer des appareils. Elle peut rester hors ligne la plupart du temps.
- Chaque **appareil** a sa propre paire de clés, qu'il utilise pour **toutes** les sessions de transport. Deux appareils = deux identités de transport distinctes → **plus d'éviction mutuelle**, l'invariant du transport est préservé intact.
- Le **code ami** identifie le **compte**, pas l'appareil.

#### 6.2.2 La liste d'appareils

C'est l'objet central. Elle doit être :

- **signée par la clé racine** (seul le propriétaire peut ajouter/révoquer) ;
- **versionnée** (numéro monotone) pour que les pairs adoptent toujours la plus récente ;
- **révocable** : un appareil retiré doit cesser d'être accepté partout ;
- **publiable** : les amis doivent pouvoir la récupérer (via la DHT, comme le record d'identité actuel).

Structure envisagée :

| Champ | Type | Rôle |
|---|---|---|
| `account` | `[u8; 32]` | Clé publique racine du compte |
| `version` | `u64` | Monotone ; une version inférieure est ignorée |
| `devices` | liste | Chaque entrée : clé publique, nom, date d'ajout, drapeaux |
| `revoked` | liste | Clés publiques révoquées, avec date |
| `issued_ms` | `u64` | Horodatage d'émission |
| `sig` | `[u8; 64]` | Signature racine sur tout ce qui précède |

⚠️ **Piège de conception** : la révocation en P2P sans serveur est un problème dur. Un ami hors ligne au moment de la révocation continuera d'accepter l'appareil révoqué jusqu'à ce qu'il récupère la nouvelle liste. Atténuations :
- version monotone + rafraîchissement à chaque connexion ;
- durée de vie explicite de la liste (au-delà, on force un rafraîchissement) ;
- l'appareil révoqué ne peut pas **empêcher** la propagation (la liste passe par la DHT et par les amis).

**Ce qu'il faut assumer et documenter** : la révocation est **éventuellement cohérente**, pas instantanée. C'est une propriété du sans-serveur, pas un défaut d'implémentation. Il faut l'écrire dans `SECURITY.md`.

#### 6.2.3 Appairage d'un nouvel appareil

Le moment le plus délicat pour la sécurité : c'est là qu'un attaquant voudrait glisser son appareil dans votre compte.

**Flux retenu** : appairage **hors bande, avec confirmation mutuelle**.

1. Sur l'appareil **déjà autorisé** : « Ajouter un appareil » → affiche un **code court** (ou QR) valable quelques minutes.
2. Sur le **nouvel** appareil : saisir/scanner ce code → il génère sa paire de clés et présente sa clé publique.
3. Les deux appareils établissent un canal chiffré dérivé du code (SPAKE2 ou équivalent — **pas** un simple secret partagé en clair).
4. Les deux écrans affichent une **empreinte courte identique** ; l'utilisateur confirme des deux côtés.
5. L'appareil autorisé ajoute le nouveau à la liste, signe la nouvelle version, la publie.
6. Le nouvel appareil reçoit la liste, la clé racine **ne quitte jamais** l'appareil d'origine.

🔒 **Contraintes**
- Le code d'appairage est **à usage unique** et **expire** (5 minutes).
- Un échec de vérification d'empreinte **annule** l'appairage — pas de « continuer quand même ».
- La clé racine du compte ne transite **jamais** sur le réseau, même chiffrée.
- Cadence limitée sur les tentatives d'appairage (anti-force brute du code court).

⚠️ **Question ouverte à trancher en conception** : que se passe-t-il si l'utilisateur perd **tous** ses appareils ? Réponse actuelle : la phrase de récupération régénère la clé racine, donc un nouvel appareil. Il faut vérifier que la liste d'appareils peut être **réinitialisée** par la racine (révocation en masse).

#### 6.2.4 Livraison des messages

Aujourd'hui : un message va à une identité = une session.

Demain : un message va à un **compte** = **N appareils**.

**Conséquences**
- L'expéditeur doit connaître la liste d'appareils du destinataire (récupérée et mise en cache, rafraîchie).
- Le message est chiffré **pour chaque appareil** (une session par appareil).
- La boîte aux lettres hors ligne doit être **par appareil** (sinon un appareil qui relève la boîte prive les autres).
- Les accusés de réception deviennent ambigus : « lu » sur quel appareil ? → convention à trancher (proposition : lu = lu sur **au moins un** appareil).

⚠️ **Coût réseau** : N appareils = N fois le trafic pour un message direct. Pour du texte c'est négligeable ; **pour la voix et la vidéo, c'est inacceptable**. Décision : **les flux temps réel restent mono-appareil** — l'appel arrive sur tous les appareils (sonnerie), mais une fois décroché sur l'un d'eux, les autres cessent de sonner et ne reçoivent pas de média. C'est aussi le comportement attendu par l'utilisateur.

#### 6.2.5 Synchronisation de l'historique

Le point le plus coûteux, et celui qu'on peut le plus facilement **réduire**.

**Ce qu'on ne fera pas** : une synchronisation totale et automatique de tout l'historique entre appareils. Ça demande un protocole de réconciliation complet, du stockage, de la résolution de conflits.

**Ce qu'on fera** (par ordre de livraison) :

1. **Les messages arrivent sur tous les appareils connectés** — c'est gratuit une fois 6.2.4 fait. Un appareil allumé reçoit tout.
2. **Rattrapage à la reconnexion** : quand un appareil se reconnecte, il demande à **ses propres autres appareils** ce qu'il a manqué depuis son dernier horodatage. Échange direct, chiffré, appareil-à-appareil.
3. **Transfert d'historique à l'appairage** (optionnel, sur demande) : le nouvel appareil peut demander l'historique complet à l'appareil d'origine. Long, explicite, avec barre de progression.

Ce découpage permet de livrer **utile dès l'étape 1**, et de s'arrêter si les étapes suivantes se révèlent trop coûteuses.

#### 6.2.6 Ce qui doit rester au niveau compte

- **Amitiés** : on est ami avec un compte, pas un appareil.
- **Appartenance aux serveurs** : idem. L'op-log doit référencer des comptes.
- **Profil** (pseudo, avatar, décorations) : compte.
- **Réglages** : ⚠️ à trancher. Proposition : **par appareil** (le volume audio ou le périphérique choisi n'a pas de sens partagé), sauf les préférences de contenu (langue, thème) qui gagneraient à suivre — mais ça exige une synchro. **Décision : tout par appareil en 7.0**, synchro des préférences repoussée.

### 6.3 Impact par crate

| Crate | Impact | Nature |
|---|---|---|
| `accord-crypto` | **Fort** | Identité de compte vs appareil, dérivation, signature de la liste |
| `accord-proto` | **Fort** | Liste d'appareils, appairage, rattrapage — nouvelles variantes |
| `accord-transport` | **Moyen** | Sessions par appareil (l'invariant reste, il s'applique à l'appareil) |
| `accord-dht` | **Moyen** | Publication/résolution de la liste d'appareils |
| `accord-core` | **Fort** | Amitiés et groupes au niveau compte ; migration de la base |
| `accord-node` | **Fort** | Livraison multi-appareils, boîte aux lettres par appareil, rattrapage |
| `accord-api` | **Moyen** | Méthodes d'appairage, de listage, de révocation |
| `accord-voice` | **Faible** | Appels : sonner partout, décrocher sur un seul |
| Frontend | **Moyen** | Écrans d'appairage, gestion des appareils, indicateurs |

### 6.4 Découpage en lots

Cinq lots, dans cet ordre. **Chacun doit laisser l'application fonctionnelle** — pas de branche longue qui casse tout pendant six semaines.

#### Lot 1.A — Conception et écriture du protocole (aucune ligne de production)

🔒 **Rien ne commence avant que ce lot soit fini.** C'est la leçon du bloqueur B1 : deux heures de lecture de code ont évité des semaines de travail dans une impasse.

**Livrables**
1. Document de conception : modèle compte/appareil, structures filaires exactes, flux d'appairage, flux de livraison, flux de rattrapage.
2. **Analyse de menaces** écrite : que peut faire un attaquant qui contrôle le réseau ? un appareil révoqué ? un ami malveillant ? un pair qui ment sur la liste d'appareils ?
3. **Plan de migration** : comment un compte 6.2 existant devient un compte 7.0 avec un seul appareil, sans intervention de l'utilisateur et sans perte.
4. Décisions explicites sur les questions ouvertes (réglages, accusés de lecture, perte de tous les appareils).

**Effort** : 4 sessions. **Critère de fin** : le document permet à quelqu'un d'autre d'implémenter sans reposer les mêmes questions.

> ✅ **Fait le 2026-07-25** — [`docs/MULTI_DEVICE.md`](docs/MULTI_DEVICE.md).
> Les quatre questions ouvertes sont tranchées (réglages par appareil, « lu sur
> au moins un appareil », perte de tous les appareils, média temps réel
> mono-appareil), les structures filaires sont écrites avec leurs bornes, et
> l'analyse de menaces dit aussi ce qu'elle **ne** couvre pas.
>
> Une cinquième question est apparue en écrivant, et elle n'était dans aucune
> liste : si la phrase de récupération régénère la racine, le compteur de
> version repart à 1 et les pairs qui détiennent une version supérieure
> **ignoreront** la nouvelle liste — l'utilisateur resterait enfermé dehors.
> Correctif proposé : dériver la version de l'horodatage d'émission plutôt que
> d'un compteur stocké. À vérifier au lot 1.B.
>
> ✅ **Tranché le 2026-07-25** — `spake2` (RustCrypto), épinglé à la 0.4.0
> stable. Voir [`docs/MULTI_DEVICE.md` §4.1](docs/MULTI_DEVICE.md) pour le
> raisonnement complet et le contre-argument.
>
> L'argument décisif n'est pas la licence — les trois candidats passent
> `deny.toml` — mais l'arbre de dépendances : **toutes** celles de `spake2`
> (`curve25519-dalek`, `sha2`, `hkdf`, `hmac`, `subtle`, `rand_core`,
> `getrandom`) sont déjà dans le workspace, à la même version. Le crate
> s'ajoute seul. Vient ensuite le maintien (250 000 téléchargements récents
> contre 884 pour `pake-cpace` et 237 pour `cpace`, dont les dernières
> publications datent de 2023 et 2020) et la stabilité de la spécification
> (RFC 9382 figée, contre un brouillon IRTF encore mouvant pour CPace).

#### Lot 1.B — Identité de compte et d'appareil (sans réseau)

Fondations locales, invisibles.

**Tâches**
1. `accord-crypto` : type `AccountIdentity` (racine) et `DeviceIdentity`, dérivation, signature/vérification de la liste d'appareils.
2. `accord-proto` : structure filaire de la liste d'appareils, encodage/décodage, bornes anti-DoS (nombre max d'appareils : proposer **8**).
3. Migration : un compte existant devient un compte à un seul appareil, la graine actuelle devenant la **racine**, et un appareil neuf étant généré. ⚠️ **Attention** : si la graine actuelle devient à la fois racine et appareil, on n'a rien gagné. Il faut bien **générer une clé d'appareil distincte** dès la migration.
4. Stockage : tables `account`, `devices`, avec la migration versionnée de T0.2.
5. Tests : signature/vérification, version monotone (une liste plus ancienne est rejetée), révocation, borne du nombre d'appareils, migration d'un profil 6.2 réel.

**Effort** : 5 sessions. **Critère de fin** : l'application démarre normalement sur un profil migré, avec un appareil, et rien ne change pour l'utilisateur.

> ✅ **Fait le 2026-07-25.**
> - `accord-proto::device` — structure filaire `DeviceList` (compte, version
>   monotone, durée de vie, appareils, révocations, signature racine), bornes au
>   décodage (8 appareils, 32 révocations, nom ≤ 32 o), préfixe de domaine.
>   14 tests.
> - `accord-crypto::device` — `AccountIdentity` / `DeviceIdentity` en types
>   distincts (le compilateur relit ce qu'un humain finit par ne plus voir),
>   signature et vérification de la liste. 10 tests.
> - `accord-core::db::devices` — tables `local_device` et `device_lists`, créées
>   par la **première étape numérotée** du registre posé en 6.2. Le rejet d'une
>   version antérieure vit dans l'écriture, pas chez l'appelant.
> - `accord-node::device` — migration au démarrage : la graine existante devient
>   la racine, une clé d'appareil **distincte** est générée et persistée.
>
> **Piège filaire trouvé en chemin** : ajouter `RecordKind::DeviceList` aurait
> cassé les nœuds actuels. `RecordKind::from_u8` rendait une `Err`, donc un
> record de genre inconnu faisait échouer le décodage de **toute la réponse**
> de lookup qui le contenait — pas seulement du record en trop. Corrigé en
> 6.3 par une variante `Unknown(u8)` : le décodage préserve l'octet (la
> signature le couvre), et c'est le stockage qui refuse. Même discipline que le
> champ de capacités : **déployer la tolérance une version avant d'en avoir
> besoin**.
>
> **Reste au lot 1.B** : la clé d'appareil est créée et persistée mais **pas
> encore utilisée** — le transport passe toujours par la clé de compte. Le
> basculement est le lot 1.C, et l'anticiper changerait le comportement filaire
> sans que rien en face ne sache le lire.

#### Lot 1.C — Transport et résolution par appareil

> 🔴 **L'ordre initial de ce lot était faux — corrigé le 2026-07-25.** Il
> commençait par « le transport utilise la clé d'appareil ». Fait en premier,
> **cela couperait toutes les amitiés du réseau.**
>
> Aujourd'hui la clé statique de transport d'un pair **est** sa clé publique de
> compte, et le nœud s'en sert telle quelle comme identité : `is_friend(&static_pub)`,
> routage des messages, ré-annonce de profil. Présenter une clé d'appareil ferait
> donc de nous un inconnu pour chacun de nos amis — et symétriquement, nous ne
> saurions pas rattacher une clé d'appareil entrante à un compte, puisque la
> résolution (tâches 2 et 3) n'existerait pas encore.
>
> Même discipline que le champ de capacités et `RecordKind::Unknown` :
> **déployer la lecture une version avant l'écriture.** D'où deux phases.

**Phase 1 — savoir résoudre (6.4, sans rupture filaire)**

1. La DHT publie la **liste d'appareils** du compte, en plus du record d'identité.
2. Résolution et cache local des listes des contacts, avec durée de vie et rafraîchissement.
3. Le nœud **accepte** une session dont la clé statique est un appareil listé dans la liste signée d'un ami, et la rattache au compte. Il continue de **présenter sa clé de compte**.
4. Tests : liste publiée et relue ; liste périmée rafraîchie ; appareil révoqué refusé ; un pair antérieur ignore proprement le nouveau genre de record (déjà garanti par `Unknown(u8)`, 6.3).

**Phase 2 — basculer (7.0, jour de rupture assumé)**

5. Le transport **présente** la clé d'appareil.
6. Test : deux appareils du même compte joignent un tiers **sans s'évincer**.

> 💡 **`install_session` n'a rien à changer.** L'invariant « au plus une session
> directe par identité » est déjà écrit contre `peer_static`, la clé statique du
> pair. Donnez au transport des clés par appareil et il devient « par appareil »
> tout seul. Le bloqueur B1 n'a jamais été la règle d'éviction : c'est l'identité
> qu'on lui donne à manger.

**Effort** : 6 sessions. ⚠️ **Le test « deux appareils sans éviction » est le cœur du jalon.** S'il ne passe pas, tout est à revoir.

#### Lot 1.D — Appairage

**Tâches**
1. Protocole d'appairage (code court, canal dérivé, empreinte de vérification).
2. Ajout signé à la liste, publication.
3. Révocation d'un appareil, propagation.
4. Cadence limitée, expiration du code, usage unique.
5. Écrans : « Ajouter un appareil » (code + QR), « Mes appareils » (liste, dernière activité, révocation), confirmation d'empreinte des deux côtés.
6. Tests : appairage nominal ; code expiré ; code réutilisé ; empreinte non confirmée ; **tentative d'appairage par un tiers qui a intercepté le code** (doit échouer sans la confirmation d'empreinte).

**Effort** : 6 sessions..

#### Lot 1.E — Livraison multi-appareils et rattrapage ✅

**Fait.** Les cinq tâches sont livrées et vertes. Deux ajouts imprévus, tous deux
nés de l'implémentation : `DEVICE_FLAG_TRANSPORT_KEY`, sans quoi une liste dit
*qui* est du compte mais pas *où* écrire pendant toute la transition ; et la
**fusion** des listes d'appareils à la place du dernier-écrivain-gagne, sans quoi
une machine à la vue incomplète effaçait les autres appareils du compte partout,
et la liste se périmait vingt-quatre heures après le premier démarrage sans
recours. Voir `docs/MULTI_DEVICE.md` §3.2.1 et §5.

**Tâches**
1. Envoi d'un message direct vers **tous** les appareils du destinataire.
2. Boîte aux lettres hors ligne **par appareil**.
3. Appels : sonnerie sur tous, décrochage exclusif, arrêt des autres sonneries.
4. Rattrapage à la reconnexion entre ses propres appareils.
5. Accusés de lecture : convention « lu sur au moins un appareil ».
6. Tests : message reçu sur deux appareils ; un appareil hors ligne récupère à la reconnexion ; appel décroché sur un appareil arrête la sonnerie ailleurs ; **pas de duplication de message** après rattrapage.

**Effort** : 7 sessions.

### 6.5 Ce qui est explicitement hors périmètre de 7.0

À écrire noir sur blanc pour éviter la dérive :

- ❌ Synchronisation des **réglages** entre appareils.
- ❌ Transfert de l'**historique complet** à l'appairage (repoussé, étape 3 de 6.2.5).
- ❌ Vidéo/voix **multi-appareils simultanée** (un seul appareil actif par appel).
- ❌ Notifications **push** hors application — définitivement hors périmètre : elles exigent APNs/FCM, donc un serveur (partie 8).

### 6.6 Définition de fin du jalon 1

> **État au 2026-07-27**, après la 7.0.0 et les durcissements de la 7.1.0. Neuf
> cases sur dix sont tenues par un test qui échouerait si on cassait la
> fonctionnalité ; la case citée à côté de chacune est le nom de ce test, pas
> une intention.

- [x] Deux appareils du même compte sont joignables **en même temps** par un ami, prouvé par test e2e — `les_deux_appareils_tiennent_leur_session_en_meme_temps`.
- [x] Un message direct arrive sur les deux — `les_deux_appareils_dun_compte_recoivent_le_meme_message`.
- [x] Un appareil hors ligne rattrape à la reconnexion, sans doublon — `un_appareil_eteint_rattrape_a_son_retour`.
- [~] Un appel sonne partout, se décroche à un seul endroit. **Implémenté et
  couvert en unitaire** (`CallAction::SendTaken` → `CoreMsg::CallTaken`, machine
  à états dans `voice/calls.rs`), mais **aucun e2e à deux appareils vivants** ne
  l'exerce : `calls_e2e` reste à un appareil par compte. C'est la seule case du
  jalon dont la preuve est plus faible que l'énoncé.
- [x] L'appairage exige une confirmation d'empreinte des deux côtés — `Node::pairing_fingerprint` + `confirm`, `appairage_e2e`, et `confirmer_sans_empreinte_est_refuse` pour le refus.
- [x] La révocation empêche l'appareil de se connecter (chez les pairs à jour) — durci en 7.1.0 : un enregistrement refusé n'est plus rapporté comme un succès, et une liste datée du futur est rejetée (`SECURITY.md` 16 et 17).
- [x] Un profil 6.2 migre sans perte et sans intervention.
- [x] `SECURITY.md` documente la cohérence éventuelle de la révocation.
- [x] Gate vert, 7.0.0 publiée, `latest.json` conforme.
- [ ] 🔴 **Vérification sur appareil** : appairage réel entre deux machines,
  documenté. **Seule action que le code ne peut pas faire à sa place** — voir
  §1.4, rien en headless ne remplace deux vraies machines sur un vrai réseau.

---

## Partie 7 — Jalon 2 : post-quantique (8.0)

**Durée estimée : 1 mois.** ⚠️ Risque moyen — périmètre net, mais zone de code critique.

### 7.1 Le problème

Le chiffrement actuel (X25519) est solide contre les ordinateurs classiques. Il ne l'est **pas** contre un ordinateur quantique suffisamment grand.

Le risque concret ne se situe pas dans dix ans : il est **aujourd'hui**. Un adversaire peut capturer et stocker le trafic chiffré maintenant, pour le déchiffrer plus tard — c'est la stratégie dite *« récolter maintenant, déchiffrer plus tard »*. Pour un messager qui vend la confidentialité, c'est le scénario qui compte.

### 7.2 L'approche : hybride, jamais de remplacement

🔒 **Contrainte absolue** : on **n'enlève pas** X25519. On ajoute ML-KEM **à côté**, et on dérive la clé de session des **deux** secrets.

Raison : ML-KEM est jeune. Si une faiblesse y est découverte, l'hybride reste au moins aussi sûr que l'actuel. Un remplacement pur serait un pari ; l'hybride n'en est pas un.

```
Aujourd'hui :  clé_session = KDF(  ECDH(X25519)  )
Demain      :  clé_session = KDF(  ECDH(X25519) ‖ Encaps(ML-KEM)  )
```

Si l'un des deux tombe, l'autre tient. C'est le choix fait par Signal (PQXDH) et par TLS hybride.

### 7.3 Négociation et anti-repli

C'est **le** point délicat. Le champ de capacités a été déployé au jalon 0 (T0.1) précisément pour ce moment.

**Séquence**
1. L'initiateur annonce `PQ_HYBRID` dans ses capacités et joint son matériel ML-KEM.
2. Le répondeur, s'il sait faire, répond en hybride ; sinon, en classique.
3. La clé de session dérive du transcript **complet**, capacités comprises.

**Pourquoi c'est sûr** : un attaquant qui retire le bit `PQ_HYBRID` en vol pour forcer le repli **modifie le transcript**. Les deux pairs dérivent alors des clés différentes et le handshake échoue. Il peut empêcher la connexion (déni de service), mais **pas** obtenir une session dégradée qu'il pourrait déchiffrer plus tard.

⚠️ **C'est exactement l'attaque à tester en priorité.** Un test doit simuler la réécriture du bit et vérifier que le handshake échoue — pas qu'il « retombe gentiment » en classique.

### 7.4 Le coût

ML-KEM a des clés et des encapsulations **nettement plus grosses** que X25519 (de l'ordre du kilo-octet contre 32 octets).

⚠️ **Conséquence directe** : le HELLO passe potentiellement **au-dessus de la MTU** (`UDP_MTU = 1200`). Or le handshake est précisément le moment où la fragmentation est la plus fragile — le Lot G a montré à quel point un HELLO perdu coûte cher en reconnexion.

**Options à évaluer en conception**
- *(a)* Fragmenter le HELLO via le mécanisme existant. ⚠️ Risqué : le réassemblage précède l'établissement de session, donc il est non authentifié → surface d'attaque par épuisement mémoire.
- *(b)* Utiliser **ML-KEM-512** plutôt que 768/1024 pour rester compact, quitte à viser une marge de sécurité moindre mais suffisante en hybride.
- *(c)* Handshake en deux temps : premier échange classique, montée en hybride juste après. Plus complexe, mais garde le HELLO petit.

**Décision par défaut** : *(b)* si ça tient sous la MTU, sinon *(c)*. *(a)* est le dernier recours et exige des bornes anti-DoS strictes.

### 7.5 Découpage

#### Lot 2.A — Étude et choix de bibliothèque

**Tâches**
1. Évaluer les crates ML-KEM disponibles : maturité, audit, absence d'`unsafe` non justifié, licence compatible `cargo deny`, taille de la dépendance.
2. Mesurer les tailles réelles de clé/encapsulation pour 512 et 768.
3. Mesurer le coût CPU (le handshake doit rester imperceptible, y compris sur machine modeste).
4. Trancher l'option MTU (a/b/c) sur la base des mesures.
5. Documenter le choix et son raisonnement.

**Effort** : 3 sessions. **Critère de fin** : décision écrite, chiffrée, argumentée.

#### Lot 2.B — Handshake hybride

**Tâches**
1. Intégrer la bibliothèque, l'ajouter à `cargo deny`.
2. Étendre le handshake : matériel ML-KEM dans HELLO/WELCOME, encapsulation, dérivation combinée.
3. Négociation via le bit `PQ_HYBRID`.
4. Transcript incluant capacités **et** matériel PQ.
5. Tests :
   - hybride ↔ hybride → session établie, clé dérivée des deux secrets ;
   - hybride ↔ classique → repli propre, session établie ;
   - classique ↔ hybride → idem ;
   - **réécriture du bit de capacité → handshake rejeté** ;
   - **réécriture du matériel PQ → handshake rejeté** ;
   - taille du HELLO sous la MTU (test de non-régression sur la taille).

**Effort** : 6 sessions. ⚠️ **Zone la plus critique du projet.** Revue adversariale obligatoire avant merge.

#### Lot 2.C — Visibilité utilisateur

Le chiffrement post-quantique ne sert à rien si personne ne sait qu'il est là — et il ne faut pas non plus le survendre.

**Tâches**
1. Indicateur dans la carte de connexion : « chiffrement renforcé (post-quantique) » quand la session est hybride, mention neutre sinon.
2. Onglet Sécurité dans les réglages : état du chiffrement par contact, explication en langage clair.
3. Documentation `SECURITY.md` : ce qui est protégé, ce qui ne l'est pas, pourquoi l'hybride.

🔒 **Contrainte de formulation** : ne jamais écrire « incassable », « inviolable », « protégé pour toujours ». Écrire ce qui est vrai : *« résiste aux attaques par ordinateur quantique connues à ce jour »*.

**Effort** : 2 sessions.

#### Lot 2.D — Basculement

**Tâches**
1. Par défaut : accepter les deux, préférer l'hybride.
2. Réglage avancé (masqué) : exiger l'hybride, refuser le classique — pour les utilisateurs qui veulent la garantie.
3. Suivi : dans quelle proportion les sessions sont-elles hybrides ? (compteur local, jamais transmis.)

**Effort** : 1 session.

### 7.5.1 🔴 Ce que l'hybride ne couvre pas — et qui n'est planifié nulle part

Découvert en écrivant le lot 2.C, et il faut l'inscrire ici avant que « Accord
est post-quantique » ne devienne une phrase qu'on se répète.

**Seule la confidentialité devient post-quantique. L'authentification, non.**
Toutes les signatures restent Ed25519. Concrètement :

- ✅ **Le trafic enregistré aujourd'hui est protégé.** C'est le scénario qui
  motivait ce jalon — « récolter maintenant, déchiffrer plus tard » — et
  l'hybride y répond.
- ❌ **Un adversaire quantique EN LIGNE n'est pas arrêté.** Il forge une
  signature Ed25519 et se place au milieu d'un handshake neuf. Rien dans le
  protocole actuel ne l'en empêche.
- ❌ **Les données scellées vers une clé X25519 statique restent classiques** :
  clés d'époque de groupe, dépôts en boîte aux lettres hors ligne.

Fermer le premier point demande des **signatures** post-quantiques (ML-DSA,
FIPS 204), ce qui touche l'identité elle-même : le code ami, le `node_id`, la
signature des listes d'appareils et de tous les op-logs. C'est-à-dire un jalon
de la taille du jalon 1, pas un lot. **Il n'est pas planifié.**

🔒 La règle de formulation du lot 2.C s'applique d'abord à nous : ne jamais
écrire « Accord est post-quantique » sans dire de quoi. Écrire « la
confidentialité des sessions résiste aux attaques par ordinateur quantique
connues à ce jour ».

### 7.6 Définition de fin du jalon 2

**Terminé le 2026-07-28.** Chaque case porte le test ou la mesure qui la tient ;
le tableau de correspondance complet est dans `docs/AUDIT_BRIEF.md`.

- [x] Deux nœuds 8.0 négocient l'hybride ; la clé dérive des deux secrets —
      `hybrid_handshake_derives_a_key_from_both_secrets`.
- [x] Un nœud 8.0 et un nœud 7.0 se parlent en classique, sans friction —
      `classic_transcript_is_unchanged_by_this_milestone` : sans matériel PQ, la
      dérivation est **octet pour octet** celle d'avant le jalon.
- [x] **Toute altération du transcript (capacités ou matériel PQ) fait échouer le
      handshake** — 4 tests dédiés, dont
      `stripping_the_pq_capability_bit_breaks_the_handshake` et
      `unsolicited_pq_ciphertext_is_refused`.
- [x] Le HELLO reste sous la MTU — `hybrid_hello_and_welcome_stay_under_the_udp_mtu`.
      ⚠️ Durci le 2026-07-28 : le test n'asserte plus « ≤ MTU » mais « ≤ taille
      mesurée + tolérance nommée ». À 984 o pour une MTU de 1 200, il ne reste
      que 216 o, et un plafond posé à la MTU aurait laissé un futur champ les
      manger en silence.
- [x] Le surcoût CPU du handshake est mesuré et documenté —
      `docs/PERFORMANCE.md` §4. **+54,5 µs (+21 %)**, et c'est le chiffre le
      moins intéressant des deux : le coût *filaire* fait ×5,5 (HELLO 180 → 984 o).
- [x] `SECURITY.md` à jour, sans promesse excessive — items 20 à 22, et le
      CHANGELOG 8.0.0 ouvre sur ce que l'hybride **ne** protège pas.
- [x] Gate vert, 8.0.0 publiée — le 2026-07-28, `latest.json` à 9 clés signées.

---

## Partie 8 — Jalon 3 : mobile — 🔴 ABANDONNÉ (2026-07-26)

**Décision prise, et à ne pas rouvrir sans nouvel élément.** Le jalon mobile est
retiré de la feuille de route. Ce qui suit explique pourquoi, parce qu'une
décision d'abandon sans raison écrite se fait rouvrir tous les six mois.

### 8.1 Ce n'est pas une impossibilité technique — c'est un prix

Il serait faux d'écrire « on ne peut pas faire une application mobile ». On peut.
Le cœur Rust compile pour `aarch64-apple-ios` et Android, Tauri 2 supporte les
deux, et la plupart des difficultés (WebCodecs absent du webview iOS, capture
audio à refaire en natif, partage d'écran via ReplayKit, SQLCipher à croiser)
sont chères mais franchissables.

**Le blocage est ailleurs, et il est frontal.** iOS suspend une application
quelques secondes après son passage en arrière-plan, et aucun droit ne permet de
garder un socket UDP vivant — le mode `voip` qui le permettait a disparu en
iOS 13. Un téléphone ne peut donc pas être un nœud P2P joignable. Le seul moyen
de réveiller l'application est une notification poussée par APNs.

Or **l'authentification APNs est par application** : une clé d'équipe. Pour
qu'un pair envoie un réveil, il faudrait que cette clé soit dans chaque client,
donc publiée. Il faut nécessairement un composant qui la détienne.

C'est-à-dire **un serveur**. Le principe 2.5 dit « zéro serveur ». Les deux ne
tiennent pas ensemble.

### 8.2 Pourquoi abandonner plutôt que payer

Le compromis existe — un relais de réveil minimal, auto-hébergeable, qui
n'enverrait qu'un « quelque chose t'attend » sans contenu. Il a été étudié
(ancienne §8.2, options A/B/C). Il reste que :

- **la promesse du produit change**. « Aucun serveur, jamais » devient « aucun
  serveur, sauf pour le mobile » — et la nuance ne survit pas à sa première
  reformulation par un tiers ;
- **la dépendance est irréductible**. Apple et Google voient *qu'il se passe
  quelque chose*, et quand, même sans le contenu. Ce sont exactement les
  métadonnées que ce projet existe pour ne pas produire ;
- **la revue de l'App Store est un risque non technique et réel**. La règle 1.2
  exige, pour toute application à contenu généré par les utilisateurs, un moyen
  de signaler et une modération publiée. Une messagerie chiffrée de bout en bout
  sans serveur n'a rien à modérer et rien à montrer à un examinateur. Des
  applications ont été refusées là-dessus. Le blocage existe déjà dans Accord ;
  le signalement — vers qui ? — n'a pas de réponse dans ce modèle.

### 8.3 Ce que le travail déjà fait garde comme valeur

Le multi-appareil (jalon 1) avait été justifié en partie par le mobile. Il garde
tout son sens sans lui : deux ordinateurs sont le cas d'usage quotidien, et la
boîte aux lettres par appareil comme le rattrapage entre ses propres machines
décrivent exactement un appareil souvent hors ligne. Si la décision devait être
rouverte un jour, le socle serait là.

### 8.4 Ce qui remplace ce jalon

Rien de nouveau n'est ajouté à sa place : les mois libérés vont au jalon 4
(échelle et robustesse), qui était déjà un fil de fond, et au renforcement du
bureau. Une version web légère en lecture seule reste une piste, jamais évaluée.

---

## Partie 9 — Jalon 4 : échelle et robustesse (10.0)

> **Livré le 2026-07-27 dans la 7.1.0.** Les cinq sections sont faites.
> §9.1 point 1 (vidéo sélective) ; §9.2 ; §9.3 y compris le changement
> d'adresse en session ; §9.4 les quatre items ; §9.5 les cinq points.
> Restent hors de portée headless : la preuve que l'image arrive (WebCodecs,
> §1.4) et l'appairage sur deux vraies machines.
>
> §9.1 points 2 (couches de qualité) et 3 (relais sélectif) ne sont pas faits :
> le 3 « mérite sa propre étude » selon cette même section, et le 2 attend une
> mesure sur un vrai appel à 10.

**Fil continu**, absorbé entre les autres jalons. Alimente les versions mineures.

### 9.1 Voix et vidéo à plus grande échelle

**Limite actuelle** : full mesh, 10 participants. À 10 personnes avec caméra, chacun envoie 9 flux et en reçoit 9 — c'est intenable.

**Pistes**, par ordre de complexité :

1. **Vidéo sélective** : ne transmettre la vidéo qu'aux participants qui l'affichent réellement (ceux dont la fenêtre est visible). Gain immédiat, sans changement d'architecture.
2. **Couches de qualité** : émettre deux résolutions, les pairs choisissent. Coût CPU en encodage, gain en bande passante.
3. **Relais sélectif entre pairs** : un participant bien connecté relaie pour les autres. Change le modèle de confiance ⚠️ — le relayeur voit-il le contenu ? (Non s'il est chiffré bout en bout, mais il voit les métadonnées.)

**Recommandation** : 1 puis 2. La 3 mérite sa propre étude.

### 9.2 Historiques volumineux

- Le fenêtrage de la liste de messages existe déjà.
- À surveiller : temps d'ouverture d'une conversation à 100 000 messages, taille de la base, coût de la recherche.
- Piste : index de recherche local incrémental plutôt que scan.

### 9.3 Robustesse réseau

- Télémétrie locale agrégée (dette D4) : historique de la qualité des liens, taux de repli relais, échecs de traversée NAT. **Jamais transmise**, consultable par l'utilisateur, exportable pour un rapport de bug.
- Campagnes de chaos réseau élargies : pertes, réordonnancements, coupures franches, changements d'adresse en cours de session.
- Cible : **30 exécutions consécutives sans échec** sur les scénarios de reconnexion (comme le Lot G).

### 9.4 Modération et sécurité communautaire

Un serveur qui grandit a besoin d'outils. Aujourd'hui : rôles, permissions, expulsion, bannissement, sourdine vocale, timeout.

**À ajouter, par valeur** :
- Filtre de mots-clés par serveur (local, configurable).
- Mode « vérification à l'entrée » (les nouveaux membres attendent une validation).
- Journal d'audit exportable.
- Blocage au niveau compte propagé à tous les appareils (dépend du jalon 1).

### 9.5 Accessibilité

Chantier continu, jamais « fini ».

**Audité le 2026-07-26. Quatre points sur cinq étaient déjà faits** — la liste
ci-dessous décrivait un chantier qui, pour l'essentiel, avait été mené sans être
enregistré.

- [x] Navigation clavier complète, y compris les surfaces récentes. La palette
      implémente le motif combobox complet (`listbox`/`option`,
      `aria-activedescendant`, flèches, Échap, région live) ; la grille vidéo
      étiquette ses groupes et ses boutons d'épinglage.
- [x] Étiquettes de lecteur d'écran sur tous les contrôles. **319 boutons sur
      319** portent un nom accessible (compté sur les sources, vérifié sur
      l'arbre rendu).
- [x] Contrastes vérifiés sur tous les thèmes intégrés — `themeContrast.test.ts`
      couvre 6 surfaces × 5 jetons de texte, plus le verre dépoli, les pastilles
      sémantiques et la coloration syntaxique.
- [x] Respect de `prefers-reduced-motion` : plancher universel (`*`) pour la
      préférence système ET pour le réglage in-app `data-motion='reduce'`, plus
      des règles ciblées sur les couches lourdes (`.profile-frame`,
      `.theme-atmosphere`).
- [x] Cibles tactiles suffisantes — **audité le 2026-07-28**, 10 manquements
      trouvés et corrigés, gardés par `e2e/cibles-tactiles.spec.ts` (7 tests,
      21 surfaces, seuil WCAG 2.2 SC 2.5.8 de 24×24 px).
      Les pires : les deux poignées de redimensionnement, à **7 px et 6 px de
      large**. Corrigées en zone de clic étendue (marges négatives), jamais en
      grossissement visuel : l'encombrement dans la grille est inchangé au
      pixel près.
      ⚠️ L'exception « espacement » de la norme est **délibérément non
      appliquée**, et c'est la décision qui compte ici : prise à la lettre, elle
      excusait **7 des 8** manquements du banc — dont la poignée de 6 px, dont
      le centre tombait dans un trou de la colonne voisine. Chacun coûtait une
      ligne de CSS. Une norme respectée en excusant tout n'aurait rien changé
      pour personne.

**Ce qui manquait n'était pas l'accessibilité, c'était ce qui la maintient.**
`e2e/a11y.spec.ts` échoue désormais sur tout contrôle que le navigateur ne sait
pas nommer, sur six surfaces, et vérifie qu'il sait encore échouer.

⚠️ Non couvert par cette garde : la palette de commandes, absente du banc de
démonstration (elle a ses propres tests jsdom).

**Effort** : 1 session par version, en continu.

---

## Partie 10 — Chantiers transverses

Ces chantiers ne portent pas de version : ils traversent tous les jalons.

### 10.1 Internationalisation

**État au 2026-07-27** : ✅ **les 10 sont livrées** — fr, en, es, pt, de, ru, zh,
hi, bn, ar. Le tableau ci-dessous garde la trace de ce que chacune coûtait ; il
se lit désormais au passé.

| Code | Langue | Locale | Sens | Difficulté |
|---|---|---|---|---|
| `zh` | Chinois simplifié | `zh-CN` | LTR | Moyenne (densité, polices) |
| `hi` | Hindi | `hi-IN` | LTR | Moyenne (rendu devanagari) |
| `ar` | Arabe | `ar-SA` | **RTL** | **Élevée** |
| `pt` | Portugais (BR) | `pt-BR` | LTR | Faible |
| `ru` | Russe | `ru-RU` | LTR | Faible (longueur) |
| `bn` | Bengali | `bn-BD` | LTR | Moyenne |
| `de` | Allemand | `de-DE` | LTR | Faible (longueur) |

⚠️ **L'arabe est le seul vrai piège.** Le droite-à-gauche demande plus qu'une traduction : direction du document, symétrie des marges, retournement des chevrons et flèches, alignement des listes. Le lot en cours ne demande que le **minimum honnête** (`dir="rtl"` sur la racine) avec obligation de **rapporter franchement** l'état visuel réel. Un vrai support RTL est un chantier à part entière (estimé 3 sessions).

**Garde-fou en place** : `parity.test.ts` confronte chaque dictionnaire au français (clés, valeurs vides, placeholders). Toute langue future est couverte automatiquement. 🔒 **Ne jamais contourner ce test.**

**Après les 10** : les langues suivantes se choisissent sur la base des utilisateurs réels, pas d'une liste théorique.

### 10.2 Performance

**Budgets** (repris de 1.2, à faire respecter à chaque livraison) :

| Cible | Budget | Note |
|---|---|---|
| Chunk JS initial | < 140 ko gzip | **136,0** au 2026-07-27 — tenu par `scripts/check-bundle-budget.mjs`, dans le gate |
| CSS | < 50 ko gzip | **33,2** au 2026-07-27 — pas de dérive (34,5 avant). ⚠️ Le budget porte sur la feuille du **premier écran** (`index-*.css`), pas sur la somme des dix fichiers : les neuf autres sont des chunks paresseux (décorations, profils animés, onglet Apparence) et les additionner punirait précisément le découpage qu'on veut. Garde automatique ajoutée le 2026-07-27. |
| Démarrage à froid → interface utilisable | < 2 s | ⚠️ toujours pas mesuré, et **pas mesurable par les tests actuels** : Playwright chronomètre le webview seul, alors que le budget porte sur l'ensemble démarrage du nœud + déverrouillage + premier écran. Une mesure du webview seul serait un chiffre flatteur et faux. |
| Ouverture d'une conversation (10 k messages) | < 300 ms | ✅ **0,94 ms** — mesuré depuis la 7.1 (`cargo bench -p accord-node --bench history`, `docs/PERFORMANCE.md` §1.1). La ligne disait encore « à instrumenter » alors que le banc existait : corrigé le 2026-07-27. |
| Latence bout en bout d'un message (LAN) | < 100 ms | déjà bon |
| CPU au repos, nœud connecté | < 2 % | ⚠️ toujours pas mesuré : demande un nœud vivant observé dans la durée, donc un banc à part et une machine au repos — pas un test. |
| Mémoire, 5 serveurs, 20 conversations | < 400 Mo | ⚠️ toujours pas mesuré. Le banc du jalon 6 mesure la mémoire de l'état de groupe matérialisé, ce qui en est une part et pas le tout. |

**Trois règles**
1. Animer uniquement des propriétés composables (`transform`, `opacity`) — jamais des propriétés de mise en page.
2. Charger paresseusement tout ce qui n'est pas sur le chemin du premier écran.
3. Mesurer avant d'optimiser ; consigner la mesure dans le CHANGELOG quand c'est un argument de version.

### 10.3 Sécurité

**Rythme** : une revue de sécurité par jalon majeur, plus une revue **adversariale dédiée** pour tout ce qui touche handshake, identité ou permissions.

**Permanent**
- Fuzzing continu (8 cibles), campagnes nocturnes, ajout d'une cible par nouvelle surface de décodage (la liste d'appareils du jalon 1 en aura besoin).
- `cargo deny` + `cargo audit` dans le gate — jamais désactivés « temporairement ».
- Toute dépendance nouvelle est justifiée par écrit : pourquoi elle, sa maturité, sa surface `unsafe`.

**Par jalon**
- Jalon 1 : menaces sur l'appairage et la révocation, propagation malveillante d'une liste d'appareils.
- Jalon 2 : attaque par repli, malléabilité du transcript, épuisement mémoire à la fragmentation du HELLO.
- ~~Jalon 3~~ : sans objet, le mobile est abandonné — et la fuite de métadonnées via le push est précisément l'une des raisons de l'abandon (partie 8).

**`SECURITY.md`** est mis à jour **à chaque jalon**, pas à la fin. Il doit dire ce qui n'est **pas** protégé aussi clairement que ce qui l'est.

### 10.4 Documentation

| Document | Public | Rythme |
|---|---|---|
| `CHANGELOG.md` | Utilisateur | Chaque version — écrit pour l'usage, pas pour les commits |
| `SECURITY.md` | Utilisateur averti | Chaque jalon |
| `docs/PROTOCOL.md` | Contributeur | À jour à chaque changement filaire |
| `docs/DEVELOPMENT.md` | Contributeur | Environnement, gate, pièges connus (dont B3) |
| `docs/MULTI_DEVICE.md` | Contributeur | Conception du multi-appareil (lot 1.A) |
| `docs/VOICE_CALLS.md` | Contributeur | À jour au jalon 1 (appels multi-appareils) |
| `ROADMAP.md` | Interne | Relu à chaque fin de jalon |

🔒 **Le CHANGELOG s'écrit pour l'utilisateur.** « Ce qui change pour toi », pas « ce qu'on a modifié ». Un utilisateur qui lit une note de version doit comprendre sans connaître le code.

### 10.5 Qualité de l'intégration continue

- Garder le miroir exact entre `ci.sh` et `ci.yml`. Toute divergence est un piège futur.
- Surveiller la durée (seuil 15 min).
- ⚠️ **Flakes d'infrastructure connus** : le runner Windows a « perdu la communication avec le serveur » deux fois de suite en juillet. Remède : `gh run rerun <id> --failed`. **Ne jamais conclure à un bug du code sans avoir lu l'annotation d'échec.**
- Ajouter au fil de l'eau : couverture de code (indicatif), taille du bundle en garde-fou automatique.

### 10.6 Journalisation exploitable par l'utilisateur

**Le problème.** Quand quelque chose ne marche pas chez toi, il n'existe
aujourd'hui aucun moyen de savoir ce qui s'est passé. Le diagnostic se fait en
te demandant de décrire ce que tu as vu — c'est ainsi qu'ont été trouvés le
hang de bannière, la panne d'envoi de la 3.0 et le flake de reconnexion, à
chaque fois au prix de plusieurs allers-retours.

**Audité le 2026-07-26.** L'instrumentation existe ; ce qui manque, c'est
d'y accéder.

- [x] `tracing` est posé dans tout le Rust (nœud, transport, DHT, voix), avec
      des niveaux cohérents.
- [x] Une sortie fichier existe dans `app/src-tauri/src/lib.rs`… **derrière la
      variable d'environnement `ACCORD_LOG_FILE`**. Pour une application
      lancée depuis le Finder ou le menu Démarrer, c'est hors d'atteinte.
⚠️ Les quatre lignes qui suivent étaient l'énoncé des **défauts** trouvés par
l'audit, écrits sous forme de cases — une erreur de forme : une case cochée y
signifie « le défaut est corrigé », pas « le défaut existe ». Elles sont
reformulées ici en constats de correction, datés.

- [x] **Fichier par défaut, sans variable d'environnement** — corrigé.
      `journal.rs::attacher_sous` ouvre `<app_data>/logs` au démarrage, et
      tamponne les lignes émises avant que ce dossier soit connu (un démarrage
      qui échoue *avant* `setup` laisse quand même une trace).
- [x] **Plus de troncature au démarrage** — corrigé. Le journal courant est
      décalé en `.1` à l'ouverture, avec rotation par taille en cours de route.
      Un seul historique conservé, à dessein : ce qu'on cherche après un
      incident, c'est l'exécution qui vient de finir.
- [x] **Le frontend écrit dans le même fichier** — corrigé. `app/src/lib/journal.ts`
      appelle la commande `journal_ui` ; une seule horloge, un seul fichier.
- [x] **Niveau réglable dans l'interface, à chaud** — fait
      (`journal_niveau`, sélecteur dans `NetworkPanel`). Un niveau inconnu rend
      `false` au lieu de passer pour « inchangé ».
- [x] **Accès depuis l'interface** — complété le 2026-07-28. Le panneau Réseau
      porte désormais « Ouvrir le dossier » (`journal_reveler`) et « Exporter
      rapport + journal » (`journal_exporter`, qui écrit
      `accord-diagnostic.txt` puis le montre), en plus du chemin copiable.
      L'export est un fichier et non le presse-papiers : le journal pèse
      jusqu'à 10 Mio, que personne ne colle dans un ticket.
      🔒 Le plugin `tauri-plugin-opener` est **volontairement absent de
      `capabilities/default.json`** : la webview ne peut pas l'invoquer. Les
      deux commandes ne prennent aucun chemin de l'appelant — un opener
      joignable avec un chemin arbitraire depuis la webview serait une
      primitive « ouvrir n'importe quoi » offerte à toute injection de contenu
      (aperçu de lien, nom de fichier reçu, message rendu).

**Périmètre proposé**, par valeur décroissante :

1. **Fichier par défaut, avec rotation.** Un journal dans le dossier de
   données de l'application, sans variable d'environnement. Rotation par
   taille avec au moins un fichier précédent conservé — sans quoi le
   redémarrage efface la preuve. Plafond total ferme (~10 Mio) : un journal
   qui remplit le disque est un bug, pas un outil.
2. **Le frontend écrit dans le même fichier.** Une commande Tauri que le
   client TypeScript appelle pour ses erreurs, ses rejets de promesse et ses
   transitions d'état importantes. Un seul fichier, une seule horloge : deux
   journaux qu'il faut recoller à la main n'aident personne.
3. **Un bouton « ouvrir le dossier des logs »** dans les réglages, à côté du
   rapport de diagnostic (§9.3 D4), et un export qui joint le journal au
   rapport.
4. **Niveau réglable dans l'interface**, sans variable d'environnement et sans
   relancer : `info` par défaut, `debug` quand on cherche quelque chose.

🔒 **La contrainte qui décide de la forme.** Un journal n'a de valeur que s'il
peut être envoyé à quelqu'un, et il ne peut être envoyé que s'il est sûr à
partager. Donc, exactement comme `diagnostics.report` : **jamais de contenu de
message, jamais de clé, jamais de code ami, jamais d'adresse IP d'un ami.** Le
travail n'est pas seulement d'écrire un fichier — c'est de relire chaque
`tracing::` existant avec cette question, parce que plusieurs journalisent
aujourd'hui des identifiants de pair en pensant les journaliser pour un
développeur sur sa propre machine.

Le garde-fou doit être un test, pas une intention : une suite qui fait tourner
un nœud avec un ami et une conversation, puis cherche dans le journal produit
les valeurs interdites — le même motif que
`diagnostics::tests_rapport::le_rapport_ne_porte_ni_cle_ni_adresse_d_ami`, qui
attrape un champ ajouté plus tard sans que personne y pense.

**Livré le 2026-07-26** — les quatre points, avec un écart assumé sur le 3.

- [x] Fichier par défaut dans `<app_data>/logs/accord.log`, sans variable
      d'environnement. Rotation au démarrage (`accord.log.1` conservé) **et** en
      cours de route au-delà de 5 Mio ; empreinte bornée à deux fichiers.
- [x] Tampon d'amorçage : `tracing` démarre avant que Tauri ne connaisse le
      dossier de données, donc les premières lignes sont gardées en mémoire puis
      versées à l'ouverture. Sans lui, l'amorçage — là où les démarrages ratés
      se produisent — aurait été la seule partie non journalisée.
- [x] Le frontend écrit dans le même fichier (`journal_ui`, cible `accord_ui`),
      avec les rejets de promesse et les erreurs non traitées branchés avant le
      rendu.
- [x] Niveau réglable à chaud (`info` / `debug`), sans redémarrer.
- [~] **Écart** : pas de bouton « ouvrir le dossier ». Aucun plugin Tauri
      d'ouverture n'est installé, et en ajouter un — ou lancer un processus
      système depuis l'app — élargit la surface pour un confort. Le panneau
      réseau affiche le chemin avec un bouton « copier », à coller dans le
      Finder. Le journal ne part pas non plus dans le rapport de diagnostic :
      5 Mio dans un presse-papiers n'a pas de sens, c'est un fichier à joindre.

⚠️ **La relecture de confidentialité des `tracing::` existants reste à faire.**
Le module écrit ce qu'on lui donne ; plusieurs appels journalisent aujourd'hui
des identifiants de pair en supposant qu'un développeur les lit sur sa propre
machine. Tant que cette passe n'est pas faite, le journal est utile mais **pas
encore garanti sûr à partager** — c'est écrit dans l'aide affichée à
l'utilisateur, qui promet aujourd'hui plus que ce qui est vérifié.

---

## Partie 11 — Méthode de travail

Travailler seul sur un projet de cette taille impose une discipline que le travail à plusieurs impose naturellement. Ces règles remplacent la revue par les pairs.

### 11.1 L'ordre d'attaque d'un chantier

1. **Lire le code avant de concevoir.** Deux heures de lecture ont évité des semaines d'impasse sur le multi-appareil. La question à se poser en premier est toujours : *« qu'est-ce qui, dans le code existant, rend ça impossible ? »*
2. **Concevoir par écrit avant de coder** dès qu'un chantier dépasse trois sessions. Le document de conception n'est pas de la bureaucratie : c'est ce qui permet de reprendre le fil une semaine plus tard.
3. **Découper en lots qui laissent l'application fonctionnelle.** Jamais de branche longue qui casse tout pendant six semaines.
4. **Une couche à la fois, testée avant la suivante.** Protocole → nœud → API → interface. Chaque couche verte avant de passer à la suivante. C'est ce qui a permis de livrer le partage d'écran et la vidéo sans dérive.
5. **Commiter souvent**, avec un message qui explique le *pourquoi*. Le futur lecteur, c'est soi-même dans trois mois.

### 11.2 Se relire comme un adversaire

Sans relecteur, il faut jouer les deux rôles. Après avoir écrit une zone sensible, changer de casquette et chercher activement la faille :

- **Que fait un attaquant** avec ce champ, ce message, cette permission ?
- **Que se passe-t-il si le pair ment** sur ce qu'il envoie ?
- **Qu'arrive-t-il si ça échoue au milieu** — connexion coupée, processus tué, disque plein ?
- **Est-ce que ce test prouve vraiment ce que je crois ?** Un test qui passe pour la mauvaise raison est pire qu'aucun test.

🔒 **Obligatoire** avant tout merge touchant : handshake, identité, clés, permissions.

### 11.3 Ne pas se croire sur parole

La règle la plus utile de toute cette méthode : **vérifier ses propres affirmations avant de les énoncer**.

Précédents dans ce projet :
- une extraction d'identifiants annoncée « 63 perdus » était un bug de mon extraction, pas du code livré ;
- un `window.confirm` « oublié » était en réalité hors périmètre, sur un autre chemin ;
- un rapport annonçant un travail terminé s'est révélé exact — mais seulement après lecture du diff, pas du rapport.

**Méthode** : quand un chiffre ou une affirmation est important, le regarder deux fois, par deux chemins différents si possible.

### 11.4 Que faire quand on est bloqué

Par ordre :

1. **Reproduire le problème isolément** — le plus petit cas qui échoue.
2. **Chercher la cause racine, pas le symptôme.** Un correctif qui masque une cause revient toujours. *(Précédent : le flake de reconnexion avait quatre causes distinctes ; en corriger une seule laissait le bug.)*
3. **Vérifier si l'environnement ment.** Le build local cassé par un attribut macOS a coûté du temps parce que le premier réflexe a été de suspecter le code.
4. **Écrire ce qu'on a éliminé.** Ça évite de refaire deux fois la même vérification.
5. **Savoir s'arrêter.** Quand un chantier révèle un bloqueur structurel, le bon geste est de documenter et de changer de sujet — pas de forcer.

### 11.5 Ce qui ne se fait jamais dans la précipitation

🔒 Ces gestes ne se font pas en fin de session, fatigué, pour « boucler » :

- toucher au handshake ou aux clés ;
- modifier un invariant de transport ;
- supprimer un test qui gêne ;
- publier une release dont le gate n'est pas vert ;
- supprimer une branche ou un worktree sans avoir vérifié son contenu ;
- retirer un lint du gate.

Il vaut toujours mieux reprendre demain.

### 11.6 Tâches de fond

Le seul parallélisme disponible en solo, à exploiter systématiquement : pendant qu'une CI, un build ou une campagne de fuzzing tourne, avancer sur autre chose — écrire un test, préparer le CHANGELOG, lire le code du chantier suivant.

⚠️ **Sauf** pendant une release : entre le tag et la publication, rester concentré dessus. Une release à moitié surveillée est une release qui reste en brouillon.

## Partie 12 — Registre des risques

| # | Risque | Probabilité | Impact | Atténuation |
|---|---|---|---|---|
| R1 | Le multi-appareil se révèle plus coûteux que 2 mois | **Élevée** | Élevé | Lot 1.A de conception **avant** tout code ; découpage en 5 lots livrables séparément ; possibilité de s'arrêter après 1.C |
| R2 | Le handshake hybride dépasse la MTU | Moyenne | Moyen | Trois options préparées (7.4) ; mesure avant décision |
| R3 | Attaque par repli non détectée sur la négociation PQ | Faible | **Critique** | Transcript authentifié ; test d'altération obligatoire ; revue adversariale |
| R4 | Le mobile se révèle infaisable proprement | Moyenne | Élevé | Lot 3.A de faisabilité qui **peut conclure non** ; plan de repli documenté |
| R5 | Refus des magasins d'applications | Moyenne | Moyen | Étudier tôt ; distribution directe en repli |
| R6 | Régression silencieuse en release (type debug_assert) | Faible | **Critique** | Lint en place ; e2e transport **en release** dans le gate |
| R7 | Travail perdu par écrasement dans un arbre partagé | Faible (corrigée) | Moyen | Worktree pour tout chantier parallèle |
| R8 | Build local durablement empêché (B3) | Moyenne | Faible | La CI produit tout ; script de nettoyage |
| R9 | Dérive du bundle au fil des fonctionnalités | Élevée | Faible | Budget en garde-fou automatique |
| R10 | Perte de la clé de signature de l'updater | Très faible | **Critique** | Sauvegarde hors ligne ⚠️ **à vérifier qu'elle existe** |
| R11 | Un dépendance abandonnée (ex. `mdns-sd`) | Moyenne | Moyen | Supervision déjà en place ; suivre l'amont ; prévoir un remplacement |
| R12 | L'op-log diverge sur un serveur très actif | Faible | Élevé | `op_id` adressé par contenu déjà en place ; chaos tests à étendre |

### 12.1 Le risque R10 mérite une action immédiate

La clé de signature de l'updater (`~/.tauri/accord-updater.key`) est **le point de défaillance unique** du projet. Sans elle :
- impossible de publier une mise à jour que les clients existants accepteront ;
- tous les utilisateurs installés devraient réinstaller manuellement.

🔒 **Action** : vérifier qu'une copie hors ligne existe, dans un endroit différent de la machine de développement. Si ce n'est pas le cas, c'est la tâche la plus urgente de toute cette feuille de route — avant toute fonctionnalité.

---

## Partie 13 — Parcours à ne jamais casser

Ces parcours sont le contrat implicite avec l'utilisateur. Une régression sur l'un d'eux est un incident, quelle que soit la fonctionnalité qui l'a causée.

Chacun doit rester vérifiable — idéalement par un test, sinon par une passe manuelle documentée avant chaque release majeure.

### 13.1 Premier lancement

1. L'application s'ouvre sur un écran clair, sans jargon.
2. Créer une identité demande **une phrase de passe et rien d'autre**.
3. La phrase de récupération est affichée **une seule fois**, avec un avertissement explicite, une possibilité de copier et de télécharger, et une vérification (retaper un mot).
4. Choisir un pseudo est proposé, et **peut être remis à plus tard**.
5. On arrive sur une interface fonctionnelle, même sans aucun ami.

⚠️ **Point de fragilité** : la preuve de travail à la création prend quelques secondes. L'attente doit être expliquée, jamais silencieuse.

### 13.2 Déverrouillage

1. Le coffre s'ouvre avec la phrase de passe.
2. **La touche Entrée valide.** *(Précédent : ce n'était pas le cas, il fallait la souris — signalé par l'utilisateur, corrigé en 4.4.)*
3. Une phrase erronée donne un message clair, sans bloquer ni compter les essais de façon punitive.
4. Le sélecteur de comptes permet de changer de profil sans redémarrer.

### 13.3 Se lier à quelqu'un

1. Mon code ami est visible, copiable, partageable en QR.
2. Coller un code d'ami trouve la personne — **y compris derrière un NAT**, y compris au premier contact, sans configuration réseau.
3. La demande est explicite des deux côtés ; personne n'est ajouté sans accord.
4. Une seule demande en attente par personne (anti-spam).

⚠️ **Le parcours le plus fragile du produit** : il dépend de la DHT, des nœuds d'amorçage, de la traversée de NAT. C'est aussi le premier que rencontre un nouvel utilisateur. **Toute régression ici est critique** — sans ce parcours, l'application est inutilisable.

### 13.4 Écrire et recevoir

1. Le message part, s'affiche immédiatement, et son état est visible (en cours, envoyé, échoué).
2. Un échec propose de réessayer.
3. Le destinataire hors ligne reçoit à son retour (boîte aux lettres, 7 jours).
4. Les non-lus sont exacts. Un compteur faux détruit la confiance dans le produit.
5. L'historique se recharge à l'ouverture, sans perte.

### 13.5 Parler

1. Rejoindre un salon vocal connecte en quelques secondes.
2. Le micro se coupe **instantanément** au clic — la latence sur ce bouton est inacceptable, c'est une question de confiance.
3. L'indicateur « parle » correspond à la réalité.
4. Quitter libère micro et réseau proprement.
5. Un appel entrant sonne, s'accepte, se refuse.

### 13.6 Rester connecté

1. Après une coupure réseau, la connexion revient **seule**.
2. Après un redémarrage de l'application, on retrouve tout.
3. Après un changement de réseau (Wi-Fi → 4G), la session se rétablit.
4. Un pair qui redémarre redevient joignable sans intervention.

⚠️ **C'est le domaine du Lot G.** Quatre causes racines y ont été corrigées ; toute régression doit être traitée avec le même sérieux — et vérifiée par une boucle de 30 exécutions, pas une seule.

### 13.7 Ne rien perdre

1. La sauvegarde chiffrée exporte tout.
2. L'import restaure sur une machine neuve.
3. La phrase de récupération régénère l'identité.
4. Une mise à jour ne perd **jamais** de données. 🔒 C'est ce que garantissent les migrations versionnées (T0.2).

### 13.8 Comment vérifier

| Parcours | Automatisable ? | Comment |
|---|---|---|
| 13.1 Premier lancement | Partiellement | e2e d'interface (D2) + passe manuelle |
| 13.2 Déverrouillage | Oui | e2e d'interface |
| 13.3 Se lier | Partiellement | e2e réseau existants + test réel entre deux machines |
| 13.4 Écrire/recevoir | Oui | e2e réseau |
| 13.5 Parler | Non | Passe manuelle obligatoire |
| 13.6 Rester connecté | Oui | e2e de reconnexion, boucle de 30 |
| 13.7 Ne rien perdre | Oui | Tests de sauvegarde/migration |

🔒 **Avant chaque version majeure** : passer les sept parcours, cocher, et consigner le résultat. Les trois non automatisables (13.1 partiel, 13.3 partiel, 13.5) prennent vingt minutes — c'est le meilleur investissement du projet.

---

## Partie 14 — Backlog de fonctionnalités

Ce backlog remplace l'analyse d'écarts de juillet, dont l'essentiel est désormais livré (fils, transfert, Markdown complet, mentions, sondages, messages vocaux, appels, vidéo, partage d'écran, AEC, soundboard, orateur prioritaire, timeout, événements, salons forum, permissions par salon, statuts riches, décorations…).

### 14.1 Grille de lecture

Chaque entrée porte :

- **Valeur** : ce que ça apporte à l'utilisateur (1 faible → 5 forte)
- **Coût** : en sessions
- **Faisabilité P2P** : 🟢 faisable · 🟠 faisable mais coûteux ou en tension avec les principes · 🔴 incompatible avec le sans-serveur
- **Voie** : qui le prend

### 14.2 Messagerie

| Fonctionnalité | Valeur | Coût | P2P | Note |
|---|---|---|---|---|
| **Groupes de MP** (conversation à 3+ sans créer de serveur) | 5 | 6 | 🟢 | Manque le plus visible en messagerie. Modèle : un groupe léger sans op-log complet, ou un serveur implicite masqué |
| ~~**Recherche avancée**~~ | — | — | 🟢 | **Déjà fait** (vérifié le 2026-07-25) : la grammaire `from:`/`in:`/`has:`/`before:`/`after:` est analysée et résolue côté nœud, et `SearchBar` l'expose. L'entrée du backlog était périmée |
| **Aperçu de liens** (unfurling) | 3 | 4 | 🟠 | ⚠️ Récupérer une URL révèle l'IP à un tiers. **Obligatoirement opt-in**, désactivé par défaut, avec avertissement explicite |
| ~~**Notes privées sur un contact**~~ | — | — | 🟢 | **Déjà fait** (vérifié le 2026-07-25) : `friends.set_note`/`get_note`, éditées dans la carte de profil |
| **Rappel « répondre plus tard »** | 3 | 2 | 🟢 | Les rappels existent (Planning) ; ajouter le geste depuis un message |
| ~~**Épingles en MP**~~ | — | — | 🟢 | **Déjà fait** (vérifié le 2026-07-25) : table `dm_pins`, exposée dans le store `dms` |
| **Formatage : spoilers, sous-texte** | 2 | 1 | 🟢 | Complément Markdown |
| **Sélecteur de GIF** | 3 | 3 | 🟠 | ⚠️ Dépend d'un service tiers centralisé (Tenor/Giphy) → fuite d'IP et de requêtes. **Contraire aux principes.** Alternative : bibliothèque locale de GIF importés |
| **Traduction de message** | 3 | 5 | 🔴 | Exige un service tiers. À écarter, sauf modèle local embarqué (trop lourd) |

### 14.3 Serveurs et communautés

| Fonctionnalité | Valeur | Coût | P2P | Note |
|---|---|---|---|---|
| **Filtre de mots-clés** (AutoMod léger) | 4 | 3 | 🟢 | Appliqué par les clients honnêtes ; pas d'autorité, mais utile contre le bruit |
| **Validation à l'entrée** (screening) | 4 | 3 | 🟢 | Les nouveaux membres attendent l'accord d'un modérateur |
| **Journal d'audit exportable** | 3 | 2 | 🟢 | La vue existe ; ajouter l'export |
| **Modèles de serveur** | 2 | 3 | 🟢 | Créer un serveur pré-structuré |
| **Salons de conférence** (stage) | 3 | 6 | 🟠 | Bloqué par la limite full mesh à 10 — dépend de 9.1 |
| **Réordonnancement par glisser-déposer** | 3 | 3 | 🟢 | Salons et catégories |
| **Serveurs à grande échelle** (100+ membres) | 3 | 10 | 🟠 | L'op-log et les listes filaires (4096 max) tiennent mal ; demande une étude dédiée |
| **Découverte publique de serveurs** | 2 | — | 🔴 | Exige un annuaire central. **Incompatible.** Alternative : partage de liens hors application |
| **Bots / webhooks** | 3 | 8 | 🟠 | Un bot = un pair qui tourne quelque part. Faisable mais change le modèle mental |

### 14.4 Voix et vidéo

| Fonctionnalité | Valeur | Coût | P2P | Note |
|---|---|---|---|---|
| **Vidéo sélective** (n'émettre qu'aux affichants) | 5 | 4 | 🟢 | Prérequis de tout passage à l'échelle vidéo — voir 9.1 |
| **Couches de qualité** (simulcast simple) | 4 | 6 | 🟢 | Deux résolutions émises, le récepteur choisit |
| **Son du système partagé avec l'écran** | 4 | 4 | 🟠 | ⚠️ Capture du son système : très dépendant de la plateforme, souvent bloqué sur macOS sans pilote tiers |
| **Grille vidéo, épinglage, plein écran** | 5 | 4 | 🟢 | Devient indispensable dès plusieurs caméras — T0.8 |
| **Réduction de bruit vidéo / flou d'arrière-plan** | 3 | 6 | 🟠 | Coûteux en CPU ; exige un modèle de segmentation |
| **Au-delà de 10 participants** | 3 | 12 | 🟠 | Demande des super-pairs relais — change le modèle de confiance |
| **Enregistrement local d'un appel** | 2 | 3 | 🟢 | ⚠️ Question de consentement : prévenir tous les participants |

### 14.5 Plateforme et confort

| Fonctionnalité | Valeur | Coût | P2P | Note |
|---|---|---|---|---|
| ~~**Mobile**~~ | — | — | 🔴 | abandonné (partie 8) |
| **Multi-appareil** | 5 | 30 | 🟢 | Jalon 1 |
| **Synchronisation des préférences** entre appareils | 3 | 4 | 🟢 | Après le jalon 1 |
| **Transfert d'historique** à l'appairage | 4 | 6 | 🟢 | Étape 3 de 6.2.5, repoussée hors 7.0 |
| **Mode compact / mode zen** | 2 | 2 | 🟢 | Masquer les décorations pour se concentrer |
| **Overlay en jeu** | 2 | 8 | 🟠 | Très dépendant de la plateforme |
| ~~**Mode streamer**~~ | — | — | 🟢 | **Fait (6.3)** : code ami masqué, révélable d'un clic ; contenu des notifications système retiré. Présenté comme une protection d'affichage, pas de confidentialité |
| **Démarrage automatique + zone de notification** | 3 | 2 | 🟢 | À vérifier : partiellement fait |
| **Import depuis une autre application** | 2 | 6 | 🟠 | Valeur d'adoption, coût élevé |

### 14.6 Confidentialité et sécurité

| Fonctionnalité | Valeur | Coût | P2P | Note |
|---|---|---|---|---|
| **Chiffrement post-quantique** | 5 | 12 | 🟢 | Jalon 2 |
| **Vérification d'identité par QR** | 4 | 2 | 🟢 | La vérification par nombre existe ; ajouter le scan direct |
| ~~**Verrouillage automatique après inactivité**~~ | — | — | 🟢 | **Fait (6.3)** : délais fermés (1 à 60 min, désactivé par défaut) ; un appel en cours suspend le compte à rebours |
| **Mode « aucune trace »** (rien sur disque) | 3 | 5 | 🟠 | Session entièrement en mémoire |
| **Historique des appareils connectés** | 4 | 2 | 🟢 | Dépend du jalon 1 — quand, d'où, quel appareil |
| **Effacement à distance d'un appareil** | 4 | 4 | 🟠 | ⚠️ Ne peut pas être garanti sans coopération de l'appareil ; ne jamais le présenter comme une garantie |
| **Rotation de la phrase de récupération** | 3 | 4 | 🟢 | Utile après une compromission suspectée |

### 14.7 Ce qu'on ne fera pas, et pourquoi

Écrire les refus est aussi utile qu'écrire les projets.

| Fonctionnalité | Raison du refus |
|---|---|
| **Découverte publique de serveurs** | Exige un annuaire central — contraire au principe 2.5 |
| **Activités intégrées** (jeux, visionnage commun) | Exige des serveurs de jeu tiers |
| **Boosts, URL personnalisées** | Modèle commercial sans objet ici |
| **Modération centralisée / signalement à une autorité** | Il n'y a pas d'autorité. La modération est par serveur, appliquée par ses membres |
| **Sauvegarde dans un nuage** | Contraire au modèle. La sauvegarde chiffrée locale existe, l'utilisateur choisit où la mettre |
| **Analytique d'usage** | Aucune télémétrie ne quitte la machine. Jamais |
| **Traduction automatique en ligne** | Fuite du contenu vers un tiers |

🔒 Ces refus ne sont pas des « pas encore » : ce sont des choix de conception. Les revisiter demanderait de revoir le principe 2.5 lui-même.

---

## Partie 15 — Stratégie de test

### 15.1 Les cinq niveaux

| Niveau | Ce qu'il prouve | Où | Coût |
|---|---|---|---|
| **Unitaire Rust** | Une fonction fait ce qu'elle dit | `#[cfg(test)]` dans le module | Faible |
| **Intégration Rust** | Deux sous-systèmes s'accordent | `crates/*/tests/` | Moyen |
| **E2E réseau** | Deux nœuds réels se parlent | `accord-node/tests/*_e2e.rs` | Élevé |
| **Unitaire/composant frontend** | Le composant rend et réagit | `*.test.tsx` (vitest + RTL) | Faible |
| **Sur appareil** | Ce qui ne se teste pas autrement | Manuel, documenté | Élevé |

### 15.2 Règles

1. **Un correctif de bug commence par un test qui échoue.** Sans ça, rien ne garantit qu'il est corrigé, ni qu'il ne reviendra pas.
2. **Un test qui devient faux par conception se réécrit, jamais ne se supprime.** *(Précédent : « startScreenShare ignoré hors appel actif » décrivait exactement la limite que la 6.1 supprimait — le test a été réécrit pour la nouvelle règle.)*
3. **Les tests de protocole vérifient l'aller-retour ET le rejet.** Un décodeur qui accepte tout est un décodeur dangereux.
4. **Les e2e réseau tournent en release** — c'est là que les régressions de type `debug_assert` se révèlent.
5. **Un test qui échoue une fois sur vingt est un bug**, pas un aléa. Le boucler 30 fois pour le prouver (méthode du Lot G).

### 15.3 Ce qu'il faut tester en priorité par jalon

**Jalon 0**
- Interop 6.1 ↔ 6.2 dans les deux sens.
- Réécriture du champ de capacités → handshake rejeté.
- Migration de schéma : montée, échec avec rollback, rétrogradation refusée.

**Jalon 1** (les plus importants de toute la feuille de route)
- **Deux appareils du même compte joignent un tiers sans s'évincer.** ← si ce test ne passe pas, le jalon est invalide.
- Message direct reçu sur les deux appareils.
- Rattrapage sans doublon après reconnexion.
- Appairage : nominal, code expiré, code réutilisé, empreinte refusée, interception du code seul.
- Révocation : l'appareil retiré est refusé.
- Liste d'appareils : version antérieure ignorée, signature invalide rejetée, borne du nombre respectée.
- Migration d'un vrai profil 6.2.

**Jalon 2**
- Hybride ↔ hybride, hybride ↔ classique, classique ↔ hybride.
- **Altération du transcript → échec** (capacités et matériel PQ).
- Taille du HELLO sous la MTU (test de non-régression).

~~**Jalon 3**~~ — abandonné (partie 8), aucun test à écrire.

### 15.4 Combler la dette D2 : e2e d'interface

Aujourd'hui, Playwright teste un *showcase*, pas l'application réelle. Il manque une couverture des parcours critiques.

**Parcours à couvrir** (par ordre de valeur) :
1. Créer une identité → choisir un pseudo → arriver sur l'écran principal.
2. Déverrouiller un coffre existant (dont **la validation par Entrée**, déjà cassée une fois).
3. Ajouter un ami par code → accepter → envoyer un message.
4. Créer un serveur → créer un salon → écrire dedans.
5. Ouvrir les réglages → changer de thème → vérifier l'application immédiate.
6. Ouvrir la palette de commandes → naviguer → exécuter une action.
7. Rejoindre un salon vocal → couper le micro → quitter.

**Approche** : réutiliser le mode démo (`lib/demo.ts`, actuellement non câblé) pour amorcer un état applicatif complet sans backend. ⚠️ **Attention** : ce fichier est gitignoré — il faudrait le versionner pour que la CI puisse s'en servir.

**Effort** : 4 sessions. 

### 15.5 Fuzzing

**Cibles actuelles** : 8, sur les décodeurs.

**À ajouter par jalon**
- Jalon 0 : décodeur du champ de capacités.
- Jalon 1 : décodeur de la liste d'appareils, décodeur des messages d'appairage.
- Jalon 2 : décodeur du matériel post-quantique dans le HELLO.

🔒 **Règle** : toute nouvelle structure filaire décodée depuis le réseau a sa cible de fuzzing **dans la même livraison**, pas après.

---

## Partie 16 — Inventaire du protocole filaire

Référence pour toute évolution. 🔒 Ce tableau doit être tenu à jour à chaque changement.

### 16.1 Canaux

| Canal | Code | Contenu |
|---|---|---|
| CONTROL | `0x00` | Ping/Pong, Close, Rekey, observation d'adresse, poinçonnage, auto-annonce DHT |
| DHT | `0x01` | RPC Kademlia |
| CORE | `0x02` | Messagerie, groupes, présence, appels |
| VOICE | `0x03` | Audio, vidéo (écran, caméra), qualité |
| FILE | `0x04` | Transfert de fichiers |
| RELAY | `0x05` | Circuits de repli |

### 16.2 Canal VOICE (état au 6.1)

| Genre | Nom | Depuis | Note |
|---|---|---|---|
| `0x01` | `AudioFrame` | v1 | Opus 20 ms, `media_type` (0x01 = audio) |
| `0x02` | `VoicePing` | v1 | Perte et RTT pour l'adaptation de débit |
| `0x03` | `ScreenFrame` | **5.0** | Fragment vidéo d'écran |
| `0x04` | `ScreenControl` | **5.0** | Début/fin de partage |
| `0x05` | `CameraFrame` | **6.0** | Fragment vidéo de caméra |
| `0x06` | `CameraControl` | **6.0** | Caméra allumée/éteinte |

**Drapeaux vidéo** : `VIDEO_FLAG_KEYFRAME = 0x01` (partagé écran/caméra depuis la 6.0).

**Bornes** : fragment vidéo ≤ 1200 octets (proto), tranche émise ≤ 1000 (node), trame réassemblée ≤ 512 Ko, ≤ 640 fragments.

### 16.3 Limites structurantes

| Constante | Valeur | Conséquence |
|---|---|---|
| `UDP_MTU` | 1200 | Toute structure d'un seul datagramme doit tenir dessous ⚠️ contraint le jalon 2 |
| `MAX_TCP_FRAME` | 1 Mio | Message applicatif réassemblé maximal |
| `MAX_LIST` | 4096 | Borne des listes filaires ⚠️ contraint les serveurs massifs |
| `VOICE_MAX_PARTICIPANTS` | 10 | Limite du full mesh |
| `DHT_K` / `DHT_ALPHA` | 20 / 3 | Paramètres Kademlia |
| `IDENTITY_POW_BITS` | 16 | Coût de création d'identité |
| `REKEY_FRAME_LIMIT` | 1 000 000 | Renouvellement de clé par volume |
| `REKEY_MAX_AGE_S` | 24 h | Renouvellement de clé par ancienneté |
| `MAX_CONCURRENT_REASSEMBLIES` | 8 | Réassembleur généraliste ⚠️ inadapté au temps réel (d'où le réassembleur vidéo dédié) |

### 16.4 Ce qui va bouger

| Jalon | Ajout filaire | Nature |
|---|---|---|
| 0 | `capabilities: u32` dans HELLO/WELCOME | Additif, décodage tolérant |
| 1 | Liste d'appareils (structure signée) | Nouvelle structure, publiée en DHT |
| 1 | Messages d'appairage | Nouveaux genres CORE |
| 1 | Rattrapage entre appareils | Nouveaux genres CORE |
| 2 | Matériel ML-KEM dans le handshake | ⚠️ Modifie le handshake — négocié |
| 4 | Couches de qualité vidéo | Additif, nouveau drapeau |

🔒 **Avant chaque livraison** : produire le diff filaire et vérifier qu'il correspond exactement à ce tableau. Un changement non prévu est un incident, pas un détail.

## Partie 17 — Plans de version

Le découpage en jalons donne la direction. Voici le découpage en **versions publiables** — l'unité réelle de livraison. Une version = une release complète, vérifiée, publiée.

🔒 **Principe de rythme** : mieux vaut une version courte qui sort qu'une version ambitieuse qui traîne. Un jalon peut prendre trois versions.

### 17.1 v6.2 — Fondations invisibles

**Thème** : payer la dette et préparer le terrain. Peu visible, indispensable.

| Contenu | Section |
|---|---|
| Champ de capacités dans le handshake | T0.1 |
| Migrations de schéma versionnées | T0.2 |
| Script de nettoyage des attributs macOS | T0.3 |
| Découpage du bundle | T0.4 |
| Décorations de profil internationalisées | T0.5 |
| Ménage des branches et de `dist/` | T0.6 |
| Indicateur de qualité de connexion | T0.7 |
| Raccourcis clavier complets | T0.9 |
| Grille vidéo par émetteur | T0.8 (avancée depuis 6.3) |

**Note de version pour l'utilisateur** : *« Démarrage plus rapide, connexion plus lisible, et des fondations posées pour les grandes nouveautés à venir. »*

**Estimation** : 9 sessions.

### 17.2 v6.3 — Confort et langues

**Thème** : la partie visible du jalon 0, plus les langues.

| Contenu | Section |
|---|---|
| Les 10 langues les plus parlées | 10.1 |
| Recherche avancée (`from:`, `in:`, `has:`) | 14.2 |
| Notes privées sur un contact | 14.2 |
| Mode streamer | 14.5 |
| Verrouillage automatique après inactivité | 14.6 |

⚠️ **Point d'attention** : l'arabe (RTL). Si le rendu est bancal, **le dire dans la note de version** plutôt que de livrer une langue à moitié utilisable. Option de repli : livrer les 6 langues LTR en 6.3, l'arabe en 6.4 avec un vrai support RTL.

**Estimation** : 12 sessions.

### 17.3 v7.0 — Multi-appareil

**Thème** : le gros morceau. Voir la partie 6 pour le détail complet.

Découpage possible en deux versions si le chantier s'étire :

- **7.0** : lots 1.A → 1.D (conception, identités, transport, appairage). À ce stade, deux appareils coexistent et se connectent, mais la livraison des messages reste mono-appareil.
- **7.1** : lot 1.E (livraison multi-appareils, rattrapage).

⚠️ **Attention** : une 7.0 où l'on peut appairer un appareil qui ne reçoit pas les messages serait **déroutante**. Si le découpage est retenu, la 7.0 doit présenter l'appairage comme une préparation, pas comme une fonctionnalité finie. **Préférable** : ne publier qu'une fois 1.E fait.

**Estimation** : 28 sessions.

### 17.4 v7.2 — Finitions du multi-appareil

| Contenu | État au 2026-07-27 |
|---|---|
| Synchronisation des préférences entre appareils | ✅ opcode `0x21`, liste blanche explicite |
| Transfert d'historique à l'appairage | ✅ 8.2 — opcode `0x23`, parcours descendant, barre de progression |
| Historique des appareils connectés (quand, d'où) | ✅ migration 17, table locale, « d'où » = la route |
| Vérification d'identité par QR | ✅ affichage **et** lecture — un QR seulement affiché ne vérifie rien |

> 🔴 **Un défaut du jalon 1 est sorti de ce lot, et il passait avant.** En
> préparant le transfert d'historique, une sonde a montré qu'un appareil du
> compte au carnet vide **ne reçoit rien** : `ingest_dm` jette tout message d'un
> pair qui n'est pas ami dans la base de CETTE machine, l'appairage part d'un
> profil neuf, et rien ne remplissait ce carnet. La machine ouvrait ses
> sessions, figurait dans l'éventail de livraison de l'ami, et jetait tout en
> silence. Corrigé par `CoreMsg::SelfContactAdd` (0x22) — c'était le prérequis
> du transfert d'historique : sans carnet, il n'y a rien où transférer.
>
> 🔴 **Ce paragraphe affirmait qu'aucun nouvel opcode n'était nécessaire.
> C'était faux, et livré le 2026-07-28 avec l'opcode `0x23`.** L'énoncé disait :
> « le rattrapage sait déjà tirer une conversation ; il lui manque d'être piloté
> en boucle jusqu'à épuisement ». Deux erreurs, chacune suffisante.
>
> **Le rattrapage ne peut pas descendre.** `SelfSyncPull` porte `since_lamport`,
> une borne **basse**, et le répondeur ne sert jamais que `window()` — les 64
> messages les plus **récents** (`accord-core/src/dm_sync.rs`). Faire avancer le
> curseur ne fait donc que rétrécir la réponse : la deuxième passe rend zéro, et
> aucune suite d'appels n'atteint un message plus ancien que la fenêtre. La
> boucle décrite aurait donné à un appareil neuf ses 64 derniers messages, puis
> se serait arrêtée **en ayant l'air d'avoir fini**. C'est le pire mode de
> panne : un succès apparent.
>
> **Et le champ ne pouvait pas simplement s'ajouter.** Contrairement au
> handshake (D-047 et ses champs additifs de fin), le décodeur `CoreMsg` rejette
> les octets restants : un appareil plus ancien aurait jeté le `SelfSyncPull`
> allongé, cassant le rattrapage **qui, lui, marche**. Un opcode neuf isole la
> panne — un appareil qui l'ignore jette ce seul datagramme.
>
> ⚠️ Le prix de cette isolation, à connaître : le demandeur ne distingue pas
> « mon frère est trop ancien » de « mon frère n'a rien de plus ancien ». Les
> deux se présentent comme une passe qui ne rapporte rien. L'interface le dit,
> plutôt que d'annoncer une réussite.
>
> Ce qui a été gardé de l'énoncé, en revanche : l'énumération depuis **le
> carnet** (`dm_conversations` lit `dm_messages`, vide sur un appareil neuf), et
> **deux passes sans nouveauté valent fin** plutôt qu'un marqueur qu'un
> datagramme perdu ferait attendre indéfiniment. Ces deux-là étaient justes.

**Estimation restante** : 0 — livré en 8.2.

### 17.5 v8.0 — Post-quantique

Voir la partie 7. Une seule version, périmètre net.

**Note de version** : formulation prudente. *« Le chiffrement d'Accord résiste désormais aux attaques par ordinateur quantique connues à ce jour, en combinant l'algorithme éprouvé actuel avec un algorithme post-quantique — si l'un venait à faiblir, l'autre protège toujours. »*

**Estimation** : 12 sessions.

### 17.6 v8.1 — Communautés

**Thème** : les outils qui manquent quand un serveur grandit.

| Contenu | Section |
|---|---|
| Filtre de mots-clés | 14.3 |
| Validation à l'entrée | 14.3 |
| Journal d'audit exportable | 14.3 |
| Blocage au niveau compte (propagé aux appareils) | 9.4 |
| Réordonnancement par glisser-déposer | 14.3 |

**Estimation** : 11 sessions.

### 17.7 v8.2 — Vidéo à l'échelle

**Thème** : rendre la vidéo de groupe réellement utilisable au-delà de trois personnes.

| Contenu | Section |
|---|---|
| Vidéo sélective (n'émettre qu'aux affichants) | 9.1 |
| Couches de qualité | 9.1 |
| Épinglage, plein écran, orateur actif | 14.4 |
| Son du système avec le partage d'écran | 14.4 |

⚠️ Le son système est **très** dépendant de la plateforme. À traiter en dernier, et à abandonner sans regret s'il exige un pilote tiers.

**Estimation** : 14 sessions.

### 17.8 v9.0 — Mobile

Voir la partie 8. **Sous réserve du lot de faisabilité 3.A.**

⚠️ Si 3.A conclut « non » ou « oui mais très cher », cette version est remplacée. Plan de repli, par ordre de préférence :

1. **v9.0 alternative — Groupes de MP** : la fonctionnalité de messagerie la plus demandée qui reste (14.2), plus les finitions du backlog.
2. **v9.0 alternative — Robustesse** : campagne de fiabilité, télémétrie, chaos tests élargis, réduction de la dette.

**Estimation** : 35 sessions (ou 15 pour le repli).

### 17.9 Versions correctives

Entre les versions planifiées, des correctives (`x.y.Z`) sortent au fil des signalements.

🔒 **Règle** : une correction de bug de fiabilité passe **devant** toute fonctionnalité en cours. Le précédent de la régression 3.0 → 3.3 est sans appel : quatre versions ont été publiées avec la messagerie cassée parce que le problème n'a pas été vu ni priorisé.

**Critères pour une corrective immédiate** :
- perte ou non-livraison de messages ;
- impossibilité de se connecter ou de se reconnecter ;
- plantage au démarrage ;
- faille de sécurité ;
- perte de données à la mise à jour.

Tout le reste attend la version suivante.

### 17.10 Récapitulatif

| Version | Thème | Sessions | Cumul |
|---|---|---|---|
| 6.2 | Fondations invisibles | 9 | 9 |
| 6.3 | Confort et langues | 12 | 21 |
| 7.0 | Multi-appareil | 28 | 49 |
| 7.2 | Finitions multi-appareil | 12 | 61 |
| 8.0 | Post-quantique | 12 | 73 |
| 8.1 | Communautés | 11 | 84 |
| 8.2 | Vidéo à l'échelle | 14 | 98 |
| 9.0 | Mobile (ou repli) | 35 | 133 |

**≈ 133 sessions.** À raison d'une session par jour ouvré, c'est un peu plus de six mois — sans marge.

⚠️ **Lecture honnête de ce chiffre** : il n'inclut ni les correctifs, ni les imprévus, ni les découvertes du type bloqueur B1. **Compter 40 % de marge** est réaliste, ce qui porte l'horizon à environ neuf mois pour l'ensemble. Les six mois annoncés couvrent confortablement jusqu'à la 8.0, et probablement la 8.1.

C'est une raison de plus de garder les versions courtes : à ce rythme, il sort quelque chose d'utile toutes les deux à trois semaines, plutôt qu'un grand saut incertain.

---

## Partie 18 — Second semestre : mois 7 à 12

Les parties 5 à 9 couvrent les six premiers mois. Voici la suite — moins détaillée par construction : plus l'horizon est lointain, plus les décisions dépendent de ce qu'on aura appris.

🔒 **Règle de lecture** : cette partie fixe des **directions**, pas des engagements. Chaque jalon ici doit être re-spécifié en détail au moment de l'attaquer, comme l'ont été les jalons 1 à 3.

### 18.1 Vue d'ensemble du second semestre

| Jalon | Version | Thème | Durée | Risque |
|---|---|---|---|---|
| **5** | 10.0 | **Messagerie de groupe** (groupes de MP, messagerie avancée) | 1,5 mois | Moyen |
| **6** | 11.0 | **Serveurs à grande échelle** | 2 mois | Élevé |
| **7** | 12.0 | **Ouverture et durabilité** | 1,5 mois | Moyen |
| **8** | 13.0 | **Consolidation et audit** | 1 mois | Faible |

---

### 18.2 Jalon 5 — Messagerie de groupe (10.0)

**Durée : 1,5 mois.** Risque moyen.

#### Le problème utilisateur

> « Je veux discuter à trois avec deux amis, sans créer un serveur avec des salons et des rôles pour ça. »

Aujourd'hui, les messages directs sont strictement à deux. Pour parler à trois, il faut créer un serveur — c'est disproportionné, et ça change la nature de la relation.

C'est le manque le plus visible en messagerie une fois le multi-appareil livré.

#### Conception

Deux approches possibles :

**(a) Serveur implicite masqué** — créer un vrai groupe en interne, sans l'exposer comme serveur. *Avantage* : réutilise l'op-log, les permissions, tout l'existant. *Inconvénient* : lourd pour un groupe de trois personnes ; l'op-log complet est surdimensionné.

**(b) Groupe léger dédié** — une nouvelle structure : liste de membres signée, pas de rôles, pas de salons, un seul fil. *Avantage* : simple, léger, adapté à l'usage. *Inconvénient* : nouveau code, nouvelle surface de test.

**Recommandation : (b)**, avec réutilisation maximale du chiffrement de groupe existant. Un groupe de MP n'a pas besoin de rôles, de catégories, ni d'invitations signées complexes — l'ajout d'un membre se fait par un membre existant, point.

⚠️ **Questions à trancher en conception**
- Qui peut ajouter quelqu'un ? (proposition : n'importe quel membre, comme un fil de discussion)
- Peut-on quitter ? Que voit-on alors de l'historique ?
- Que se passe-t-il quand le dernier membre part ?
- Combien de membres au maximum ? (proposition : 20 — au-delà, un serveur est plus adapté)

#### Contenu du jalon

| Élément | Note |
|---|---|
| Groupes de MP (3 à 20 personnes) | Le cœur |
| Nom et image du groupe | Modifiables par les membres |
| Ajout et départ de membres | Avec message système dans le fil |
| Appels de groupe dans un groupe de MP | Réutilise le full mesh existant |
| Notifications par groupe | Réglage indépendant |
| Aperçu de liens (opt-in, désactivé par défaut) | ⚠️ Fuite d'IP — avertissement explicite |
| Formatage : spoilers, sous-texte | Complément Markdown |
| Rappel « répondre plus tard » depuis un message | Réutilise le système de rappels |

**Estimation** : 20 sessions.

#### Définition de fin

- [x] Un groupe à trois se crée, reçoit des messages, et les trois les voient.
- [x] Un membre ajouté voit le fil à partir de son arrivée. Tranché et écrit
      dans `docs/DM_GROUPS.md` : les membres existants ne renvoient pas
      l'historique, c'est le comportement déjà en place de `GROUP_SYNC`.
- [x] Un appel de groupe fonctionne dans un groupe de MP. ⚠️ Vérifié au sens
      où le salon est atteignable et vise le bon identifiant (le vocal d'un
      groupe porte `channel_id == group_id`, il ne dépend d'aucun salon
      déclaré) ; **l'audio lui-même relève du parcours 13.5, non automatisable**.
- [x] L'aperçu de liens est **désactivé par défaut** et son risque est expliqué —
      y compris la récolte d'IP en groupe, nommée dans le libellé du réglage.
- [ ] Les sept parcours de la partie 13 passent. 🔴 Reste ouvert : trois d'entre
      eux (13.1 partiel, 13.3 partiel, 13.5) ne sont pas automatisables — voir
      §15.3. Cette case ne peut pas être cochée depuis le code.

⚠️ **Deux corrections de conception faites en cours de route**, l'une et
l'autre parce que la feuille de route avait mal posé le problème :

- « L'ajout d'un membre se fait par un membre existant, point » (ci-dessus) a
  été **abandonné** : c'était une adhésion forcée — le groupe poussait sa clé à
  quelqu'un qui n'avait rien demandé. Remplacé par le consentement explicite en
  deux temps (décision D-045). Je l'avais d'ailleurs réintroduit par accident
  dans la liste blanche du nœud avant qu'un sondage ne le rattrape.
- « Notifications par groupe » n'était pas un manque de mécanisme mais de
  surface : `isConversationSilenced` couvrait déjà les groupes de MP sans le
  savoir.

---

### 18.3 Jalon 6 — Serveurs à grande échelle (11.0)

**Durée : 2 mois.** 🔴 Risque élevé — c'est un chantier d'architecture.

#### Le problème

Les serveurs actuels sont conçus pour des groupes d'amis. Trois limites structurelles apparaissent quand un serveur grandit :

1. **L'op-log est intégral et répliqué chez tous.** Un serveur avec des années d'historique de configuration fait porter à chaque nouveau membre le coût de tout rejouer.
2. **`MAX_LIST = 4096`** borne les listes filaires — donc le nombre de membres transmissibles en une fois.
3. **La diffusion est en étoile depuis l'émetteur** : envoyer à 200 membres, c'est 200 envois.

#### Pistes, par ordre de faisabilité

**A. Compaction de l'op-log** *(indispensable, faisable)*
Un instantané signé de l'état à un moment donné, remplaçant les opérations antérieures. Un nouveau membre récupère l'instantané plus les opérations récentes, pas dix mille opérations.
⚠️ Difficulté : qui signe l'instantané ? Il faut qu'il soit vérifiable sans confiance aveugle. Piste : signature par plusieurs administrateurs, ou instantané reconstructible et vérifiable par recalcul.

**B. Pagination des listes** *(nécessaire, simple)*
Transmettre les membres par pages plutôt qu'en une liste bornée à 4096.

**C. Diffusion arborescente** *(gros gain, gros risque)*
Au lieu d'envoyer à N membres, envoyer à quelques-uns qui relaient. Réduit le coût pour l'émetteur.
⚠️ **Change le modèle de confiance** : le relayeur voit les métadonnées (qui parle, quand), même si le contenu reste chiffré. À documenter honnêtement, et à rendre optionnel.

**D. Salons de conférence** *(dépend de C ou de 9.1)*
Quelques orateurs, beaucoup d'auditeurs. Impossible en full mesh pur.

#### Contenu

| Élément | Priorité |
|---|---|
| Compaction de l'op-log par instantanés | Indispensable |
| Pagination des listes de membres | Indispensable |
| Chargement paresseux de la liste des membres | Confort |
| Mesures : temps de rejoindre, mémoire, à 50 / 200 / 500 membres | Indispensable |
| Diffusion arborescente | Optionnel, sous réserve d'étude |
| Salons de conférence | Sous réserve |

⚠️ **Ce jalon doit commencer par des mesures**, pas par du code. Combien de membres avant que ça devienne pénible ? Où est le goulot exact ? Optimiser sans mesurer, c'est deviner.

**Estimation** : 25 sessions.

#### Définition de fin

- [x] Un serveur de 200 membres se rejoint en moins de 10 secondes. **Mesuré à
      70 ms**, soit 140 fois sous la cible — et 173 ms à 500 membres. Le seuil
      de dix secondes était très au-dessus de la réalité.
- [x] La mémoire reste sous le budget avec 5 serveurs de 200 membres —
      **mesurée à 334 828 octets (0,3 Mo)** contre un budget de 400 Mo, soit
      trois ordres de grandeur de marge (`docs/PERFORMANCE.md` §3.5).
      ⚠️ C'est le coût de l'état de groupe replié, pas le RSS du processus :
      lu comme « la mémoire de l'application », le chiffre serait faux.
- [ ] Un instantané d'op-log est vérifiable et remplace l'historique complet.
      **Non fait — et chiffré plutôt que fait, délibérément.** Voir
      « La compaction : chiffrée, puis écartée » juste en dessous.

#### La compaction : chiffrée, puis écartée

**Décision du 2026-07-28.** Ce point était présenté comme « le principal reste
du jalon » sur la foi d'une prémisse que les mesures ont démentie.

Le raisonnement était : l'op-log grossit, donc le repli à froid devient le coût
dominant d'une adhésion, donc il faut un instantané. Mesuré
(`docs/PERFORMANCE.md` §3.6), le repli à froid coûte **16,7 ms à 10 000 ops**
quand l'adhésion elle-même en coûte **1 580**. La compaction s'attaquerait à
1 % du coût.

Et elle le ferait à un prix qui n'est pas un prix de performance. Un op-log est
vérifiable de bout en bout : chaque op porte sa signature, n'importe qui peut
recalculer l'état et constater qu'il découle des ops. Un instantané remplace
cette vérification par **la parole de celui qui l'a produit**. C'est un
changement de modèle de confiance, du niveau de §2.5 — pas une optimisation.

🔒 **Ce point reste donc ouvert à dessein, et ne doit pas être « terminé »
discrètement par quelqu'un qui verrait une case à cocher.** Le rouvrir demande
un élément neuf : un op-log réellement assez gros pour que les 16,7 ms comptent,
ou un schéma d'instantané qui reste vérifiable (par exemple signé par un quorum
de membres, ce qui est un autre chantier).
- [x] Les mesures avant/après sont documentées — dans `docs/PERFORMANCE.md` §3
      plutôt que le CHANGELOG, avec ce que les chiffres ne disent pas.

⚠️ **Ce que les mesures ont démenti dans ce qui précède.**

- La limite 2 (`MAX_LIST = 4096` bornerait le nombre de membres transmissibles)
  **est fausse** : la liste des membres ne passe pas par une liste filaire
  bornée. Le vrai coût mesuré est ailleurs — le JSON `groups.state`, 115,8 Kio
  à 500 membres, transmis d'un bloc.
- La compaction de l'op-log était donnée comme le levier principal. Les mesures
  ont montré que **62 % du coût d'une adhésion** n'était ni le rejeu ni la
  crypto, mais l'état re-dérivé depuis SQLite après chaque op. Le repli
  incrémental a traité ça (827 → 173 ms à 500 membres) sans toucher à la
  compaction, qui reste à faire mais pèse moins qu'annoncé.
- La question ouverte de la compaction — « qui signe l'instantané ? » — n'est
  pas tranchée. Signer par administrateurs introduirait une autorité dans un
  projet qui revendique zéro serveur (§2.5). Une piste sans autorité existe :
  demander le même instantané à K pairs indépendants et l'accepter s'ils
  s'accordent sur son empreinte. C'est un ajout de protocole, pas un refactor.

---

### 18.4 Jalon 7 — Ouverture et durabilité (12.0)

**Durée : 1,5 mois.** Risque moyen.

**Thème** : faire en sorte qu'Accord survive à son auteur — que d'autres puissent l'étendre, le vérifier, et récupérer leurs données.

#### 19.4.1 API locale documentée

L'API JSON-RPC locale existe déjà (WebSocket, `accord-api`). Elle est **utilisée par l'interface elle-même**, donc complète et testée. Il manque : la documenter et la stabiliser comme contrat public.

**Contenu**
- Documentation de référence de toutes les méthodes et événements.
- Politique de stabilité : ce qui est gelé, ce qui peut bouger.
- Authentification pour les clients tiers (aujourd'hui pensée pour un client unique).
- Exemple minimal de client tiers.

**Ce que ça ouvre** : des bots comme pairs, des interfaces alternatives, des scripts personnels, des ponts vers d'autres réseaux — **sans qu'Accord ait à les héberger**. C'est la réponse « fidèle aux principes » à la question des bots (14.3).

⚠️ **Risque de sécurité** : ouvrir l'API élargit la surface d'attaque locale. L'authentification et le contrôle d'origine existent déjà (vérification de l'`Origin`, anti-DNS-rebinding) — à renforcer et à auditer avant d'inviter des tiers.

#### 19.4.2 Portabilité des données

- Export complet lisible (pas seulement le format de sauvegarde chiffré) : conversations en Markdown ou JSON, pièces jointes.
- Import depuis cet export.
- Documentation du format.

**Pourquoi** : un utilisateur qui ne peut pas partir avec ses données est captif. Un projet qui prône la souveraineté doit être exemplaire là-dessus.

#### 19.4.3 Reproductibilité et vérifiabilité

- Builds reproductibles, ou à défaut, procédure de vérification documentée.
- Publication des empreintes des artefacts.
- Documentation permettant à un tiers de reconstruire et comparer.

**Pourquoi** : « le code est ouvert » ne prouve rien si le binaire distribué n'est pas vérifiable.

#### 19.4.4 Documentation de contribution

- `CONTRIBUTING.md` : comment construire, tester, proposer.
- Architecture expliquée pour un nouvel arrivant.
- Les principes de la partie 2, formulés pour un contributeur externe.

**Estimation** : 18 sessions.

---

### 18.5 Jalon 8 — Consolidation et audit (13.0)

**Durée : 1 mois.** Risque faible, valeur élevée.

Un jalon sans nouveauté. Après un an de fonctionnalités, s'arrêter et consolider.

#### Contenu

| Élément | Objectif |
|---|---|
| **Audit de sécurité externe** | Faire regarder le protocole et la crypto par quelqu'un d'autre |
| Campagne de fuzzing longue | Toutes les cibles, plusieurs jours |
| Revue complète de la dette | Reprendre le tableau 1.3, tout traiter ou tout justifier |
| Accessibilité : passe complète | Les 24 thèmes, tous les parcours, lecteur d'écran réel |
| Performance : passe complète | Tous les budgets de 10.2 mesurés et tenus |
| Documentation : relecture intégrale | Cohérence, exactitude, ce qui a bougé en un an |
| Nettoyage du code mort | Ce qui a été remplacé et jamais supprimé |
| Relecture de cette feuille de route | Ce qui s'est vérifié, ce qui s'est trompé |

⚠️ **L'audit externe est le point le plus important.** Un an de développement seul sur de la cryptographie et un protocole réseau, sans regard extérieur, est un angle mort. Même un audit limité au handshake et au modèle d'identité vaut plus que six mois de fonctionnalités supplémentaires.

**Estimation** : 15 sessions + le délai de l'audit externe.

---

### 18.6 Récapitulatif des douze mois

| Version | Thème | Sessions | Cumul |
|---|---|---|---|
| 6.2 | Fondations invisibles | 9 | 9 |
| 6.3 | Confort et langues | 12 | 21 |
| 7.0 | Multi-appareil | 28 | 49 |
| 7.2 | Finitions multi-appareil | 12 | 61 |
| 8.0 | Post-quantique | 12 | 73 |
| 8.1 | Communautés | 11 | 84 |
| 8.2 | Vidéo à l'échelle | 14 | 98 |
| 9.0 | Mobile (ou repli) | 35 | 133 |
| 10.0 | Messagerie de groupe | 20 | 153 |
| 11.0 | Serveurs à grande échelle | 25 | 178 |
| 12.0 | Ouverture et durabilité | 18 | 196 |
| 13.0 | Consolidation et audit | 15 | 211 |

**≈ 211 sessions.** Avec 40 % de marge pour les imprévus et les correctifs : **environ 295 sessions**, soit un peu plus de quatorze mois à raison d'une session par jour ouvré.

**Lecture honnête** : les douze mois annoncés couvrent confortablement jusqu'à la **11.0**. Les jalons 7 et 8 (ouverture, audit) glisseront probablement sur le treizième et quatorzième mois — sauf si le mobile conclut « non », auquel cas tout se resserre de deux mois.

### 18.7 Au-delà : les questions ouvertes

Ce qui n'est pas planifié, mais qui se posera.

**Le projet grandit-il en équipe ?** Toute cette feuille de route suppose un développement solo. Si des contributeurs arrivent, le jalon 7 (documentation, API, contribution) devient prioritaire — il passe devant.

**Y a-t-il un modèle économique ?** Aucun n'est prévu, et les principes en excluent la plupart (pas de serveur à vendre, pas de données à exploiter). Les pistes compatibles seraient des dons ou du support — mais ça ne se décide pas dans une feuille de route technique.

**Le web ?** Une version navigateur est régulièrement tentante. Elle est **très** difficile : pas de socket UDP, pas de nœud persistant, stockage limité. Une version en lecture seule connectée à son propre nœud de bureau serait envisageable — à étudier si le mobile échoue.

**La fédération ?** Se relier à Matrix ou à d'autres réseaux. Techniquement possible via l'API locale (jalon 7) et un pont. Philosophiquement discutable : un pont expose le contenu au réseau d'en face. À laisser aux tiers plutôt qu'à intégrer.

**Que se passe-t-il si un principe doit céder ?** Le plus probable est 2.5 (zéro serveur), sous la pression des notifications mobiles. La méthode reste la même : documenter le compromis, le rendre optionnel, ne jamais le dissimuler.

---

## Partie 19 — Annexes

### 19.1 Carte du dépôt

```
accord/
├── crates/
│   ├── accord-proto/       Formats filaires, limites, encodage/décodage
│   │   ├── plaintext.rs    Canaux CONTROL/DHT/CORE/VOICE/FILE/RELAY
│   │   ├── core_msg.rs     Messages applicatifs, ops de groupe
│   │   ├── limits.rs       MTU, bornes, timeouts — source de vérité
│   │   └── wire.rs         Lecteur/écrivain primitifs
│   ├── accord-crypto/      Identité, signatures, dérivation
│   │   └── identity.rs     ⚠️ Cœur du jalon 1
│   ├── accord-transport/   Sessions chiffrées, handshake, fragmentation
│   │   ├── endpoint.rs     ⚠️ Zone la plus critique (Lot G, invariant B1)
│   │   └── frag.rs         Fragmentation transparente
│   ├── accord-dht/         Kademlia, résolution des codes d'ami
│   ├── accord-core/        État applicatif, base chiffrée, groupes
│   ├── accord-api/         Serveur JSON-RPC local (WebSocket)
│   ├── accord-node/        Orchestration : runtime, service, voix
│   │   ├── runtime.rs      Routage réseau
│   │   ├── service/        Méthodes RPC par domaine
│   │   └── voice/          Moteur voix + média
│   │       ├── engine.rs   Boucle 20 ms, diffusion
│   │       └── media.rs    Fragmentation vidéo temps réel
│   ├── accord-voice/       DSP, codec, gigue, mixage, AEC
│   └── accord-macos/       Pont natif (permissions)
├── app/
│   ├── src/                Frontend React/TS
│   │   ├── components/     Surfaces d'interface
│   │   ├── stores/         État zustand par domaine
│   │   ├── lib/            Utilitaires, API, média
│   │   └── i18n/           Dictionnaires + test de parité
│   └── src-tauri/          Hôte Tauri
├── scripts/                Builds locaux par plateforme
├── .github/workflows/      ci.yml, release.yml, fuzz.yml
└── ci.sh                   Gate local (miroir de ci.yml)
```

### 19.2 Glossaire

| Terme | Définition |
|---|---|
| **Compte** | Identité racine, ce que voient les amis. Signe la liste d'appareils. *(à partir du jalon 1)* |
| **Appareil** | Une installation d'Accord, avec sa propre clé de session. *(jalon 1)* |
| **Code ami** | Identifiant public partageable (`accord-mot-mot-12345`), résolu via la DHT |
| **Op-log** | Journal répliqué des opérations d'un serveur, ordonné par Lamport |
| **Salon** | Canal dans un serveur : texte, vocal, annonces ou forum |
| **Full mesh** | Chacun envoie à chacun, sans relais central |
| **Boîte aux lettres** | Dépôt chiffré pour un pair hors ligne (7 jours) |
| **Poinçonnage** | Traversée de NAT par ouverture simultanée de part et d'autre |
| **Session cadavre** | Session périmée survivant au redémarrage d'un pair (Lot G, cause 4) |
| **Keyframe** | Image vidéo décodable seule ; point de reprise après perte |
| **Gate** | `./ci.sh` — la barrière de qualité avant toute livraison |
| **Jalon** | Version publiée marquant une étape de cette feuille de route |
| **Hybride (PQ)** | Chiffrement combinant classique et post-quantique *(jalon 2)* |

### 19.3 Journal des décisions

Décisions structurantes prises, avec leur raison. À compléter au fil de l'eau.

| Date | Décision | Raison |
|---|---|---|
| 2026-07 | Fragmentation vidéo **maison** plutôt que celle du transport | Le réassembleur généraliste a 8 slots et 30 s de timeout — inadapté à un flux temps réel |
| 2026-07 | Caméra en **variantes filaires neuves** plutôt qu'un drapeau | Un client 5.0 rejette proprement au lieu de mal interpréter |
| 2026-07 | Capture/rendu vidéo dans le **webview** plutôt qu'en natif | `getUserMedia` fonctionne déjà ; le pont natif ne gère que les permissions |
| 2026-07 | Flux temps réel **mono-appareil** au jalon 1 | N appareils = N fois la bande passante — inacceptable pour la vidéo |
| 2026-07 | Réglages **par appareil**, pas synchronisés, au jalon 1 | Volume et périphériques n'ont pas de sens partagé ; la synchro attendra |
| 2026-07 | Post-quantique **hybride**, jamais en remplacement | ML-KEM est jeune ; l'hybride n'est jamais pire que l'existant |
| 2026-07 | Notifications mobiles : push **vide** (option B) | Le contenu ne transite pas chez Apple/Google ; compromis assumé et documenté |
| 2026-07 | **Worktree** obligatoire pour tout chantier parallèle | Une branche ne protège pas un arbre de travail partagé |

### 19.4 Aide-mémoire des commandes

```bash
# Environnement
source "$HOME/.cargo/env"
export CMAKE_POLICY_VERSION_MINIMUM=3.5
export PATH="/opt/homebrew/opt/node@22/bin:$PATH"   # Node 26 casse vitest

# Gate complet
./ci.sh                                             # doit finir par "CI OK"

# Frontend seul
cd app && npx tsc --noEmit && npx vitest run        # lire "Test Files … / Tests …"

# Rust seul
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --lib

# Worktree pour un chantier parallèle
git worktree add ../accord-<lot> -b feat/<lot> origin/main

# Release
gh run watch <id> --exit-status
gh release edit vX.Y.Z --draft=false --latest
gh run rerun <id> --failed                          # flake d'infrastructure

# Build local signé (macOS)
export TAURI_SIGNING_PRIVATE_KEY="$(cat ~/.tauri/accord-updater.key)"
# Mot de passe de la clé : à reprendre de votre gestionnaire. Ne pas
# écrire sa valeur ici — ce document est versionné depuis le 2026-07-28,
# et dire qu'il est vide renseignerait qui mettrait la main sur la clé.
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD="${TAURI_SIGNING_PRIVATE_KEY_PASSWORD:?}"
./scripts/build-macos.sh
```

### 19.5 Prochaines actions immédiates

Mis à jour le 2026-07-27 au soir, après la publication de la **7.1.0** (jalons
0, 1, 4 et 5 complets ; le jalon 3 abandonné, partie 8 ; le jalon 6 entamé par
ses mesures).

#### Urgent — pour l'utilisateur, pas pour le code

1. 🔴 **Vérifier la sauvegarde hors ligne de `~/.tauri/accord-updater.key`.**
   Toujours pas fait. C'est le **point de défaillance unique** du projet : sans
   cette clé, plus aucune mise à jour ne peut être acceptée par les clients
   installés — ils devraient tous réinstaller à la main. Une copie doit exister
   ailleurs que sur la machine de développement. Rien dans ce document n'est
   plus urgent, et c'est la seule action que le code ne peut pas faire à la
   place de son auteur.

2. 🔴 **Appairer deux vraies machines**, dernière case du jalon 1 (§6.6). Tout
   le reste du multi-appareil est prouvé par des tests ; celle-ci ne peut pas
   l'être — §1.4 dit pourquoi.

#### Ce qui a bougé le 2026-07-27 (jalon 5 clos, jalon 6 entamé)

- **Le jalon 5 est complet** hors les sept parcours (§18.2) : groupes de MP,
  invitation avec consentement, appel dans le fil, niveau de notification,
  aperçus de liens opt-in.
- **Le jalon 6 a commencé par des mesures**, comme il le demandait — et elles
  ont démenti deux de ses prémisses (§18.3). Adhésion à 500 membres : 827 →
  173 ms.
- **La compaction de l'op-log reste ouverte**, et sa question de conception
  aussi : qui signe l'instantané. À trancher avant d'écrire la moindre ligne.
- **Trois garde-fous ajoutés au gate** : cliquet de taille de fichier,
  constantes publiques citées dans les docs, et `CMAKE_POLICY_VERSION_MINIMUM`
  que `ci.sh` seul n'exportait pas.
- **`docs/API.md` annonçait 1 Mio de taille maximale de message ; le code dit
  16 Mio depuis le premier commit.** La doc était fausse dès l'origine, pas
  dérivée. Corrigée, et gardée par le gate.

#### Ce qui a bougé depuis le 2026-07-25

3. **Les 10 langues sont faites** — fr, en, es, pt, de, ru, zh, hi, bn, ar,
   arabe et RTL compris. Le test de parité couvre les clés, les valeurs vides
   et les marqueurs `{…}` ; chaque dictionnaire reste un chunk séparé.

4. **Le PAKE est tranché et livré** : `spake2` (RustCrypto), épinglé, licence
   vérifiée contre `deny.toml`. L'appairage exige la confirmation d'empreinte
   des deux côtés.

5. **Le compteur de version de la liste d'appareils est déjà dérivé de
   l'horodatage** (`version_for(now_ms)` dans `accord-node/src/device.rs`), et
   la fusion prend `max(horodatage, plus haute des deux + 1)` : une horloge en
   retard ne peut donc pas produire une version que les pairs refuseraient. Le
   piège annoncé ici le 25 est fermé.

6. **B3 a changé de cause** — voir §3.3. L'attribut `com.apple.macl` ne bloque
   plus rien ; ce qui bloquait le 2026-07-27 était CMake 4, corrigé dans
   `scripts/build-macos.sh`.

#### Ce qui reste ouvert

7. **Allumer l'émission des capacités** (`EndpointConfig::capabilities`) quand
   le parc 6.2 sera répandu. Changement d'une ligne. Tant que c'est à `None`,
   le champ est déployé mais muet — c'est voulu, pas un oubli.

8. **Le jalon 2 (post-quantique, 8.0)** est le prochain gros morceau ; les lots
   2.A à 2.D existent sur une branche non fusionnée.

9. **Trois manques connus, documentés et assumés** — aucun n'est un bloqueur,
   aucun n'est caché :
   - un message filtré par l'automod qui vous mentionne allume quand même la
     pastille de mention ;
   - les extraits de recherche contournent le masque de l'automod ;
   - `playwright.config.ts` fige le port 1420 avec `reuseExistingServer: true`,
     donc deux sessions simultanées se marchent dessus et produisent des échecs
     qui ne disent rien du code.

### 19.6 Comment reprendre ce document après une longue pause

1. Lire la **partie 3** — les bloqueurs ont-ils bougé ?
2. Lire la **partie 19.5** — les actions urgentes sont-elles faites ?
3. Vérifier l'état réel : `git log --oneline -20`, `gh release list`, `./ci.sh`.
4. Comparer les **métriques** (1.2) à la réalité — le bundle a-t-il dérivé ? les tests ont-ils baissé ?
5. Relire le **journal des décisions** (19.3) avant de remettre en cause un choix : il y a peut-être une raison oubliée.
6. Mettre ce document à jour **avant** de coder, pas après.

🔒 **Un plan qui ne bouge pas est un plan qu'on ne suit pas.** Ce document doit être corrigé à chaque fin de jalon : ce qui s'est vérifié, ce qui s'est trompé, ce qu'on a appris.

---

*Feuille de route rédigée le 2026-07-24, sur la base du code réel en v6.1.0.*
*Horizon 12 mois : jalons 0 à 4 en partie 5 à 9, jalons 5 à 8 en partie 18.*
*À relire et corriger à la fin de chaque jalon — un plan qui ne bouge pas est un plan qu'on ne suit pas.*

---

