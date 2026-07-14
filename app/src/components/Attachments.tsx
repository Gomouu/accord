/**
 * Pièces jointes d'un message : lecteur dédié pour les messages vocaux (mime
 * `audio/*`, toujours rendu ainsi — indépendant du réglage aperçu, voir
 * `VoiceMessagePlayer`), vignette inline pour les images (progression du
 * téléchargement via event.file_progress/files.status, clic = plein écran),
 * carte de fichier téléchargeable sinon. Au-delà de 8 Mio (borne `files.read`),
 * l'aperçu/lecture inline se rabat sur la carte de fichier, mais le
 * téléchargement reste possible : le blob complet est copié via `files.save`
 * (sélecteur natif Tauri, sans plafond), avec indicateur de progression.
 */

import { useCallback, useEffect, useRef, useState } from 'react';
import { interpolate } from '../i18n';
import type { FileAttachment } from '../lib/api';
import { estAudio, estImage, MAX_TAILLE_PIECE } from '../lib/attachments';
import { isTauri } from '../lib/bridge';
import { api } from '../lib/client';
import {
  FILE_WAIT_TIMEOUT_MS,
  lireFichier,
  observerProgression,
  statutFichier,
} from '../lib/files';
import { tailleLisible } from '../lib/format';
import { useUi, useT } from '../stores/ui';
import { CloseIcon } from './ContextMenu';
import { VoiceMessagePlayer } from './VoiceMessagePlayer';

/** Durée de l'animation de fermeture de l'aperçu (aligne `--duration-fast`). */
const LIGHTBOX_EXIT_MS = 150;

/**
 * Déclenche puis attend le téléchargement COMPLET d'un blob (sans plafond :
 * `media` non posé, contrairement à `lireFichier`) avant une copie par
 * `files.save`. Résout à l'événement `complete: true`, ou tout de suite si le
 * nœud rend déjà les octets (blob complet en local). Un délai d'inactivité
 * glissant borne l'attente pour ne pas figer le bouton si le flux stagne.
 */
function attendreTelechargementComplet(merkleRoot: string, hint?: string): Promise<void> {
  return new Promise((resolve, reject) => {
    let off: (() => void) | null = null;
    let timer: ReturnType<typeof setTimeout> | null = null;
    let settled = false;
    const finish = (err: Error | null): void => {
      if (settled) return;
      settled = true;
      if (timer !== null) clearTimeout(timer);
      if (off !== null) off();
      if (err === null) resolve();
      else reject(err);
    };
    const arm = (): void => {
      if (timer !== null) clearTimeout(timer);
      timer = setTimeout(
        () => finish(new Error('téléchargement du fichier interrompu')),
        FILE_WAIT_TIMEOUT_MS,
      );
    };
    off = observerProgression(merkleRoot, (_done, _total, complete) => {
      if (complete) finish(null);
      else arm();
    });
    // `media` non posé : télécharge le blob COMPLET (aucun plafond 8 Mio).
    api
      .filesRead(merkleRoot, hint)
      .then((r) => {
        if (r.pending !== true) finish(null);
      })
      .catch((e: unknown) =>
        finish(e instanceof Error ? e : new Error('lecture du fichier impossible')),
      );
    arm();
  });
}

/** Plein écran très simple : clic n'importe où ou Échap pour fermer. */
function Lightbox({
  url,
  name,
  onClose,
}: {
  url: string;
  name: string;
  onClose: () => void;
}) {
  const t = useT();
  const [closing, setClosing] = useState(false);
  const closedRef = useRef(false);

  // Fermeture différée : joue l'animation de sortie puis démonte réellement.
  const fermer = useCallback((): void => {
    if (closedRef.current) return;
    closedRef.current = true;
    setClosing(true);
    window.setTimeout(onClose, LIGHTBOX_EXIT_MS);
  }, [onClose]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent): void => {
      if (e.key === 'Escape') fermer();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [fermer]);

  return (
    <div
      role="dialog"
      aria-label={name}
      className={`fixed inset-0 z-50 flex items-center justify-center bg-black/80 p-8 ${
        closing ? 'lightbox-exit' : 'lightbox-enter'
      }`}
      onClick={fermer}
    >
      <img
        src={url}
        alt={name}
        className={`max-h-full max-w-full rounded-lg object-contain ${
          closing ? 'lightbox-image-exit' : 'lightbox-image-enter'
        }`}
      />
      <button
        type="button"
        aria-label={t.fichiers.fermerApercu}
        title={t.fichiers.fermerApercu}
        onClick={fermer}
        className="absolute right-4 top-4 rounded-full p-2 text-white/80 transition-colors duration-fast hover:text-white focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple"
      >
        <CloseIcon size={22} />
      </button>
    </div>
  );
}

