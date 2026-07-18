/**
 * Recadreur interactif d'image façon Discord : l'image (toute taille) est
 * affichée dans une fenêtre masquée — cercle pour l'avatar, carré pour l'icône
 * de serveur, rectangle paysage (3:1) pour la bannière de profil. Zoom molette
 * + curseur, déplacement souris/tactile, bornes empêchant tout vide dans le
 * cadre. À la validation, la région visible est dessinée sur un canvas aux
 * proportions du cadre et encodée en base64 (PNG, repli JPEG si > 512 Kio) via
 * `encoderRecadrage`.
 */

import { useEffect, useRef, useState } from 'react';
import { fichierEnDataUrl } from '../lib/attachments';
import {
  ZOOM_MAX,
  chargerImage,
  contraindreDecalage,
  echelleCouverture,
  encoderRecadrage,
  type AvatarEncode,
  type Point,
} from '../lib/image';
import { useT } from '../stores/ui';

/** Côté du cadre carré d'affichage (px) — indépendant de la taille de sortie. */
const CADRE_PX = 260;

/** Largeur du cadre paysage de la bannière (px, ratio 3:1). */
const CADRE_BANNIERE_W = 300;
/** Hauteur du cadre paysage de la bannière (px, ratio 3:1). */
const CADRE_BANNIERE_H = 100;
/** Largeur du canvas de sortie de la bannière (px, ratio 3:1). */
const SORTIE_BANNIERE_W = 600;
/** Hauteur du canvas de sortie de la bannière (px, ratio 3:1). */
const SORTIE_BANNIERE_H = 200;

/** Sensibilité de la molette (fraction de zoom par cran). */
const WHEEL_SENSIBILITE = 0.0015;

/** Forme du masque de recadrage. */
export type FormeRecadreur = 'cercle' | 'carre' | 'banniere';

export interface AvatarCropperProps {
  /** Image source choisie par l'utilisateur. */
  fichier: Blob;
  /**
   * Forme du masque : rond pour un avatar, carré pour une icône de serveur,
   * rectangle paysage pour une bannière de profil.
   */
  forme: FormeRecadreur;
  /** Côté du canvas de sortie carré (px) ; ignoré pour la bannière. */
  taille?: number;
  /** Fermeture sans enregistrer. */
  onAnnuler: () => void;
  /** Validation : reçoit l'image recadrée encodée. */
  onValider: (resultat: AvatarEncode) => void | Promise<void>;
}

/** Éléments focusables visibles du piège à focus. */
function focusables(racine: HTMLElement | null): HTMLElement[] {
  if (racine === null) return [];
  const selecteur =
    'button:not([disabled]), input:not([disabled]), [tabindex]:not([tabindex="-1"])';
  return Array.from(racine.querySelectorAll<HTMLElement>(selecteur));
}

