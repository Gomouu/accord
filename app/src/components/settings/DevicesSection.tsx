/**
 * Section « Mes appareils » (multi-appareil, jalon 1).
 *
 * Un seul appareil aujourd'hui — celui de cette machine. L'appairage en
 * ajoutera d'autres sans que cette liste change de forme, et c'est lui qui
 * apportera la révocation ; renommer est donc la seule action pour l'instant.
 */

import { useEffect, useState } from 'react';
import { api } from '../../lib/client';
import { type AccountDevice } from '../../lib/api';
import { useSettingsT, useT, useUi } from '../../stores/ui';
import { SettingsSection } from './controls';
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

/**
 * L'appairage est-il utilisable de bout en bout ?
 *
 * `false` tant que l'adoption du compte n'est pas câblée côté hôte : sans elle,
 * la machine qui rejoint termine le protocole et reste son propre compte.
 */
const APPAIRAGE_UTILISABLE = false;

export function DevicesSection() {
  const t = useT();
  const ts = useSettingsT();
  const toast = useUi((s) => s.toast);
  const [devices, setDevices] = useState<AccountDevice[] | null>(null);
  const [draft, setDraft] = useState('');
  const [saving, setSaving] = useState(false);

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
            <li
              key={d.pubkey}
              className="flex items-center gap-3 rounded-lg bg-sidebar px-4 py-3"
            >
              <div className="min-w-0 flex-1">
                <div className="truncate text-sm font-medium">{d.name}</div>
                <div className="selectable truncate font-mono text-xs text-muted">
                  {d.pubkey.slice(0, 16)}…
                </div>
              </div>
              {d.is_current && (
                <span className="shrink-0 rounded-full bg-blurple/15 px-2 py-0.5 text-xs font-medium text-blurple">
                  {ts.settings.deviceCurrent}
                </span>
              )}
            </li>
          ))}
        </ul>
      )}

      {/* 🔒 Les deux côtés de l'appairage sont écrits, testés, et volontairement
          PAS montrés. Le nœud sait tout faire jusqu'à remettre la racine du
          compte à la machine qui rejoint ; ce qui manque est l'adoption
          elle-même — rouvrir le coffre demande la phrase de passe que le nœud
          ne détient pas, et la clé de base dérive de la graine, donc la base
          ouverte ne peut pas être réutilisée. Il faut une commande hôte puis un
          redémarrage du nœud.

          Les montrer maintenant afficherait « appareil appairé » à quelqu'un
          dont la machine est restée son propre compte : un message de succès
          pour quelque chose qui n'a pas eu lieu, ce qui est pire que l'absence
          du bouton. Rien ne régresse — cet écran n'a jamais été publié.

          À rétablir avec le câblage hôte (`identity::adopt_account_seed`,
          `Node::pairing_take_adoption`). */}
      {APPAIRAGE_UTILISABLE && (
        <>
          <PairDeviceButton />
          <JoinDeviceForm />
        </>
      )}

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
