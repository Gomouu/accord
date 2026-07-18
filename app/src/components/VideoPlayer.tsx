/**
 * Lecteur vidéo intégré (D-053, D-055) : deux régimes selon la taille.
 *
 * - **≤ 8 Mio** (borne de la lecture en ligne) : chargement automatique en
 *   `data:` URL — même chemin content-addressed que les images
 *   (`lireFichier`), les `blob:` étant cassés en WKWebView.
 * - **> 8 Mio, jusqu'au plafond choisi** (Paramètres → Texte & médias,
 *   app de bureau seulement) : carte « Lire la vidéo » — le téléchargement
 *   COMPLET ne démarre qu'au clic (chemin sollicité, l'anti-DoS du
 *   téléchargement automatique reste intact), puis la vidéo est servie en
 *   STREAMING depuis le disque via le protocole asset (`convertFileSrc`) —
 *   jamais de `data:` URL géante en mémoire.
 *
 * Progression pendant le téléchargement, erreur explicite ET relançable
 * (l'expéditeur peut être revenu en ligne). Jamais de lecture automatique.
 * Un codec illisible par la webview bascule sur la carte d'erreur.
 */

import { useEffect, useState } from 'react';
import { convertFileSrc } from '@tauri-apps/api/core';
import { interpolate } from '../i18n';
import type { FileAttachment } from '../lib/api';
import { MAX_TAILLE_PIECE } from '../lib/attachments';
import {
  lireFichier,
  observerProgression,
  statutFichier,
  telechargerComplet,
} from '../lib/files';
import { tailleLisible } from '../lib/format';
import { useT, useUi } from '../stores/ui';

export function VideoPlayer({
  piece,
  hint,
}: {
  piece: FileAttachment;
  hint?: string | undefined;
}) {
  const t = useT();
  const lang = useUi((s) => s.lang);
  const [url, setUrl] = useState<string | null>(null);
  const [echec, setEchec] = useState(false);
  const [progression, setProgression] = useState<{ done: number; total: number } | null>(
    null,
  );
  // Reprise manuelle : relance le chargement complet (même mécanique que la
  // vignette d'image — un échec n'est jamais définitif, D-052).
  const [tentative, setTentative] = useState(0);
  // Grand format : vrai une fois « Lire la vidéo » cliqué (le téléchargement
  // ne démarre jamais tout seul au-delà de la lecture en ligne).
  const [demande, setDemande] = useState(false);

  const enStreaming = piece.size > MAX_TAILLE_PIECE;

  useEffect(() => {
    if (enStreaming && !demande) return undefined;
    let alive = true;
    setUrl(null);
    setEchec(false);
    setProgression(null);
    const off = observerProgression(piece.merkle_root, (done, total) => {
      if (alive) setProgression({ done, total });
    });
    statutFichier(piece.merkle_root, hint)
      .then((statut) => {
        if (alive && !statut.complete && statut.total > 0) {
          setProgression((p) => p ?? { done: statut.done, total: statut.total });
        }
      })
      .catch(() => {
        // Sans statut, la barre démarre à zéro.
      });
    const chargement = enStreaming
      ? telechargerComplet(piece.merkle_root, hint).then(convertFileSrc)
      : lireFichier(piece.merkle_root, hint);
    chargement
      .then((source) => {
        if (alive) setUrl(source);
      })
      .catch(() => {
        if (alive) setEchec(true);
      });
    return () => {
      alive = false;
      off();
    };
  }, [piece.merkle_root, hint, tentative, enStreaming, demande]);

  if (enStreaming && !demande) {
    return (
      <button
        type="button"
        onClick={() => setDemande(true)}
        className="flex aspect-video w-96 max-w-full flex-col items-center justify-center gap-2 rounded-lg border border-rail bg-sidebar px-4 text-center transition-colors duration-fast hover:border-blurple/50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple"
      >
        <span aria-hidden className="text-3xl leading-none text-norm">
          ▶
        </span>
        <span className="text-sm font-medium text-norm">
          {interpolate(t.fichiers.videoCharger, {
            size: tailleLisible(piece.size, lang),
          })}
        </span>
        <span className="max-w-full truncate text-xs text-faint">{piece.name}</span>
      </button>
    );
  }

  if (echec) {
    return (
      <div className="flex aspect-video w-96 max-w-full flex-col items-center justify-center gap-2 rounded-lg border border-rail bg-sidebar px-4 text-center text-sm text-faint">
        <span>{t.fichiers.videoIndisponible}</span>
        <button
          type="button"
          onClick={() => setTentative((n) => n + 1)}
          className="rounded-md bg-rail px-3 py-1 text-xs font-medium text-norm transition-colors duration-fast hover:bg-input hover:text-header focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple"
        >
          {t.dm.retry}
        </button>
      </div>
    );
  }

  if (url === null) {
    const pct =
      progression !== null && progression.total > 0
        ? Math.min(100, Math.round((progression.done / progression.total) * 100))
        : 0;
    return (
      <div
        role="status"
        className="flex aspect-video w-96 max-w-full items-center justify-center rounded-lg border border-rail bg-sidebar text-xs text-muted"
      >
        {interpolate(t.fichiers.enTelechargement, { pct: String(pct) })}
      </div>
    );
  }

  return (
    <video
      controls
      preload="metadata"
      src={url}
      aria-label={piece.name}
      className="max-h-80 w-96 max-w-full rounded-lg bg-black"
      onError={() => setEchec(true)}
    />
  );
}
