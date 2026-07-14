/** Briques partagées des paramètres du serveur : confirmations, erreurs. */

import { useState } from 'react';
import { useT } from '../../stores/ui';

/**
 * Message d'erreur affichable : celui du nœud (français, « refusé : … »)
 * quand il existe, sinon le repli générique fourni.
 */
export function messageOf(error: unknown, fallback: string): string {
  if (error instanceof Error && error.message.trim() !== '') return error.message;
  return fallback;
}

/**
 * Bouton destructeur à confirmation en deux temps : le premier clic révèle
 * la question et les boutons Confirmer/Annuler en place.
 */
export function ConfirmButton({
  action,
  question,
  onConfirm,
  disabled = false,
}: {
  action: string;
  question: string;
  onConfirm: () => void;
  disabled?: boolean;
}) {
  const t = useT();
  const [confirming, setConfirming] = useState(false);

  if (confirming) {
    return (
      <span
        role="alertdialog"
        aria-label={question}
        className="flex flex-wrap items-center gap-2"
      >
        <span className="text-sm text-norm">{question}</span>
        <button
          type="button"
          onClick={() => {
            setConfirming(false);
            onConfirm();
          }}
          className="rounded-md bg-red px-2.5 py-1 text-xs font-medium text-on-red transition-colors hover:bg-red/80 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-red focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar"
        >
          {t.app.confirm}
        </button>
        <button
          type="button"
          onClick={() => setConfirming(false)}
          className="rounded-md px-1 py-1 text-xs font-medium text-norm hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar"
        >
          {t.app.cancel}
        </button>
      </span>
    );
  }

  return (
    <button
      type="button"
      disabled={disabled}
      onClick={() => setConfirming(true)}
      className="rounded-lg border border-red px-3 py-1 text-sm font-medium text-red transition-colors enabled:hover:bg-red enabled:hover:text-on-red focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-red focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar disabled:opacity-50"
    >
      {action}
    </button>
  );
}
