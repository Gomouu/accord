#!/usr/bin/env bash
#
# Build local du bundle macOS d'Accord (DMG + application .app).
#
# À lancer SUR macOS. Produit par défaut un binaire UNIVERSEL
# (Apple Silicon aarch64 + Intel x86_64), identique à celui du workflow CI.
#
# Prérequis :
#   - macOS (Xcode Command Line Tools : `xcode-select --install`)
#   - Node 20+ et npm
#   - Rust stable (rustup) — https://rustup.rs
#
# La cible x86_64-apple-darwin est ajoutée automatiquement plus bas pour
# permettre le binaire universel ; sinon, forcer un build natif seul avec :
#   ACCORD_CIBLE=aarch64-apple-darwin ./scripts/build-macos.sh
#
set -euo pipefail

# Racine du dépôt (le script vit dans scripts/).
RACINE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$RACINE"
export PATH="$HOME/.cargo/bin:$PATH"

# audiopus_sys embarque un CMakeLists ancien ; CMake >= 4 refuse désormais son
# `cmake_minimum_required`. Le build universel N'A PAS d'échappatoire par
# pkg-config : Homebrew ne fournit libopus que pour l'architecture native, donc
# la seconde tranche du binaire universel passe forcément par le Opus embarqué,
# donc par CMake. Sans cette variable, le build échoue sur une machine à jour —
# c'est la même que celle posée par `.github/workflows/ci.yml`.
export CMAKE_POLICY_VERSION_MINIMUM="${CMAKE_POLICY_VERSION_MINIMUM:-3.5}"

# Cible de compilation : universelle par défaut.
CIBLE="${ACCORD_CIBLE:-universal-apple-darwin}"

echo "== Vérification de la plateforme =="
if [[ "$(uname)" != "Darwin" ]]; then
  echo "Erreur : ce script doit être exécuté sur macOS." >&2
  exit 1
fi

echo "== Ajout des cibles Rust (binaire universel) =="
# Ces cibles sont nécessaires pour un binaire universel ; l'ajout est idempotent.
rustup target add aarch64-apple-darwin x86_64-apple-darwin

echo "== Installation des dépendances frontend (si nécessaire) =="
cd "$RACINE/app"
[ -d node_modules ] || npm ci

echo "== Signature (identité stable) =="
# macOS attache les autorisations TCC (micro) et l'accord du pare-feu à la
# SIGNATURE du binaire. En signature ad-hoc (défaut sans identité), chaque
# build produit une empreinte différente : macOS redemande alors le micro à
# chaque nouvelle build et le pare-feu redemande les connexions ENTRANTES
# (indispensables en P2P) à chaque lancement. Une identité stable — même un
# simple certificat auto-signé local — fait persister les deux accords.
#
# Priorité : $ACCORD_SIGNING_IDENTITY > $APPLE_SIGNING_IDENTITY déjà posée >
# certificat local « Accord Dev » s'il existe > ad-hoc (avec avertissement).
if [[ -n "${ACCORD_SIGNING_IDENTITY:-}" ]]; then
  export APPLE_SIGNING_IDENTITY="$ACCORD_SIGNING_IDENTITY"
elif [[ -z "${APPLE_SIGNING_IDENTITY:-}" ]] \
  && security find-identity -v -p codesigning 2>/dev/null | grep -q '"Accord Dev"'; then
  export APPLE_SIGNING_IDENTITY="Accord Dev"
fi
if [[ -n "${APPLE_SIGNING_IDENTITY:-}" ]]; then
  echo "Identité de signature : $APPLE_SIGNING_IDENTITY"
else
  # « - » = signature ad-hoc COMPLÈTE du bundle par Tauri (Info.plist lié,
  # ressources scellées, identifiant = fr.accord.desktop). Sans elle, le
  # bundle sort simplement « linker-signed » : TCC ne peut pas rattacher
  # durablement l'accord micro et macOS REDEMANDE même après acceptation.
  export APPLE_SIGNING_IDENTITY="-"
  cat <<'AVERTISSEMENT'
Signature ad-hoc complète (aucune identité stable trouvée).
  L'accord micro et pare-feu persiste pour CE build, mais macOS redemandera
  après chaque REBUILD (empreinte différente). Pour une identité stable
  locale (une seule fois) :
    voir docs/DISTRIBUTION.md § « Signature locale stable (macOS) ».
AVERTISSEMENT
fi

echo "== Build Tauri (cible : $CIBLE) =="
# CI=true évite l'échec cosmétique du DMG (script AppleScript de mise en page de
# la fenêtre du DMG, qui échoue notamment sans session graphique interactive).
#
# `|| CODE=$?` : le code de sortie est jugé plus bas, sur les artefacts réels.
# Raison — tauri termine en ERREUR quand il ne peut pas SIGNER l'artefact de
# mise à jour (pas de `TAURI_SIGNING_PRIVATE_KEY`), alors même que le DMG et le
# .app sont produits et signés. Or ce script fait une app de TEST local : la
# signature de mise à jour n'a de sens que pour une release, que seule la CI
# produit. Faire échouer le script là-dessus revient à jeter un build complet
# pour un artefact dont on n'a pas l'usage.
CODE=0
CI=true npx tauri build --target "$CIBLE" || CODE=$?

# Emplacement des artefacts : target/<cible>/release/bundle/{dmg,macos}
BUNDLE="$RACINE/target/$CIBLE/release/bundle"

echo ""
echo "== Artefacts produits =="
if compgen -G "$BUNDLE/dmg/*.dmg" > /dev/null && [ -d "$BUNDLE/macos" ]; then
  ls -la "$BUNDLE/dmg" 2>/dev/null || true
  ls -la "$BUNDLE/macos" 2>/dev/null || true
  echo ""
  echo "Dossier des bundles : $BUNDLE"
  if [ "$CODE" -ne 0 ]; then
    cat <<'AVERTISSEMENT'

⚠️ tauri a terminé en erreur APRÈS avoir produit les bundles — voir la sortie
  ci-dessus. Le cas normal est l'absence de TAURI_SIGNING_PRIVATE_KEY : le
  `.app.tar.gz` n'est alors pas accompagné de son `.sig`, donc cette build ne
  peut pas servir de mise à jour. Le DMG et le .app, eux, sont utilisables.
AVERTISSEMENT
  fi
else
  echo "Aucun bundle trouvé sous $BUNDLE — vérifier la sortie du build ci-dessus." >&2
  exit "${CODE:-1}"
fi
