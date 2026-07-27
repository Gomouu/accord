/**
 * Lecteur de QR du numéro de sécurité (§17.4) : la caméra pointe le QR
 * affiché par l'ami d'en face, l'application décode et compare au numéro
 * qu'elle a calculé localement.
 *
 * Sous-composant **paresseux** : il embarque le décodeur, qui n'a rien à faire
 * dans le chunk initial (`FriendVerifyModal` est importé statiquement par
 * `AppShell`). Voir le `manualChunks` de `vite.config.ts`.
 *
 * 🔒 Ce composant ne conserve, n'adopte et n'affiche jamais ce qu'il décode :
 * il en tire un verdict (`verdictForScan`) et jette la chaîne. Un désaccord se
 * dit tel quel — la boucle s'arrête sur le verdict, elle ne « réessaie » pas
 * jusqu'à tomber sur une concordance.
 */

// ─────────────────────────────────────────────────────────────────────────────
// Dépendance : `jsqr` 1.4.0 (§10.3 — « toute dépendance nouvelle est justifiée
// par écrit : pourquoi elle, sa maturité, sa surface `unsafe` »).
//
// `qrcode`, déjà présent, ne sait que **produire** un QR. Décoder en demande un
// autre. Les candidats, et pourquoi celui-là :
//
// • `BarcodeDetector` (natif, zéro octet) — absent de WKWebView (macOS) et de
//   WebKitGTK (Linux) ; il n'existe que dans WebView2 (Windows). Une
//   fonctionnalité qui marche sur une plateforme sur trois n'en est pas une. Et
//   comme le décodeur JS doit de toute façon être embarqué pour les deux
//   autres, l'ajouter en voie rapide ne ferait que doubler les chemins à tester
//   pour un gain invisible à l'utilisateur : écarté, pas même en repli.
// • `zxing-wasm` (1,41 M/sem.) et `barcode-detector` (1,28 M/sem., qui
//   l'enveloppe) — du WebAssembly, donc un binaire opaque dans le bundle et une
//   surface native (C++) qu'on ne peut pas relire. L'entrée de ce décodeur est
//   une image venue d'une caméra, c'est-à-dire une donnée hostile par
//   construction : c'est exactement là qu'on ne veut pas de code mémoire-non-sûr.
// • `@zxing/library` (1,39 M/sem.) — port complet de ZXing, une douzaine de
//   formats de codes-barres dont aucun ne nous servira, pour environ le double
//   d'octets.
// • `qr-scanner` (250 k/sem.) — enveloppe un fork de jsQR dans un Web Worker.
//   La commodité ne compense pas la couche supplémentaire et le worker à
//   empaqueter, pour un scan qui dure quelques secondes.
//
// Ce qui tranche, comme pour `spake2` côté Rust, ce n'est pas la licence
// (Apache-2.0, permissive, même famille que la moitié du workspace) mais
// **l'arbre de dépendances** : `jsqr` n'en a aucune. Pas une seule dépendance
// transitive, pas de binaire natif, pas de WASM, pas d'`eval` — du JavaScript
// pur compilé depuis TypeScript, dont il fournit lui-même les types. Sa surface
// `unsafe` est donc nulle au sens du dépôt : il reçoit un `Uint8ClampedArray`
// qu'on lui tend et rend une chaîne, et cette chaîne n'est jamais crue (voir
// `lib/safetyQr.ts`).
//
// Le point faible, dit franchement : **la 1.4.0 date du 24 avril 2021**, et
// c'est la dernière. Le dépôt est calme. Deux choses le rendent acceptable ici,
// et il faut les deux : la spécification qu'il implémente (ISO/IEC 18004) est
// figée depuis 2000 — un décodeur de format gelé n'a pas de raison de sortir
// des versions —, et l'usage reste massif (1,8 M de téléchargements par
// semaine, davantage que `@zxing/library`), donc les défauts de décodage
// seraient connus. Si la bibliothèque devait être remplacée, l'échange tient en
// un import : tout ce qui porte une décision vit dans `lib/safetyQr.ts`.
//
// `deny.toml` ne voit rien de tout cela — il ne régit que les crates Rust. La
// trace écrite est donc ici et dans `docs/THIRD_PARTY.md`.
// ─────────────────────────────────────────────────────────────────────────────
import jsQR from 'jsqr';

