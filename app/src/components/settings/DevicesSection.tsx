/**
 * Section « Mes appareils » (multi-appareil, jalon 1).
 *
 * La liste des appareils du compte, et les deux bouts de l'appairage qui
 * l'alimentent. Chaque machine SŒUR porte en plus la récupération d'historique
 * (§17.4) ; la révocation viendra avec son propre lot.
 */

import { useEffect, useState } from 'react';
import { api } from '../../lib/client';
import { type AccountDevice } from '../../lib/api';
import { interpolate } from '../../i18n';
import { formatEventDateTime } from '../../lib/format';
import { useSettingsT, useT, useUi } from '../../stores/ui';
import { SettingsSection } from './controls';
import { HistoryTransferButton } from './HistoryTransferButton';
import { JoinDeviceForm } from './JoinDeviceForm';
import { PairDeviceButton } from './PairDeviceButton';

/**
 * Longueur maximale d'un nom d'appareil, **en octets UTF-8**.
 *
 * 🔒 C'est la borne du fil, et compter les caractères serait plus laxiste :
 * « é » pèse deux octets, donc 32 caractères accentués seraient acceptés ici
 * et refusés par le nœud — un réglage qui a l'air pris et ne l'est pas.
 */
const MAX_NAME_BYTES = 32;

/** Poids d'une chaîne une fois encodée en UTF-8. */
function octets(value: string): number {
  return new TextEncoder().encode(value).length;
}

