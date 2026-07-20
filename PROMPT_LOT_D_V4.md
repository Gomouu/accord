# Prompt pour l'autre Claude Code — Lot D (route vers la v4 d'Accord)

> Copie-colle tout ce qui suit à l'autre session Claude Code, ouverte dans le
> même dépôt `accord`.

---

Tu prends en charge le **Lot D de la v4 d'Accord** : robustesse réseau P2P,
sécurité, performance backend, observabilité, tests/CI et distribution.
Le détail (78 points) est dans `PLAN_V4.md` à la racine, section « LOT D » — lis-la
en entier avant de commencer.

Accord est une messagerie **P2P chiffrée de bout en bout, sans serveur**, façon
Discord (workspace Rust de 9 crates + hôte Tauri 2). ~72 500 lignes Rust. La base
est **très mûre et bien testée** (~380 tests Rust, CI verte, zéro TODO/FIXME dans
les crates). Ton job n'est **pas** d'ajouter des fonctionnalités manquantes : c'est
de **durcir, prouver, instrumenter et sécuriser** un socle déjà solide.

## ⚠️ À lire en premier : ne reconstruis pas l'existant

Beaucoup de choses que d'anciennes notes de `PROGRESS.md` marquent « RESTE » sont
en réalité **déjà faites**. Exemple concret : la traversée NAT complète — tunnel
client de relais, sélection de relais (`crates/accord-node/src/node/relay.rs`),
repli hole-punch→relais, classification NAT — **existe et passe les tests**
(`crates/accord-transport/tests/relay_tunnel_e2e.rs`). **Vérifie toujours l'état
réel du code avant d'implémenter** ; `PROGRESS.md` peut avoir des mois de retard.
Si tu crois avoir trouvé un « trou », prouve-le d'abord par un test qui échoue.

## Contraintes produit non négociables

- **Aucune infrastructure centralisée / hébergée.** Le propriétaire n'a pas de
  VPS. Un « relais » est un **pair qui a opté** (`relay_serving`), jamais un
  serveur à héberger. Ne propose ni bootstrap payant, ni service tiers, ni
  télémétrie, ni endpoint distant. Tout reste P2P.
- **Vie privée d'abord.** Aucune fuite de métadonnées, aucun appel réseau sortant
  non sollicité, rien qui trace l'utilisateur.
- **Compat wire 3.x.** Ne casse pas le protocole filaire (bandes de kinds, champs
  additifs seulement). Les amis de l'utilisateur tournent sur des versions 3.x
  qui se mettent à jour via GitHub : un changement cassant coupe la communication.
- **Zéro panic en production.** Pas de `unwrap`/`expect`/`panic!`/`todo!` hors
  tests dans les chemins d'exécution. (C'est exactement la classe de bug qui a
  causé une **panne totale de messagerie 3.0→3.3** : un `debug_assert!` avalait
  `install_session` — non évalué en release. Verrouille ça avec des lints.)

## Périmètre de fichiers (strict)

- **Tu modifies UNIQUEMENT** : `crates/**`, `app/src-tauri/src/**`, `fuzz/**`,
  `.github/workflows/**`, `scripts/**`, `deny.toml`, et les *dépendances* de
  `Cargo.toml` (jamais le champ `version`).
- **Tu ne touches JAMAIS** : `app/src/**` (front React — lot du propriétaire),
  `website/**`, ni les fichiers de suivi `ROADMAP.md`, `PROGRESS.md`,
  `DECISIONS.md`, `CHANGELOG.md`, `PLAN_V4.md`, `README.md`.
- **Tu ne bumps pas la version, tu ne tags rien, tu ne fais aucune release**
  (`gh release`, tags `v*` : interdits). Tu livres du code + un rapport ; le
  propriétaire merge, met à jour les fichiers de suivi et release.
- La **clé de signature updater** (`~/.tauri/…`) ne doit **jamais** être committée.
- Travaille sur une **branche dédiée** (ex. `feat/lot-d-reseau`), pas sur `main`.

## Environnement (obligatoire, sinon rien ne compile)

```bash
source "$HOME/.cargo/env"
export CMAKE_POLICY_VERSION_MINIMUM=3.5
export PATH="/opt/homebrew/opt/node@22/bin:$PATH"
```

## Priorités, dans l'ordre — avec définition de « fini »

### P1 — Observabilité & diagnostics *(le plus utile : débloque l'UI du propriétaire)*
La pile réseau est excellente mais **invisible**. `network.status` expose déjà
`p2p_port`, `local_addrs`, `external_addr`, `port_mapping`, `lan_peers`,
`nat_kind`, `connected_peers`, `dht_nodes` (voir
`crates/accord-node/src/node/network.rs`). Ce qui **manque** et que l'UI (carte de
connexion, panneau de diagnostic) attend :
- par **pair** : direct vs relayé, quel relais, latence estimée, dernière remise ;
- **compteurs** : succès/échec de hole-punch, usage de relais, remises via
  boîte aux lettres, reconnexions ;