import { useCallback, useEffect, useRef, useState } from 'react';
import { type SafetyScanVerdict, verdictForScan } from '../lib/safetyQr';
import { useT } from '../stores/ui';

/** Délai entre deux analyses d'image (ms) : assez pour ne pas saturer l'UI. */
const INTERVALLE_ANALYSE_MS = 200;

/**
 * Largeur maximale analysée (px). Une webcam 1080p ferait passer jsQR sur
 * deux millions de pixels à chaque tour ; on réduit avant de décoder, un QR
 * tenu devant la caméra restant très lisible à cette taille.
 */
const LARGEUR_ANALYSE_MAX = 640;

/** Définition demandée à la caméra (indicative : le pilote peut l'ignorer). */
const DEFINITION_SOUHAITEE = { width: { ideal: 640 }, height: { ideal: 480 } };

/** Ce que l'utilisateur voit à un instant donné. */
type Etat =
  | { phase: 'scan'; etranger: boolean }
  | { phase: 'verdict'; verdict: 'match' | 'mismatch' }
  | { phase: 'erreur'; cause: 'refus' | 'indisponible' };

/**
 * Coche ou croix du verdict. Décoratif : le texte à côté porte tout le sens,
 * un lecteur d'écran ne doit pas l'annoncer deux fois.
 */
function VerdictIcon({ ok }: { ok: boolean }) {
  return (
    <svg
      width={18}
      height={18}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2.5}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
      className="shrink-0"
    >
      {ok ? <path d="m5 13 4 4L19 7" /> : <path d="M6 6l12 12M18 6L6 18" />}
    </svg>
  );
}

/** Vrai si ce runtime expose une caméra utilisable. */
function cameraDisponible(): boolean {
  return (
    typeof navigator !== 'undefined' &&
    navigator.mediaDevices !== undefined &&
    typeof navigator.mediaDevices.getUserMedia === 'function'
  );
}

/**
 * Refus de l'utilisateur ou absence de caméra ? Les deux méritent un message
 * différent : le premier se corrige dans les réglages système, le second non.
 */
function causeDuRefus(erreur: unknown): 'refus' | 'indisponible' {
  const nom = erreur instanceof Error ? erreur.name : '';
  return nom === 'NotAllowedError' || nom === 'SecurityError' ? 'refus' : 'indisponible';
}

