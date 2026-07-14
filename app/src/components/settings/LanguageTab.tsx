/**
 * Onglet Langue et heure : sélecteur FR/EN (application immédiate,
 * persistée) et format des heures affichées (horodatages de messages —
 * `lib/format.ts`).
 */

import type { Lang } from '../../i18n';
import { useUi, useT, type TimeFormat } from '../../stores/ui';
import { OptionPill, SettingsSection } from './controls';

export function LanguageTab() {
  const t = useT();
  const lang = useUi((s) => s.lang);
  const setLang = useUi((s) => s.setLang);
  const timeFormat = useUi((s) => s.timeFormat);
  const setTimeFormat = useUi((s) => s.setTimeFormat);

  const langs: { id: Lang; label: string }[] = [
    { id: 'fr', label: t.settings.french },
    { id: 'en', label: t.settings.english },
  ];

  const timeFormats: { id: TimeFormat; label: string }[] = [
    { id: 'auto', label: t.settings.timeFormatAuto },
    { id: '12h', label: t.settings.timeFormat12 },
    { id: '24h', label: t.settings.timeFormat24 },
  ];

  return (
    <div>
      <SettingsSection title={t.settings.language} hint={t.settings.languageHint}>
        <div className="flex flex-wrap gap-2">
          {langs.map(({ id, label }) => (
            <OptionPill key={id} selected={lang === id} onSelect={() => setLang(id)}>
              {label}
            </OptionPill>
          ))}
        </div>
      </SettingsSection>

      <SettingsSection
        title={t.settings.timeFormatTitle}
        hint={t.settings.timeFormatHint}
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
