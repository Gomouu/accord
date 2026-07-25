/**
 * Onglet Texte & médias : aperçu en ligne des images/pièces jointes
 * (`Attachments.tsx` s'y réfère directement) et taille des émojis
 * personnalisés du serveur dans le corps des messages (`MarkdownText.tsx`).
 *
 * Pas d'entrée « embeds de liens » ici : la fonctionnalité n'existe pas
 * ailleurs dans le code (aucun rendu d'aperçu de lien) — un interrupteur sans
 * effet aurait été une pastille décorative, exclue par les règles du projet.
 */

import {
  useUi,
  useSettingsT,
  VIDEO_PREVIEW_MAX_CHOICES,
  type EmojiSize,
} from '../../stores/ui';
import { OptionPill, SettingsSection, ToggleRow } from './controls';

export function TextMediaTab() {
  const ts = useSettingsT();
  const showMediaPreviews = useUi((s) => s.showMediaPreviews);
  const setShowMediaPreviews = useUi((s) => s.setShowMediaPreviews);
  const emojiSize = useUi((s) => s.emojiSize);
  const setEmojiSize = useUi((s) => s.setEmojiSize);
  const videoPreviewMaxMio = useUi((s) => s.videoPreviewMaxMio);
  const setVideoPreviewMaxMio = useUi((s) => s.setVideoPreviewMaxMio);

  const emojiOptions: { id: EmojiSize; label: string }[] = [
    { id: 'normal', label: ts.settings.emojiSizeNormal },
    { id: 'large', label: ts.settings.emojiSizeLarge },
  ];

  return (
    <div>
      <SettingsSection title={ts.settings.mediaPreviewsTitle}>
        <ToggleRow
          label={ts.settings.mediaPreviewsTitle}
          hint={ts.settings.mediaPreviewsHint}
          checked={showMediaPreviews}
          onChange={setShowMediaPreviews}
        />
      </SettingsSection>

      <SettingsSection
        title={ts.settings.emojiSizeTitle}
        hint={ts.settings.emojiSizeHint}
      >
        <div className="flex flex-wrap gap-2">
          {emojiOptions.map(({ id, label }) => (
            <OptionPill
              key={id}
              selected={emojiSize === id}
              onSelect={() => setEmojiSize(id)}
            >
              {label}
            </OptionPill>
          ))}
        </div>
      </SettingsSection>

      <SettingsSection
        title={ts.settings.videoPreviewMaxTitle}
        hint={ts.settings.videoPreviewMaxHint}
      >
        <div className="flex flex-wrap gap-2">
          {VIDEO_PREVIEW_MAX_CHOICES.map((mio) => (
            <OptionPill
              key={mio}
              selected={videoPreviewMaxMio === mio}
              onSelect={() => setVideoPreviewMaxMio(mio)}
            >
              {`${mio} Mio`}
            </OptionPill>
          ))}
        </div>
      </SettingsSection>
    </div>
  );
}