export function DevicesSection() {
  const t = useT();
  const ts = useSettingsT();
  const lang = useUi((s) => s.lang);
  const timeFormat = useUi((s) => s.timeFormat);
  const toast = useUi((s) => s.toast);
  const [devices, setDevices] = useState<AccountDevice[] | null>(null);
  const [draft, setDraft] = useState('');
  const [saving, setSaving] = useState(false);
  /**
   * Appareil dont l'historique est en cours de récupération, ou `null`.
   *
   * ⚠️ Un seul transfert à la fois, et c'est cet état qui le tient :
   * `event.history_transfer` ne nomme PAS l'appareil source, donc deux
   * transferts menés ensemble mélangeraient leurs avancements dans les deux
   * barres — chacune racontant la somme des deux.
   */
  const [transfert, setTransfert] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    void api
      .devicesList()
      .then((r: { devices: AccountDevice[] }) => {
        if (cancelled) return;
        setDevices(r.devices);
        setDraft(r.devices.find((d: AccountDevice) => d.is_current)?.name ?? '');
      })
      .catch(() => {
        // Un profil ouvert hors du chemin de démarrage normal n'a pas encore
        // d'appareil : liste vide plutôt qu'une erreur qui n'apprend rien.
        if (!cancelled) setDevices([]);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  /**
   * L'histoire d'un appareil en une ligne : quand il est entré dans le compte,
   * et quand cette machine l'a joint pour la dernière fois — par quel chemin.
   *
   * 🔒 « D'où » se dit ici par la ROUTE (directe ou par relais), jamais par une
   * adresse : c'est l'information utile (un appareil qu'on ne joint que par
   * relais est un appareil dont la connexion directe ne passe pas) et c'est la
   * seule qui puisse s'afficher sans mettre un lieu dans la première capture
   * d'écran venue.
   */
  const historique = (d: AccountDevice): string => {
    const ajout =
      d.added_ms > 0
        ? interpolate(ts.settings.deviceAdded, {
            date: formatEventDateTime(d.added_ms, lang, timeFormat),
          })
        : // Zéro : l'appareil vient de la migration et n'a pas de date d'ajout.
          ts.settings.deviceAddedUnknown;
    // La machine qu'on a sous les yeux ne se raconte pas sa propre visite :
    // « vu il y a deux secondes » n'apprend rien et ferait passer une évidence
    // pour un fait de réseau.
    if (d.is_current) return `${ajout} · ${ts.settings.deviceLastSeenHere}`;
    if (d.last_seen_ms === null || d.last_seen_route === null) {
      return `${ajout} · ${ts.settings.deviceLastSeenNever}`;
    }
    const vu = interpolate(ts.settings.deviceLastSeen, {
      date: formatEventDateTime(d.last_seen_ms, lang, timeFormat),
    });
    const route =
      d.last_seen_route === 'relay'
        ? ts.settings.deviceRouteRelay
        : ts.settings.deviceRouteDirect;
    return `${ajout} · ${vu} (${route})`;
  };

  const current = devices?.find((d) => d.is_current) ?? null;
  const trimmed = draft.trim();
  const dirty = current !== null && trimmed !== current.name;
  const valid = trimmed.length > 0 && octets(trimmed) <= MAX_NAME_BYTES;

  const save = async () => {
    if (!dirty || !valid || saving) return;
    setSaving(true);
    try {
      const { name } = await api.devicesRename(trimmed);
      setDevices((list) => (list ?? []).map((d) => (d.is_current ? { ...d, name } : d)));
      toast('success', ts.settings.deviceRenamed);
    } catch {
      toast('error', t.errors.actionFailed);
    } finally {
      setSaving(false);
    }
  };

  return (
    <SettingsSection
      title={ts.settings.devicesListTitle}
      hint={ts.settings.devicesListHint}
    >
      {devices === null ? (
        <p className="text-sm text-muted">{t.app.loading}</p>
      ) : devices.length === 0 ? (
        <p className="text-sm text-muted">{ts.settings.devicesEmpty}</p>
      ) : (
        <ul className="flex flex-col gap-2">
          {devices.map((d) => (
            <li key={d.pubkey} className="rounded-lg bg-sidebar px-4 py-3">
              <div className="flex items-center gap-3">
                <div className="min-w-0 flex-1">
                  <div className="truncate text-sm font-medium">{d.name}</div>
                  <div className="selectable truncate font-mono text-xs text-muted">
                    {d.pubkey.slice(0, 16)}…
                  </div>
                  <div className="mt-0.5 text-xs text-muted">{historique(d)}</div>
                </div>
                {d.is_current && (
                  <span className="shrink-0 rounded-full bg-blurple/15 px-2 py-0.5 text-xs font-medium text-blurple">
                    {ts.settings.deviceCurrent}
                  </span>
                )}
              </div>
              {/* Jamais sur la machine qu'on tient : se demander son propre
                  historique n'irait chercher nulle part. */}
              {!d.is_current && (
                <HistoryTransferButton
                  pubkey={d.pubkey}
                  bloque={transfert !== null && transfert !== d.pubkey}
                  onActif={(actif) => setTransfert(actif ? d.pubkey : null)}
                />
              )}
            </li>
          ))}
        </ul>
      )}

      {/* Les deux côtés de l'appairage, côte à côte et sans choix préalable :
          une machine ne sait pas d'avance si elle affichera le code ou le
          recopiera, et demander « autorise ou rejoins ? » avant d'avoir rien
          montré ne ferait qu'ajouter une question à la place d'une réponse.
          C'est l'appareil qui a déjà le compte qui ouvre l'offre. */}
      <PairDeviceButton />
      <JoinDeviceForm />

      {current !== null && (
        <div className="mt-4 flex flex-wrap items-center gap-2">
          <input
            type="text"
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') void save();
            }}
            aria-label={ts.settings.deviceNameLabel}
            placeholder={ts.settings.deviceNameLabel}
            className="min-w-0 flex-1 rounded-md bg-chat px-3 py-2 text-sm outline-none ring-blurple focus-visible:ring-2"
          />
          <button
            type="button"
            onClick={() => void save()}
            disabled={!dirty || !valid || saving}
            className="shrink-0 rounded-md bg-blurple px-3 py-2 text-sm font-medium text-white transition-colors hover:bg-blurple-hover disabled:opacity-50"
          >
            {ts.settings.pseudonymSave}
          </button>
        </div>
      )}
    </SettingsSection>
  );
}
