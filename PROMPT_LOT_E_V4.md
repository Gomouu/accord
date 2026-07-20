# Lot E — Confidentialité vérifiable (handoff Claude Code)

> À coller en entier dans l'autre Claude Code, ou en-tête + une section E1/E2/E3 à la fois.
> Le propriétaire (moi) vérifie, merge, bump la version et coupe la release. **Toi, tu ne bumps rien, tu ne tags rien.**

## Directives strictes
- **Français** partout (code, messages de commit, docs).
- **Zéro commentaire de paraphrase** dans le code (pas de `//` ni `#` qui redisent ce que fait la ligne). Commentaire seulement si un invariant non évident l'exige.
- **Concision, autonomie maximale.** Tu ne reviens pas tant que le lot n'est pas fini et le gate vert.
- **Pas de sous-agents / workflows** (coût tokens).
- **Compat wire 3.x — RÈGLE DURE :** les 3 features sont **100 % locales**. **Aucun** nouveau/modifié format de message réseau, aucun octet ajouté sur le fil. Si un choix de conception te pousse vers un changement wire : **tu t'arrêtes et tu laisses une note**, tu ne touches pas au wire.
- **Zéro panic en prod :** respecte `clippy.toml` (interdits `unwrap/expect/panic/todo/unimplemented` dans lib+bins ; tolérés en tests). Gère les erreurs explicitement.
- **Immutabilité, fichiers courts (<400 l., 800 max), KISS/DRY/YAGNI.**

## Branche
`feat/lot-e-confidentialite`, basée sur `main` à jour (4.1.0).

## Environnement
```bash
source "$HOME/.cargo/env"; export CMAKE_POLICY_VERSION_MINIMUM=3.5     # Rust
export PATH="/opt/homebrew/opt/node@22/bin:$PATH"                       # Front (Node 26 casse vitest)
```

## Gate (doit être 100 % vert avant de rendre)
Lance `./ci.sh` à la racine (source de vérité). Équivalent explicite :
```bash
# Rust
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings -D clippy::debug_assert_with_mut_call
cargo clippy --workspace --lib --bins -- -D clippy::unwrap_used -D clippy::expect_used -D clippy::panic -D clippy::todo -D clippy::unimplemented
cargo test --workspace --lib
cargo test -p accord-transport --release --test handshake_e2e --test hole_punch_e2e --test relay_e2e --test relay_tunnel_e2e
cargo deny check && cargo audit
# Front
cd app
npx tsc --noEmit && npx eslint src && npx prettier --check "src/**/*.{ts,tsx,css}"
npx vitest run          # LIS la ligne récap "Test Files … / Tests … passed | … failed" — JAMAIS `tail`, ça masque les échecs
npm run build
```
`tsconfig` a `exactOptionalPropertyTypes` + `noUncheckedIndexedAccess` : une prop optionnelle qui peut être `undefined` doit avoir `| undefined` dans le type.

## Zone interdite (mes fichiers Lot A/B — NE PAS TOUCHER)
`app/src/components/{Sidebar,MessageList,MessageInput,Modals,MarkdownText,ChatView,NetworkPanel}.tsx`, `app/src/lib/markdown.ts`, `app/src/styles/*.css`, thèmes (`customTheme.ts`, `figurative-themes*`, `theme-scenes*`), `PLAN_V4.md`.
- Crée des **nouveaux fichiers composants**. N'édite `i18n/{fr,en}.ts` qu'en **ajout en fin de section** (jamais de renommage/réordonnancement des clés existantes). Les points d'accroche dans des fichiers existants doivent être **minimes et isolés** (un import + un bouton/section).
- Nouvelles méthodes RPC → **ajoute des bras de match** dans `crates/accord-node/src/service/{dm,friends,groups}.rs` (peu conflictuel), documente-les dans `docs/API.md`, et ajoute une entrée CHANGELOG sous une section `## [Non publié]` (le propriétaire coupe la version).

---

## E1 — Numéros de sécurité (vérification d'identité anti-MITM, façon Signal)
**But :** rendre visible l'absence de MITM. Deux amis comparent un **numéro de sécurité** dérivé de leurs clés d'identité ; s'il concorde, la conversation est authentifiée. Argument de vente « privacy-first ».

**Dérivation (crypto, `crates/accord-crypto`) — nouvelle fonction + tests :**
- Entrée : les deux clés publiques d'identité ed25519 (32 o chacune). Chaque pair dispose déjà de la sienne (`identity.public_key()`) et de celle de l'ami (`Contact.pubkey`, cf. `crates/accord-core/src/friends.rs`). **Aucun échange réseau nouveau.**
- Trie les deux pubkeys par ordre lexicographique → construction **symétrique** : `safety_number(a,b) == safety_number(b,a)`.
- Itère un hash (SHA-512, ~5200 tours à la Signal sur `version ‖ pubkey ‖ hash_pubkey`), prends 30 octets → **60 chiffres** (12 groupes de 5). Expose aussi un rendu **emoji** (map d'octets → liste fixe de 256 emojis) pour comparaison rapide.
- API : `pub fn safety_number(mine: &[u8;32], theirs: &[u8;32]) -> SafetyNumber` avec `SafetyNumber { digits: String, emoji: Vec<&'static str> }`.

**Stockage (`accord-core` + `db/mod.rs`) :** flag `verified` par contact (bool + `verified_at` + **la pubkey vue au moment de la vérif**, pour détecter un changement de clé). Migration additive de la table `friends`.

**RPC (`service/friends.rs`) :**
- `friends.safety_number { peer } -> { digits, emoji, verified, key_changed }` (`key_changed = true` si la pubkey courante diffère de celle vérifiée).
- `friends.set_verified { peer, verified }`.
- Émets un flag `key_changed`/événement quand la clé d'un ami vérifié change (l'UI avertira « vérification rompue »).

