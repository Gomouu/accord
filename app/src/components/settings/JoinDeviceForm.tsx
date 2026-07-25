/**
 * « J'ai un code » : le côté **nouvel appareil** de l'appairage (jalon 1, lot 1.D).
 *
 * Miroir de `PairDeviceButton` jusqu'à l'empreinte — là-bas on affiche un code,
 * ici on le recopie — puis les deux écrans divergent. Côté autorisant,
 * confirmer TERMINE l'appairage. Ici, confirmer ne fait que l'ouvrir : la
 * racine du compte arrive ensuite sur le canal confirmé, et tant qu'elle n'est
 * pas adoptée cette machine reste son propre compte.
 *
 * L'adoption ne peut pas se faire dans le nœud : rouvrir un coffre demande la
 * phrase de passe locale, que le nœud ne détient pas, et la clé de la base
 * dérive de la graine — la base déjà ouverte n'est donc pas réutilisable. D'où
 * la fin du parcours : une phrase de passe demandée ici, une commande hôte qui
 * scelle un compte NEUF, et un redémarrage du nœud dessus.
 */

import { useEffect, useState } from 'react';
import { api } from '../../lib/client';
import { useSession } from '../../stores/session';
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

/**
 * Attente maximale de la racine du compte après confirmation, en millisecondes.
 *
 * Bien plus courte que l'attente de l'empreinte, et pour une bonne raison :
 * celle-là couvrait une saisie humaine sur un autre appareil, celle-ci ne
 * couvre qu'un envoi sur un canal déjà établi entre deux machines du même
 * réseau local. Une minute est déjà hors de proportion — au-delà, ce n'est
 * plus de la lenteur, c'est que rien ne viendra.
 */
const ADOPT_WAIT_MAX_MS = 60 * 1000;

/**
 * Longueur minimale d'une phrase de passe locale.
 *
 * Recopiée de `screens/Onboarding` plutôt qu'importée : ce module est dans le
 * morceau paresseux des réglages, et tirer un écran d'accueil ici y ferait
 * entrer tout son habillage. Même règle, même écran de saisie — c'est la
 * phrase de passe d'un compte, pas une variante.
 */
const MIN_PASSPHRASE = 12;

/** Forme du code montrée en filigrane — identique dans toutes les langues. */
const CODE_SAMPLE = 'ABCD-EFGH';

/**
 * Étape du parcours, du code recopié jusqu'au compte adopté.
 *
 * Une union plutôt que des drapeaux indépendants : « en réception » et « en
 * attente d'empreinte » sondent le même nœud avec la même échéance et ne
 * diffèrent que par leur conclusion, et rien ne doit pouvoir les rendre vraies
 * ensemble.
 */
type Etape =
  | { readonly kind: 'code' }
  | { readonly kind: 'attente'; readonly deadlineMs: number }
  | { readonly kind: 'empreinte'; readonly valeur: string }
  | { readonly kind: 'reception'; readonly deadlineMs: number }
  | { readonly kind: 'phrase' }
  | { readonly kind: 'adoption' };

/**
 * Motif d'échec, gardé sous forme de clé et non de texte : la phrase est relue
 * à chaque rendu, donc un changement de langue la met à jour.
 */
