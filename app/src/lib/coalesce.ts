/**
 * Regroupement de rafales, par clé.
 *
 * Certains événements du nœud arrivent par paquets serrés : `event.group_state`
 * est émis à CHAQUE op insérée, si bien que rejoindre un serveur de 500 membres
 * en produit ~1 092 d'affilée (`docs/PERFORMANCE.md` §3.2). Traiter chacun
 * séparément revient à recharger tout l'état autant de fois.
 */

/**
 * Enveloppe `run` de sorte qu'une rafale de N appels sur la même clé coûte deux
 * exécutions au plus : celle en vol, plus une dernière qui rattrape tout ce que
 * les autres auraient fait.
 *
 * 🔒 **Ne convient qu'à un travail idempotent qui lit un instantané complet.**
 * Sauter les tours intermédiaires n'est licite que si le dernier porte ce que
 * les précédents auraient porté ; sur un flux de deltas, ce serait une perte
 * silencieuse. L'appelant doit vérifier que c'est bien son cas.
 *
 * Les clés sont indépendantes : une clé bavarde ne fait pas taire les autres.
 */
export function coalescePerKey(
  run: (key: string) => Promise<void>,
): (key: string) => Promise<void> {
  /** Clés dont une exécution est en vol. */
  const busy = new Set<string>();
  /** Clés rappelées pendant leur exécution — un tour de plus leur est dû. */
  const again = new Set<string>();

  return async (key) => {
    if (busy.has(key)) {
      again.add(key);
      return;
    }
    busy.add(key);
    try {
      do {
        // Effacé AVANT le tour : un appel arrivant pendant celui-ci doit en
        // redemander un autre. L'effacer après avalerait cette demande.
        again.delete(key);
        await run(key);
      } while (again.has(key));
    } finally {
      busy.delete(key);
      again.delete(key);
    }
  };
}
