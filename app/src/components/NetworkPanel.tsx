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
import { interpolate } from '../i18n';
import type {
  DiagnosticsCounters,
  DiagnosticsSelftest,
  NetworkStatus,
  PeerLink,
} from '../lib/api';
import { api, rpc } from '../lib/client';
import { displayNameOf, useFriends } from '../stores/friends';
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
  const [peers, setPeers] = useState<PeerLink[]>([]);
  const contacts = useFriends((s) => s.contacts);
  const [erreur, setErreur] = useState<string | null>(null);
  const [saisie, setSaisie] = useState('');
  const [ajoutErreur, setAjoutErreur] = useState(false);
  const [enCours, setEnCours] = useState(false);
  const [copiee, setCopiee] = useState<string | null>(null);
  const [counters, setCounters] = useState<DiagnosticsCounters | null>(null);
  const [selftest, setSelftest] = useState<DiagnosticsSelftest | null>(null);
  const [selftestEnCours, setSelftestEnCours] = useState(false);
  const [selftestErreur, setSelftestErreur] = useState(false);

  const rafraichir = useCallback((): void => {
    api
      .networkStatus()
      .then((s) => {
        setStatus(s);
        setErreur(null);
      })
      .catch(() => setErreur(t.reseau.refreshFailed));
    api
      .networkPeers()
      .then(setPeers)
      .catch(() => {});
    // Compteurs de diagnostic : silencieux si le nœud est antérieur à 4.0.
    api
      .diagnosticsCounters()
      .then(setCounters)
      .catch(() => setCounters(null));
  }, [t.reseau.refreshFailed]);

  const lancerAutotest = (): void => {
    if (selftestEnCours) return;
    setSelftestEnCours(true);
    setSelftestErreur(false);
    api
      .diagnosticsSelftest()
      .then(setSelftest)
      .catch(() => setSelftestErreur(true))
      .finally(() => setSelftestEnCours(false));
  };

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

  // Amis d'abord connectés, puis par nom : la connectivité active en tête.
  const peersTries = useMemo(() => {
    return [...peers].sort((a, b) => {
      if (a.live !== b.live) return a.live ? -1 : 1;
      return displayNameOf(contacts, a.pubkey).localeCompare(
        displayNameOf(contacts, b.pubkey),
      );
    });
  }, [peers, contacts]);

  const portMapping = status?.port_mapping ?? 'aucun';
  const externalAddr = status?.external_addr ?? null;

  const natLabel = (k: NetworkStatus['nat_kind']): string =>
    k === 'cone'
      ? t.reseau.natCone
      : k === 'symmetric'
        ? t.reseau.natSymmetric
        : t.reseau.natUnknown;
  const reachLabel = (r: DiagnosticsSelftest['reachability']): string =>
    r === 'direct'
      ? t.reseau.reachDirect
      : r === 'punch'
        ? t.reseau.reachPunch
        : r === 'relay'
          ? t.reseau.reachRelay
          : t.reseau.reachUnknown;
  /** Une paire « n/m » pour un compteur (réussis/tentés). */
  const paire = (a: number, b: number): string => `${a} / ${b}`;

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

      <SettingsSection title={t.reseau.friendsTitle} hint={t.reseau.friendsHint}>
        {peers.length === 0 ? (
          <p className="rounded-lg bg-sidebar p-4 text-sm text-muted">
            {t.reseau.friendsEmpty}
          </p>
        ) : (
          <ul className="divide-y divide-input overflow-hidden rounded-lg bg-sidebar">
            {peersTries.map((p) => {
              const relaye = p.live && p.transport === 'relay';
              const direct = p.live && p.transport === 'direct';
              const sousLigne = relaye
                ? p.relay !== null && p.relay !== undefined
                  ? interpolate(t.reseau.linkVia, { relay: p.relay })
                  : t.reseau.linkRelay
                : direct && p.addr !== null
                  ? p.addr
                  : p.addr !== null
                    ? `${t.reseau.friendLastAddr} : ${p.addr}`
                    : t.reseau.friendNoAddr;
              return (
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
                      {sousLigne}
                      {p.rtt_ms !== null && p.rtt_ms !== undefined && ` · ${p.rtt_ms} ms`}
                    </div>
                  </div>
                  {direct || relaye ? (
                    <span
                      className={`shrink-0 rounded-full px-2 py-0.5 text-[11px] font-medium ${
                        direct ? 'bg-green/15 text-green' : 'bg-yellow/15 text-yellow'
                      }`}
                    >
                      {direct ? t.reseau.linkDirect : t.reseau.linkRelay}
                    </span>
                  ) : (
                    <span
                      className={`shrink-0 text-xs ${p.live ? 'text-green' : 'text-faint'}`}
                    >
                      {p.live ? t.reseau.friendLive : t.reseau.friendOffline}
                    </span>
                  )}
                </li>
              );
            })}
          </ul>
        )}
      </SettingsSection>

      <SettingsSection title={t.reseau.diagnosticTitle} hint={t.reseau.diagnosticHint}>
        <div className="space-y-3 rounded-lg bg-sidebar p-4">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <div className="text-sm text-muted">
              {t.reseau.natKind} :{' '}
              <span className="font-medium text-norm">{natLabel(status?.nat_kind)}</span>
            </div>
            <button
              type="button"
              onClick={lancerAutotest}
              disabled={selftestEnCours}
              className="shrink-0 rounded-lg bg-blurple px-4 py-2 text-sm font-medium text-white transition-colors duration-fast hover:bg-blurple-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar disabled:opacity-50"
            >
              {selftestEnCours ? t.reseau.selftestRunning : t.reseau.selftestRun}
            </button>
          </div>

          {selftestErreur && (
            <p className="text-sm text-red">{t.reseau.selftestFailed}</p>
          )}

          {selftest !== null && (
            <div className="rounded-md bg-input/60 p-3">
              <div className="text-sm font-medium text-norm">
                {t.reseau.reachability} :{' '}
                <span className="text-green">{reachLabel(selftest.reachability)}</span>
              </div>
              <ul className="mt-1.5 space-y-0.5 font-mono text-xs text-faint">
                {selftest.bootstrap.map((b) => (
                  <li key={b.addr}>
                    {b.addr} {b.ok ? '✓' : '✗'}
                  </li>
                ))}
                {selftest.relay_probe !== null && (
                  <li>
                    {t.reseau.linkRelay} {selftest.relay_probe.addr}{' '}
                    {selftest.relay_probe.ok ? '✓' : '✗'}
                  </li>
                )}
              </ul>
            </div>
          )}

          {counters !== null && (
            <div>
              <div className="mb-2 text-xs font-medium uppercase text-faint">
                {t.reseau.counters}
              </div>
              <div className="grid grid-cols-2 gap-3 text-center">
                <Stat
                  label={t.reseau.countersPunch}
                  value={paire(counters.punch.ok, counters.punch.requested)}
                />
                <Stat
                  label={t.reseau.countersRelay}
                  value={paire(counters.relay.open_ok, counters.relay.open_fail)}
                />
                <Stat
                  label={t.reseau.countersReconnect}
                  value={paire(counters.reconnect.ok, counters.reconnect.attempts)}
                />
                <Stat
                  label={t.reseau.countersMailbox}
                  value={paire(counters.mailbox.deposits, counters.mailbox.pickups)}
                />
              </div>
            </div>
          )}
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
