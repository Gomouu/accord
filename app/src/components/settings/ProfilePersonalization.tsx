import { useState } from 'react';
import {
  AVATAR_DECORATIONS,
  PROFILE_EFFECTS,
  PROFILE_FRAMES,
  decorationById,
  effectById,
  frameById,
} from '../../lib/decorations';
import { decorationLabel } from '../../i18n';
import { useSession } from '../../stores/session';
import { useSettingsT, useT, useUi } from '../../stores/ui';
import { Avatar } from '../Avatar';
import { SettingsSection } from './controls';
import { ProfileCardPreview } from './ProfileCardPreview';

type BusyKind = 'decoration' | 'effect' | 'frame' | null;

function SelectedMark() {
  return (
    <span className="personalization-choice__check" aria-hidden>
      ✓
    </span>
  );
}

export function ProfilePersonalization() {
  const t = useT();
  const ts = useSettingsT();
  const toast = useUi((state) => state.toast);
  const self = useSession((state) => state.self);
  const setAvatarDecoration = useSession((state) => state.setAvatarDecoration);
  const setProfileEffect = useSession((state) => state.setProfileEffect);
  const setProfileFrame = useSession((state) => state.setProfileFrame);
  const [busy, setBusy] = useState<BusyKind>(null);

  // Les libellés vivent dans les dictionnaires, indexés par l'identifiant du
  // registre : ajouter une langue ne demande plus de passe manuelle ici, et le
  // test de parité couvre automatiquement toute langue future.
  const label = (id: string): string => decorationLabel(ts, id);
  const labelOf = (item: { id: string } | undefined): string | undefined =>
    item === undefined ? undefined : label(item.id);

  if (self === null) return null;

  const avatarName = self.name ?? self.friend_code;
  const selectedDecoration = decorationById(self.avatar_decoration);
  const selectedEffect = effectById(self.profile_effect);
  const selectedFrame = frameById(self.profile_frame);
  const previewMeta = [
    labelOf(selectedDecoration) ?? ts.decorations.none,
    labelOf(selectedEffect) ?? ts.decorations.none,
    labelOf(selectedFrame) ?? ts.decorations.none,
  ].join(' · ');

  const apply = async (kind: Exclude<BusyKind, null>, action: () => Promise<void>) => {
    if (busy !== null) return;
    setBusy(kind);
    try {
      await action();
      toast('success', ts.decorations.saved);
    } catch {
      toast('error', t.errors.actionFailed);
    } finally {
      setBusy(null);
    }
  };

  const pickDecoration = (id: string | null): void => {
    if (id === self.avatar_decoration) return;
    void apply('decoration', () => setAvatarDecoration(id));
  };

  const pickEffect = (id: string | null): void => {
    if (id === self.profile_effect) return;
    void apply('effect', () => setProfileEffect(id));
  };

  const pickFrame = (id: string | null): void => {
    if (id === self.profile_frame) return;
    void apply('frame', () => setProfileFrame(id));
  };

  return (
    <>
      <SettingsSection
        title={ts.decorations.decorationTitle}
        hint={ts.decorations.decorationHint}
      >
        <div className="mb-4">
          <ProfileCardPreview />
          <p className="mt-2 text-center text-xs text-faint">
            <span className="font-medium uppercase tracking-wide">
              {ts.decorations.preview}
            </span>
            <span className="mx-1.5" aria-hidden>
              —
            </span>
            <span>{previewMeta}</span>
          </p>
        </div>

        <div
          role="group"
          aria-label={ts.decorations.decorationTitle}
          aria-busy={busy === 'decoration'}
          className="personalization-grid"
        >
          <button
            type="button"
            disabled={busy !== null}
            aria-pressed={self.avatar_decoration === null}
            onClick={() => pickDecoration(null)}
            className="personalization-choice"
          >
            <Avatar
              id={self.pubkey}
              name={avatarName}
              size={54}
              avatarHash={self.avatar}
              hint={self.pubkey}
            />
            <span className="personalization-choice__label">{ts.decorations.none}</span>
            <SelectedMark />
          </button>
          {AVATAR_DECORATIONS.map((decoration) => (
            <button
              key={decoration.id}
              type="button"
              disabled={busy !== null}
              aria-pressed={self.avatar_decoration === decoration.id}
              onClick={() => pickDecoration(decoration.id)}
              className="personalization-choice"
            >
              <Avatar
                id={self.pubkey}
                name={avatarName}
                size={54}
                avatarHash={self.avatar}
                hint={self.pubkey}
                decoration={decoration.id}
                decorationMotion="interaction"
              />
              <span className="personalization-choice__label">
                {label(decoration.id)}
              </span>
              <SelectedMark />
            </button>
          ))}
        </div>
      </SettingsSection>

      <SettingsSection
        title={ts.decorations.effectTitle}
        hint={ts.decorations.effectHint}
      >
        <div
          role="group"
          aria-label={ts.decorations.effectTitle}
          aria-busy={busy === 'effect'}
          className="personalization-grid"
        >
          <button
            type="button"
            disabled={busy !== null}
            aria-pressed={self.profile_effect === null}
            onClick={() => pickEffect(null)}
            className="personalization-choice"
          >
            <span className="personalization-choice__effect bg-rail" aria-hidden />
            <span className="personalization-choice__label">{ts.decorations.none}</span>
            <SelectedMark />
          </button>
          {PROFILE_EFFECTS.map((effect) => (
            <button
              key={effect.id}
              type="button"
              disabled={busy !== null}
              aria-pressed={self.profile_effect === effect.id}
              onClick={() => pickEffect(effect.id)}
              className="personalization-choice"
            >
              <span className="personalization-choice__effect">{effect.render()}</span>
              <span className="personalization-choice__label">{label(effect.id)}</span>
              <SelectedMark />
            </button>
          ))}
        </div>
      </SettingsSection>

      <SettingsSection title={ts.decorations.frameTitle} hint={ts.decorations.frameHint}>
        <div
          role="group"
          aria-label={ts.decorations.frameTitle}
          aria-busy={busy === 'frame'}
          className="personalization-grid"
        >
          <button
            type="button"
            disabled={busy !== null}
            aria-pressed={self.profile_frame === null}
            onClick={() => pickFrame(null)}
            className="personalization-choice"
          >
            <span className="personalization-choice__frame bg-rail" aria-hidden />
            <span className="personalization-choice__label">{ts.decorations.none}</span>
            <SelectedMark />
          </button>
          {PROFILE_FRAMES.map((frame) => (
            <button
              key={frame.id}
              type="button"
              disabled={busy !== null}
              aria-pressed={self.profile_frame === frame.id}
              onClick={() => pickFrame(frame.id)}
              className="personalization-choice"
            >
              <span className="personalization-choice__frame">{frame.render()}</span>
              <span className="personalization-choice__label">{label(frame.id)}</span>
              <SelectedMark />
            </button>
          ))}
        </div>
      </SettingsSection>
    </>
  );
}
