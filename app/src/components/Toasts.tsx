/** Notifications éphémères en bas de l'écran. */

import { useUi } from '../stores/ui';

/** Icône d'alerte (toast d'erreur) — voir ICON SPEC, styles/global.css. */
function ErrorToastIcon() {
  return (
    <svg
      width="16"
      height="16"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
    >
      <circle cx="12" cy="12" r="10" />
      <line x1="12" x2="12" y1="8" y2="12" />
      <line x1="12" x2="12.01" y1="16" y2="16" />
    </svg>
  );
}

/** Icône d'information (toast neutre) — voir ICON SPEC, styles/global.css. */
function InfoToastIcon() {
  return (
    <svg
      width="16"
      height="16"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
    >
      <circle cx="12" cy="12" r="10" />
      <line x1="12" x2="12" y1="16" y2="12" />
      <line x1="12" x2="12.01" y1="8" y2="8" />
    </svg>
  );
}

export function Toasts() {
  const toasts = useUi((s) => s.toasts);
  const dismiss = useUi((s) => s.dismissToast);

  if (toasts.length === 0) return null;

  return (
    <div
      className="pointer-events-none fixed bottom-6 left-1/2 z-50 flex w-[min(420px,calc(100vw-2rem))] -translate-x-1/2 flex-col gap-2"
      role="status"
      aria-live="polite"
    >
      {toasts.map((toast) => (
        <button
          key={toast.id}
          type="button"
          onClick={() => dismiss(toast.id)}
          className={`glass-strong pointer-events-auto flex min-h-11 w-full items-center gap-2.5 rounded-lg px-4 py-2.5 text-left text-sm font-medium text-norm shadow-2 transition-transform duration-fast focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-modal active:scale-[0.98] ${
            toast.leaving === true ? 'toast-leaving' : 'toast-enter'
          }`}
        >
          <span
            aria-hidden
            className={`flex h-4 w-4 shrink-0 items-center justify-center ${
              toast.kind === 'error' ? 'text-red' : 'text-blurple'
            }`}
          >
            {toast.kind === 'error' ? <ErrorToastIcon /> : <InfoToastIcon />}
          </span>
          <span className="min-w-0 flex-1 break-words text-pretty">{toast.text}</span>
        </button>
      ))}
    </div>
  );
}