export function SafetyQrScanner({ localDigits }: { localDigits: string }) {
  const t = useT();
  const videoRef = useRef<HTMLVideoElement>(null);
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  /** Incrémenté par « Scanner à nouveau » : relance l'effet de capture. */
  const [session, setSession] = useState(0);
  const [etat, setEtat] = useState<Etat>({ phase: 'scan', etranger: false });

  /**
   * Analyse l'image courante. Rend `null` tant qu'aucun QR n'est trouvé —
   * l'appelant continue alors à scanner.
   */
  const analyserImage = useCallback((): SafetyScanVerdict | null => {
    const video = videoRef.current;
    if (video === null || video.videoWidth === 0 || video.videoHeight === 0) return null;
    const echelle = Math.min(1, LARGEUR_ANALYSE_MAX / video.videoWidth);
    const largeur = Math.max(1, Math.round(video.videoWidth * echelle));
    const hauteur = Math.max(1, Math.round(video.videoHeight * echelle));

    const canvas = (canvasRef.current ??= document.createElement('canvas'));
    canvas.width = largeur;
    canvas.height = hauteur;
    const ctx = canvas.getContext('2d', { willReadFrequently: true });
    if (ctx === null) return null;
    ctx.drawImage(video, 0, 0, largeur, hauteur);
    const image = ctx.getImageData(0, 0, largeur, hauteur);
    // `dontInvert` : nos QR sont toujours sombres sur clair (fond blanc
    // permanent de `SafetyQrCode`), inutile de payer la passe inversée.
    const code = jsQR(image.data, image.width, image.height, {
      inversionAttempts: 'dontInvert',
    });
    return verdictForScan(code?.data ?? null, localDigits);
  }, [localDigits]);

  useEffect(() => {
    let vivant = true;
    let flux: MediaStream | null = null;
    let minuteur: ReturnType<typeof setTimeout> | null = null;

    const arreter = (): void => {
      vivant = false;
      if (minuteur !== null) clearTimeout(minuteur);
      minuteur = null;
      // Relâcher la caméra dès qu'on a fini : le voyant de l'appareil doit
      // s'éteindre avec le scan, pas à la fermeture de l'application.
      for (const piste of flux?.getTracks() ?? []) piste.stop();
      flux = null;
    };

    const tour = (): void => {
      if (!vivant) return;
      const verdict = analyserImage();
      if (verdict === 'match' || verdict === 'mismatch') {
        // 🔒 On s'arrête sur le verdict, quel qu'il soit. Poursuivre après un
        // désaccord reviendrait à réessayer jusqu'à tomber sur une
        // concordance — exactement ce qu'une vérification ne doit pas faire.
        arreter();
        setEtat({ phase: 'verdict', verdict });
        return;
      }
      if (verdict === 'foreign') {
        // QR lu mais étranger : rien n'a été comparé, donc rien n'est conclu.
        setEtat((prec) =>
          prec.phase === 'scan' && !prec.etranger
            ? { phase: 'scan', etranger: true }
            : prec,
        );
      }
      minuteur = setTimeout(tour, INTERVALLE_ANALYSE_MS);
    };

    const demarrer = async (): Promise<void> => {
      if (!cameraDisponible()) {
        setEtat({ phase: 'erreur', cause: 'indisponible' });
        return;
      }
      let capture: MediaStream;
      try {
        capture = await navigator.mediaDevices.getUserMedia({
          video: { ...DEFINITION_SOUHAITEE, facingMode: 'environment' },
          audio: false,
        });
      } catch (erreur) {
        if (vivant) setEtat({ phase: 'erreur', cause: causeDuRefus(erreur) });
        return;
      }
      if (!vivant) {
        // Démonté pendant l'attente de l'autorisation : la caméra ne doit pas
        // rester allumée derrière une modale déjà fermée.
        for (const piste of capture.getTracks()) piste.stop();
        return;
      }
      flux = capture;
      const video = videoRef.current;
      if (video !== null) {
        video.srcObject = capture;
        // `play()` peut être refusé (politique d'autoplay) : l'aperçu reste
        // noir, on ne casse pas le scan pour autant.
        await Promise.resolve(video.play()).catch(() => undefined);
      }
      tour();
    };

    void demarrer();
    return arreter;
  }, [session, analyserImage]);

  if (etat.phase === 'erreur') {
    return (
      <p
        role="alert"
        className="rounded-lg border-s-4 border-red bg-red/10 px-3 py-2 text-sm text-norm"
      >
        {etat.cause === 'refus'
          ? t.friends.verifyScanDenied
          : t.friends.verifyScanNoCamera}
      </p>
    );
  }

  if (etat.phase === 'verdict') {
    const ok = etat.verdict === 'match';
    return (
      <div className="flex flex-col items-center gap-2">
        <p
          role="alert"
          className={`flex w-full items-center gap-2 rounded-lg border-s-4 px-3 py-2 text-sm font-medium text-norm ${
            ok ? 'border-green bg-green/10' : 'border-red bg-red/10'
          }`}
        >
          <span className={ok ? 'text-green' : 'text-red'}>
            <VerdictIcon ok={ok} />
          </span>
          {ok ? t.friends.verifyScanMatch : t.friends.verifyScanMismatch}
        </p>
        <button
          type="button"
          onClick={() => {
            setEtat({ phase: 'scan', etranger: false });
            setSession((n) => n + 1);
          }}
          className="rounded-full bg-rail px-3 py-1.5 text-xs font-medium text-norm transition-colors duration-fast hover:bg-input focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-modal active:scale-[0.98]"
        >
          {t.friends.verifyScanAgain}
        </button>
      </div>
    );
  }

  return (
    <div className="flex flex-col items-center gap-2">
      <video
        ref={videoRef}
        muted
        playsInline
        aria-label={t.friends.verifyScanPreview}
        className="h-[180px] w-full rounded-lg bg-black object-cover"
      />
      <p className="text-center text-xs text-muted">{t.friends.verifyScanHint}</p>
      {etat.etranger && (
        <p role="status" className="text-center text-xs text-red">
          {t.friends.verifyScanForeign}
        </p>
      )}
    </div>
  );
}
