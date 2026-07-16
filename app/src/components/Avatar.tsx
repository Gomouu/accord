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
  /** URL locale d'aperçu, prioritaire sur le hash persistant. */
  imageUrl?: string | null;
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
  decorationMotion?: 'full' | 'interaction' | 'static';
}

export function Avatar({
  id,
  name,
  size = 40,
  avatarHash = null,
  imageUrl = null,
  hint,
  online,
  decoration = null,
  decorationMotion,
}: AvatarProps) {
  const [url, setUrl] = useState<string | null>(null);
  const [imageFailed, setImageFailed] = useState(false);

  useEffect(() => {
    let alive = true;
    setUrl(null);
    setImageFailed(false);
    if (avatarHash === null || imageUrl !== null) return undefined;
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
  }, [avatarHash, hint, imageUrl]);

  const resolvedUrl = imageFailed ? null : (imageUrl ?? url);
  const motion = decorationMotion ?? (size >= 54 ? 'full' : 'interaction');

  const cercle = (
    <div
      aria-hidden
      className="avatar-core flex h-full w-full items-center justify-center overflow-hidden rounded-full font-semibold text-white"
      style={{ fontSize: size * 0.4, backgroundColor: avatarColor(id) }}
    >
      {resolvedUrl === null ? (
        initials(name)
      ) : (
        <img
          src={resolvedUrl}
          alt=""
          className="h-full w-full object-cover"
          onError={() => setImageFailed(true)}
        />
      )}
    </div>
  );

  // Décoration intégrée (cadre/anneau) superposée, si l'id est connu.
  const cadre = decorationById(decoration)?.render(size) ?? null;

  // Sans présence connue : cercle nu + éventuelle décoration.
  if (online === undefined) {
    return (
      <div
        className="avatar-root relative shrink-0"
        data-decoration-motion={motion}
        style={{ width: size, height: size }}
      >
        {cercle}
        {cadre}
      </div>
    );
  }

  // Avec présence : pastille verte/grise en bas à droite.
  const pastille = Math.max(8, Math.round(size * 0.3));
  return (
    <div
      className="avatar-root relative shrink-0"
      data-decoration-motion={motion}
      style={{ width: size, height: size }}
    >
      {cercle}
      {cadre}
      <span
        aria-label={online ? 'en ligne' : 'hors ligne'}
        className={`avatar-presence absolute bottom-0 right-0 rounded-full border-2 border-rail ${
          online ? 'bg-green' : 'bg-faint'
        }`}
        style={{ width: pastille, height: pastille }}
      />
    </div>
  );
}
