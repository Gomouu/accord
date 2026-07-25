/**
 * Onglet Accessibilité : réduction des animations (tri-état), saturation
 * globale des couleurs et taille de police — chaque réglage s'applique
 * immédiatement à la racine du document et est persisté par le store
 * d'interface (voir `stores/ui.ts` : `applyReducedMotion`/`applySaturation`).
 */

import {
  FONT_SCALES,
  SATURATION_MAX,
  SATURATION_MIN,
  useUi,
  useSettingsT,
  type ReducedMotionPref,
} from '../../stores/ui';
import { OptionPill, SettingsSection } from './controls';

export function AccessibilityTab() {
  const ts = useSettingsT();
  const reducedMotion = useUi((s) => s.reducedMotion);
  const setReducedMotion = useUi((s) => s.setReducedMotion);
  const saturation = useUi((s) => s.saturation);
  const setSaturation = useUi((s) => s.setSaturation);
  const fontScale = useUi((s) => s.fontScale);
  const setFontScale = useUi((s) => s.setFontScale);

  const motionOptions: { id: ReducedMotionPref; label: string }[] = [
    { id: 'system', label: ts.settings.reducedMotionSystem },
    { id: 'on', label: ts.settings.reducedMotionOn },
    { id: 'off', label: ts.settings.reducedMotionOff },
  ];

  return (
    <div>
      <SettingsSection
        title={ts.settings.reducedMotionTitle}
        hint={ts.settings.reducedMotionHint}
      >
        <div className="flex flex-wrap gap-2">
          {motionOptions.map(({ id, label }) => (
            <OptionPill
              key={id}
              selected={reducedMotion === id}
              onSelect={() => setReducedMotion(id)}
            >
              {label}
            </OptionPill>
          ))}
        </div>
      </SettingsSection>

      <SettingsSection
        title={ts.settings.saturationTitle}
        hint={ts.settings.saturationHint}
      >
        <div className="flex items-center gap-4 rounded-lg bg-sidebar px-4 py-3">
          <input
            type="range"
            min={SATURATION_MIN}
            max={SATURATION_MAX}
            step={5}
            value={saturation}
            aria-label={ts.settings.saturationSliderLabel}
            onChange={(e) => setSaturation(Number(e.target.value))}
            className="h-5 w-full rounded-full accent-blurple outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar"
          />
          <span className="w-12 shrink-0 text-end text-sm tabular-nums text-norm">
            {saturation}%
          </span>
        </div>
      </SettingsSection>

      <SettingsSection title={ts.settings.fontSize}>
        <div className="flex flex-wrap gap-2">
          {FONT_SCALES.map((scale) => (
            <OptionPill
              key={scale}
              selected={fontScale === scale}
              onSelect={() => setFontScale(scale)}
            >
              {scale} %
            </OptionPill>
          ))}
        </div>
      </SettingsSection>
    </div>
  );
}
