/**
 * Onglet Sécurité (jalon 2, lots 2.C et 2.D) : comment la clé de chaque session
 * a été mise en accord, expliqué en langage clair, plus le réglage avancé qui
 * refuse les sessions classiques.
 *
 * 🔒 Discipline de formulation, non négociable : aucune chaîne d'ici ne promet
 * « incassable » ni « pour toujours ». La formule est « résiste aux attaques par
 * ordinateur quantique connues à ce jour » — ce qui est vrai et ce qui est
 * vérifiable. Une interface qui survend la cryptographie fait prendre à
 * l'utilisateur des risques qu'il n'aurait pas pris en connaissance de cause.
 *
 * ⚠️ L'état affiché par contact décrit la SESSION, pas l'appareil d'en face.
 * Une session standard ne prouve pas que le contact « ne sait pas faire » : dans
 * un WELCOME, le bit de capacité dit ce que le répondeur a fait, pas ce qu'il
 * sait faire (SPEC §2.2.2). C'est pourquoi le libellé parle de la session et que
 * l'indice l'explicite.
 */

import { useCallback, useEffect, useMemo, useState } from 'react';
import type { PeerLink, SecurityState } from '../../lib/api';
import { api, rpc } from '../../lib/client';
import { displayNameOf, useFriends } from '../../stores/friends';
import { useSettingsT, useT, useUi } from '../../stores/ui';
import { SettingsSection, ToggleRow } from './controls';

