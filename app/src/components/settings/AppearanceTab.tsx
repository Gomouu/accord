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
import { OptionPill, SettingsSection } from './controls';

/** Clé i18n du libellé de chaque thème (voir `settings.theme*` dans fr.ts/en.ts). */
const THEME_LABEL_KEYS: Record<Theme, keyof Dict['settings']> = {
  dark: 'themeDark',
  light: 'themeLight',
  midnight: 'themeMidnight',
  storm: 'themeStorm',
  forest: 'themeForest',
  sunset: 'themeSunset',
  ocean: 'themeOcean',
  crimson: 'themeCrimson',
};

/** Nombre de colonnes de la grille — source de vérité pour la navigation
 * clavier haut/bas (voir `ThemeGallery`), garder en phase avec `grid-cols-4`. */
const THEME_GRID_COLUMNS = 4;

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
 * dupliquer la moindre couleur en JS (voir global.css, sélecteurs
 * `[data-theme='…']`). Membre d'un `radiogroup` (voir `ThemeGallery`) : le
 * focus roving (`tabIndex`) est géré par le parent.
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
      className="group w-full rounded-lg text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-chat"
    >
      <span
        data-theme={id}
        aria-hidden
        className={`relative flex h-20 w-full overflow-hidden rounded-lg border-2 bg-chat transition-colors duration-150 ${
          selected ? 'border-blurple' : 'border-input group-hover:border-faint'
        }`}
      >
        <span className="h-full w-1/4 shrink-0 bg-sidebar" />
        <span className="flex min-w-0 flex-1 flex-col gap-1 bg-chat p-1.5">
          <span className="h-1.5 w-3/4 rounded-full bg-input" />
          <span className="h-1.5 w-1/2 rounded-full bg-input" />
          <span className="mt-auto h-1.5 w-2/3 rounded-full bg-blurple" />
        </span>
        {selected && (
          <span className="absolute right-1 top-1 flex h-4 w-4 items-center justify-center rounded-full bg-blurple text-white">
            <ThemeCheckIcon />
          </span>
        )}
      </span>
      <span
        className={`mt-1.5 block text-sm font-medium ${
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
 * boutons radio natif, où sélection = focus), gauche/droite dans la grille,
 * haut/bas d'une rangée entière (`THEME_GRID_COLUMNS`), avec retour au
 * début en bout de liste.
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
    if (e.key === 'ArrowRight') {
      e.preventDefault();
      selectAt(currentIndex + 1);
    } else if (e.key === 'ArrowLeft') {
      e.preventDefault();
      selectAt(currentIndex - 1);
    } else if (e.key === 'ArrowDown') {
      e.preventDefault();
      selectAt(currentIndex + THEME_GRID_COLUMNS);
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      selectAt(currentIndex - THEME_GRID_COLUMNS);
    }
  };

  return (
    <div
      role="radiogroup"
      aria-label={t.settings.theme}
      onKeyDown={onKeyDown}
      className="grid grid-cols-4 gap-4"
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
