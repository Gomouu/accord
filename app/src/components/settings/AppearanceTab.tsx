/**
 * Onglet Apparence : galerie de thèmes et densité des messages. Chaque
 * réglage s'applique immédiatement à la racine du document et est persisté
 * dans localStorage par le store d'interface. La taille de police a migré
 * vers l'onglet Accessibilité (voir `AccessibilityTab.tsx`) pour rester
 * groupée avec les autres réglages de confort visuel.
 */

import { useRef } from 'react';
import { useUi, useT, THEME_IDS, type Theme } from '../../stores/ui';
import type { Dict } from '../../i18n';
import { ThemeAtmosphere } from '../ThemeAtmosphere';
import { OptionPill, SettingsSection } from './controls';

/** Clé i18n du libellé de chaque thème (voir `settings.theme*` dans fr.ts/en.ts). */
export const THEME_LABEL_KEYS: Record<Theme, keyof Dict['settings']> = {
  dark: 'themeDark',
  light: 'themeLight',
  midnight: 'themeMidnight',
  storm: 'themeStorm',
  forest: 'themeForest',
  sunset: 'themeSunset',
  ocean: 'themeOcean',
  crimson: 'themeCrimson',
  boreal: 'themeBoreal',
  paper: 'themePaper',
  topography: 'themeTopography',
  signal: 'themeSignal',
  nebula: 'themeNebula',
  synthwave: 'themeSynthwave',
  sakura: 'themeSakura',
  wisteria: 'themeWisteria',
  lotus: 'themeLotus',
  manga: 'themeManga',
  shojo: 'themeShojo',
  abyss: 'themeAbyss',
  ember: 'themeEmber',
  frost: 'themeFrost',
  circuit: 'themeCircuit',
  dream: 'themeDream',
};

function ThemeCheckIcon() {
  return (
    <svg
      width="12"
      height="12"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={3}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
    >
      <polyline points="20 6 9 17 4 12" />
    </svg>
  );
}

/**
 * Vignette de thème : aperçu miniature fidèle + libellé, façon Discord.
 * `data-theme` sur l'aperçu (et lui seul — jamais sur le libellé) scope les
 * tokens CSS du thème représenté à ce petit bloc, indépendamment du thème
 * réellement actif ; c'est ce qui permet à l'aperçu de rester exact sans
 * dupliquer la moindre couleur en JS (voir `global.css` et
 * `theme-scenes.css`, sélecteurs `[data-theme='…']`). Membre d'un
 * `radiogroup` (voir `ThemeGallery`) : le focus roving (`tabIndex`) est géré
 * par le parent.
 */
function ThemeCard({
  id,
  label,
  selected,
  onSelect,
  buttonRef,
}: {
  id: Theme;
  label: string;
  selected: boolean;
  onSelect: () => void;
  buttonRef: (el: HTMLButtonElement | null) => void;
}) {
  return (
    <button
      ref={buttonRef}
      type="button"
      role="radio"
      aria-checked={selected}
      tabIndex={selected ? 0 : -1}
      onClick={onSelect}
      className="group w-full rounded-lg text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-header focus-visible:ring-offset-2 focus-visible:ring-offset-chat"
    >
      <span
        data-theme={id}
        aria-hidden
        className={`theme-preview relative flex h-20 w-full overflow-hidden rounded-lg border-2 transition-colors duration-150 ${
          selected ? 'border-blurple' : 'border-input group-hover:border-faint'
        }`}
      >
        <span className="theme-preview__rail flex h-full w-[18%] shrink-0 flex-col items-center gap-1 pt-2">
          <span className="h-2 w-2 rounded-full bg-blurple" />
          <span className="theme-preview__secondary h-2 w-2 rounded-full" />
        </span>
        <span className="theme-preview__sidebar flex h-full w-[28%] shrink-0 flex-col gap-1 p-1.5">
          <span className="h-1.5 w-4/5 rounded-full bg-input" />
          <span className="h-1.5 w-3/5 rounded-full bg-input" />
          <span className="mt-1 h-1.5 w-full rounded-full bg-chat/70" />
        </span>
        <span className="theme-preview__chat flex min-w-0 flex-1 flex-col gap-1 p-1.5">
          <span className="theme-preview__motion" />
          <ThemeAtmosphere theme={id} preview />
          <span className="h-1.5 w-3/4 rounded-full bg-input/80" />
          <span className="h-1.5 w-1/2 rounded-full bg-input/80" />
          <span className="theme-preview__accent mt-auto h-1.5 w-4/5 rounded-full" />
        </span>
        {selected && (
          <span className="absolute right-1 top-1 flex h-4 w-4 items-center justify-center rounded-full bg-blurple text-white">
            <ThemeCheckIcon />
          </span>
        )}
      </span>
      <span
        className={`mt-1.5 block min-h-9 text-sm font-medium leading-tight ${
          selected ? 'text-header' : 'text-muted group-hover:text-norm'
        }`}
      >
        {label}
      </span>
    </button>
  );
}

