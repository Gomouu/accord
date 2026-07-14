/**
 * Onglet Voix : sélection des périphériques d'entrée/sortie (voice.devices /
 * voice.set_devices), volume de sortie principal (voice.set_volume, persisté),
 * test du micro avec vumètre animé sur `event.voice_level` (transform scaleX,
 * compositor-friendly) et réglage de l'appui-pour-parler (interrupteur +
 * touche capturée au prochain appui, persistés).
 */

import { useEffect, useState } from 'react';
import type { VoiceDeviceSelection, VoiceDevices } from '../../lib/api';
import { api, rpc } from '../../lib/client';
import { formatKeyLabel } from '../../hooks/usePushToTalk';
import { useUi, useT } from '../../stores/ui';
import { useVoice } from '../../stores/voice';
import { SettingsSection, ToggleRow } from './controls';

/** Niveau micro poussé par le nœud pendant le test (event.voice_level). */
interface MicLevel {
  level: number;
  speaking: boolean;
}

const IDLE_LEVEL: MicLevel = { level: 0, speaking: false };

/** Valide la charge utile d'un `event.voice_level` (frontière système). */
function readMicLevel(params: unknown): MicLevel | null {
  if (typeof params !== 'object' || params === null) return null;
  const p = params as { level?: unknown; speaking?: unknown };
  if (typeof p.level !== 'number' || typeof p.speaking !== 'boolean') return null;
  return { level: p.level, speaking: p.speaking };
}

/**
 * Vumètre : barre remplie par `transform: scaleX` (jamais de largeur animée),
 * surlignée en vert quand le nœud détecte de la parole.
 */
export function MicMeter({ level, speaking }: MicLevel) {
  const t = useT();
  const clamped = Math.max(0, Math.min(1, level));
  return (
    <div
      role="meter"
      aria-label={t.settings.micLevel}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={Math.round(clamped * 100)}
      className="h-2 w-full overflow-hidden rounded-full bg-input"
    >
      <div
        data-testid="mic-meter-bar"
        className={`h-full w-full origin-left rounded-full transition-transform duration-100 ease-out ${
          speaking ? 'bg-green' : 'bg-blurple'
        }`}
        style={{ transform: `scaleX(${clamped})` }}
      />
    </div>
  );
}

/** Sélecteur de périphérique ; liste vide → « Périphérique par défaut » seul. */
function DeviceSelect({
  label,
  options,
  selected,
  onSelect,
}: {
  label: string;
  options: string[];
  selected: string | null;
  onSelect: (name: string | null) => void;
}) {
  const t = useT();
  return (
    <label className="block">
      <span className="mb-1.5 block text-xs font-medium uppercase text-faint">
        {label}
      </span>
      <select
        value={selected ?? ''}
        onChange={(e) => onSelect(e.target.value === '' ? null : e.target.value)}
        className="w-full rounded-md bg-input px-3 py-2 text-sm text-norm placeholder-faint outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar"
      >
        <option value="">{t.settings.defaultDevice}</option>
        {options.map((name) => (
          <option key={name} value={name}>
            {name}
          </option>
        ))}
      </select>
    </label>
  );
}

