/**
 * Aperçu fidèle de la carte de profil dans les paramètres : reprend
 * exactement les couches de `ProfilePopover` (cadre, verre, effet, teinte,
 * bannière, avatar décoré, pastille de présence) alimentées par la session —
 * ce que les amis voient en cliquant sur le profil, ni plus ni moins.
 */

import type { PresenceStatus } from '../../lib/api';
import { profileCardGradient, profileColorCss } from '../../lib/color';
import { effectById, frameById } from '../../lib/decorations';
import { useFriends } from '../../stores/friends';
import { selfDisplayName, useSession } from '../../stores/session';
import { useT } from '../../stores/ui';
import { Avatar } from '../Avatar';
import { MarkdownText } from '../MarkdownText';
import { PresenceDot } from '../PresenceDot';
import { ProfileBanner } from '../ProfileBanner';

/** Largeur de la carte (px) — identique à `ProfilePopover.CARD_WIDTH`. */
const CARD_WIDTH = 340;

export function ProfileCardPreview() {
  const t = useT();
  const self = useSession((s) => s.self);
  const ownStatus = useFriends((s) => s.ownStatus);
  const ownStatusText = useFriends((s) => s.ownStatusText);

  if (self === null) return null;

  const name = selfDisplayName(self);
  const effect = effectById(self.profile_effect);
  const frame = frameById(self.profile_frame);
  const accentHex = profileColorCss(self.accent_color);
  const cardGradient = profileCardGradient(self.banner_color ?? self.accent_color);
  const status: PresenceStatus = ownStatus === 'invisible' ? 'offline' : ownStatus;

  return (
    <div className="personalization-card-stage">
      <div
        data-testid="profile-card-preview"
        className="profile-card-shell"
        style={{ width: CARD_WIDTH, maxWidth: '100%' }}
      >
        {frame?.render()}
        <div className="profile-card-canvas profile-card-shell__surface glass-strong overflow-hidden rounded-xl">
          {effect?.render()}
          {cardGradient !== null && (
            <span
              aria-hidden
              className="profile-card-tint"
              style={{ backgroundImage: cardGradient }}
            />
          )}
          <div className="profile-card-content">
            <ProfileBanner
              hash={self.banner}
              hint={self.pubkey}
              color={self.banner_color}
            />
            <div className="-mt-10 px-4 pb-4">
              <div className="mb-2 flex items-end justify-between">
                <div className="relative z-10 rounded-full bg-modal p-1 shadow-2">
                  <Avatar
                    id={self.pubkey}
                    name={name}
                    size={80}
                    avatarHash={self.avatar}
                    hint={self.pubkey}
                    decoration={self.avatar_decoration}
                  />
                </div>
                <span className="mb-1 flex items-center gap-1.5 rounded-full border border-[color:var(--glass-border)] bg-modal/75 px-2.5 py-1 text-xs font-medium text-muted shadow-1">
                  <PresenceDot status={status} />
                  {t.profil[status]}
                </span>
              </div>

              <div className="profile-card-surface relative overflow-hidden rounded-lg p-3">
                <div className="relative">
                  {accentHex !== null && (
                    <div
                      aria-hidden
                      className="mb-2 h-1 w-10 rounded-full"
                      style={{ backgroundColor: accentHex }}
                    />
                  )}
                  <span
                    className="block truncate text-lg font-semibold text-header"
                    style={accentHex !== null ? { color: accentHex } : undefined}
                  >
                    {name}
                  </span>
                  {self.pronouns !== null && self.pronouns !== '' && (
                    <p className="mt-0.5 truncate text-xs text-muted">{self.pronouns}</p>
                  )}
                  {ownStatusText !== null && ownStatusText !== '' && (
                    <p className="mt-0.5 truncate text-sm text-muted">{ownStatusText}</p>
                  )}
                  {self.bio !== null && self.bio !== '' && (
                    <>
                      <div className="mt-3 h-px bg-input/60" role="separator" />
                      <div className="mt-2 whitespace-pre-wrap break-words text-sm text-norm">
                        <MarkdownText text={self.bio} />
                      </div>
                    </>
                  )}
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