export function SecurityTab() {
  const t = useT();
  const ts = useSettingsT();
  const toast = useUi((s) => s.toast);
  const contacts = useFriends((s) => s.contacts);
  const loadFriends = useFriends((s) => s.load);
  const [state, setState] = useState<SecurityState | null>(null);
  const [peers, setPeers] = useState<PeerLink[]>([]);
  /** Vrai le temps d'un aller-retour vers le nœud : évite le double clic. */
  const [enCours, setEnCours] = useState(false);

  const rafraichir = useCallback((): void => {
    api
      .securityState()
      .then(setState)
      .catch(() => setState(null));
    api
      .networkPeers()
      .then(setPeers)
      .catch(() => setPeers([]));
  }, []);

  useEffect(() => {
    loadFriends().catch(() => toast('error', t.errors.loadFailed));
  }, [loadFriends, toast, t]);

  useEffect(() => {
    rafraichir();
    // Une session qui s'ouvre ou se ferme change l'état affiché ; sans cet
    // abonnement l'écran resterait juste sur la photographie de son ouverture.
    return rpc.onEvent((method) => {
      if (method === 'event.network') rafraichir();
    });
  }, [rafraichir]);

  // Contacts connectés d'abord, puis par nom : ce sont les seuls dont l'état de
  // chiffrement soit renseigné, donc les seuls que l'écran puisse décrire.
  const tries = useMemo(() => {
    return [...peers].sort((a, b) => {
      if (a.live !== b.live) return a.live ? -1 : 1;
      return displayNameOf(contacts, a.pubkey).localeCompare(
        displayNameOf(contacts, b.pubkey),
      );
    });
  }, [peers, contacts]);

  const basculerExigence = (require: boolean): void => {
    if (enCours) return;
    setEnCours(true);
    api
      .securitySetRequireHybrid(require)
      .then(setState)
      .catch(() => toast('error', ts.settings.securityRequireFailed))
      .finally(() => setEnCours(false));
  };

  if (state === null) {
    return (
      <div>
        <SettingsSection title={ts.settings.securityStateTitle}>
          <p className="rounded-lg bg-sidebar p-4 text-sm text-muted">
            {ts.settings.securityUnavailable}
          </p>
        </SettingsSection>
      </div>
    );
  }

  const total = state.hybrid_sessions + state.classic_sessions;

  return (
    <div>
      <SettingsSection
        title={ts.settings.securityStateTitle}
        hint={ts.settings.securityStateHint}
      >
        <p className="rounded-lg bg-sidebar p-4 text-sm leading-relaxed text-muted">
          <span className="mb-1 block text-sm font-medium text-header">
            {ts.settings.securityHybridExplainTitle}
          </span>
          {ts.settings.securityHybridExplain}
        </p>
      </SettingsSection>

      <SettingsSection
        title={ts.settings.securityPerContactTitle}
        hint={ts.settings.securityPerContactHint}
      >
        {tries.length === 0 ? (
          <p className="rounded-lg bg-sidebar p-4 text-sm text-muted">
            {ts.settings.securityPerContactEmpty}
          </p>
        ) : (
          <ul className="divide-y divide-input overflow-hidden rounded-lg bg-sidebar">
            {tries.map((p) => (
              <li key={p.pubkey} className="flex items-center gap-3 p-3">
                <span
                  aria-hidden
                  className={`h-2.5 w-2.5 shrink-0 rounded-full ${
                    p.live ? 'bg-green' : 'bg-faint/50'
                  }`}
                />
                <div className="min-w-0 flex-1">
                  <div className="truncate text-sm font-medium text-norm">
                    {displayNameOf(contacts, p.pubkey)}
                  </div>
                  <div className="truncate text-xs text-faint">
                    {etatChiffrement(p, {
                      hybride: t.reseau.encryptionHybrid,
                      classique: t.reseau.encryptionClassic,
                      aucune: ts.settings.securityNoSession,
                    })}
                  </div>
                </div>
                {p.live && p.post_quantum === true && (
                  <span className="shrink-0 rounded-full bg-green/15 px-2 py-0.5 text-[11px] font-medium text-green">
                    {ts.settings.securityRatioHybrid}
                  </span>
                )}
              </li>
            ))}
          </ul>
        )}
      </SettingsSection>

      <SettingsSection title={ts.settings.securityRatioTitle}>
        <div className="rounded-lg bg-sidebar p-4">
          <div className="grid grid-cols-2 gap-3 text-center">
            <Compteur
              label={ts.settings.securityRatioHybrid}
              valeur={state.hybrid_sessions}
              part={part(state.hybrid_sessions, total)}
            />
            <Compteur
              label={ts.settings.securityRatioClassic}
              valeur={state.classic_sessions}
              part={part(state.classic_sessions, total)}
            />
          </div>
          <p className="mt-3 text-xs leading-relaxed text-faint">
            {ts.settings.securityRatioLocal}
          </p>
        </div>
      </SettingsSection>

      <SettingsSection title={ts.settings.securityRequireTitle}>
        <ToggleRow
          label={ts.settings.securityRequireTitle}
          hint={ts.settings.securityRequireHint}
          checked={state.require_hybrid}
          disabled={enCours}
          onChange={basculerExigence}
        />
        <p className="px-1 text-xs text-faint">
          {state.require_hybrid
            ? ts.settings.securityRequireOn
            : ts.settings.securityRequireOff}
        </p>
      </SettingsSection>
    </div>
  );
}

/**
 * Libellé de l'état de chiffrement d'un lien.
 *
 * ⚠️ `post_quantum` absent (nœud antérieur au jalon 2) n'est PAS « standard » :
 * c'est « on ne sait pas ». Le rabattre sur « standard » ferait afficher une
 * affirmation que rien n'appuie, ce qui est pire qu'une case vide.
 */
export function etatChiffrement(
  lien: PeerLink,
  libelles: { hybride: string; classique: string; aucune: string },
): string {
  if (!lien.live) return libelles.aucune;
  if (lien.post_quantum === undefined) return '';
  return lien.post_quantum ? libelles.hybride : libelles.classique;
}

/** Part entière d'un compteur dans le total, ou `null` si le total est nul. */
export function part(valeur: number, total: number): number | null {
  if (total <= 0) return null;
  return Math.round((valeur / total) * 100);
}

function Compteur({
  label,
  valeur,
  part: pourcentage,
}: {
  label: string;
  valeur: number;
  part: number | null;
}) {
  return (
    <div className="rounded-lg bg-rail py-2">
      <div className="text-lg font-medium text-header">
        {valeur}
        {pourcentage !== null && (
          <span className="ms-1 text-xs font-normal text-faint">{pourcentage} %</span>
        )}
      </div>
      <div className="mt-0.5 text-xs text-faint">{label}</div>
    </div>
  );
}
