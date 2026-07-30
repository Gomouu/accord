/**
 * Garde de schéma pour toute URL destinée à un attribut `href`.
 *
 * 🔒 Un `href` porte du code quand son schéma le permet : `javascript:…`
 * s'exécute au clic, `data:text/html,…` ouvre un document que l'attaquant
 * écrit. Les URL rendues par l'application viennent toutes d'ailleurs — corps
 * d'un message écrit par un pair, aperçu composé à partir d'une page tierce —
 * donc aucune n'est de confiance à l'endroit où elle est posée.
 *
 * Vit dans `lib/` plutôt que dans un composant : les deux surfaces qui rendent
 * un lien (`MarkdownText`, `LinkPreview`) doivent appliquer LA MÊME règle, et
 * une garde recopiée est une garde qui finit par diverger.
 */

/**
 * Rend `url` si elle est analysable et de schéma `http:`/`https:`, sinon
 * `undefined` — à l'appelant de retomber sur du texte brut.
 */
export function lienHttpSur(url: string): string | undefined {
  try {
    const p = new URL(url);
    if (p.protocol === 'http:' || p.protocol === 'https:') return url;
  } catch {
    // URL non analysable : traitée comme du texte par l'appelant.
  }
  return undefined;
}
