/**
 * Avatar rond : image de profil (racine Merkle lue via `lireFichier`) quand
 * un hash est fourni, repli initiales + couleur stable pendant le chargement
 * et en cas d'échec (avatar indisponible, pair injoignable…).
 */

import { useEffect, useState } from 'react';
import { lireFichier } from '../lib/files';
import { avatarColor, initials } from '../lib/format';
import { decorationById } from '../lib/decorations';

interface AvatarProps {
  id: string;
  name: string;
  size?: number;
  /** Racine Merkle de l'image (hex 64) ; absent ou `null` = initiales. */
  avatarHash?: string | null;
  /** Pair source probable du téléchargement (clé publique hex). */
  hint?: string;
  /**
   * Présence : `true` en ligne (pastille verte), `false` hors ligne (grise),
   * `undefined` = pas de pastille (contexte sans présence connue).
   */
  online?: boolean | undefined;
  /**
   * Id de décoration d'avatar (cadre/anneau décoratif intégré). Absent, `null`
   * ou inconnu = aucun cadre (l'avatar reste identique). La décoration est
   * décorative (`pointer-events:none`) et se met à l'échelle avec `size`.
   */
  decoration?: string | null;
}

export function Avatar({
  id,
  name,
  size = 40,
  avatarHash = null,
  hint,
  online,
  decoration = null,
}: AvatarProps) {
  const [url, setUrl] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    setUrl(null);
    if (avatarHash === null) return undefined;
    lireFichier(avatarHash, hint)
      .then((blobUrl) => {
        if (alive) setUrl(blobUrl);
      })
      .catch(() => {
        // Image indisponible : on reste sur les initiales.
      });
    return () => {
      alive = false;
    };
  }, [avatarHash, hint]);

  const cercle = (
    <div
      aria-hidden
      className="flex h-full w-full items-center justify-center overflow-hidden rounded-full font-semibold text-white"
      style={{ fontSize: size * 0.4, backgroundColor: avatarColor(id) }}
    >
      {url === null ? (
        initials(name)
      ) : (
        <img src={url} alt="" className="h-full w-full object-cover" />
      )}
    </div>
  );

  // Décoration intégrée (cadre/anneau) superposée, si l'id est connu.
  const cadre = decorationById(decoration)?.render(size) ?? null;

  // Sans présence connue : cercle nu + éventuelle décoration.
  if (online === undefined) {
    return (
      <div className="relative shrink-0" style={{ width: size, height: size }}>
        {cercle}
        {cadre}
      </div>
    );
  }

  // Avec présence : pastille verte/grise en bas à droite.
  const pastille = Math.max(8, Math.round(size * 0.3));
  return (
    <div className="relative shrink-0" style={{ width: size, height: size }}>
      {cercle}
      {cadre}
      <span
        aria-label={online ? 'en ligne' : 'hors ligne'}
        className={`absolute bottom-0 right-0 rounded-full border-2 border-rail ${
          online ? 'bg-green' : 'bg-faint'
        }`}
        style={{ width: pastille, height: pastille }}
      />
    </div>
  );
}