export function VoiceTab() {
  const t = useT();
  const toast = useUi((s) => s.toast);
  const pttEnabled = useUi((s) => s.pttEnabled);
  const pttKey = useUi((s) => s.pttKey);
  const setPttEnabled = useUi((s) => s.setPttEnabled);
  const setPttKey = useUi((s) => s.setPttKey);
  const masterVolume = useVoice((s) => s.masterVolume);
  const setVolume = useVoice((s) => s.setVolume);
  const dsp = useVoice((s) => s.dsp);
  const setNoiseSuppression = useVoice((s) => s.setNoiseSuppression);
  const setAgc = useVoice((s) => s.setAgc);

  const [devices, setDevices] = useState<VoiceDevices | null>(null);
  const [testing, setTesting] = useState(false);
  const [mic, setMic] = useState<MicLevel>(IDLE_LEVEL);
  const [capturing, setCapturing] = useState(false);

  // Périphériques : repli silencieux (sélecteurs par défaut) si indisponible.
  useEffect(() => {
    let cancelled = false;
    api
      .voiceDevices()
      .then((d) => {
        if (!cancelled) setDevices(d);
      })
      .catch(() => {
        // Nœud indisponible ou mode simulé sans réponse : défauts seuls.
      });
    return () => {
      cancelled = true;
    };
  }, []);

  // Volume de sortie principal persisté : rechargé à l'ouverture de l'onglet.
  useEffect(() => {
    useVoice
      .getState()
      .loadMasterVolume()
      .catch(() => {
        // Nœud indisponible : la valeur du store fait foi.
      });
  }, []);

  // Test du micro : abonnement aux niveaux, arrêt propre à la fermeture.
  useEffect(() => {
    if (!testing) return;
    const off = rpc.onEvent((method, params) => {
      if (method !== 'event.voice_level') return;
      const next = readMicLevel(params);
      if (next !== null) setMic(next);
    });
    return () => {
      off();
      api.voiceMicTest(false).catch(() => {
        // La capture s'arrête d'elle-même si la connexion se ferme.
      });
    };
  }, [testing]);

  // Capture de la prochaine touche pour l'appui-pour-parler (Échap annule).
  useEffect(() => {
    if (!capturing) return;
    const onKey = (e: KeyboardEvent): void => {
      e.preventDefault();
      e.stopPropagation();
      if (e.key !== 'Escape') setPttKey(e.code);
      setCapturing(false);
    };
    window.addEventListener('keydown', onKey, { capture: true });
    return () => window.removeEventListener('keydown', onKey, { capture: true });
  }, [capturing, setPttKey]);

  const applyDevices = (selection: VoiceDeviceSelection): void => {
    api
      .voiceSetDevices(selection)
      .then(() => {
        setDevices((d) => {
          if (d === null) return d;
          return {
            ...d,
            selected_input:
              selection.input !== undefined ? selection.input : d.selected_input,
            selected_output:
              selection.output !== undefined ? selection.output : d.selected_output,
          };
        });
      })
      .catch(() => toast('error', t.errors.actionFailed));
  };

  const startTest = (): void => {
    api
      .voiceMicTest(true)
      .then(() => setTesting(true))
      .catch((e: unknown) => {
        const message = e instanceof Error && e.message !== '' ? e.message : null;
        toast('error', message ?? t.errors.actionFailed);
      });
  };

  const stopTest = (): void => {
    setTesting(false);
    setMic(IDLE_LEVEL);
  };

  const onMasterVolume = (value: number): void => {
    setVolume(null, value).catch(() => toast('error', t.errors.actionFailed));
  };

  return (
    <div>
      <SettingsSection title={t.settings.devicesTitle}>
        <div className="space-y-3 rounded-lg bg-sidebar p-4">
          <DeviceSelect
            label={t.settings.inputDevice}
            options={devices?.inputs ?? []}
            selected={devices?.selected_input ?? null}
            onSelect={(input) => applyDevices({ input })}
          />
          <DeviceSelect
            label={t.settings.outputDevice}
            options={devices?.outputs ?? []}
            selected={devices?.selected_output ?? null}
            onSelect={(output) => applyDevices({ output })}
          />
        </div>
      </SettingsSection>

      <SettingsSection
        title={t.settings.outputVolumeTitle}
        hint={t.settings.outputVolumeHint}
      >
        <div className="flex items-center gap-4 rounded-lg bg-sidebar px-4 py-3">
          <input
            type="range"
            min={0}
            max={200}
            step={1}
            value={masterVolume}
            aria-label={t.settings.outputVolumeLabel}
            onChange={(e) => onMasterVolume(Number(e.target.value))}
            className="h-1 w-full rounded-full accent-blurple outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar"
          />
          <span className="w-12 shrink-0 text-right text-sm tabular-nums text-norm">
            {masterVolume}%
          </span>
        </div>
      </SettingsSection>

      <SettingsSection title={t.settings.micTestTitle} hint={t.settings.micTestHint}>
        <div className="flex items-center gap-4 rounded-lg bg-sidebar px-4 py-3">
          <button
            type="button"
            aria-pressed={testing}
            onClick={testing ? stopTest : startTest}
            className={`shrink-0 rounded-lg px-4 py-2 text-sm font-medium transition-colors duration-fast focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar ${
              testing
                ? 'bg-red text-on-red hover:brightness-110 focus-visible:ring-red'
                : 'bg-blurple text-white hover:bg-blurple-hover focus-visible:ring-blurple'
            }`}
          >
            {testing ? t.settings.micTestStop : t.settings.micTestStart}
          </button>
          <MicMeter level={mic.level} speaking={mic.speaking} />
        </div>
      </SettingsSection>

      <SettingsSection title={t.settings.dspTitle} hint={t.settings.dspHint}>
        <ToggleRow
          label={t.settings.noiseSuppression}
          hint={t.settings.noiseSuppressionHint}
          checked={dsp.noiseSuppression}
          onChange={(enabled) => {
            setNoiseSuppression(enabled).catch(() => toast('error', t.errors.actionFailed));
          }}
        />
        <ToggleRow
          label={t.settings.agc}
          hint={t.settings.agcHint}
          checked={dsp.agc}
          onChange={(enabled) => {
            setAgc(enabled).catch(() => toast('error', t.errors.actionFailed));
          }}
        />
      </SettingsSection>

      <SettingsSection title={t.settings.pttTitle}>
        <ToggleRow
          label={t.settings.pttEnable}
          hint={t.settings.pttEnableHint}
          checked={pttEnabled}
          onChange={setPttEnabled}
        />
        <div className="flex items-center justify-between gap-4 rounded-lg bg-sidebar px-4 py-3">
          <span className="text-sm font-medium text-header">{t.settings.pttKey}</span>
          <button
            type="button"
            aria-label={t.settings.pttKey}
            onClick={() => setCapturing(true)}
            className={`min-w-[96px] rounded-md px-3 py-1.5 text-center text-sm font-medium transition-colors duration-150 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar ${
              capturing
                ? 'bg-blurple text-white'
                : 'bg-rail font-mono text-norm hover:bg-input hover:text-header'
            }`}
          >
            {capturing ? t.settings.pttPressKey : formatKeyLabel(pttKey)}
          </button>
        </div>
      </SettingsSection>
    </div>
  );
}