**Frontend (nouveaux fichiers) :** `FriendVerifyModal.tsx` (affiche numéro + emojis + instructions de comparaison + bascule vérifié/non-vérifié), un **badge bouclier** discret surfacé via le ProfilePopover (accroche isolée). i18n en ajout.

**Tests requis :** crypto (déterminisme, symétrie a↔b, sensibilité au changement de clé) ; service (round-trip flag, `key_changed`) ; render du modal avec api mockée.

**Definition of done :** dérivation déterministe & symétrique testée ; flag persistant ; RPC ↔ UI de bout en bout ; 0 octet wire ; gate vert ; `docs/API.md` + CHANGELOG à jour.

---

## E2 — Messages éphémères (auto-suppression locale par conversation)
**But :** minuteur de disparition par conversation, **honoré localement par le client** (C5 : local, pas de négociation wire). Supprime de MON stockage les messages plus vieux que le TTL choisi.

**Stockage (`crates/accord-core/src/db/mod.rs`, près de `dm_messages` l.199 / `group_messages` l.235) :**
- Table additive `conversation_ephemeral(scope TEXT, ttl_secs INTEGER)` où `scope` = pubkey du pair (DM) ou `group_id` (salon). `NULL`/absent = désactivé.

**Purge (`crates/accord-node/src/maintenance.rs`) :** routine planifiée au démarrage **et** périodiquement : pour chaque conversation avec un TTL, `DELETE` des `dm_messages`/`group_messages` dont l'âge > `ttl_secs`, **et** des pièces jointes référencées (`msg_attachments`). Bornée, sans panic.

**RPC (`service/dm.rs`, `service/groups.rs`) :**
- `dm.set_ephemeral { peer, ttl_secs | null }`, `dm.ephemeral { peer } -> ttl_secs | null`.
- Équivalents `groups.set_ephemeral` / `groups.ephemeral`.

**Comportement à documenter (`docs/API.md`) :** purement **local** (supprime seulement sur cet appareil), aucun message de contrôle réseau, aucun octet wire. (Une version négociée bilatérale serait une extension future — hors scope.)

**Frontend (nouveau fichier) :** `EphemeralPicker.tsx` (Désactivé / 1 h / 8 h / 1 j / 7 j…), accroché de façon isolée dans le menu/en-tête de conversation ; badge/compte à rebours optionnel. i18n en ajout.

**Tests requis :** db/node (règle TTL, insère anciens+récents, purge, assert anciens supprimés / récents gardés + jointures purgées) ; round-trip RPC ; variante groupe.

**Definition of done :** TTL réglable & persistant ; purge correcte et bornée ; RPC ↔ UI ; 0 octet wire ; gate vert ; docs + CHANGELOG.

---

## E3 — Tableau de bord vie privée (« tout est local & chiffré »)
**But :** montrer noir sur blanc ce qui est stocké **localement et chiffré**, et que **rien** ne part vers un serveur central. C'est l'argument de vente unique d'Accord, rendu concret.

**Backend (`crates/accord-node/src/service/privacy.rs`, nouveau + bras de dispatch) :** RPC lecture seule `privacy.report -> { … }` agrégeant :
- Comptes locaux : amis, DM, groupes, messages, pièces jointes, épingles.
- Tailles sur disque : fichier DB (chiffré SQLCipher au repos), dossier des pièces jointes.
- Chiffrement : « coffre chiffré au repos (SQLCipher) : oui ».
- Ce qui **sort** de l'appareil : la liste des types d'endpoints réellement contactés (bootstrap, DHT, relais) avec une phrase par ligne — **aucun serveur central**. Réutilise `diagnostics.counters` / `network.*` existants.
- Dernière sauvegarde connue si disponible.
Tout en lecture seule, sans panic.

**Frontend (nouveau fichier) :** `PrivacyDashboard.tsx` rendu comme onglet/section des réglages (accroche isolée), présentation claire « 0 serveur central, 100 % local & chiffré ». i18n en ajout.

**Tests requis :** service (forme du rapport, comptes cohérents avec une DB semée) ; render front avec api mockée.

**Definition of done :** rapport exact & lecture seule ; UI lisible ; 0 octet wire ; gate vert ; docs + CHANGELOG.

---

## Rendu final attendu
Rapporte : nom de branche, liste des commits, résumé par feature (E1/E2/E3), **sortie du gate** (la ligne récap vitest + les totaux `cargo test`), et toute **note de pression wire** rencontrée. Ne bump aucune version, ne tag rien — le propriétaire vérifie, merge, bump et release.
