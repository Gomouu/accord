/**
 * « Ajouter un appareil » : ouvre une offre d'appairage et affiche le code
 * avec son compte à rebours (jalon 1, lot 1.D).
 *
 * L'échange lui-même et la confirmation d'empreinte viennent ensuite ; ici on
 * ne fait qu'ouvrir la porte et montrer combien de temps elle reste ouverte.
 */

import { useEffect, useRef, useState } from 'react';
import { api } from '../../lib/client';
import { interpolate } from '../../i18n';
import { useT, useUi } from '../../stores/ui';

/** Une offre ouverte, telle que le composant la suit. */
interface Offer {
  code: string;
  expiresMs: number;
}

/** Rend un reste de millisecondes en `m:ss`. */
export function formatRemaining(ms: number): string {
  const total = Math.max(0, Math.ceil(ms / 1000));
  const m = Math.floor(total / 60);
  const s = total % 60;
  return `${m}:${String(s).padStart(2, '0')}`;
}

export function PairDeviceButton() {
  const t = useT();
  const toast = useUi((s) => s.toast);
  const [offer, setOffer] = useState<Offer | null>(null);
  const [now, setNow] = useState(() => Date.now());
  const [busy, setBusy] = useState(false);

  // Un seul intervalle, et seulement tant qu'une offre est affichée : un
  // compte à rebours qui tourne dans le vide réveille l'application pour rien.
  const timer = useRef<ReturnType<typeof setInterval> | null>(null);
  useEffect(() => {
    if (offer === null) return;
    timer.current = setInterval(() => setNow(Date.now()), 1000);
    return () => {
      if (timer.current !== null) clearInterval(timer.current);
      timer.current = null;
    };
  }, [offer]);

  const remaining = offer === null ? 0 : offer.expiresMs - now;
  const expired = offer !== null && remaining <= 0;

  const start = async () => {
    if (busy) return;
    setBusy(true);
    try {
      const r = await api.devicesPairStart();
      setNow(Date.now());
      setOffer({ code: r.code, expiresMs: r.expires_ms });
    } catch {
      toast('error', t.errors.actionFailed);
    } finally {
      setBusy(false);
    }
  };

  const cancel = async () => {
    setOffer(null);
    // Le nœud doit oublier l'offre, pas seulement l'écran : sinon le code
    // resterait acceptable alors que plus personne ne le regarde.
    try {
      await api.devicesPairCancel();
    } catch {
      // Sans conséquence : l'offre expirera d'elle-même dans cinq minutes.
    }
  };

  if (offer === null) {
    return (
      <button
        type="button"
        onClick={() => void start()}
        disabled={busy}
        className="mt-4 rounded-md bg-blurple px-3 py-2 text-sm font-medium text-white transition-colors hover:bg-blurple-hover disabled:opacity-50"
      >
        {t.settings.pairAdd}
      </button>
    );
  }

  return (
    <div className="mt-4 rounded-lg bg-sidebar px-4 py-4">
      <p className="text-sm leading-relaxed text-muted">{t.settings.pairHint}</p>

      <div
        aria-label={t.settings.pairCodeLabel}
        className="selectable mt-3 font-mono text-2xl font-semibold tracking-[0.2em]"
      >
        {offer.code}
      </div>

      <p className={`mt-1 text-xs ${expired ? 'text-red' : 'text-muted'}`}>
        {expired
          ? t.settings.pairExpired
          : interpolate(t.settings.pairExpiresIn, {
              time: formatRemaining(remaining),
            })}
      </p>

      <div className="mt-3 flex flex-wrap gap-2">
        <button
          type="button"
          onClick={() => void start()}
          disabled={busy}
          className="rounded-md bg-blurple px-3 py-2 text-sm font-medium text-white transition-colors hover:bg-blurple-hover disabled:opacity-50"
        >
          {t.settings.pairNewCode}
        </button>
        <button
          type="button"
          onClick={() => void cancel()}
          className="rounded-md bg-chat px-3 py-2 text-sm font-medium transition-colors hover:bg-chat/70"
        >
          {t.settings.pairCancel}
        </button>
      </div>
    </div>
  );
}
