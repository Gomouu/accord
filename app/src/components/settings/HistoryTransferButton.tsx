/**
 * « Récupérer l'historique depuis cet appareil » (feuille de route §17.4),
 * une ligne d'appareil à la fois.
 *
 * `devices.transfer_history` ne rend la main qu'à la FIN — des minutes
 * possibles. Rien n'attend donc cette réponse pour s'afficher : l'abonnement à
 * `event.history_transfer` est pris AVANT l'appel, et c'est lui seul qui
 * alimente la barre. La réponse ne sert qu'à conclure.
 *
 * ⚠️ **Ce composant ne dit jamais « terminé » tout court.** Un transfert qui
 * finit sans une seule page reçue a deux causes que le nœud ne sait pas
 * distinguer — l'appareil d'en face n'a rien de plus ancien, ou il tourne une
 * version trop ancienne pour répondre. Les taire ferait passer un pair
 * dépassé pour un « vous étiez déjà à jour ». Voir `lib/historyTransfer.ts`.
 *
 * ⚠️ **L'état vit dans ce composant, donc il ne survit pas à sa disparition.**
 * Changer d'onglet de réglages pendant un transfert démonte la section : le
 * nœud continue, mais la barre et la conclusion sont perdues. Relancer est
 * sans danger (chaque passe redemande simplement la page suivante), et c'est
 * la seule reprise offerte pour l'instant — porter l'état dans un magasin
 * partagé serait la façon d'y remédier le jour où ça gêne.
 */

import { useEffect, useRef, useState } from 'react';
import { api } from '../../lib/client';
import { interpolate } from '../../i18n';
import {
  conclure,
  observerTransfertHistorique,
  type AvancementHistorique,
  type IssueTransfert,
} from '../../lib/historyTransfer';
import { useSettingsT, useT, useUi } from '../../stores/ui';

interface Props {
  /** Clé publique (hex) de l'appareil d'où tirer l'historique. */
  pubkey: string;
  /** Vrai quand un transfert tourne déjà sur une AUTRE ligne de la liste. */
  bloque: boolean;
  /** Prévient la liste qu'un transfert démarre (`true`) ou s'achève (`false`). */
  onActif: (actif: boolean) => void;
}

/** Résumé final rendu par `devices.transfer_history`. */
interface Resume {
  conversations: number;
  pages: number;
}

export function HistoryTransferButton({ pubkey, bloque, onActif }: Props) {
  const t = useT();
  const ts = useSettingsT();
  const toast = useUi((s) => s.toast);
  const [enCours, setEnCours] = useState(false);
  const [avancement, setAvancement] = useState<AvancementHistorique | null>(null);
  const [issue, setIssue] = useState<IssueTransfert | null>(null);
  const [resume, setResume] = useState<Resume | null>(null);

  /**
   * Désabonnement du transfert en cours, et vivacité de l'écran.
   *
   * Le transfert survit à plusieurs rendus et peut durer des minutes : sans ces
   * deux références, refermer les réglages en cours de route laisserait un
   * abonnement branché sur un composant démonté.
   */
  const desabonner = useRef<(() => void) | null>(null);
  const vivant = useRef(true);
  useEffect(() => {
    // Réarmé à chaque montage : en mode strict, le démontage simulé du premier
    // passage laisserait sinon la référence à `false` pour de bon.
    vivant.current = true;
    return () => {
      vivant.current = false;
      desabonner.current?.();
      desabonner.current = null;
    };
  }, []);

  const lancer = () => {
    if (enCours || bloque) return;
    setEnCours(true);
    setAvancement(null);
    setIssue(null);
    setResume(null);
    onActif(true);
    // Abonnement d'abord : la promesse ci-dessous ne se dénoue qu'à la toute
    // fin, et tout ce que l'utilisateur voit d'ici là vient de l'événement.
    const off = observerTransfertHistorique((a) => {
      if (vivant.current) setAvancement(a);
    });
    desabonner.current = off;
    void api
      .devicesTransferHistory(pubkey)
      .then((r) => {
        if (!vivant.current) return;
        setResume(r);
        setIssue(conclure(r.conversations, r.pages));
      })
      .catch(() => {
        // Appareil hors de la liste signée du compte, ou lien coupé : l'écran
        // revient à son bouton sans afficher de conclusion inventée.
        if (vivant.current) toast('error', t.errors.actionFailed);
      })
      .finally(() => {
        off();
        if (desabonner.current === off) desabonner.current = null;
        if (!vivant.current) return;
        setEnCours(false);
        onActif(false);
      });
  };

  const done = avancement?.done ?? 0;
  const total = avancement?.total ?? 0;
  const pages = avancement?.messages ?? 0;
  // Avant le premier événement, `total` vaut zéro : la barre bat sans prétendre
  // à un pourcentage qu'on ne connaît pas encore.
  const pourcent = total > 0 ? Math.round((done / total) * 100) : null;

  return (
    <div className="mt-3">
      <button
        type="button"
        onClick={lancer}
        disabled={enCours || bloque}
        className="rounded-md bg-chat px-3 py-1.5 text-sm font-medium transition-colors duration-fast hover:bg-chat-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple disabled:opacity-50"
      >
        {ts.settings.historyTransferAction}
      </button>

      {enCours && (
        <>
          <div
            role="progressbar"
            aria-label={ts.settings.historyTransferProgressLabel}
            aria-valuemin={0}
            aria-valuemax={total > 0 ? total : undefined}
            aria-valuenow={pourcent === null ? undefined : done}
            className="mt-2 h-1.5 w-full overflow-hidden rounded-full bg-input"
          >
            <div
              className={`h-full rounded-full bg-blurple ${pourcent === null ? 'w-full animate-pulse' : ''}`}
              style={pourcent === null ? undefined : { width: `${pourcent}%` }}
            />
          </div>
          <p role="status" className="mt-1 text-xs text-muted">
            {interpolate(ts.settings.historyTransferRunning, {
              done: String(done),
              total: String(total),
              pages: String(pages),
            })}
          </p>
        </>
      )}

      {/* 🔴 La seule issue qui se raconte en couleur d'alerte : celle où
          l'utilisateur risquerait de croire son historique complet. */}
      {!enCours && issue === 'ambigu' && (
        <p role="status" className="mt-2 text-xs leading-relaxed text-yellow">
          {ts.settings.historyTransferAmbiguous}
        </p>
      )}

      {!enCours && issue === 'carnet-vide' && (
        <p role="status" className="mt-2 text-xs leading-relaxed text-muted">
          {ts.settings.historyTransferEmptyBook}
        </p>
      )}

      {!enCours && issue === 'recu' && resume !== null && (
        <p role="status" className="mt-2 text-xs leading-relaxed text-muted">
          {interpolate(ts.settings.historyTransferDone, {
            pages: String(resume.pages),
            conversations: String(resume.conversations),
          })}
        </p>
      )}
    </div>
  );
}
