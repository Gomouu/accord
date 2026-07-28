/**
 * Identity-verification modal (safety numbers, E1). Shows the 60-digit
 * safety number (12 groups of 5) and its 8-emoji quick rendering, derived
 * locally from both identity keys — both peers see the exact same number.
 * The user compares it out of band, then toggles the verified flag. A key
 * change after verification is surfaced as "verification broken".
 * Same modal conventions as the rest of the repo: role="dialog", Escape
 * closes, Tab looped, focus returned to the trigger.
 */

import { lazy, Suspense, useEffect, useRef, useState } from 'react';
import { interpolate } from '../i18n';
import type { SafetyNumberInfo } from '../lib/api';
import { api } from '../lib/client';
import { bouclerTab } from '../lib/focus';
import { displayNameOf, useFriends } from '../stores/friends';
import { useT, useUi } from '../stores/ui';
import { CloseIcon } from './ContextMenu';

/** Shield glyph of the verified badge (stroke follows the current color). */
export function ShieldIcon({ size = 16 }: { size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
    >
      <path d="M20 13c0 5-3.5 7.5-7.66 8.95a1 1 0 0 1-.67-.01C7.5 20.5 4 18 4 13V6a1 1 0 0 1 1-1c2 0 4.5-1.2 6.24-2.72a1.17 1.17 0 0 1 1.52 0C14.51 3.81 17 5 19 5a1 1 0 0 1 1 1z" />
      <path d="m9 12 2 2 4-4" />
    </svg>
  );
}

// Les deux moitiés du QR (§17.4) sont paresseuses, et elles n'ont pas le
// choix : cette modale est importée statiquement par `AppShell`, donc tout ce
// qu'elle importe statiquement atterrit dans le chunk initial. Le générateur
// (`qrcode`) et le décodeur (`jsqr`) ont chacun leur entrée dans le
// `manualChunks` de `vite.config.ts` ; ils ne descendent qu'au clic.
const SafetyQrCode = lazy(async () => ({
  default: (await import('./SafetyQrCode')).SafetyQrCode,
}));
const SafetyQrScanner = lazy(async () => ({
  default: (await import('./SafetyQrScanner')).SafetyQrScanner,
}));

/** Panneau QR ouvert sous les chiffres — jamais à leur place. */
type PanneauQr = 'aucun' | 'afficher' | 'scanner';

/** Réservation de place pendant le chargement d'un panneau paresseux. */
function PanneauEnAttente() {
  return <div aria-hidden className="h-40 animate-pulse rounded-lg bg-rail/60" />;
}

/**
 * Bouton qui ouvre ou referme un panneau QR. Le libellé ne change pas avec
 * l'état — c'est `aria-pressed` qui le porte, pour que le nom accessible reste
 * stable (garde `e2e/a11y.spec.ts`).
 */
function PanelToggle({
  label,
  actif,
  onClick,
}: {
  label: string;
  actif: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      aria-pressed={actif}
      onClick={onClick}
      className={`flex-1 rounded-full px-3 py-2 text-xs font-medium transition-colors duration-fast focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-modal active:scale-[0.98] ${
        actif
          ? 'bg-blurple text-white hover:bg-blurple-hover'
          : 'bg-rail text-norm hover:bg-input'
      }`}
    >
      {label}
    </button>
  );
}

/** 60 digits → 12 space-separated groups of 5 (display form). */
function groupDigits(digits: string): string[] {
  const groups: string[] = [];
  for (let i = 0; i + 5 <= digits.length; i += 5) {
    groups.push(digits.slice(i, i + 5));
  }
  return groups;
}

