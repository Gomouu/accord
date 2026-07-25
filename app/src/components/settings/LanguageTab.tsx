/**
 * Onglet Langue et heure : sélecteur de langue (application immédiate,
 * persistée) et format des heures affichées (horodatages de messages —
 * `lib/format.ts`).
 */

import { LANGS, type Lang } from '../../i18n';
import { useUi, useSettingsT, type TimeFormat } from '../../stores/ui';
import { OptionPill, SettingsSection } from './controls';

export function LanguageTab() {
  const ts = useSettingsT();
  const lang = useUi((s) => s.lang);
  const setLang = useUi((s) => s.setLang);
  const timeFormat = useUi((s) => s.timeFormat);
  const setTimeFormat = useUi((s) => s.setTimeFormat);

  // Le type `Record<Lang, string>` est ce qui rend l'ajout d'une langue sûr :
  // une langue déclarée dans `LANGS` mais sans nom natif dans `fr.ts` casse la
  // compilation ici, au lieu d'afficher une pastille vide.
  const names: Record<Lang, string> = ts.settings.languageNames;

  const timeFormats: { id: TimeFormat; label: string }[] = [
    { id: 'auto', label: ts.settings.timeFormatAuto },
    { id: '12h', label: ts.settings.timeFormat12 },
    { id: '24h', label: ts.settings.timeFormat24 },
  ];

  return (
    <div>
      <SettingsSection title={ts.settings.language} hint={ts.settings.languageHint}>
        <div className="flex flex-wrap gap-2">
          {LANGS.map((id) => (
            <OptionPill key={id} selected={lang === id} onSelect={() => setLang(id)}>
              {names[id]}
            </OptionPill>
          ))}
        </div>
      </SettingsSection>

      <SettingsSection
        title={ts.settings.timeFormatTitle}
        hint={ts.settings.timeFormatHint}
      >
        <div className="flex flex-wrap gap-2">
          {timeFormats.map(({ id, label }) => (
            <OptionPill
              key={id}
              selected={timeFormat === id}
              onSelect={() => setTimeFormat(id)}
            >
              {label}
            </OptionPill>
          ))}
        </div>
      </SettingsSection>
    </div>
  );
}
