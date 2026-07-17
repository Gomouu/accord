/**
 * Modale QR du lien d'ami : affiche le lien `accord://friend/<code>` en QR
 * (~240 px, marge 1, couleurs par défaut du canvas — sombre sur clair pour
 * rester scannable quel que soit le thème) avec le lien en clair dessous.
 * Mêmes conventions que les autres modales du dépôt : `role="dialog"`,
 * Échap ferme, Tab bouclé (`bouclerTab`), focus rendu au déclencheur.
 */

import { useEffect, useRef, useState } from 'react';
import { toDataURL } from 'qrcode';
import { bouclerTab } from '../lib/focus';
import { useT } from '../stores/ui';
import { CloseIcon } from './ContextMenu';

/** Côté (px) de l'image QR générée — lisible sans dominer la modale. */
const QR_SIZE = 240;

/** Modules de marge silencieuse autour du QR (le fond blanc suffit). */
const QR_MARGIN = 1;

interface FriendQrModalProps {
  /** Lien d'ami complet à encoder (`buildFriendLink(code)`). */
  link: string;
  onClose: () => void;
}

export function FriendQrModal({ link, onClose }: FriendQrModalProps) {
  const t = useT();
  const ref = useRef<HTMLDivElement>(null);
  /** Data-URL du QR généré, ou `null` tant qu'il n'est pas prêt (ou en échec). */
  const [dataUrl, setDataUrl] = useState<string | null>(null);

  useEffect(() => {
    // Génération asynchrone annulable : ne pose jamais d'état après démontage.
    let cancelled = false;
    toDataURL(link, { width: QR_SIZE, margin: QR_MARGIN })
      .then((url) => {
        if (!cancelled) setDataUrl(url);
      })
      .catch(() => {
        // Échec improbable (canvas indisponible) : le lien en clair reste
        // affiché dessous, la modale garde donc son utilité.
        if (!cancelled) setDataUrl(null);
      });
    return () => {
      cancelled = true;
    };
  }, [link]);

  useEffect(() => {
    // Focus rendu au déclencheur à la fermeture (s'il est toujours monté).
    const precedent =
      document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
      else if (e.key === 'Tab') bouclerTab(e, ref.current);
    };
    window.addEventListener('keydown', onKey);
    ref.current?.focus();
    return () => {
      window.removeEventListener('keydown', onKey);
      if (precedent !== null && precedent.isConnected) precedent.focus();
    };
  }, [onClose]);

  return (
    <div
      className="modal-overlay-enter fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div
        ref={ref}
        role="dialog"
        aria-modal="true"
        aria-label={t.friends.qrTitle}
        tabIndex={-1}
        className="glass modal-panel-enter flex w-[20rem] max-w-[92vw] flex-col overflow-hidden rounded-xl shadow-3 focus:outline-none"
      >
        <div className="flex items-center justify-between px-5 pt-5">
          <h2 className="text-lg font-semibold text-header">{t.friends.qrTitle}</h2>
          <button
            type="button"
            aria-label={t.app.close}
            onClick={onClose}
            className="rounded-sm p-1 text-faint transition-colors duration-fast hover:text-norm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-modal active:scale-95"
          >
            <CloseIcon size={20} />
          </button>
        </div>
        <div className="flex flex-col items-center gap-3 px-5 pb-5 pt-4">
          {/* Fond blanc permanent : un QR doit rester sombre-sur-clair pour
              être décodé, y compris en thème sombre. */}
          <div
            className="flex items-center justify-center rounded-lg bg-white p-2"
            style={{ width: QR_SIZE + 16, height: QR_SIZE + 16 }}
          >
            {dataUrl !== null && (
              <img
                src={dataUrl}
                alt={t.friends.qrTitle}
                width={QR_SIZE}
                height={QR_SIZE}
              />
            )}
          </div>
          <p className="text-center text-sm text-muted">{t.friends.qrHint}</p>
          <code className="selectable max-w-full break-all text-center font-mono text-xs text-faint">
            {link}
          </code>
        </div>
      </div>
    </div>
  );
}
