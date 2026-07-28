/**
 * QR du numéro de sécurité, affiché dans la modale de vérification (§17.4) :
 * l'ami d'en face le scanne au lieu de se faire lire soixante chiffres.
 *
 * Sous-composant **paresseux** : il embarque `qrcode`, qui n'a rien à faire
 * dans le chunk initial (`FriendVerifyModal` est importé statiquement par
 * `AppShell`). Voir le `manualChunks` de `vite.config.ts`.
 */

import { useEffect, useState } from 'react';
import { toDataURL } from 'qrcode';
import { buildSafetyQrPayload } from '../lib/safetyQr';
import { useT } from '../stores/ui';

/** Côté (px) du QR — lisible par une webcam sans dominer la modale. */
const QR_SIZE = 200;

/** Modules de marge silencieuse autour du QR (le fond blanc suffit). */
const QR_MARGIN = 1;

export function SafetyQrCode({ digits }: { digits: string }) {
  const t = useT();
  /** Data-URL du QR généré, ou `null` tant qu'il n'est pas prêt (ou en échec). */
  const [dataUrl, setDataUrl] = useState<string | null>(null);

  useEffect(() => {
    // Génération asynchrone annulable : ne pose jamais d'état après démontage.
    let annule = false;
    toDataURL(buildSafetyQrPayload(digits), { width: QR_SIZE, margin: QR_MARGIN })
      .then((url) => {
        if (!annule) setDataUrl(url);
      })
      .catch(() => {
        // Échec improbable (canvas indisponible) : les chiffres restent
        // affichés au-dessus, la cérémonie à voix haute reste possible.
        if (!annule) setDataUrl(null);
      });
    return () => {
      annule = true;
    };
  }, [digits]);

  return (
    <div className="flex flex-col items-center gap-2">
      {/* Fond blanc permanent : un QR doit rester sombre-sur-clair pour être
          décodé, y compris en thème sombre. */}
      <div
        className="flex items-center justify-center rounded-lg bg-white p-2"
        style={{ width: QR_SIZE + 16, height: QR_SIZE + 16 }}
      >
        {dataUrl !== null && (
          <img
            src={dataUrl}
            alt={t.friends.verifyQrAlt}
            width={QR_SIZE}
            height={QR_SIZE}
          />
        )}
      </div>
      <p className="text-center text-xs text-muted">{t.friends.verifyQrHint}</p>
    </div>
  );
}
