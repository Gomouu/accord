#!/usr/bin/env bash
# CI locale Accord — le dépôt n'est jamais laissé dans un état où ce script échoue.
# Couvre tout le projet : workspace Rust (crates/* + app/src-tauri) puis frontend app/.
set -euo pipefail
cd "$(dirname "$0")"
export PATH="$HOME/.cargo/bin:$PATH"

step() { printf '\n\033[1;34m== %s ==\033[0m\n' "$1"; }

# --- Rust (workspace complet, hôte Tauri inclus) ---

step "Rust: cargo fmt --all --check"
cargo fmt --all --check

step "Rust: cargo clippy --workspace --all-targets -D warnings"
cargo clippy --workspace --all-targets -- -D warnings

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
  (cd app && [ -d node_modules ] || npm ci --no-audit --no-fund)

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
fi

printf '\n\033[1;32mCI OK\033[0m\n'
