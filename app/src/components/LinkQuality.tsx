/**
 * Pastille de qualité du lien vers un ami.
 *
 * Trois états seulement — direct, relayé, hors ligne — parce que c'est la
 * granularité sur laquelle un utilisateur peut agir. La latence exacte reste
 * dans l'infobulle : utile quand on cherche pourquoi un appel grésille,
 * bruyante en permanence.
 */

import { qualityOf, rttOf, usePeerLinks, type LinkQuality } from '../stores/peerLinks';
import { useT } from '../stores/ui';

const TEINTE: Record<LinkQuality, string> = {
  direct: 'bg-green',
  relay: 'bg-yellow',
  offline: 'bg-faint/50',
};

export function LinkQualityDot({
  pubkey,
  className = '',
}: {
  pubkey: string;
  className?: string;
}) {
  const t = useT();
  const links = usePeerLinks((s) => s.links);
  const quality = qualityOf(links, pubkey);
  const rtt = rttOf(links, pubkey);

  // Rien à dire tant que le diagnostic n'a rien remonté : une pastille grise
  // affirmerait « hors ligne » sur la foi d'un état non encore chargé.
  if (Object.keys(links).length === 0) return null;

  const libelle = t.linkQuality[quality];
  const infobulle =
    rtt === null
      ? libelle
      : `${libelle} · ${t.linkQuality.latency.replace('{ms}', String(rtt))}`;

  return (
    <span
      title={infobulle}
      aria-label={infobulle}
      role="img"
      className={`inline-flex h-2 w-2 shrink-0 rounded-full ${TEINTE[quality]} ${className}`}
    />
  );
}
