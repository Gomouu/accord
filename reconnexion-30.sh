#!/usr/bin/env bash
#
# Cible §9.3 de la feuille de route : 30 exécutions consécutives sans échec des
# scénarios de reconnexion.
#
# Pourquoi 30 exécutions et pas une. Ces tests montent de vrais nœuds, sur de
# vraies sockets, et attendent des événements réseau. Un passage vert prouve
# qu'ils PEUVENT passer ; il ne dit rien de la fréquence à laquelle ils
# échouent. Les défauts déjà rencontrés ici — dial abandonné trop tôt, WELCOME
# perdu, session cadavre non évincée — se manifestaient une fois sur dix ou
# vingt. Une seule exécution ne les voit pas.
#
# Un échec, où que ce soit dans la série, remet le compteur à zéro : c'est
# « 30 d'affilée », pas « 30 dont quelques-unes ».
#
#   ./reconnexion-30.sh          # 30 exécutions
#   RUNS=5 ./reconnexion-30.sh   # série plus courte, pour une vérification rapide
#
# ⚠️ À lancer sur une machine au repos. Ces tests sont sensibles au temps ;
# une compilation ou un autre test qui tourne en parallèle produit des échecs
# qui ne disent rien du code.
set -uo pipefail
cd "$(dirname "$0")" || exit 1

RUNS=${RUNS:-30}
LOGS=$(mktemp -d)
echecs=0
serie=0
meilleure=0

echo "Reconnexion : $RUNS exécutions consécutives (logs : $LOGS)"
echo

for i in $(seq 1 "$RUNS"); do
  ok=1
  cargo test --release -p accord-node \
    --test reconnexion_e2e --test reconnexion_lifecycle_e2e \
    --test profil_perdu_e2e --test profil_reboot_e2e \
    --test chaos_reseau_e2e \
    -- --test-threads=1 > "$LOGS/$i-node.log" 2>&1 || ok=0
  cargo test --release -p accord-transport \
    --test reconnexion_transport_e2e --test multi_appareil_e2e --test handshake_e2e \
    -- --test-threads=1 > "$LOGS/$i-transport.log" 2>&1 || ok=0

  if [ $ok -eq 1 ]; then
    serie=$((serie + 1))
    [ $serie -gt $meilleure ] && meilleure=$serie
    echo "  $i/$RUNS  ok (série $serie)"
    rm -f "$LOGS/$i-node.log" "$LOGS/$i-transport.log"
  else
    echecs=$((echecs + 1))
    serie=0
    echo "  $i/$RUNS  ÉCHEC — logs conservés dans $LOGS"
    grep -hE '^test .* FAILED|panicked at|test result: FAILED' \
      "$LOGS/$i-node.log" "$LOGS/$i-transport.log" 2>/dev/null | head -6
  fi
done

echo
echo "$echecs échec(s) sur $RUNS ; plus longue série sans échec : $meilleure"
[ $echecs -eq 0 ] || exit 1
