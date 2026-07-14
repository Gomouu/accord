/**
 * Onglet Réseau : affiche l'adresse locale à communiquer à un ami, les
 * compteurs de connexions (rafraîchis par `event.network`), et permet
 * d'ajouter/retirer un pair d'amorçage par son adresse `ip:port`. C'est le
 * point d'entrée de l'usage « je donne l'app à un ami et on se connecte ».
 */

import { useCallback, useEffect, useState } from 'react';
import type { NetworkStatus } from '../../lib/api';
import { api, rpc } from '../../lib/client';
import { useT } from '../../stores/ui';
import { SettingsSection } from './controls';

const COPY_FEEDBACK_MS = 1500;

export function NetworkTab() {
  const t = useT();
  const [status, setStatus] = useState<NetworkStatus | null>(null);
  const [erreur, setErreur] = useState<string | null>(null);
  const [saisie, setSaisie] = useState('');
  const [ajoutErreur, setAjoutErreur] = useState(false);
  const [enCours, setEnCours] = useState(false);
  const [copiee, setCopiee] = useState<string | null>(null);

  const rafraichir = useCallback((): void => {
    api
      .networkStatus()
      .then((s) => {
        setStatus(s);
        setErreur(null);
      })
      .catch(() => setErreur(t.reseau.refreshFailed));
  }, [t.reseau.refreshFailed]);

  useEffect(() => {
    rafraichir();
    // Les compteurs changent en direct : on rafraîchit sur `event.network`.
    const off = rpc.onEvent((method) => {
      if (method === 'event.network') rafraichir();
    });
    return off;
  }, [rafraichir]);

  const copier = (valeur: string): void => {
    void navigator.clipboard.writeText(valeur).then(() => {
      setCopiee(valeur);
      setTimeout(() => setCopiee((c) => (c === valeur ? null : c)), COPY_FEEDBACK_MS);
    });
  };

  const ajouter = (): void => {
    const addr = saisie.trim();
    if (addr === '' || enCours) return;
    setEnCours(true);
    setAjoutErreur(false);
    api
      .networkAddPeer(addr)
      .then((s) => {
        setStatus(s);
        setSaisie('');
      })
      .catch(() => setAjoutErreur(true))
      .finally(() => setEnCours(false));
  };

  const retirer = (addr: string): void => {
    api
      .networkRemovePeer(addr)
      .then(setStatus)
      .catch(() => setErreur(t.reseau.refreshFailed));
  };

  // Valeurs dérivées pour la section « Connexion automatique ». Tant qu'on n'a
  // pas de statut, on retombe sur des valeurs neutres (aucun mapping, pas
  // d'adresse externe) pour ne rien afficher de trompeur.
  const portMapping = status?.port_mapping ?? 'aucun';
  const externalAddr = status?.external_addr ?? null;

  return (
    <div>
      <SettingsSection title={t.reseau.title} hint={t.reseau.intro}>
        {erreur !== null && <p className="mb-3 text-sm text-red">{erreur}</p>}
        <div className="rounded-lg bg-sidebar p-4">
          <div className="text-xs font-medium uppercase text-faint">
            {t.reseau.myAddress}
          </div>
          {status !== null && status.local_addrs.length > 0 ? (
            <ul className="mt-2 space-y-1.5">
              {status.local_addrs.map((addr) => (
                <li key={addr} className="flex items-center justify-between gap-3">
                  <span className="selectable truncate font-mono text-sm text-norm">
                    {addr}
                  </span>
                  <button
                    type="button"
                    onClick={() => copier(addr)}
                    className="shrink-0 rounded-md bg-blurple px-3 py-1 text-xs font-medium text-white transition-colors duration-fast hover:bg-blurple-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar"
                  >
                    {copiee === addr ? t.app.copied : t.reseau.copy}
                  </button>
                </li>
              ))}
            </ul>
          ) : (
            <p className="mt-2 text-sm text-muted">{t.reseau.noAddress}</p>
          )}
          <div className="mt-4 grid grid-cols-3 gap-3 text-center">
            <Stat label={t.reseau.port} value={status?.p2p_port ?? '—'} />
            <Stat
              label={t.reseau.connectedPeers}
              value={status?.connected_peers ?? '—'}
            />
            <Stat label={t.reseau.dhtNodes} value={status?.dht_nodes ?? '—'} />
          </div>
        </div>
      </SettingsSection>

      <SettingsSection title={t.reseau.addPeer}>
        <div className="flex gap-2 rounded-lg bg-sidebar p-3">
          <input
            type="text"
            value={saisie}
            onChange={(e) => {
              setSaisie(e.target.value);
              setAjoutErreur(false);
            }}
            onKeyDown={(e) => {
              if (e.key === 'Enter') ajouter();
            }}
            placeholder={t.reseau.addPeerPlaceholder}
            aria-label={t.reseau.addPeer}
            className="min-w-0 flex-1 rounded-md border border-transparent bg-input px-3 py-2 font-mono text-sm text-norm placeholder-faint outline-none transition-colors duration-fast focus:border-blurple/50"
          />
          <button
            type="button"
            onClick={ajouter}
            disabled={enCours || saisie.trim() === ''}
            className="shrink-0 rounded-lg bg-blurple px-4 py-2 text-sm font-medium text-white transition-colors duration-fast hover:bg-blurple-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar disabled:opacity-50"
          >
            {t.reseau.addPeerButton}
          </button>
        </div>
        {ajoutErreur && <p className="mt-2 text-sm text-red">{t.reseau.addPeerError}</p>}

        {status !== null && status.bootstrap.length > 0 && (
          <ul className="mt-4 space-y-1.5">
            {status.bootstrap.map((addr) => (
              <li
                key={addr}
                className="flex items-center justify-between gap-3 rounded-lg bg-sidebar px-4 py-2"
              >
                <span className="selectable truncate font-mono text-sm text-norm">
                  {addr}
                </span>
                <button
                  type="button"
                  onClick={() => retirer(addr)}
                  className="shrink-0 rounded-md px-2 py-1 text-xs font-medium text-red transition-colors duration-fast hover:bg-red hover:text-on-red focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar"
                >
                  {t.reseau.remove}
                </button>
              </li>
            ))}
          </ul>
        )}
      </SettingsSection>

      <SettingsSection title={t.reseau.autoTitle} hint={t.reseau.autoHint}>
        <div className="rounded-lg bg-sidebar p-4">
          {/* État d'ouverture du port : ligne verte quand un mapping
              automatique est actif, ligne discrète sinon (ou tant qu'on n'a
              pas encore récupéré le statut). */}
          {portMapping === 'upnp' || portMapping === 'natpmp' ? (
            <p className="text-sm text-green">
              {portMapping === 'upnp' ? t.reseau.portOpenUpnp : t.reseau.portOpenNatpmp}
            </p>
          ) : (
            <p className="text-sm text-muted">{t.reseau.portClosed}</p>
          )}

          {/* Adresse externe ouverte par le mapping de port, si disponible. */}
          {externalAddr !== null && (
            <div className="mt-4">
              <div className="text-xs font-medium uppercase text-faint">
                {t.reseau.externalAddr}
              </div>
              <div className="mt-2 flex items-center justify-between gap-3">
                <span className="selectable truncate font-mono text-sm text-norm">
                  {externalAddr}
                </span>
                <button
                  type="button"
                  onClick={() => copier(externalAddr)}
                  className="shrink-0 rounded-md bg-blurple px-3 py-1 text-xs font-medium text-white transition-colors duration-fast hover:bg-blurple-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar"
                >
                  {copiee === externalAddr ? t.app.copied : t.reseau.copy}
                </button>
              </div>
            </div>
          )}

          {/* Nombre de pairs Accord découverts sur le réseau local (mDNS). */}
          <div className="mt-4 flex items-center justify-between">
            <span className="text-xs font-medium uppercase text-faint">
              {t.reseau.lanPeers}
            </span>
            <span className="text-lg font-medium text-header">
              {status?.lan_peers ?? '—'}
            </span>
          </div>
        </div>
      </SettingsSection>

      <SettingsSection title={t.reseau.natTitle}>
        <p className="rounded-lg bg-sidebar p-4 text-sm leading-relaxed text-muted">
          {t.reseau.natHint}
        </p>
      </SettingsSection>
    </div>
  );
}

function Stat({ label, value }: { label: string; value: number | string }) {
  return (
    <div className="rounded-lg bg-rail py-2">
      <div className="text-lg font-medium text-header">{value}</div>
      <div className="mt-0.5 text-xs text-faint">{label}</div>
    </div>
  );
}
