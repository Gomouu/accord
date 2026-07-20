# Lot F — Planification & rappels (handoff Claude Code)

> S'enchaîne **après le Lot E**. À coller en entier dans l'autre Claude Code, ou en-tête + une section F1/F2/F3.
> Le propriétaire vérifie, merge, bump la version, release. **Toi, tu ne bumps rien, tu ne tags rien.**

## Idée directrice
Les trois features partagent **un seul primitif local** : des tâches persistées en DB, déclenchées par la boucle périodique existante `crates/accord-node/src/maintenance.rs` (`spawn_periodic`, intervalles jitterés — réutilise-la, n'en crée pas une autre). **100 % local, zéro octet wire.** Un message programmé qui part réutilise le chemin d'envoi normal + l'outbox déjà en place pour les pairs hors-ligne (`is_queueable_offline`).

## Directives strictes
- **Français** partout (code, commits, docs). **Zéro commentaire de paraphrase** (`//`/`#` seulement pour un invariant non évident).
- **Concision, autonomie maximale.** Tu ne reviens pas tant que le lot n'est pas fini et le gate vert. **Pas de sous-agents/workflows.**
- **Compat wire 3.x — RÈGLE DURE :** tout est **local**. Aucun format réseau nouveau/modifié, aucun octet sur le fil. Pression vers un changement wire ⇒ **tu t'arrêtes et tu notes**, tu ne touches pas au wire.
- **Zéro panic en prod** (respecte `clippy.toml` : pas de `unwrap/expect/panic/todo/unimplemented` en lib+bins ; tolérés en tests). Erreurs gérées explicitement.
- **Immutabilité, fichiers <400 l. (800 max), KISS/DRY/YAGNI.**

## Branche & dépendance au Lot E
- Le Lot F **touche les mêmes fichiers backend partagés** que le Lot E (`db/mod.rs`, `maintenance.rs`, `service/dm.rs`). Pour éviter les conflits :
  - **Si le Lot E est déjà mergé dans `main`** → branche `feat/lot-f-planification` depuis `main` à jour.
  - **Sinon** → branche `feat/lot-f-planification` depuis `feat/lot-e-confidentialite`.
- Dans ces fichiers partagés : **ajouts uniquement** (nouvelles tables, nouveaux bras de match, nouveau tick enregistré dans la boucle). Ne réécris pas l'existant.

## Environnement
```bash
source "$HOME/.cargo/env"; export CMAKE_POLICY_VERSION_MINIMUM=3.5     # Rust
export PATH="/opt/homebrew/opt/node@22/bin:$PATH"                       # Front (Node 26 casse vitest)
```

## Gate (100 % vert avant de rendre) — via `./ci.sh`, ou explicitement :
```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings -D clippy::debug_assert_with_mut_call
cargo clippy --workspace --lib --bins -- -D clippy::unwrap_used -D clippy::expect_used -D clippy::panic -D clippy::todo -D clippy::unimplemented
cargo test --workspace --lib
cargo test -p accord-transport --release --test handshake_e2e --test hole_punch_e2e --test relay_e2e --test relay_tunnel_e2e
cargo deny check && cargo audit
cd app && npx tsc --noEmit && npx eslint src && npx prettier --check "src/**/*.{ts,tsx,css}"
npx vitest run     # LIS la ligne récap "Test Files … / Tests … passed | … failed" — JAMAIS `tail`
npm run build
```
`tsconfig` : `exactOptionalPropertyTypes` + `noUncheckedIndexedAccess` actifs (prop optionnelle possiblement `undefined` ⇒ `| undefined` dans le type).

## Zone interdite (mes fichiers Lot A/B)
`app/src/components/{Sidebar,MessageList,MessageInput,Modals,MarkdownText,ChatView,NetworkPanel}.tsx`, `app/src/lib/markdown.ts`, `app/src/styles/*.css`, thèmes, `PLAN_V4.md`.
- Crée des **nouveaux fichiers composants** (panneaux de gestion). `i18n/{fr,en}.ts` en **ajout en fin de section** uniquement.
- **Exception accroche autorisée :** **un seul** point d'entrée isolé par feature dans un fichier existant (ex. un item de menu contextuel « Me le rappeler », un bouton horloge) — un import + un élément, rien de plus. Le gros de l'UI vit dans tes nouveaux panneaux.
- Nouvelles méthodes RPC → bras de match dans `service/*.rs`, documentées dans `docs/API.md`, + entrée CHANGELOG sous `## [Non publié]`.

---

## F1 — Messages programmés (envoi différé local)
**But :** rédiger un message et l'envoyer à une heure choisie (ou « quand le pair est joignable »). Réutilise l'envoi normal + l'outbox.

**Stockage (`crates/accord-core/src/db/mod.rs`, près de `dm_messages` l.199 / `group_messages` l.235) :** table `scheduled_messages(id, scope TEXT, scope_id BLOB, body TEXT, attachments…, fire_at INTEGER, created_at)` — `scope` = `dm`|`group`.

**Déclenchement (`maintenance.rs`) :** nouveau tick enregistré dans la boucle existante : à chaque passe, prend les `scheduled_messages` dus (`fire_at <= now`), les **route par le chemin d'envoi normal** (`service`/`node` dm/group send ; si pair hors-ligne, l'outbox existant prend le relais), puis supprime la ligne. Idempotent, borné, sans panic.

