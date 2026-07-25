/**
 * « Ajouter un appareil » : ouvre une offre d'appairage, affiche le code avec
 * son compte à rebours, puis fait confirmer l'empreinte (jalon 1, lot 1.D).
 *
 * Deux étapes, dans cet ordre : le code ouvre la porte, l'empreinte vérifie
 * qui l'a franchie. Un code lu par-dessus l'épaule ne suffit donc pas — encore
 * faut-il que les deux écrans affichent le même nombre.
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

/** Cadence de sondage de l'état de l'offre, en millisecondes. */
const POLL_MS = 1000;

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
  const [fingerprint, setFingerprint] = useState<string | null>(null);
  const [now, setNow] = useState(() => Date.now());
  const [busy, setBusy] = useState(false);

  const remaining = offer === null ? 0 : offer.expiresMs - now;
  const expired = offer !== null && remaining <= 0;
  /**
   * Vrai tant qu'un code valable est affiché et qu'on attend l'autre appareil.
   *
   * C'est la seule fenêtre où les deux intervalles ont quelque chose à faire :
   * une fois l'empreinte obtenue, l'offre annulée ou le code expiré, un
   * intervalle qui tourne dans le vide réveille l'application pour rien.
   */
  const awaitingPeer = offer !== null && fingerprint === null && !expired;

  // Compte à rebours du code.
  const timer = useRef<ReturnType<typeof setInterval> | null>(null);
  useEffect(() => {
    if (!awaitingPeer) return;
    timer.current = setInterval(() => setNow(Date.now()), 1000);
    return () => {
      if (timer.current !== null) clearInterval(timer.current);
      timer.current = null;
    };
  }, [awaitingPeer]);

  // Sondage de l'état de l'offre : c'est lui qui apporte l'empreinte dès qu'un
  // échange a abouti.
  useEffect(() => {
    if (!awaitingPeer) return;
    let stopped = false;
    let inflight = false;
    const id = setInterval(() => {
      // Une réponse lente ne doit pas faire empiler les appels suivants.
      if (inflight) return;
      inflight = true;
      void api
        .devicesPairStatus()
        .then((r) => {
          if (!stopped && r.fingerprint !== null) setFingerprint(r.fingerprint);
        })
        .catch(() => {
          // Un sondage raté n'apprend rien à l'utilisateur : le suivant
          // réessaiera, et l'offre expirera d'elle-même s'il n'aboutit jamais.
        })
        .finally(() => {
          inflight = false;
        });
    }, POLL_MS);
    return () => {
      stopped = true;
      clearInterval(id);
    };
  }, [awaitingPeer]);

  const start = async () => {
    if (busy) return;
    setBusy(true);
    try {
      const r = await api.devicesPairStart();
      setNow(Date.now());
      setFingerprint(null);
      setOffer({ code: r.code, expiresMs: r.expires_ms });
    } catch {
      toast('error', t.errors.actionFailed);
    } finally {
      setBusy(false);
    }
  };

  const cancel = async () => {
    setOffer(null);
    setFingerprint(null);
    // Le nœud doit oublier l'offre, pas seulement l'écran : sinon le code
    // resterait acceptable alors que plus personne ne le regarde.
    try {
      await api.devicesPairCancel();
    } catch {
      // Sans conséquence : l'offre expirera d'elle-même dans cinq minutes.
    }
  };

  const confirm = async () => {
    if (busy) return;
    setBusy(true);
    try {
      await api.devicesPairConfirm();
      setOffer(null);
      setFingerprint(null);
      toast('success', t.settings.pairConfirmed);
    } catch {
      // L'écran reste sur l'empreinte : rien n'a été appairé, et réessayer (ou
      // annuler) doit rester à portée de main.
      toast('error', t.errors.actionFailed);
    } finally {
      setBusy(false);
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

  // 🔒 Étape de confirmation. Elle n'offre que deux issues — confirmer parce
  // que les deux nombres sont identiques, ou annuler. Un troisième bouton qui
  // passerait outre viderait la vérification de son sens : c'est elle qui
  // transforme un code volé en tentative échouée.
  if (fingerprint !== null) {
    return (
      <div className="mt-4 rounded-lg bg-sidebar px-4 py-4">
        <p className="text-sm leading-relaxed text-muted">
          {t.settings.pairFingerprintHint}
        </p>

        <div
          aria-label={t.settings.pairFingerprintLabel}
          className="selectable mt-3 font-mono text-4xl font-semibold tracking-[0.3em]"
        >
          {fingerprint}
        </div>

        <p className="mt-2 text-sm leading-relaxed text-red">
          {t.settings.pairFingerprintMismatch}
        </p>

        <div className="mt-3 flex flex-wrap gap-2">
          <button
            type="button"
            onClick={() => void confirm()}
            disabled={busy}
            className="rounded-md bg-blurple px-3 py-2 text-sm font-medium text-white transition-colors hover:bg-blurple-hover disabled:opacity-50"
          >
            {t.settings.pairConfirm}
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