type Echec = 'rejected' | 'expired' | 'neverArrived' | 'adoptFailed';

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
  const adoptPairedAccount = useSession((s) => s.adoptPairedAccount);
  const [draft, setDraft] = useState('');
  const [etape, setEtape] = useState<Etape>({ kind: 'code' });
  const [failure, setFailure] = useState<Echec | null>(null);
  const [pass, setPass] = useState('');
  const [busy, setBusy] = useState(false);

  // Sondage de l'état de l'appairage. Les deux attentes — l'empreinte, puis la
  // racine du compte — lisent la même méthode à la même cadence et abandonnent
  // à leur échéance de la même façon ; seule leur conclusion diffère.
  useEffect(() => {
    if (etape.kind !== 'attente' && etape.kind !== 'reception') return;
    const { kind, deadlineMs } = etape;
    let stopped = false;
    let inflight = false;
    const id = setInterval(() => {
      if (Date.now() >= deadlineMs) {
        // Arrêté ici plutôt qu'au seul nettoyage de l'effet : un tick de plus
        // avant le rendu suivant annulerait la tentative une deuxième fois.
        stopped = true;
        clearInterval(id);
        setEtape({ kind: 'code' });
        if (kind === 'attente') {
          setFailure('expired');
          // Le nœud doit oublier la tentative, pas seulement l'écran.
          void api.devicesPairCancel().catch(() => {
            // Sans conséquence : elle expire d'elle-même côté nœud.
          });
        } else {
          // 🔒 Rien à annuler ici, contrairement à l'attente d'empreinte :
          // l'appairage a bien eu lieu côté autorisant, cet appareil y figure.
          // C'est la racine qui manque — et sans elle cette machine reste son
          // propre compte. Le dire vaut mieux qu'une attente sans fin.
          setFailure('neverArrived');
        }
        return;
      }
      // Une réponse lente ne doit pas faire empiler les appels suivants.
      if (inflight) return;
      inflight = true;
      void api
        .devicesPairStatus()
        .then((r) => {
          if (stopped) return;
          if (kind === 'attente') {
            if (r.fingerprint !== null) {
              setEtape({ kind: 'empreinte', valeur: r.fingerprint });
            }
          } else if (r.adopted) setEtape({ kind: 'phrase' });
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
  }, [etape]);

  const submit = async () => {
    if (busy || !isCodeComplete(draft)) return;
    setBusy(true);
    setFailure(null);
    try {
      // La saisie part telle quelle : c'est le nœud qui normalise, et lui seul
      // qui juge de l'alphabet.
      await api.devicesPairSubmit(draft);
      setEtape({ kind: 'attente', deadlineMs: Date.now() + WAIT_MAX_MS });
    } catch {
      // Mal formé ou refusé : le nœud ne dit pas lequel des deux, et prétendre
      // le savoir serait deviner. Le message couvre les deux cas.
      setFailure('rejected');
    } finally {
      setBusy(false);
    }
  };

  const cancel = async () => {
    setEtape({ kind: 'code' });
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
      // 🔒 Aucun succès annoncé ici, et c'est tout l'écart avec l'autre écran :
      // là-bas confirmer termine l'appairage, ici il l'ouvre. Cette machine est
      // encore son propre compte tant que la racine n'est pas adoptée — un
      // « appareil appairé ! » à cet instant annoncerait ce qui n'a pas eu lieu.
      setEtape({ kind: 'reception', deadlineMs: Date.now() + ADOPT_WAIT_MAX_MS });
    } catch {
      // L'écran reste sur l'empreinte : rien n'a été appairé, et réessayer (ou
      // annuler) doit rester à portée de main.
      toast('error', t.errors.actionFailed);
    } finally {
      setBusy(false);
    }
  };

  const passTooShort = pass.length > 0 && pass.length < MIN_PASSPHRASE;
  const passReady = pass.length >= MIN_PASSPHRASE;

  const adopt = async () => {
    if (etape.kind !== 'phrase' || !passReady || busy) return;
    setBusy(true);
    setEtape({ kind: 'adoption' });
    try {
      await adoptPairedAccount(pass);
      // La bascule a déjà fermé la modale de réglages (elle parlait de
      // l'ancien profil) et démonté cet écran : il ne reste qu'à annoncer, et
      // c'est ici seulement que « appareil appairé » devient vrai.
      toast('success', ts.settings.pairAdopted);
    } catch {
      // 🔒 La racine reçue a été consommée par la tentative — l'hôte la reprend
      // avant tout ce qui peut échouer. Il n'y a donc plus rien à adopter :
      // un bouton « réessayer » échouerait autrement, et une phrase de passe
      // corrigée ne rattraperait rien. Retour à la saisie du code, en le disant.
      setEtape({ kind: 'code' });
      setDraft('');
      setFailure('adoptFailed');
    } finally {
      // La phrase de passe ne survit à aucune des deux issues.
      setPass('');
      setBusy(false);
    }
  };

  // 🔒 Dernière étape, et la seule sans bouton d'abandon. La racine est arrivée
  // et n'existe qu'en mémoire du nœud : « annuler » ici, ce serait proposer de
  // jeter le compte. Fermer la modale la laisse en revanche en plan — l'écran
  // repart du code au prochain passage, et il faut refaire un appairage.
  if (etape.kind === 'phrase' || etape.kind === 'adoption') {
    const sealing = etape.kind === 'adoption';
    return (
      <form
        className="mt-4 rounded-lg bg-sidebar px-4 py-4"
        onSubmit={(e) => {
          e.preventDefault();
          void adopt();
        }}
      >
        <p className="text-sm leading-relaxed text-muted">{ts.settings.pairAdoptHint}</p>

        <label
          htmlFor="pair-adopt-passphrase"
          className="mt-3 block text-sm font-medium text-norm"
        >
          {t.onboarding.passphrase}
        </label>
        <input
          id="pair-adopt-passphrase"
          type="password"
          value={pass}
          disabled={sealing}
          onChange={(e) => setPass(e.target.value)}
          className="mt-1 w-full rounded-lg border border-input bg-input px-3 py-2 text-sm text-norm outline-none focus-visible:ring-2 focus-visible:ring-blurple"
        />
        <p className="mt-1 text-xs leading-relaxed text-faint">
          {t.onboarding.passphraseHint}
        </p>
        {passTooShort && (
          <p className="mt-1 text-sm text-red">{t.onboarding.passphraseTooShort}</p>
        )}

        {/* Le compte qui a servi à l'appairage reste dans le sélecteur, à côté
            de celui qu'on adopte. Le dire AVANT la bascule plutôt qu'après :
            c'est ici que l'utilisateur décide, et deux entrées apparues sans
            explication ressembleraient à un bug. Rien ne les efface — supprimer
            un compte détruirait une identité dont la phrase de récupération est
            peut-être l'unique copie. */}
        <p className="mt-3 rounded-md border-s-4 border-yellow bg-rail/60 px-3 py-2 text-sm leading-relaxed text-muted">
          {ts.settings.pairAdoptLeftover}
        </p>

        <button
          type="submit"
          disabled={!passReady || sealing}
          className="mt-3 rounded-md bg-blurple px-3 py-2 text-sm font-medium text-white transition-colors hover:bg-blurple-hover disabled:opacity-50"
        >
          {sealing ? t.app.loading : ts.settings.pairAdoptSubmit}
        </button>
      </form>
    );
  }

  // 🔒 Étape de confirmation, identique à celle de l'autre écran. Deux issues,
  // et deux seulement : confirmer parce que les deux nombres concordent, ou
  // annuler. Un troisième bouton qui passerait outre viderait la vérification
  // de son sens — c'est elle qui transforme un code volé en tentative échouée.
  if (etape.kind === 'empreinte') {
    return (
      <div className="mt-4 rounded-lg bg-sidebar px-4 py-4">
        <p className="text-sm leading-relaxed text-muted">
          {ts.settings.pairFingerprintHint}
        </p>

        <div
          aria-label={ts.settings.pairFingerprintLabel}
          className="selectable mt-3 font-mono text-4xl font-semibold tracking-[0.3em]"
        >
          {etape.valeur}
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

  // Attente de la racine : sans état propre, l'écran serait retombé sur le
  // formulaire de saisie, ce qui aurait laissé croire que rien n'était en cours.
  // Pas de bouton d'annulation non plus — l'appairage est confirmé des deux
  // côtés, il n'y a plus d'offre à retirer, seulement un envoi à attendre.
  if (etape.kind === 'reception') {
    return (
      <div className="mt-4 rounded-lg bg-sidebar px-4 py-4">
        <p role="status" className="text-sm leading-relaxed text-muted">
          {ts.settings.pairAdoptWaiting}
        </p>
      </div>
    );
  }

  if (etape.kind === 'attente') {
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
        <p role="alert" className="mt-2 text-sm leading-relaxed text-red">
          {failure === 'expired'
            ? ts.settings.pairExpired
            : failure === 'neverArrived'
              ? ts.settings.pairAdoptNeverArrived
              : failure === 'adoptFailed'
                ? ts.settings.pairAdoptFailed
                : ts.settings.pairJoinRejected}
        </p>
      )}
    </form>
  );
}
