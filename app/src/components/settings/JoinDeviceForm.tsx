/**
 * « J'ai un code » : le côté **nouvel appareil** de l'appairage (jalon 1, lot 1.D).
 *
 * Miroir de `PairDeviceButton` : là-bas on affiche un code, ici on le recopie.
 * Les deux écrans se rejoignent ensuite sur la même étape d'empreinte — c'est
 * elle, et non le code, qui dit qui se trouve en face.
 */

import { useEffect, useState } from 'react';
import { api } from '../../lib/client';
import { useSettingsT, useT, useUi } from '../../stores/ui';

/** Longueur d'un code, espaces et tirets ignorés (`CODE_LEN` côté nœud). */
const CODE_LEN = 8;

/** Cadence de sondage de l'état de l'appairage, en millisecondes. */
const POLL_MS = 1000;

/**
 * Attente maximale de l'empreinte, en millisecondes.
 *
 * Un code ne vit que cinq minutes (`CODE_TTL_MS` côté nœud) : passé ce délai
 * il n'y a plus rien à attendre, et un intervalle qui continue d'interroger le
 * nœud réveille l'application pour rien.
 */
const WAIT_MAX_MS = 5 * 60 * 1000;

/** Forme du code montrée en filigrane — identique dans toutes les langues. */
const CODE_SAMPLE = 'ABCD-EFGH';

/**
 * Vrai quand la saisie a la longueur d'un code, espaces et tirets ignorés.
 *
 * 🔒 Seul contrôle fait ici, et délibérément. Le code se recopie d'un écran à
 * l'autre : espaces, tirets et minuscules doivent passer, c'est le nœud qui
 * normalise. Et un caractère hors alphabet (`0`, `O`, `1`, `I`, `L`) part tel
 * quel se faire refuser plutôt que d'être corrigé en silence — corriger un
 * « 0 » en « O », ce serait valider un code que l'utilisateur croit avoir tapé
 * alors qu'il en a tapé un autre.
 */
export function isCodeComplete(input: string): boolean {
  return input.replace(/[\s-]/g, '').length === CODE_LEN;
}