export function AvatarCropper({
  fichier,
  forme,
  taille,
  onAnnuler,
  onValider,
}: AvatarCropperProps) {
  const t = useT();
  const dialogRef = useRef<HTMLDivElement>(null);
  const glisseRef = useRef<{ x: number; y: number; base: Point } | null>(null);

  const [image, setImage] = useState<HTMLImageElement | null>(null);
  const [dims, setDims] = useState<{ w: number; h: number } | null>(null);
  const [zoom, setZoom] = useState(1);
  const [decalage, setDecalage] = useState<Point>({ x: 0, y: 0 });
  const [erreur, setErreur] = useState(false);
  const [busy, setBusy] = useState(false);

  const estBanniere = forme === 'banniere';
  const cadreW = estBanniere ? CADRE_BANNIERE_W : CADRE_PX;
  const cadreH = estBanniere ? CADRE_BANNIERE_H : CADRE_PX;

  const echelleMin =
    dims !== null ? echelleCouverture(dims.w, dims.h, cadreW, cadreH) : 1;
  const echelle = echelleMin * zoom;

  // Chargement de l'image + cadrage initial centré au zoom minimal.
  // Lecture en data: URL — les URL blob: ne sont pas rendues par la
  // WKWebView de l'app packagée (« fichier pas une image exploitable »).
  useEffect(() => {
    let vivant = true;
    fichierEnDataUrl(fichier)
      .then((url) => chargerImage(url))
      .then((img) => {
        if (!vivant) return;
        const w = img.naturalWidth || img.width;
        const h = img.naturalHeight || img.height;
        const em = echelleCouverture(w, h, cadreW, cadreH);
        setImage(img);
        setDims({ w, h });
        setZoom(1);
        setDecalage(contraindreDecalage({ x: 0, y: 0 }, w, h, cadreW, cadreH, em));
      })
      .catch(() => {
        if (vivant) setErreur(true);
      });
    return () => {
      vivant = false;
    };
  }, [fichier, cadreW, cadreH]);

  // Piège à focus : focus initial sur le dialogue, cycle Tab borné.
  useEffect(() => {
    dialogRef.current?.focus();
  }, []);

  const clamp = (d: Point, ech: number): Point =>
    dims === null ? d : contraindreDecalage(d, dims.w, dims.h, cadreW, cadreH, ech);

  /** Applique un nouveau zoom en gardant le centre du cadre ancré. */
  const appliquerZoom = (prochainZoom: number): void => {
    const borne = Math.min(ZOOM_MAX, Math.max(1, prochainZoom));
    const echelleApres = echelleMin * borne;
    const centreX = cadreW / 2;
    const centreY = cadreH / 2;
    const facteur = echelleApres / echelle;
    const recentre: Point = {
      x: centreX - facteur * (centreX - decalage.x),
      y: centreY - facteur * (centreY - decalage.y),
    };
    setZoom(borne);
    setDecalage(clamp(recentre, echelleApres));
  };

  const onWheel = (e: React.WheelEvent<HTMLDivElement>): void => {
    e.preventDefault();
    appliquerZoom(zoom * (1 - e.deltaY * WHEEL_SENSIBILITE));
  };

  const onPointerDown = (e: React.PointerEvent<HTMLDivElement>): void => {
    if (dims === null) return;
    glisseRef.current = { x: e.clientX, y: e.clientY, base: decalage };
    e.currentTarget.setPointerCapture?.(e.pointerId);
  };

  const onPointerMove = (e: React.PointerEvent<HTMLDivElement>): void => {
    const glisse = glisseRef.current;
    if (glisse === null) return;
    const suivant: Point = {
      x: glisse.base.x + (e.clientX - glisse.x),
      y: glisse.base.y + (e.clientY - glisse.y),
    };
    setDecalage(clamp(suivant, echelle));
  };

  const onPointerUp = (e: React.PointerEvent<HTMLDivElement>): void => {
    glisseRef.current = null;
    e.currentTarget.releasePointerCapture?.(e.pointerId);
  };

  const onKeyDown = (e: React.KeyboardEvent<HTMLDivElement>): void => {
    if (e.key === 'Escape') {
      e.stopPropagation();
      onAnnuler();
      return;
    }
    if (e.key !== 'Tab') return;
    const cibles = focusables(dialogRef.current);
    const premier = cibles[0];
    const dernier = cibles[cibles.length - 1];
    if (premier === undefined || dernier === undefined) return;
    if (e.shiftKey && document.activeElement === premier) {
      e.preventDefault();
      dernier.focus();
    } else if (!e.shiftKey && document.activeElement === dernier) {
      e.preventDefault();
      premier.focus();
    }
  };

  const valider = async (): Promise<void> => {
    if (image === null || dims === null || busy) return;
    setBusy(true);
    try {
      const resultat = encoderRecadrage(image, {
        largeur: dims.w,
        hauteur: dims.h,
        cadreW,
        cadreH,
        echelle,
        decalage,
        tailleW: estBanniere ? SORTIE_BANNIERE_W : taille,
        tailleH: estBanniere ? SORTIE_BANNIERE_H : taille,
      });
      await onValider(resultat);
    } catch {
      setErreur(true);
      setBusy(false);
    }
  };

  const titre = estBanniere
    ? t.recadreur.titreBanniere
    : forme === 'cercle'
      ? t.recadreur.titreAvatar
      : t.recadreur.titreIcone;
  const rayon = forme === 'cercle' ? '9999px' : estBanniere ? '10px' : '14px';
  const pret = image !== null && dims !== null && !erreur;

  return (
    <div
      className="liquid-overlay fixed inset-0 z-50 flex items-center justify-center"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onAnnuler();
      }}
    >
      <div
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-label={titre}
        aria-describedby="recadreur-instructions"
        tabIndex={-1}
        onKeyDown={onKeyDown}
        className="liquid-cropper max-h-[92vh] w-[340px] max-w-[92vw] overflow-y-auto rounded-lg p-5 outline-none"
      >
        <h2 className="mb-1 text-lg font-semibold text-header">{titre}</h2>
        <p id="recadreur-instructions" className="mb-4 text-sm text-muted">
          {t.recadreur.instructions}
        </p>

        {erreur ? (
          <p className="py-8 text-center text-sm text-red">{t.recadreur.invalide}</p>
        ) : (
          <div className="flex flex-col items-center">
            <div
              className="relative touch-none select-none bg-rail"
              style={{
                width: cadreW,
                height: cadreH,
                cursor: pret ? 'grab' : 'default',
              }}
              onWheel={onWheel}
              onPointerDown={onPointerDown}
              onPointerMove={onPointerMove}
              onPointerUp={onPointerUp}
              onPointerCancel={onPointerUp}
            >
              <div
                className="absolute inset-0 overflow-hidden"
                style={{ borderRadius: rayon }}
              >
                {pret && image !== null && dims !== null && (
                  <img
                    src={image.src}
                    alt=""
                    draggable={false}
                    style={{
                      width: dims.w * echelle,
                      height: dims.h * echelle,
                      maxWidth: 'none',
                      transform: `translate(${decalage.x}px, ${decalage.y}px)`,
                      transformOrigin: 'top left',
                    }}
                  />
                )}
              </div>
              <div
                aria-hidden
                className="pointer-events-none absolute inset-0"
                style={{
                  borderRadius: rayon,
                  boxShadow: '0 0 0 9999px rgba(0,0,0,0.55)',
                  outline: '2px solid rgba(255,255,255,0.85)',
                  outlineOffset: '-2px',
                }}
              />
              {!pret && (
                <div className="absolute inset-0 flex items-center justify-center text-sm text-muted">
                  {t.recadreur.chargement}
                </div>
              )}
            </div>

            <div className="mt-4 flex w-full items-center gap-3">
              <span aria-hidden className="text-xs text-faint">
                {t.recadreur.zoomMoins}
              </span>
              <input
                type="range"
                min={1}
                max={ZOOM_MAX}
                step={0.01}
                value={zoom}
                disabled={!pret}
                aria-label={t.recadreur.zoom}
                onChange={(e) => appliquerZoom(Number(e.target.value))}
                className="min-w-0 flex-1 accent-blurple"
              />
              <span aria-hidden className="text-xs text-faint">
                {t.recadreur.zoomPlus}
              </span>
            </div>
          </div>
        )}

        <div className="mt-5 flex justify-end gap-3">
          <button
            type="button"
            onClick={onAnnuler}
            className="rounded-lg px-4 py-2 text-sm font-medium text-norm hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-modal"
          >
            {t.recadreur.annuler}
          </button>
          <button
            type="button"
            disabled={!pret || busy}
            onClick={() => void valider()}
            className="rounded-lg bg-blurple px-4 py-2 text-sm font-medium text-white transition-colors duration-150 hover:bg-blurple-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-modal disabled:opacity-50"
          >
            {t.recadreur.valider}
          </button>
        </div>
      </div>
    </div>
  );
}
