import type { DiagnosticsCounters } from './api';

/**
 * Surfaces « Sécurité » et « Diagnostic » de l'API locale : deux familles de
 * types purement descriptifs, sans logique.
 *
 * Extraite de `api.ts` : ce fichier dépasse 2 700 lignes pour une limite de
 * 800, et le cliquet refuse — à raison — qu'une nouvelle fonctionnalité
 * l'alourdisse encore.
 */

export interface SecurityState {
  /** Ce nœud sait-il négocier le chiffrement hybride post-quantique ? */
  hybrid_supported: boolean;
  /** Refuse-t-il les sessions classiques (réglage avancé) ? */
  require_hybrid: boolean;
  /** Sessions hybrides établies depuis le démarrage. */
  hybrid_sessions: number;
  /** Sessions classiques établies depuis le démarrage. */
  classic_sessions: number;
}

/** Résultat de l'auto-test réseau borné (voir `diagnostics.selftest`). */
export interface DiagnosticsSelftest {
  p2p_port: number;
  nat_kind: 'unknown' | 'cone' | 'symmetric';
  port_mapping: 'upnp' | 'natpmp' | 'aucun';
  external_addr: string | null;
  observed_consensus: string | null;
  dht_nodes: number;
  connected_peers: number;
  relay_eligible: boolean;
  bootstrap: { addr: string; ok: boolean }[];
  relay_probe: { addr: string; ok: boolean } | null;
  /** Verdict de joignabilité : `'direct'`, `'punch'`, `'relay'` ou `'unknown'`. */
  reachability: 'direct' | 'punch' | 'relay' | 'unknown';
}

/**
 * Rapport de diagnostic caviardé (`diagnostics.report`), à joindre à un
 * rapport de bug.
 *
 * 🔒 Noter ce que ce type N'A PAS, par rapport à `PeerLink` : ni `pubkey`
 * (la clé publique d'un ami est son code ami), ni `addr` (son adresse IP).
 * Ces deux champs sont retirés par le nœud, parce que ce rapport est fait
 * pour être envoyé à quelqu'un d'autre. Les rajouter ici ne les ferait pas
 * apparaître — mais ce serait le signe qu'on est en train de reconstruire le
 * rapport du mauvais côté de la frontière.
 */
export interface DiagnosticsReport {
  /** Version de l'application qui a produit le rapport. */
  version: string;
  /** Système et architecture, par exemple `macos/aarch64`. */
  platform: string;
  counters: DiagnosticsCounters;
  selftest: Omit<DiagnosticsSelftest, 'external_addr' | 'observed_consensus'> & {
    /** Hôte masqué, port conservé (`masqué:41234`). */
    external_addr: string | null;
    /** Hôte masqué, port conservé. */
    observed_consensus: string | null;
  };
  links: {
    /** Rang dans la liste — le seul identifiant, valable pour ce rapport seul. */
    peer: number;
    live: boolean;
    transport: 'direct' | 'relay' | 'none';
    /** Adresse du relais : de l'infrastructure, pas celle de l'ami. */
    relay: string | null;
    last_recv_age_ms: number | null;
    rtt_ms: number | null;
    capabilities: number;
  }[];
}