/** Vignette d'image : progression pendant le téléchargement, puis aperçu. */
function VignetteImage({
  piece,
  hint,
}: {
  piece: FileAttachment;
  hint?: string | undefined;
}) {
  const t = useT();
  const [url, setUrl] = useState<string | null>(null);
  const [echec, setEchec] = useState(false);
  const [progression, setProgression] = useState<{ done: number; total: number } | null>(
    null,
  );
  const [pleinEcran, setPleinEcran] = useState(false);

  useEffect(() => {
    let alive = true;
    setUrl(null);
    setEchec(false);
    setProgression(null);
    const off = observerProgression(piece.merkle_root, (done, total) => {
      if (alive) setProgression({ done, total });
    });
    // Progression initiale en best effort (téléchargement déjà entamé).
    statutFichier(piece.merkle_root, hint)
      .then((statut) => {
        if (alive && !statut.complete && statut.total > 0) {
          setProgression((p) => p ?? { done: statut.done, total: statut.total });
        }
      })
      .catch(() => {
        // Sans statut, la barre démarre à zéro.
      });
    lireFichier(piece.merkle_root, hint)
      .then((blobUrl) => {
        if (alive) setUrl(blobUrl);
      })
      .catch(() => {
        if (alive) setEchec(true);
      });
    return () => {
      alive = false;
      off();
    };
  }, [piece.merkle_root, hint]);

  if (echec) {
    return (
      <div className="flex h-24 w-64 max-w-full items-center justify-center rounded-lg border border-rail bg-sidebar px-4 text-sm text-faint">
        {t.fichiers.imageIndisponible}
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
        className="flex h-40 w-64 max-w-full flex-col items-center justify-center gap-2 rounded-lg border border-rail bg-sidebar px-4"
      >
        <div className="text-xs text-muted">
          {interpolate(t.fichiers.enTelechargement, { pct: String(pct) })}
        </div>
        <div className="h-1.5 w-full overflow-hidden rounded-full bg-input">
          <div
            className="h-full origin-left rounded-full bg-blurple"
            style={{ transform: `scaleX(${pct / 100})` }}
          />
        </div>
        <div className="w-full truncate text-center text-xs text-faint">{piece.name}</div>
      </div>
    );
  }

  return (
    <>
      <button
        type="button"
        aria-label={interpolate(t.fichiers.agrandir, { name: piece.name })}
        title={piece.name}
        onClick={() => setPleinEcran(true)}
        className="w-fit rounded-lg focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple"
      >
        <img
          src={url}
          alt={piece.name}
          className="max-h-80 max-w-full rounded-lg object-contain"
        />
      </button>
      {pleinEcran && (
        <Lightbox url={url} name={piece.name} onClose={() => setPleinEcran(false)} />
      )}
    </>
  );
}

/**
 * Carte de fichier : icône, nom, taille lisible et bouton de téléchargement.
 * Petits fichiers (≤ 8 Mio) : lecture en ligne (`lireFichier`) puis lien
 * `download` (fonctionne partout, build navigateur inclus). Gros fichiers
 * (> 8 Mio) : sélecteur natif Tauri (`save`) puis copie du blob complet via
 * `files.save` (sans plafond) ; si le contenu n'est pas encore complet en
 * local, le téléchargement est d'abord déclenché et attendu. Un indicateur de
 * progression remplace l'icône pendant l'opération (`aria-busy`, `role=status`).
 */