export function JoinDeviceForm() {
  const t = useT();
  const ts = useSettingsT();
  const toast = useUi((s) => s.toast);
  const [draft, setDraft] = useState('');
  const [deadlineMs, setDeadlineMs] = useState<number | null>(null);
  const [fingerprint, setFingerprint] = useState<string | null>(null);
  /**
   * Motif d'échec, gardé sous forme de clé et non de texte : la phrase est
   * relue à chaque rendu, donc un changement de langue la met à jour.
   */
  const [failure, setFailure] = useState<'rejected' | 'expired' | null>(null);
  const [busy, setBusy] = useState(false);

  /**
   * Vrai entre l'envoi accepté et l'empreinte.
   *
   * C'est la seule fenêtre où le sondage a quelque chose à apprendre : une
   * fois l'empreinte obtenue, l'écran attend une décision humaine.
   */
  const awaitingPeer = deadlineMs !== null && fingerprint === null;

  useEffect(() => {
    if (!awaitingPeer || deadlineMs === null) return;
    let stopped = false;
    let inflight = false;
    const id = setInterval(() => {
      if (Date.now() >= deadlineMs) {
        // Arrêté ici plutôt qu'au seul nettoyage de l'effet : un tick de plus
        // avant le rendu suivant annulerait la tentative une deuxième fois.
        stopped = true;
        clearInterval(id);
        setDeadlineMs(null);
        setFailure('expired');
        // Le nœud doit oublier la tentative, pas seulement l'écran.
        void api.devicesPairCancel().catch(() => {
          // Sans conséquence : elle expire d'elle-même côté nœud.
        });
        return;
      }
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
          // réessaiera, et l'attente s'arrêtera d'elle-même à l'échéance.
        })
        .finally(() => {
          inflight = false;
        });
    }, POLL_MS);
    return () => {
      stopped = true;
      clearInterval(id);
    };
  }, [awaitingPeer, deadlineMs]);

  const submit = async () => {
    if (busy || !isCodeComplete(draft)) return;
    setBusy(true);
    setFailure(null);
    try {
      // La saisie part telle quelle : c'est le nœud qui normalise, et lui seul
      // qui juge de l'alphabet.
      await api.devicesPairSubmit(draft);
      setFingerprint(null);
      setDeadlineMs(Date.now() + WAIT_MAX_MS);
    } catch {
      // Mal formé ou refusé : le nœud ne dit pas lequel des deux, et prétendre
      // le savoir serait deviner. Le message couvre les deux cas.
      setFailure('rejected');
    } finally {
      setBusy(false);
    }
  };

  const cancel = async () => {
    setDeadlineMs(null);
    setFingerprint(null);
    setFailure(null);
    // La saisie repart de zéro : un code abandonné parce que les empreintes
    // divergeaient ne doit pas rester à portée d'un second envoi.
    setDraft('');
    try {
      await api.devicesPairCancel();
    } catch {
      // Sans conséquence : la tentative expire d'elle-même côté nœud.
    }
  };

  const confirm = async () => {
    if (busy) return;
    setBusy(true);
    try {
      await api.devicesPairConfirm();
      setDeadlineMs(null);
      setFingerprint(null);
      setDraft('');
      toast('success', ts.settings.pairConfirmed);
    } catch {
      // L'écran reste sur l'empreinte : rien n'a été appairé, et réessayer (ou
      // annuler) doit rester à portée de main.
      toast('error', t.errors.actionFailed);
    } finally {
      setBusy(false);
    }
  };

  // 🔒 Étape de confirmation, identique à celle de l'autre écran. Deux issues,
  // et deux seulement : confirmer parce que les deux nombres concordent, ou
  // annuler. Un troisième bouton qui passerait outre viderait la vérification
  // de son sens — c'est elle qui transforme un code volé en tentative échouée.
  if (fingerprint !== null) {
    return (
      <div className="mt-4 rounded-lg bg-sidebar px-4 py-4">
        <p className="text-sm leading-relaxed text-muted">
          {ts.settings.pairFingerprintHint}
        </p>

        <div
          aria-label={ts.settings.pairFingerprintLabel}
          className="selectable mt-3 font-mono text-4xl font-semibold tracking-[0.3em]"
        >
          {fingerprint}
        </div>

        <p className="mt-2 text-sm leading-relaxed text-red">
          {ts.settings.pairFingerprintMismatch}
        </p>

        <div className="mt-3 flex flex-wrap gap-2">
          <button
            type="button"
            onClick={() => void confirm()}
            disabled={busy}
            className="rounded-md bg-blurple px-3 py-2 text-sm font-medium text-white transition-colors hover:bg-blurple-hover disabled:opacity-50"
          >
            {ts.settings.pairConfirm}
          </button>
          <button
            type="button"
            onClick={() => void cancel()}
            className="rounded-md bg-chat px-3 py-2 text-sm font-medium transition-colors hover:bg-chat/70"
          >
            {ts.settings.pairCancel}
          </button>
        </div>
      </div>
    );
  }

  if (awaitingPeer) {
    return (
      <div className="mt-4 rounded-lg bg-sidebar px-4 py-4">
        <p className="text-sm leading-relaxed text-muted">
          {ts.settings.pairJoinWaiting}
        </p>

        <button
          type="button"
          onClick={() => void cancel()}
          className="mt-3 rounded-md bg-chat px-3 py-2 text-sm font-medium transition-colors hover:bg-chat/70"
        >
          {ts.settings.pairCancel}
        </button>
      </div>
    );
  }

  return (
    <form
      className="mt-4 rounded-lg bg-sidebar px-4 py-4"
      onSubmit={(e) => {
        e.preventDefault();
        void submit();
      }}
    >
      <p className="text-sm leading-relaxed text-muted">{ts.settings.pairJoinHint}</p>

      <div className="mt-3 flex flex-wrap items-center gap-2">
        <input
          type="text"
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          aria-label={ts.settings.pairJoinLabel}
          placeholder={CODE_SAMPLE}
          autoComplete="off"
          spellCheck={false}
          className="min-w-0 flex-1 rounded-md bg-chat px-3 py-2 font-mono text-sm uppercase tracking-[0.2em] outline-none ring-blurple placeholder:tracking-normal focus-visible:ring-2"
        />
        <button
          type="submit"
          disabled={busy || !isCodeComplete(draft)}
          className="shrink-0 rounded-md bg-blurple px-3 py-2 text-sm font-medium text-white transition-colors hover:bg-blurple-hover disabled:opacity-50"
        >
          {ts.settings.pairJoinSubmit}
        </button>
      </div>

      {failure !== null && (
        <p className="mt-2 text-sm leading-relaxed text-red">
          {failure === 'expired' ? ts.settings.pairExpired : ts.settings.pairJoinRejected}
        </p>
      )}
    </form>
  );
}
