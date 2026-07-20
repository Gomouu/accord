/**
 * Primitives de chargement (« squelettes ») : un bloc pulsant neutre et une
 * composition qui imite quelques lignes de messages le temps que la première
 * page d'historique arrive. Le pouls Tailwind (`animate-pulse`) est
 * automatiquement neutralisé sous « réduire les animations » (règle globale
 * `prefers-reduced-motion` / `data-motion='reduce'` de global.css) : le bloc
 * reste alors visible, figé.
 */

/** Bloc gris pulsant ; `className` porte la forme (hauteur, largeur, rayon). */
export function Skeleton({ className = '' }: { className?: string }) {
  return <span aria-hidden className={`block animate-pulse bg-sidebar ${className}`} />;
}

/** Largeurs cyclées pour donner un rythme naturel aux fausses lignes. */
const LARGEURS = ['w-3/5', 'w-4/5', 'w-2/5', 'w-3/4', 'w-1/2', 'w-2/3'] as const;

/**
 * Faux fil de discussion (avatar + pseudo + ligne de texte) répété `rows`
 * fois. `label` alimente l'étiquette d'accessibilité (`role="status"`) pour
 * annoncer le chargement au lecteur d'écran.
 */
export function MessageListSkeleton({
  rows = 6,
  label,
}: {
  rows?: number;
  label?: string;
}) {
  return (
    <div
      role="status"
      aria-label={label}
      aria-busy
      className="flex flex-1 flex-col justify-end gap-5 px-4 py-5"
    >
      {Array.from({ length: rows }).map((_, i) => (
        <div key={i} className="flex gap-3">
          <Skeleton className="h-10 w-10 shrink-0 rounded-full" />
          <div className="flex min-w-0 flex-1 flex-col gap-2 pt-1">
            <Skeleton className="h-3 w-24 rounded" />
            <Skeleton className={`h-3 rounded ${LARGEURS[i % LARGEURS.length]}`} />
          </div>
        </div>
      ))}
    </div>
  );
}
