#!/usr/bin/env bash
#
# Déploie website/ sur le serveur local : copie les fichiers puis sert le site
# via un conteneur Docker nginx (accord-site). Idempotent — relançable à chaque
# mise à jour du site ; les fichiers étant montés en volume, un simple re-run
# suffit (pas de redémarrage du conteneur).
#
# Prérequis : accès SSH par clé au serveur, Docker installé côté serveur.
#   ACCORD_SITE_SSH  (défaut antho@192.168.1.51)
#   ACCORD_SITE_PORT (défaut 8090) — port HTTP local, à proxifier ensuite
#                    (Nginx Proxy Manager → http://IP_SERVEUR:8090).
#
set -euo pipefail

SERVEUR="${ACCORD_SITE_SSH:-antho@192.168.1.51}"
PORT="${ACCORD_SITE_PORT:-8090}"
RACINE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
HOTE="${SERVEUR#*@}"

echo "== Copie de website/ vers $SERVEUR:~/accord-site/www =="
# Remplace le CONTENU du dossier (jamais le dossier lui-même) : le bind mount
# du conteneur pointe sur son inode — un `mv` du dossier laisserait nginx
# servir l'ancienne version jusqu'au redémarrage.
COPYFILE_DISABLE=1 tar --no-xattrs -C "$RACINE/website" -cf - . | ssh "$SERVEUR" '
  set -e
  mkdir -p ~/accord-site/www
  find ~/accord-site/www -mindepth 1 -delete
  tar -C ~/accord-site/www -xf -
'

echo "== Conteneur nginx (accord-site → port $PORT) =="
ssh "$SERVEUR" "
  set -e
  if ! docker ps --format '{{.Names}}' | grep -qx accord-site; then
    docker rm -f accord-site >/dev/null 2>&1 || true
    docker run -d --name accord-site --restart unless-stopped \
      -p $PORT:80 \
      -v \"\$HOME/accord-site/www:/usr/share/nginx/html:ro\" \
      nginx:alpine >/dev/null
    echo 'conteneur créé'
  else
    echo 'conteneur déjà actif — fichiers rafraîchis'
  fi
"

echo "== Vérification =="
sleep 1
CODE=$(curl -fsS -o /dev/null -w '%{http_code}' "http://$HOTE:$PORT/" || echo "ECHEC")
echo "http://$HOTE:$PORT/ → $CODE"
[ "$CODE" = "200" ] || { echo "Le site ne répond pas — vérifier 'docker logs accord-site' sur le serveur." >&2; exit 1; }
echo "Site en ligne (local) : http://$HOTE:$PORT/  (en/ pour l'anglais)"
