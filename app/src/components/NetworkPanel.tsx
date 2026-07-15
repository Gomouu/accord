/**
 * Panneau réseau : adresse à communiquer (distinguant PUBLIQUE, joignable
 * depuis Internet, et LOCALE, même réseau seulement), compteurs de connexions
 * (rafraîchis par `event.network`), ajout/retrait d'un pair d'amorçage par
 * `ip:port`, et état de la connexion automatique (UPnP / mDNS).
 *
 * Consolidé DANS l'onglet « Ajouter un ami » (et non plus dans les réglages) :
 * tout ce qui concerne « se connecter à un ami » vit au même endroit.
 */

import { useCallback, useEffect, useMemo, useState } from 'react';
import type { NetworkStatus } from '../lib/api';
import { api, rpc } from '../lib/client';
import { useT } from '../stores/ui';
import { SettingsSection } from './settings/controls';

const COPY_FEEDBACK_MS = 1500;

/** Extrait l'hôte (IP) d'une adresse `ip:port` ou `[ipv6]:port`. */
function hostOf(addr: string): string {
  if (addr.startsWith('[')) {
    const end = addr.indexOf(']');
    return end > 0 ? addr.slice(1, end) : addr;
  }
  const colon = addr.lastIndexOf(':');
  return colon > 0 ? addr.slice(0, colon) : addr;
}

/**
 * Vrai si l'adresse n'est joignable que sur le RÉSEAU LOCAL (privée/RFC1918,
 * CGNAT, lien-local, loopback, ULA IPv6…) — inutile à communiquer à un ami sur
 * Internet. Toute autre adresse (IPv4 publique, IPv6 globale `2000::/3`) est
 * considérée joignable depuis Internet.
 */
export function isLocalAddr(addr: string): boolean {
  const host = hostOf(addr).toLowerCase();
  if (host.includes(':')) {
    // IPv6 : loopback, lien-local (fe80::/10), ULA (fc00::/7).
    if (host === '::1') return true;
    if (
      host.startsWith('fe8') ||
      host.startsWith('fe9') ||
      host.startsWith('fea') ||
      host.startsWith('feb')
    ) {
      return true;
    }
    if (host.startsWith('fc') || host.startsWith('fd')) return true;
    return false; // IPv6 globale (2000::/3, etc.) : joignable.
  }
  const o = host.split('.').map((n) => Number.parseInt(n, 10));
  const [a, b] = o;
  if (
    a === undefined ||
    b === undefined ||
    o.length !== 4 ||
    o.some((n) => Number.isNaN(n))
  ) {
    return false;
  }
  if (a === 10 || a === 127) return true; // privé 10/8, loopback
  if (a === 192 && b === 168) return true; // privé 192.168/16
  if (a === 172 && b >= 16 && b <= 31) return true; // privé 172.16/12
  if (a === 169 && b === 254) return true; // lien-local 169.254/16
  if (a === 100 && b >= 64 && b <= 127) return true; // CGNAT 100.64/10
  return false;
}

function CopyRow({
  addr,
  copied,
  onCopy,
  copyLabel,
  copiedLabel,
}: {
  addr: string;
  copied: boolean;
  onCopy: (v: string) => void;
  copyLabel: string;
  copiedLabel: string;
}) {
  return (
    <li className="flex items-center justify-between gap-3">
      <span className="selectable truncate font-mono text-sm text-norm">{addr}</span>
      <button
        type="button"
        onClick={() => onCopy(addr)}
        className="shrink-0 rounded-md bg-blurple px-3 py-1 text-xs font-medium text-white transition-colors duration-fast hover:bg-blurple-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar"
      >
        {copied ? copiedLabel : copyLabel}
      </button>
    </li>
  );
}

export function NetworkPanel() {
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

  // Sépare les adresses joignables depuis Internet des adresses purement
  // locales : ne pas les distinguer était le piège (un ami sur Internet reçoit
  // une IP `192.168.x` inutilisable). Les IPv6 globales (2000::/3) sont
  // publiques et souvent joignables SANS redirection de port.
  const { publiques, locales } = useMemo(() => {
    const all = status?.local_addrs ?? [];
    return {
      publiques: all.filter((a) => !isLocalAddr(a)),
      locales: all.filter((a) => isLocalAddr(a)),
    };
  }, [status?.local_addrs]);

  const portMapping = status?.port_mapping ?? 'aucun';
  const externalAddr = status?.external_addr ?? null;

  return (
    <div>
      <SettingsSection title={t.reseau.myAddress} hint={t.reseau.intro}>
        {erreur !== null && <p className="mb-3 text-sm text-red">{erreur}</p>}
        <div className="rounded-lg bg-sidebar p-4">
          {/* Adresses PUBLIQUES : joignables depuis Internet. */}
          <div className="text-xs font-medium uppercase text-faint">
            {t.reseau.publicAddress}
          </div>
          {publiques.length > 0 ? (
            <ul className="mt-2 space-y-1.5">
              {publiques.map((addr) => (
                <CopyRow
                  key={addr}
                  addr={addr}
                  copied={copiee === addr}
                  onCopy={copier}
                  copyLabel={t.reseau.copy}
                  copiedLabel={t.app.copied}
                />
              ))}
            </ul>
          ) : (
            <p className="mt-2 text-sm text-muted">{t.reseau.noPublicAddress}</p>
          )}

          {/* Adresses LOCALES : même réseau (Wi-Fi) seulement. */}
          {locales.length > 0 && (
            <>
              <div className="mt-4 text-xs font-medium uppercase text-faint">
                {t.reseau.localAddress}
              </div>
              <ul className="mt-2 space-y-1.5">
                {locales.map((addr) => (
                  <CopyRow
                    key={addr}
                    addr={addr}
                    copied={copiee === addr}
                    onCopy={copier}
                    copyLabel={t.reseau.copy}
                    copiedLabel={t.app.copied}
                  />
                ))}
              </ul>
            </>
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

      <SettingsSection title={t.reseau.addPeer} hint={t.reseau.addPeerHint}>
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
          {portMapping === 'upnp' || portMapping === 'natpmp' ? (
            <p className="text-sm text-green">
              {portMapping === 'upnp' ? t.reseau.portOpenUpnp : t.reseau.portOpenNatpmp}
            </p>
          ) : (
            <p className="text-sm text-muted">{t.reseau.portClosed}</p>
          )}

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
