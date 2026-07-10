/**
 * Pastille de présence riche (style Discord) : vert plein (en ligne), lune
 * jaune (inactif), rouge barré (ne pas déranger), anneau gris (hors ligne).
 * Utilisée partout où la présence s'affiche (liste d'amis, MP, profils).
 */

import { useId } from 'react';
import type { PresenceStatus } from '../lib/api';

/** Classe de couleur Tailwind du statut (remplie via `currentColor`). */
const STATUS_COLOR: Record<PresenceStatus, string> = {
  online: 'text-green',
  idle: 'text-yellow',
  dnd: 'text-red',
  offline: 'text-faint',
};

export function PresenceDot({
  status,
  label,
  size = 10,
  className = '',
}: {
  status: PresenceStatus;
  /** Libellé accessible (i18n) ; absent = purement décoratif. */
  label?: string | undefined;
  /** Diamètre en px (10 par défaut). */
  size?: number;
  className?: string;
}) {
  // Identifiants de masque uniques par instance (plusieurs pastilles par page).
  const maskId = useId();

  return (
    <span
      role={label === undefined ? undefined : 'img'}
      aria-label={label}
      aria-hidden={label === undefined}
      data-status={status}
      className={`inline-block ${STATUS_COLOR[status]} ${className}`}
      style={{ width: size, height: size }}
    >
      <svg viewBox="0 0 10 10" width={size} height={size} fill="currentColor">
        {status === 'online' && <circle cx="5" cy="5" r="5" />}
        {status === 'idle' && (
          <>
            <mask id={maskId}>
              <rect width="10" height="10" fill="white" />
              <circle cx="3.2" cy="3.2" r="3.4" fill="black" />
            </mask>
            <circle cx="5" cy="5" r="5" mask={`url(#${maskId})`} />
          </>
        )}
        {status === 'dnd' && (
          <>
            <mask id={maskId}>
              <rect width="10" height="10" fill="white" />
              <rect x="1.8" y="3.9" width="6.4" height="2.2" rx="1.1" fill="black" />
            </mask>
            <circle cx="5" cy="5" r="5" mask={`url(#${maskId})`} />
          </>
        )}
        {status === 'offline' && (
          <>
            <mask id={maskId}>
              <rect width="10" height="10" fill="white" />
              <circle cx="5" cy="5" r="2.4" fill="black" />
            </mask>
            <circle cx="5" cy="5" r="5" mask={`url(#${maskId})`} />
          </>
        )}
      </svg>
    </span>
  );
}
