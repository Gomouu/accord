/**
 * Onglet Langue et heure : sélecteur de langue (application immédiate,
 * persistée) et format des heures affichées (horodatages de messages —
 * `lib/format.ts`).
 */

import { LANGS, type Lang } from '../../i18n';
import { useUi, useT, type TimeFormat } from '../../stores/ui';
import { OptionPill, SettingsSection } from './controls';

export function LanguageTab() {
  const t = useT();
  const lang = useUi((s) => s.lang);
  const setLang = useUi((s) => s.setLang);
  const timeFormat = useUi((s) => s.timeFormat);
  const setTimeFormat = useUi((s) => s.setTimeFormat);

  // Le type `Record<Lang, string>` est ce qui rend l'ajout d'une langue sûr :
  // une langue déclarée dans `LANGS` mais sans nom natif dans `fr.ts` casse la
  // compilation ici, au lieu d'afficher une pastille vide.
  const names: Record<Lang, string> = t.settings.languageNames;

  const timeFormats: { id: TimeFormat; label: string }[] = [
    { id: 'auto', label: t.settings.timeFormatAuto },
    { id: '12h', label: t.settings.timeFormat12 },
    { id: '24h', label: t.settings.timeFormat24 },
  ];

  return (
    <div>
      <SettingsSection title={t.settings.language} hint={t.settings.languageHint}>
        <div className="flex flex-wrap gap-2">
          {LANGS.map((id) => (
            <OptionPill key={id} selected={lang === id} onSelect={() => setLang(id)}>
              {names[id]}
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
