#!/usr/bin/env bash
# CI locale Accord — le dépôt n'est jamais laissé dans un état où ce script échoue.
# Couvre tout le projet : workspace Rust (crates/* + app/src-tauri) puis frontend app/.
set -euo pipefail
cd "$(dirname "$0")"
export PATH="$HOME/.cargo/bin:$PATH"
# Opus vendu (audiopus_sys) se compile via CMake, et CMake >= 4 a retiré la
# compatibilité avec les projets < 3.5. Sans cette variable, tout changement
# touchant app/src-tauri fait échouer le gate sur un message qui ne parle ni
# d'Accord ni d'audio. `ci.yml` et `scripts/build-macos.sh` la posaient déjà ;
# ici elle manquait, et la panne restait invisible tant que le crate hôte
# n'était pas reconstruit.
export CMAKE_POLICY_VERSION_MINIMUM="${CMAKE_POLICY_VERSION_MINIMUM:-3.5}"

step() { printf '\n\033[1;34m== %s ==\033[0m\n' "$1"; }

# --- Rust (workspace complet, hôte Tauri inclus) ---

step "Rust: cargo fmt --all --check"
cargo fmt --all --check

# `debug_assert_with_mut_call` (nursery) : interdit tout appel mutant dans un
# `debug_assert!` — son argument n'est PAS évalué en release. Cette classe de
# bug a produit la 3.0.0→3.3.0 sans sessions initiateur. `ci.yml` posait le lint
# ici, `ci.sh` non : un `debug_assert!` mutant écrit dans un test d'intégration
# ou un bench passait le gate local et n'échouait qu'en CI.
step "Rust: cargo clippy --workspace --all-targets -D warnings"
cargo clippy --workspace --all-targets -- -D warnings -D clippy::debug_assert_with_mut_call

# Zéro panic en chemin de production (D23) : unwrap/expect/panic!/todo! sont
# interdits dans les libs et binaires (les tests inline sont exclus via
# clippy.toml, les tests d'intégration via la portée --lib --bins). Les rares
# infaillibilités prouvées portent un #[allow] justifié en commentaire.
# Même famille de garde-fous que debug_assert_with_mut_call (régression 3.0.0).
step "Rust: clippy anti-panic (libs + bins)"
cargo clippy --workspace --lib --bins -- -D warnings \
  -D clippy::debug_assert_with_mut_call \
  -D clippy::unwrap_used -D clippy::expect_used \
  -D clippy::panic -D clippy::todo -D clippy::unimplemented

step "Rust: cargo test --workspace"
cargo test --workspace --quiet

# Le transport en PROFIL RELEASE. Le comportement du code peut diverger entre
# profils — un `debug_assert!` n'est pas évalué en release, ce qui est
# exactement la régression 3.0.0 — donc une suite verte en debug ne dit rien de
# la release. Ces e2e SimNet sont hermétiques (réseau simulé en mémoire ;
# `tcp_link_e2e` reste exclu, il ouvre de vraies sockets). Le pas existait dans
# `ci.yml` et manquait ici : le gate local ne voyait pas ce que la CI bloque.
step "Rust: tests transport (release)"
cargo test -p accord-transport --release --quiet \
  --test handshake_e2e --test hole_punch_e2e \
  --test relay_e2e --test relay_tunnel_e2e

# Audits de chaîne d'approvisionnement — optionnels en local (binaires non
# fournis par rustup), obligatoires en CI (.github/workflows/ci.yml). Si un
# binaire manque, on avertit sans faire échouer le gate local.
step "Rust: cargo deny/audit (si installés)"
if command -v cargo-deny >/dev/null 2>&1; then
  cargo deny check
else
  printf 'avertissement: cargo-deny absent — étape sautée (cargo install cargo-deny)\n'
fi
if command -v cargo-audit >/dev/null 2>&1; then
  cargo audit
else
  printf 'avertissement: cargo-audit absent — étape sautée (cargo install cargo-audit)\n'
fi

# --- Frontend (app/) ---

if [ -d app ] && [ -f app/package.json ]; then
  step "UI: install (si nécessaire)"
  # Les accolades ne sont pas décoratives : sans elles, `cd app && test || npm`
  # se lit `(cd && test) || npm`, et un `cd` qui échoue lançait `npm ci` à la
  # RACINE du dépôt — installation dans le mauvais dossier, sans le moindre
  # message.
  (cd app && { [ -d node_modules ] || npm ci --no-audit --no-fund; })

  step "UI: typecheck (tsc --noEmit)"
  (cd app && npx tsc --noEmit)

  step "UI: eslint"
  (cd app && npm run lint --silent)

  step "UI: prettier --check"
  (cd app && npx prettier --check src)

  step "UI: tests (vitest run)"
  (cd app && npm test --silent)

  step "UI: build de production"
  (cd app && npm run build --silent)

  step "UI: budget du chunk initial"
  node scripts/check-bundle-budget.mjs

  # Dette D3 (ROADMAP §1.3) : mesurée deux fois, écrite deux fois, et les
  # fichiers avaient grossi les deux fois. Un cliquet plutôt qu'un vœu.
  step "Cliquet des fichiers de plus de 800 lignes"
  node scripts/check-file-size.mjs

  # docs/API.md a annoncé « 1 MiB » de taille maximale de message alors que le
  # code dit 16 MiB — non par dérive, mais faux depuis le premier commit. Un
  # auteur de client tiers y lisait une contrainte inexistante.
  step "Constantes publiques citées dans les docs"
  node scripts/check-doc-constants.mjs

  # SECURITY.md promet que « les secrets ne sont jamais journalisés » et dit que
  # ce qui tient la promesse est « une règle sur les appels tracing ». Une règle,
  # donc rien d'exécutable : la revue d'audit a constaté qu'aucun lint ne la
  # faisait respecter, sur 201 appels.
  step "Secrets absents des appels de journalisation"
  node scripts/check-log-secrets.mjs

  # Les e2e d'interface étaient hors du gate : « Marquer comme lu » a disparu
  # du menu serveur à la refonte 4.5.0 et la régression a été publiée, alors
  # que le test qui la couvrait existait déjà et échouait dans son coin.
  step "UI: e2e Playwright (interface réelle)"
  (cd app && npx playwright test --reporter=line)
fi

printf '\n\033[1;32mCI OK\033[0m\n'
