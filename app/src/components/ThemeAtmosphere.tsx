import type { Theme } from '../stores/ui';

type FigurativeTheme = Extract<
  Theme,
  'sakura' | 'wisteria' | 'lotus' | 'manga' | 'shojo'
>;

function isFigurativeTheme(theme: Theme): theme is FigurativeTheme {
  return ['sakura', 'wisteria', 'lotus', 'manga', 'shojo'].includes(theme);
}

function SakuraMotion() {
  return (
    <span className="theme-atmosphere__motion theme-atmosphere__motion--sakura">
      {Array.from({ length: 14 }, (_, index) => (
        <i key={index} className={`theme-petal theme-petal--${index + 1}`} />
      ))}
    </span>
  );
}

function WisteriaMotion() {
  return (
    <span className="theme-atmosphere__motion theme-atmosphere__motion--wisteria">
      {Array.from({ length: 6 }, (_, index) => (
        <i
          key={`cluster-${index}`}
          className={`theme-wisteria theme-wisteria--${index + 1}`}
        />
      ))}
      {Array.from({ length: 12 }, (_, index) => (
        <i
          key={`firefly-${index}`}
          className={`theme-firefly theme-firefly--${index + 1}`}
        />
      ))}
    </span>
  );
}

function LotusMotion() {
  return (
    <span className="theme-atmosphere__motion theme-atmosphere__motion--lotus">
      {Array.from({ length: 5 }, (_, index) => (
        <i
          key={`ripple-${index}`}
          className={`theme-ripple theme-ripple--${index + 1}`}
        />
      ))}
      {Array.from({ length: 3 }, (_, index) => (
        <i key={`koi-${index}`} className={`theme-koi theme-koi--${index + 1}`} />
      ))}
    </span>
  );
}

function MangaMotion() {
  return (
    <span className="theme-atmosphere__motion theme-atmosphere__motion--manga">
      {Array.from({ length: 4 }, (_, index) => (
        <i
          key={`panel-${index}`}
          className={`theme-manga-panel theme-manga-panel--${index + 1}`}
        />
      ))}
      {Array.from({ length: 14 }, (_, index) => (
        <i
          key={`line-${index}`}
          className={`theme-manga-line theme-manga-line--${index + 1}`}
        />
      ))}
    </span>
  );
}

function ShojoMotion() {
  return (
    <span className="theme-atmosphere__motion theme-atmosphere__motion--shojo">
      <i className="theme-shojo-ribbon theme-shojo-ribbon--one" />
      <i className="theme-shojo-ribbon theme-shojo-ribbon--two" />
      {Array.from({ length: 8 }, (_, index) => (
        <i key={`rose-${index}`} className={`theme-rose theme-rose--${index + 1}`} />
      ))}
      {Array.from({ length: 12 }, (_, index) => (
        <i
          key={`spark-${index}`}
          className={`theme-shojo-spark theme-shojo-spark--${index + 1}`}
        />
      ))}
    </span>
  );
}

function ThemeMotion({ theme }: { theme: FigurativeTheme }) {
  if (theme === 'sakura') return <SakuraMotion />;
  if (theme === 'wisteria') return <WisteriaMotion />;
  if (theme === 'lotus') return <LotusMotion />;
  if (theme === 'manga') return <MangaMotion />;
  return <ShojoMotion />;
}

export function ThemeAtmosphere({
  theme,
  preview = false,
}: {
  theme: Theme;
  preview?: boolean;
}) {
  if (!isFigurativeTheme(theme)) return null;
  return (
    <span
      aria-hidden
      data-scene={theme}
      className={`theme-atmosphere${preview ? ' theme-atmosphere--preview' : ''}`}
    >
      <span className="theme-atmosphere__art" />
      <ThemeMotion theme={theme} />
    </span>
  );
}