- un **auto-test réseau** déclenchable (joignabilité, type de NAT, test d'un relais).
- **Fini quand** : nouvelle(s) méthode(s) `diagnostics.*` et/ou champs additifs sur
  `network.status`, plus les événements associés, **documentés dans `docs/API.md`**
  (contrat JSON stable que le propriétaire branchera côté front), couverts par des
  tests, gate vert. Tu n'écris pas d'UI.

### P2 — Stabiliser les tests e2e flaky
Les suites réseau (`calls_e2e`, `reconnexion_e2e`, `maintenance_e2e`,
`tcp_link_e2e`, `profil_reboot_e2e`) échouent par **starvation** sous parallélisme
complet ; elles passent isolées (`--test-threads=1`).
- **Fini quand** : la cause est traitée à la racine (sérialisation/isolation propre,
  ou budget de ressources), prouvée par plusieurs exécutions vertes en parallélisme
  complet, sans marquer de test `ignore`.

### P3 — Sécurité & fuzzing
- **Revue adverse** des surfaces récentes (droits, forge, amplification, rejeu) :
  sauvegarde chiffrée `crates/accord-crypto/src/archive.rs`, invitations en MP,
  import de sauvegarde `crates/accord-node/src/backup.rs`.
- **Chasse aux panics** en prod + lints clippy qui les interdisent (dans la lignée
  de la régression `debug_assert`).
- **Fuzzing** : au-delà des 3 cibles (`fuzz/fuzz_targets/{core_msg,group_op_body,
  proto_decode}.rs`), ajoute handshake, session AEAD, état de groupe, records DHT,
  manifests fichiers, et **archive de sauvegarde**.
- **Fini quand** : failles trouvées corrigées + testées, lints anti-panic actifs en
  CI, nouvelles cibles de fuzz tournant une campagne bornée sans crash, corpus committé.

### P4 — Anti-régression perf
- **Benchmarks criterion** : handshake/AEAD (crypto), codec voix, décodage proto,
  requêtes DB (`accord-core/src/db/messages.rs`).
- **Property tests** (proptest) : codecs `accord-proto`, repli déterministe du CRDT
  de groupe, codes amis.
- **Fini quand** : benches reproductibles committés, property tests verts, gate vert.

### Ensuite
Le reste du Lot D (D5–D16, D24–D38, D52–D78) selon le temps, même discipline.

## Méthode de travail

1. **Lis avant de coder** : `PLAN_V4.md` (LOT D), `docs/SPEC.md` §10–§11 (relais/NAT),
   `docs/THREAT-MODEL.md`, puis `crates/accord-node/src/node/relay.rs` +
   `crates/accord-transport/src/{relay.rs,endpoint.rs,nat.rs}` + les 5 suites e2e.
2. **Prouve par un test** l'existence d'un problème avant de le corriger (TDD).
3. **Toute nouvelle surface réseau passe en revue adverse** avant d'être dite finie.
4. **Petits commits** conventionnels (`feat:`/`fix:`/`test:`/`perf:`/`chore:`),
   messages en français, attribution désactivée globalement (n'ajoute pas de
   `Co-Authored-By`).
5. **N'utilise pas** d'agents/sous-agents/workflows multi-agents (coût tokens).
6. Réponds et code **en français**, concis, **zéro commentaire de bruit** (les
   commentaires expliquent le *pourquoi*, jamais ne paraphrasent le code) ;
   immutabilité, petits fichiers (< 800 lignes), erreurs explicites.

## Porte de sortie (gate — tout doit passer avant « fini »)

```bash
source "$HOME/.cargo/env"; export CMAKE_POLICY_VERSION_MINIMUM=3.5
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings -D clippy::debug_assert_with_mut_call
cargo test --workspace --lib
cargo test -p accord-transport --release --test handshake_e2e --test hole_punch_e2e --test relay_e2e --test relay_tunnel_e2e
cargo deny check
cargo audit
```
Les suites e2e réseau doivent être **stables** (prouve-le par des runs répétés en
parallélisme complet). Les cibles de fuzz : campagne bornée sans crash.

## Livrable

Une **branche** + un **rapport final en français** : problèmes prouvés (avec les
tests qui échouaient), ce qui a changé, fichiers touchés, tests/benches ajoutés,
revues adverses menées, résultats du gate, contrat `diagnostics.*` documenté, et
ce qui reste. Le propriétaire vérifie, merge, met à jour le suivi et release.

## Ne fais pas (récap)

- ❌ reconstruire la pile NAT/relais (elle existe) — audite/durcis/instrumente.
- ❌ infra hébergée, VPS, bootstrap centralisé, service tiers, télémétrie.
- ❌ changement wire cassant la compat 3.x.
- ❌ `unwrap`/`expect`/`panic!` en prod ; ❌ toucher au front ou aux fichiers de suivi.
- ❌ bump de version, tag, release.

Commence par lire l'état réel du code (étape 1), puis attaque **P1 (observabilité)**.