/**
 * Galerie de thèmes : `radiogroup` accessible au clavier — les flèches
 * déplacent le focus roving ET la sélection à la fois (comme un groupe de
 * boutons radio natif, où sélection = focus). Droite/Bas avancent,
 * Gauche/Haut reculent, indépendamment du nombre responsive de colonnes.
 */
function ThemeGallery({
  theme,
  setTheme,
  t,
}: {
  theme: Theme;
  setTheme: (theme: Theme) => void;
  t: Dict;
}) {
  const buttonRefs = useRef<Array<HTMLButtonElement | null>>([]);

  const selectAt = (next: number): void => {
    const bounded = ((next % THEME_IDS.length) + THEME_IDS.length) % THEME_IDS.length;
    const id = THEME_IDS[bounded];
    if (id === undefined) return;
    setTheme(id);
    buttonRefs.current[bounded]?.focus();
  };

  const onKeyDown = (e: React.KeyboardEvent<HTMLDivElement>): void => {
    const currentIndex = THEME_IDS.indexOf(theme);
    if (e.key === 'ArrowRight' || e.key === 'ArrowDown') {
      e.preventDefault();
      selectAt(currentIndex + 1);
    } else if (e.key === 'ArrowLeft' || e.key === 'ArrowUp') {
      e.preventDefault();
      selectAt(currentIndex - 1);
    }
  };

  return (
    <div
      role="radiogroup"
      aria-label={t.settings.theme}
      onKeyDown={onKeyDown}
      className="grid grid-cols-2 gap-4 sm:grid-cols-4"
    >
      {THEME_IDS.map((id, index) => (
        <ThemeCard
          key={id}
          id={id}
          label={t.settings[THEME_LABEL_KEYS[id]]}
          selected={theme === id}
          onSelect={() => setTheme(id)}
          buttonRef={(el) => {
            buttonRefs.current[index] = el;
          }}
        />
      ))}
    </div>
  );
}

export function AppearanceTab() {
  const t = useT();
  const theme = useUi((s) => s.theme);
  const density = useUi((s) => s.density);
  const setTheme = useUi((s) => s.setTheme);
  const setDensity = useUi((s) => s.setDensity);

  return (
    <div>
      <SettingsSection title={t.settings.theme}>
        <ThemeGallery theme={theme} setTheme={setTheme} t={t} />
      </SettingsSection>

      <SettingsSection title={t.settings.density} hint={t.settings.densityHint}>
        <div className="flex flex-wrap gap-2">
          <OptionPill
            selected={density === 'comfortable'}
            onSelect={() => setDensity('comfortable')}
          >
            {t.settings.densityComfortable}
          </OptionPill>
          <OptionPill
            selected={density === 'compact'}
            onSelect={() => setDensity('compact')}
          >
            {t.settings.densityCompact}
          </OptionPill>
        </div>
      </SettingsSection>
    </div>
  );
}
