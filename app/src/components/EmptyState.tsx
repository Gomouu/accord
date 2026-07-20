/**
 * État vide réutilisable : icône ronde centrée, libellé, description
 * facultative, et action fondatrice facultative (bouton vert). Mutualise le
 * motif qui était dupliqué (liste d'amis, recherche, futures vues) pour que
 * chaque surface vide soit designée plutôt qu'une ligne morte. `compact`
 * resserre l'espacement pour un conteneur étroit (menu déroulant de recherche).
 */

import type { ReactNode } from 'react';

interface EmptyStateAction {
  label: string;
  onClick: () => void;
  icon?: ReactNode | undefined;
}

interface EmptyStateProps {
  icon: ReactNode;
  label: string;
  description?: string | undefined;
  action?: EmptyStateAction | undefined;
  compact?: boolean | undefined;
}

export function EmptyState({
  icon,
  label,
  description,
  action,
  compact,
}: EmptyStateProps) {
  return (
    <div
      className={`flex flex-col items-center text-center text-muted ${
        compact ? 'gap-2 py-6' : 'gap-3 py-12'
      }`}
    >
      <span
        aria-hidden
        className={`flex items-center justify-center rounded-full bg-sidebar text-faint ${
          compact ? 'h-9 w-9' : 'h-11 w-11'
        }`}
      >
        {icon}
      </span>
      <p className="max-w-xs text-pretty">{label}</p>
      {description !== undefined && (
        <p className="max-w-xs text-pretty text-sm text-faint">{description}</p>
      )}
      {action !== undefined && (
        <button
          type="button"
          onClick={action.onClick}
          className="mt-1 inline-flex h-9 items-center gap-2 rounded-full bg-green px-4 text-sm font-medium text-on-green transition-colors duration-fast hover:brightness-110 active:scale-95 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-green focus-visible:ring-offset-2 focus-visible:ring-offset-chat"
        >
          {action.icon}
          {action.label}
        </button>
      )}
    </div>
  );
}
