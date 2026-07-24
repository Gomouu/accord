/**
 * Valeur masquée en mode streamer, révélable d'un clic.
 *
 * Sert au code ami : la seule chaîne à l'écran qui, filmée, permet à n'importe
 * qui de vous ajouter. Le masque est **visuel** — la valeur reste dans le DOM
 * et dans le presse-papiers. C'est assumé : le mode streamer protège d'une
 * capture d'écran, pas d'un attaquant sur la machine, et le prétendre serait
 * une fausse garantie.
 */

import { useState } from 'react';
import { useT, useUi } from '../stores/ui';

/** Points de remplacement, longueur fixe : la vraie longueur ne fuite pas. */
const MASK = '••••••••••••';

export function SecretText({
  value,
  className = '',
}: {
  value: string;
  className?: string;
}) {
  const t = useT();
  const streamerMode = useUi((s) => s.streamerMode);
  const [revealed, setRevealed] = useState(false);

  if (!streamerMode || revealed) {
    return <span className={`selectable ${className}`}>{value}</span>;
  }

  return (
    <button
      type="button"
      onClick={() => setRevealed(true)}
      title={t.settings.streamerReveal}
      aria-label={t.settings.streamerReveal}
      className={`cursor-pointer rounded-sm text-left tracking-[0.15em] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple ${className}`}
    >
      {MASK}
    </button>
  );
}