function CarteFichier({
  piece,
  hint,
  mention,
}: {
  piece: FileAttachment;
  hint?: string | undefined;
  mention?: string | undefined;
}) {
  const t = useT();
  const lang = useUi((s) => s.lang);
  const toast = useUi((s) => s.toast);
  const [occupe, setOccupe] = useState(false);
  const [progression, setProgression] = useState<{ done: number; total: number } | null>(
    null,
  );

  // Progression affichée tant que le téléchargement est en cours : même source
  // que la vignette d'image (`observerProgression` + amorce `statutFichier`).
  useEffect(() => {
    if (!occupe) {
      setProgression(null);
      return;
    }
    const off = observerProgression(piece.merkle_root, (done, total) => {
      setProgression({ done, total });
    });
    statutFichier(piece.merkle_root, hint)
      .then((statut) => {
        if (!statut.complete && statut.total > 0) {
          setProgression((p) => p ?? { done: statut.done, total: statut.total });
        }
      })
      .catch(() => {
        // Sans statut, la barre démarre à zéro (spinner).
      });
    return () => off();
  }, [occupe, piece.merkle_root, hint]);

  const telecharger = async (): Promise<void> => {
    if (occupe) return;
    setOccupe(true);
    try {
      if (piece.size <= MAX_TAILLE_PIECE) {
        // Petit fichier : lecture en ligne + lien `download` (partout).
        const url = await lireFichier(piece.merkle_root, hint);
        const lien = document.createElement('a');
        lien.href = url;
        lien.download = piece.name;
        lien.click();
        return;
      }
      // Gros fichier : chemin natif obligatoire (copie du blob complet).
      if (!isTauri()) {
        toast('error', t.fichiers.telechargementEchoue);
        return;
      }
      const { save } = await import('@tauri-apps/plugin-dialog');
      const dest = await save({ defaultPath: piece.name });
      if (dest === null) return; // Sélecteur annulé : rien à signaler.
      try {
        await api.filesSave(piece.merkle_root, dest);
      } catch {
        // Blob pas encore complet en local (`files.save` → NotFound) : on
        // déclenche puis attend le téléchargement, et on réessaie une fois.
        await attendreTelechargementComplet(piece.merkle_root, hint);
        await api.filesSave(piece.merkle_root, dest);
      }
    } catch {
      toast('error', t.fichiers.telechargementEchoue);
    } finally {
      setOccupe(false);
    }
  };

  const pct =
    progression !== null && progression.total > 0
      ? Math.min(100, Math.round((progression.done / progression.total) * 100))
      : 0;

  return (
    <div className="flex w-full max-w-md items-center gap-3 rounded-lg border border-rail bg-sidebar px-3 py-2">
      <svg
        width="28"
        height="28"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth={2}
        strokeLinecap="round"
        strokeLinejoin="round"
        aria-hidden
        className="shrink-0 text-faint"
      >
        <path d="M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z" />
        <path d="M14 2v4a2 2 0 0 0 2 2h4" />
      </svg>
      <div className="min-w-0 flex-1">
        <div className="truncate text-sm font-medium text-header" title={piece.name}>
          {piece.name}
        </div>
        <div className="truncate text-xs text-faint">
          {tailleLisible(piece.size, lang)}
          {mention !== undefined && ` — ${mention}`}
        </div>
      </div>
      <button
        type="button"
        aria-label={interpolate(t.fichiers.telecharger, { name: piece.name })}
        title={interpolate(t.fichiers.telecharger, { name: piece.name })}
        aria-busy={occupe}
        disabled={occupe}
        onClick={() => void telecharger()}
        className="flex h-[30px] w-[30px] items-center justify-center rounded-md text-muted transition-colors enabled:hover:bg-chat-hover enabled:hover:text-norm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar disabled:opacity-70"
      >
        {occupe ? (
          <span
            role="status"
            aria-label={interpolate(t.fichiers.enTelechargement, { pct: String(pct) })}
            className="flex items-center justify-center text-[10px] font-semibold tabular-nums text-muted"
          >
            {pct > 0 ? (
              `${pct}%`
            ) : (
              <svg
                width="18"
                height="18"
                viewBox="0 0 24 24"
                fill="none"
                aria-hidden
                className="animate-spin"
              >
                <circle
                  cx="12"
                  cy="12"
                  r="10"
                  stroke="currentColor"
                  strokeWidth={3}
                  className="opacity-25"
                />
                <path
                  d="M22 12a10 10 0 0 1-10 10"
                  stroke="currentColor"
                  strokeWidth={3}
                  strokeLinecap="round"
                />
              </svg>
            )}
          </span>
        ) : (
          <svg
            width="18"
            height="18"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth={2}
            strokeLinecap="round"
            strokeLinejoin="round"
            aria-hidden
          >
            <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" />
            <polyline points="7 10 12 15 17 10" />
            <line x1="12" x2="12" y1="15" y2="3" />
          </svg>
        )}
      </button>
    </div>
  );
}

/**
 * Pièces jointes de l'enveloppe d'un message, empilées sous le corps.
 * Réglage « Aperçu des images et médias » (Paramètres → Texte & médias) :
 * désactivé, les images se rabattent sur la carte de fichier existante
 * (nom + téléchargement) au lieu de la vignette en ligne. Les messages
 * vocaux (mime `audio/*`) ignorent ce réglage : ils SONT le message, donc
 * toujours rendus comme lecteur tant qu'ils restent sous la borne de 8 Mio.
 */
export function AttachmentRow({
  pieces,
  hint,
}: {
  pieces: readonly FileAttachment[];
  hint?: string | undefined;
}) {
  const t = useT();
  const showPreviews = useUi((s) => s.showMediaPreviews);
  if (pieces.length === 0) return null;
  return (
    <div className="mt-1 flex flex-col items-start gap-1.5">
      {pieces.map((piece, i) => {
        const tropGrandPourApercu =
          (estImage(piece.mime) || estAudio(piece.mime)) && piece.size > MAX_TAILLE_PIECE;
        if (estAudio(piece.mime) && !tropGrandPourApercu) {
          return (
            <VoiceMessagePlayer key={`${piece.merkle_root}-${i}`} piece={piece} hint={hint} />
          );
        }
        return showPreviews && estImage(piece.mime) && !tropGrandPourApercu ? (
          <VignetteImage key={`${piece.merkle_root}-${i}`} piece={piece} hint={hint} />
        ) : (
          <CarteFichier
            key={`${piece.merkle_root}-${i}`}
            piece={piece}
            hint={hint}
            mention={tropGrandPourApercu ? t.fichiers.apercuTropVolumineux : undefined}
          />
        );
      })}
    </div>
  );
}
