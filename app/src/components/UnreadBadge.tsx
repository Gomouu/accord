/**
 * Pastille de non-lus façon Discord : compte sur fond rouge, alignée à
 * droite de sa ligne. Rien n'est rendu sans non-lu.
 *
 * `MentionBadge` en est la variante « mention » : un « @ » précède le compte
 * pour la distinguer d'un simple non-lu (une conversation peut porter les
 * deux — la mention prime à l'affichage).
 */

import { interpolate } from '../i18n';
import { useT } from '../stores/ui';

export function UnreadBadge({ count }: { count: number }) {
  const t = useT();
  if (count <= 0) return null;
  return (
    <span
      aria-label={interpolate(t.dm.unreadBadge, { count: String(count) })}
      className="badge-pop ml-auto min-w-4 shrink-0 rounded-full bg-red px-1.5 text-center text-xs font-semibold leading-4 text-white"
    >
      {count}
    </span>
  );
}

/** Pastille de mentions non lues : « @ » + compte sur fond rouge. */
export function MentionBadge({ count }: { count: number }) {
  const t = useT();
  if (count <= 0) return null;
  return (
    <span
      aria-label={interpolate(t.mentions.badge, { count: String(count) })}
      className="badge-pop ml-auto flex shrink-0 items-center gap-0.5 rounded-full bg-red px-1.5 text-center text-xs font-semibold leading-4 text-white"
    >
      <span aria-hidden className="font-bold leading-none">
        @
      </span>
      {count}
    </span>
  );
}
