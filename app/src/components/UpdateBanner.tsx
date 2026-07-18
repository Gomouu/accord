/**
 * Bandeau de mise à jour (D-049) : prévient qu'une nouvelle version est
 * disponible et pilote le cycle installer → redémarrer sans quitter
 * l'application. « Plus tard » écarte la version proposée (la section
 * « Mises à jour » des paramètres reste le point de reprise).
 */

import { interpolate } from '../i18n';
import { useUpdater } from '../stores/updater';
import { useT } from '../stores/ui';

function BannerButton({
  label,
  onClick,
  emphasis = false,
}: {
  label: string;
  onClick: () => void;
  emphasis?: boolean;
}) {
  const style = emphasis
    ? 'rounded-md bg-white/20 px-2.5 py-0.5 font-semibold transition-colors duration-fast hover:bg-white/30'
    : 'rounded-sm px-1.5 py-0.5 font-semibold underline decoration-white/40 underline-offset-2 transition-colors duration-fast hover:decoration-white';
  return (
    <button
      type="button"
      onClick={onClick}
      className={`${style} focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-white/60`}
    >
      {label}
    </button>
  );
}

export function UpdateBanner() {
  const t = useT();
  const status = useUpdater((s) => s.status);
  const version = useUpdater((s) => s.version);
  const progress = useUpdater((s) => s.progress);
  const error = useUpdater((s) => s.error);
  const dismissedVersion = useUpdater((s) => s.dismissedVersion);
  const install = useUpdater((s) => s.install);
  const restart = useUpdater((s) => s.restart);
  const dismissBanner = useUpdater((s) => s.dismissBanner);

  const relevant =
    status === 'available' ||
    status === 'downloading' ||
    status === 'ready' ||
    status === 'error';
  if (!relevant || version === null || version === dismissedVersion) return null;

  const percent = progress !== null ? Math.round(progress * 100) : null;

  return (
    <div
      role="status"
      aria-live="polite"
      aria-label={t.updates.bannerLabel}
      className="relative z-30 flex min-h-8 shrink-0 flex-wrap items-center justify-center gap-x-2.5 gap-y-1 border-b border-white/10 bg-blurple/95 px-4 py-1.5 text-center text-sm font-medium text-white shadow-1"
    >
      {status === 'available' && (
        <>
          <span className="text-pretty">
            {interpolate(t.updates.available, { version })}
          </span>
          <BannerButton
            label={t.updates.install}
            onClick={() => void install()}
            emphasis
          />
          <BannerButton label={t.updates.later} onClick={dismissBanner} />
        </>
      )}
      {status === 'downloading' && (
        <>
          <span
            aria-hidden
            className="h-3 w-3 animate-spin rounded-full border-2 border-white/40 border-t-white"
          />
          <span className="text-pretty">
            {percent !== null
              ? interpolate(t.updates.downloading, { percent: String(percent) })
              : t.updates.downloadingIndeterminate}
          </span>
        </>
      )}
      {status === 'ready' && (
        <>
          <span className="text-pretty">{t.updates.ready}</span>
          <BannerButton
            label={t.updates.restart}
            onClick={() => void restart()}
            emphasis
          />
          <BannerButton label={t.updates.later} onClick={dismissBanner} />
        </>
      )}
      {status === 'error' && (
        <>
          <span className="text-pretty">
            {interpolate(t.updates.error, { error: error ?? '' })}
          </span>
          <BannerButton label={t.updates.retry} onClick={() => void install()} emphasis />
          <BannerButton label={t.updates.later} onClick={dismissBanner} />
        </>
      )}
    </div>
  );
}