export function FriendVerifyModal() {
  const t = useT();
  const target = useUi((s) => s.verifyTarget);
  const closeVerify = useUi((s) => s.closeVerify);
  const toast = useUi((s) => s.toast);
  const contacts = useFriends((s) => s.contacts);
  const reloadFriends = useFriends((s) => s.load);
  const ref = useRef<HTMLDivElement>(null);
  const [info, setInfo] = useState<SafetyNumberInfo | null>(null);
  const [busy, setBusy] = useState(false);
  const [panneau, setPanneau] = useState<PanneauQr>('aucun');

  useEffect(() => {
    setInfo(null);
    setPanneau('aucun');
    if (target === null) return undefined;
    let alive = true;
    api
      .friendsSafetyNumber(target)
      .then((value) => {
        if (alive) setInfo(value);
      })
      .catch(() => {
        if (alive) {
          toast('error', t.errors.loadFailed);
          closeVerify();
        }
      });
    return () => {
      alive = false;
    };
  }, [target, toast, t, closeVerify]);

  useEffect(() => {
    if (target === null) return undefined;
    const previous =
      document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') closeVerify();
      else if (e.key === 'Tab') bouclerTab(e, ref.current);
    };
    window.addEventListener('keydown', onKey);
    ref.current?.focus();
    return () => {
      window.removeEventListener('keydown', onKey);
      if (previous !== null && previous.isConnected) previous.focus();
    };
  }, [target, closeVerify]);

  if (target === null) return null;

  const name = displayNameOf(contacts, target);

  const toggleVerified = (): void => {
    if (info === null || busy) return;
    setBusy(true);
    api
      .friendsSetVerified(target, !info.verified)
      .then(() => {
        setInfo({ ...info, verified: !info.verified, key_changed: false });
        return reloadFriends();
      })
      .catch(() => toast('error', t.errors.actionFailed))
      .finally(() => setBusy(false));
  };

  return (
    <div
      className="modal-overlay-enter fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) closeVerify();
      }}
    >
      <div
        ref={ref}
        role="dialog"
        aria-modal="true"
        aria-label={t.friends.verifyTitle}
        tabIndex={-1}
        className="glass modal-panel-enter flex w-[26rem] max-w-[92vw] flex-col overflow-hidden rounded-xl shadow-3 focus:outline-none"
      >
        <div className="flex items-center justify-between px-5 pt-5">
          <h2 className="text-lg font-semibold text-header">{t.friends.verifyTitle}</h2>
          <button
            type="button"
            aria-label={t.app.close}
            onClick={closeVerify}
            // 28 → 44 px : le rembourrage grandit dans le `px-5 pt-5` vide de
            // l'en-tête, la marge négative le reprend au layout.
            className="relative -m-2 rounded-sm p-3 text-faint transition-colors duration-fast hover:text-norm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-modal active:scale-95"
          >
            <CloseIcon size={20} />
          </button>
        </div>
        <div className="flex flex-col gap-3 px-5 pb-5 pt-3">
          <p className="text-sm text-muted">
            {interpolate(t.friends.verifyIntro, { name })}
          </p>
          {info !== null && info.key_changed && (
            <p
              role="alert"
              className="rounded-lg border-s-4 border-red bg-red/10 px-3 py-2 text-sm text-norm"
            >
              {t.friends.verifyBroken}
            </p>
          )}
          {info === null ? (
            <div aria-hidden className="h-32 animate-pulse rounded-lg bg-rail/60" />
          ) : (
            <>
              <div
                aria-label={t.friends.verifyTitle}
                className="selectable grid grid-cols-4 gap-x-3 gap-y-1.5 rounded-lg bg-rail/50 px-4 py-3 text-center font-mono text-sm tracking-wide text-norm"
              >
                {groupDigits(info.digits).map((group, i) => (
                  // Index keys are safe: the list is fixed-length and static.
                  <span key={i}>{group}</span>
                ))}
              </div>
              <div className="flex items-center justify-center gap-1.5 text-2xl">
                {info.emoji.map((symbol, i) => (
                  <span key={i} aria-hidden>
                    {symbol}
                  </span>
                ))}
              </div>
              <p className="text-center text-xs text-faint">
                {t.friends.verifyEmojiHint}
              </p>
              {/* Le QR s'ajoute à la cérémonie, il ne la remplace pas : les
                  chiffres au-dessus restent lisibles et comparables à voix
                  haute quoi qu'il arrive à la caméra. */}
              <div className="flex gap-2">
                <PanelToggle
                  label={t.friends.verifyQrShow}
                  actif={panneau === 'afficher'}
                  onClick={() =>
                    setPanneau((p) => (p === 'afficher' ? 'aucun' : 'afficher'))
                  }
                />
                <PanelToggle
                  label={t.friends.verifyQrScan}
                  actif={panneau === 'scanner'}
                  onClick={() =>
                    setPanneau((p) => (p === 'scanner' ? 'aucun' : 'scanner'))
                  }
                />
              </div>
              {panneau !== 'aucun' && (
                <Suspense fallback={<PanneauEnAttente />}>
                  {panneau === 'afficher' ? (
                    <SafetyQrCode digits={info.digits} />
                  ) : (
                    // 🔒 Le scanner compare au numéro calculé localement, et
                    // c'est le seul point d'entrée : rien de ce qu'il décode
                    // ne remonte ici.
                    <SafetyQrScanner localDigits={info.digits} />
                  )}
                </Suspense>
              )}
              <button
                type="button"
                disabled={busy}
                onClick={toggleVerified}
                className={`mt-1 flex items-center justify-center gap-2 rounded-full px-3 py-2 text-sm font-medium transition-colors duration-fast focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-modal active:scale-[0.98] disabled:opacity-50 ${
                  info.verified
                    ? 'bg-rail text-norm hover:bg-input'
                    : 'bg-blurple text-white hover:bg-blurple-hover'
                }`}
              >
                <ShieldIcon />
                {info.verified ? t.friends.verifyUnmark : t.friends.verifyMark}
              </button>
              {info.verified && !info.key_changed && (
                <p className="flex items-center justify-center gap-1.5 text-xs font-medium text-green">
                  <ShieldIcon size={14} />
                  {t.friends.verifiedBadge}
                </p>
              )}
            </>
          )}
        </div>
      </div>
    </div>
  );
}
