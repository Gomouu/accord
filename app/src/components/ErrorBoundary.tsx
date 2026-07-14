/**
 * Garde-fou de rendu : capture les exceptions levées pendant le rendu du
 * sous-arbre et affiche un écran de repli traduit au lieu de laisser toute
 * l'application en écran blanc.
 *
 * Composant de classe : React ne fournit `componentDidCatch` qu'aux classes.
 * L'enveloppe fonctionnelle exportée fournit les libellés (les hooks sont
 * interdits en classe) et la clé de réinitialisation.
 */

import { Component, type ErrorInfo, type ReactNode } from 'react';
import { useT } from '../stores/ui';

interface InnerProps {
  title: string;
  reloadLabel: string;
  /** Changer de clé (navigation) retente un rendu normal après une capture. */
  resetKey: string;
  children: ReactNode;
}

interface InnerState {
  hasError: boolean;
}

class ErrorBoundaryInner extends Component<InnerProps, InnerState> {
  state: InnerState = { hasError: false };

  static getDerivedStateFromError(): InnerState {
    return { hasError: true };
  }

  componentDidCatch(error: unknown, info: ErrorInfo): void {
    // Trace de diagnostic volontaire : l'erreur serait sinon invisible.
    console.error('[ErrorBoundary]', error, info.componentStack);
  }

  componentDidUpdate(prev: InnerProps): void {
    if (this.state.hasError && prev.resetKey !== this.props.resetKey) {
      this.setState({ hasError: false });
    }
  }

  render(): ReactNode {
    if (!this.state.hasError) return this.props.children;
    return (
      <div
        role="alert"
        className="app-ambient flex h-full items-center justify-center bg-chat p-8"
      >
        <div className="flex max-w-sm flex-col items-center rounded-xl border border-[color:var(--glass-border)] bg-sidebar px-8 py-7 text-center shadow-2">
          <span
            aria-hidden
            className="mb-4 flex h-12 w-12 items-center justify-center rounded-full bg-red/10 text-red ring-1 ring-red/20"
          >
            <svg
              width="22"
              height="22"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth={2}
              strokeLinecap="round"
              strokeLinejoin="round"
            >
              <path d="M10.3 2.9 1.8 17a2 2 0 0 0 1.7 3h17a2 2 0 0 0 1.7-3L13.7 2.9a2 2 0 0 0-3.4 0Z" />
              <path d="M12 9v4" />
              <path d="M12 17h.01" />
            </svg>
          </span>
          <p className="text-balance text-lg font-semibold text-header">
            {this.props.title}
          </p>
          <button
            type="button"
            onClick={() => window.location.reload()}
            className="mt-5 min-h-10 rounded-lg bg-blurple px-5 py-2 text-sm font-medium text-white shadow-1 transition-[transform,background-color,box-shadow] duration-fast hover:-translate-y-px hover:bg-blurple-hover hover:shadow-2 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar active:translate-y-0 active:scale-[0.98]"
          >
            {this.props.reloadLabel}
          </button>
        </div>
      </div>
    );
  }
}

export function ErrorBoundary({
  children,
  resetKey = '',
}: {
  children: ReactNode;
  resetKey?: string;
}) {
  const t = useT();
  return (
    <ErrorBoundaryInner
      title={t.errors.boundaryTitle}
      reloadLabel={t.errors.boundaryReload}
      resetKey={resetKey}
    >
      {children}
    </ErrorBoundaryInner>
  );
}
