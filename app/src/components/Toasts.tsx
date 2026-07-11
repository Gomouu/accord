/** Notifications éphémères en bas de l'écran. */

import { useUi } from '../stores/ui';

export function Toasts() {
  const toasts = useUi((s) => s.toasts);
  const dismiss = useUi((s) => s.dismissToast);

  if (toasts.length === 0) return null;

  return (
    <div
      className="pointer-events-none fixed bottom-6 left-1/2 z-50 flex -translate-x-1/2 flex-col gap-2"
      role="status"
      aria-live="polite"
    >
      {toasts.map((toast) => (
        <button
          key={toast.id}
          type="button"
          onClick={() => dismiss(toast.id)}
          className={`toast-enter pointer-events-auto rounded-lg px-4 py-2.5 text-sm font-medium text-white shadow-elevation transition-transform duration-fast active:scale-95 ${
            toast.kind === 'error' ? 'bg-red' : 'bg-blurple'
          }`}
        >
          {toast.text}
        </button>
      ))}
    </div>
  );
}