**RPC (`service/dm.rs` + nouveau `service/schedule.rs`) :** `dm.schedule { peer, body, fire_at }`, `schedule.list -> [{ id, scope, fire_at, aperçu }]`, `schedule.cancel { id }`, `schedule.reschedule { id, fire_at }`. Équivalent groupe `groups.schedule`.

**Frontend (nouveaux fichiers) :** `ScheduledMessagesPanel.tsx` (liste : heure, aperçu, annuler/replanifier). Accroche isolée autorisée : un bouton horloge près du compositeur qui ouvre un mini-sélecteur d'heure → `dm.schedule`. i18n en ajout. Formatage d'heure via `Intl`.

**Tests requis :** db/node — insère un message dû + un non-dû, tick, assert le dû est parti (mock du chemin d'envoi) et supprimé, le non-dû reste ; round-trip RPC list/cancel/reschedule.

**Definition of done :** programmer/lister/annuler/replanifier de bout en bout ; déclenchement fiable et borné ; 0 octet wire ; gate vert ; docs + CHANGELOG.

---

## F2 — Rappels sur un message (« me le rappeler dans 3 h »)
**But :** épingler un rappel local sur un message ; à l'heure dite, notification + boîte de rappels. Purement local.

**Stockage (`db/mod.rs`) :** table `reminders(id, scope, scope_id BLOB, msg_ref, note TEXT, fire_at INTEGER, done INTEGER, created_at)`.

**Déclenchement (`maintenance.rs`) :** tick : rappels dus (`fire_at <= now`, `done=0`) → émet un événement `event.reminder { id, scope, scope_id, msg_ref, note }` (motif `event.*` existant) que le front transforme en notification via `app/src/lib/notifications.ts`. Marque `fired` (ne re-déclenche pas), l'utilisateur `dismiss`.

**RPC (nouveau `service/reminders.rs`) :** `reminders.add { scope, scope_id, msg_ref, note, fire_at }`, `reminders.list -> [...]`, `reminders.dismiss { id }`.

**Frontend (nouveaux fichiers) :** `RemindersPanel.tsx` (liste des rappels : message cible, échéance, aller au message, marquer fait). Accroche isolée autorisée : **un** item de menu contextuel de message « Me le rappeler » (préréglages 20 min / 1 h / 3 h / demain / heure perso) → `reminders.add`. i18n en ajout.

**Tests requis :** node — rappel dû émet l'événement une seule fois puis marqué ; `list`/`dismiss` ; render du panneau avec api mockée + assert émission de notification.

**Definition of done :** ajouter/lister/dismiss + notification à l'heure ; pas de double déclenchement ; 0 octet wire ; gate vert ; docs + CHANGELOG.

---

## F3 — Sauvegarde automatique planifiée (rappel + `.accordbackup`)
**But :** ne jamais perdre son identité/historique. Réutilise `crates/accord-node/src/backup.rs` (sauvegarde chiffrée manuelle déjà présente).

**Stockage/réglages (`db/mod.rs` ou table de réglages existante) :** `backup_cadence` (`off`|`weekly`|`monthly`…), `backup_dir` (optionnel), `last_backup_at`.

**Déclenchement (`maintenance.rs`) :** tick : si cadence active et âge de `last_backup_at` > cadence →
- **Mode par défaut = rappel :** émet `event.reminder`/notification « il est temps de sauvegarder » (aucune écriture surprise sur disque).
- **Mode auto opt-in** (si `backup_dir` défini) : crée un `.accordbackup` chiffré dans ce dossier via `backup.rs`, met à jour `last_backup_at`. Bornée, erreurs gérées (dossier absent → repli sur rappel).

**RPC (nouveau `service/backup_schedule.rs` ou extension) :** `backup.schedule { cadence, dir | null }`, `backup.status -> { cadence, dir, last_backup_at, next_due_at }`, `backup.run_now` (déclenche immédiatement le chemin existant).

**Frontend (nouveau fichier) :** `BackupSettingsPanel.tsx` — section réglages : cadence, dossier (via `plugin-dialog` déjà présent), dernière sauvegarde, « sauvegarder maintenant ». i18n en ajout. Dates via `Intl`.

**Tests requis :** node — âge > cadence en mode rappel émet l'événement ; en mode auto écrit le fichier et met à jour `last_backup_at` (dir temporaire) ; `status` cohérent ; render du panneau.

**Definition of done :** cadence réglable, statut exact, rappel **et** auto opt-in fonctionnels, « sauvegarder maintenant » OK ; 0 octet wire ; gate vert ; docs + CHANGELOG.

---

## Rendu final attendu
Rapporte : nom de branche + base (main ou Lot E), commits, résumé par feature (F1/F2/F3), **sortie du gate** (ligne récap vitest + totaux `cargo test`), et toute **note de pression wire**. Ne bump aucune version, ne tag rien — le propriétaire vérifie, merge, bump, release.
