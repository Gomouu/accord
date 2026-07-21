/**
 * Onglet Apparence : galerie de thèmes et densité des messages. Chaque
 * réglage s'applique immédiatement à la racine du document et est persisté
 * dans localStorage par le store d'interface. La taille de police a migré
 * vers l'onglet Accessibilité (voir `AccessibilityTab.tsx`) pour rester
 * groupée avec les autres réglages de confort visuel.
 */

import { useMemo, useRef, useState } from 'react';
import {
  deriverVariables,
  exporterTheme,
  importerTheme,
  type CouleursPerso,
} from '../../lib/customTheme';
import { copyToClipboard } from '../../lib/clipboard';
import { useUi, useT, THEME_IDS, type Theme } from '../../stores/ui';
import { interpolate, type Dict } from '../../i18n';
import { ThemeAtmosphere } from '../ThemeAtmosphere';
import { OptionPill, SettingsSection } from './controls';
import './appearance-theme-gallery.css';

/**
 * Éditeur du thème personnalisé : trois sélecteurs de couleur natifs + base
 * claire/sombre. Chaque changement est persisté et, si le thème « custom »
 * est actif, appliqué immédiatement (voir `setCustomTheme` du store).
 */
function CustomThemeEditor({
  couleurs,
  onChange,
  t,
}: {
  couleurs: CouleursPerso;
  onChange: (c: CouleursPerso) => void;
  t: Dict;
}) {
  const champ = (cle: 'fond' | 'panneaux' | 'accent', label: string): React.ReactNode => (
    <label className="flex items-center justify-between gap-3 rounded-lg bg-sidebar px-4 py-3">
      <span className="text-sm font-medium text-norm">{label}</span>
      <input
        type="color"
        value={couleurs[cle]}
        onChange={(e) => onChange({ ...couleurs, [cle]: e.target.value })}
        aria-label={label}
        className="h-8 w-14 cursor-pointer rounded-md border border-input bg-transparent"
      />
    </label>
  );
  const [copie, setCopie] = useState(false);
  const [codeImport, setCodeImport] = useState('');
  const [erreurImport, setErreurImport] = useState(false);

  const copier = (): void => {
    copyToClipboard(
      exporterTheme(couleurs),
      () => {
        setCopie(true);
        setTimeout(() => setCopie(false), 1500);
      },
      () => {},
    );
  };

  const importer = (): void => {
    const lu = importerTheme(codeImport);
    if (lu === null) {
      setErreurImport(true);
      return;
    }
    setErreurImport(false);
    setCodeImport('');
    onChange(lu);
  };

  return (
    <div className="flex flex-col gap-2">
      {champ('fond', t.settings.customFond)}
      {champ('panneaux', t.settings.customPanneaux)}
      {champ('accent', t.settings.customAccent)}
      <div className="flex items-center gap-2 pt-1">
        <span className="text-sm font-medium text-norm">{t.settings.customBase}</span>
        <OptionPill
          selected={couleurs.base === 'dark'}
          onSelect={() => onChange({ ...couleurs, base: 'dark' })}
        >
          {t.settings.customBaseDark}
        </OptionPill>
        <OptionPill
          selected={couleurs.base === 'light'}
          onSelect={() => onChange({ ...couleurs, base: 'light' })}
        >
          {t.settings.customBaseLight}
        </OptionPill>
      </div>
      <button
        type="button"
        onClick={copier}
        className="mt-1 self-start rounded-md bg-sidebar px-3 py-1.5 text-sm font-medium text-norm transition-colors hover:bg-chat-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-chat"
      >
        {copie ? t.settings.customExportDone : t.settings.customExport}
      </button>
      <div className="flex items-center gap-2">
        <input
          type="text"
          value={codeImport}
          onChange={(e) => {
            setCodeImport(e.target.value);
            setErreurImport(false);
          }}
          placeholder={t.settings.customImportPlaceholder}
          aria-label={t.settings.customImport}
          className="min-w-0 flex-1 rounded-md border border-input bg-sidebar px-3 py-1.5 text-sm text-norm placeholder:text-faint focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple"
        />
        <button
          type="button"
          onClick={importer}
          disabled={codeImport.trim() === ''}
          className="shrink-0 rounded-md bg-blurple px-3 py-1.5 text-sm font-medium text-white transition-colors hover:bg-blurple-hover disabled:opacity-50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-chat"
        >
          {t.settings.customImport}
        </button>
      </div>
      {erreurImport && <p className="text-sm text-red">{t.settings.customImportError}</p>}
    </div>
  );
}

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
  custom: 'themeCustom',
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

let themeTransitionId = 0;

function prefersReducedThemeMotion(): boolean {
  return (
    document.documentElement.dataset.motion === 'reduce' ||
    window.matchMedia?.('(prefers-reduced-motion: reduce)').matches === true
  );
}

function applyThemeWithTransition(theme: Theme, setTheme: (theme: Theme) => void): void {
  const startViewTransition = Reflect.get(document, 'startViewTransition') as
    ((update: () => void) => ViewTransition) | undefined;
  if (startViewTransition === undefined || prefersReducedThemeMotion()) {
    setTheme(theme);
    return;
  }

  const transitionId = ++themeTransitionId;
  const root = document.documentElement;
  root.dataset.themeTransition = 'active';
  const clearTransition = (): void => {
    if (transitionId === themeTransitionId) delete root.dataset.themeTransition;
  };

  try {
    const transition = startViewTransition.call(document, () => setTheme(theme));
    void transition.finished.then(clearTransition, clearTransition);
  } catch {
    clearTransition();
    setTheme(theme);
  }
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
  previewVars,
  previewBase,
  accountName,
  accountStatus,
  composerLabel,
}: {
  id: Theme;
  label: string;
  selected: boolean;
  onSelect: () => void;
  buttonRef: (el: HTMLButtonElement | null) => void;
  /** Variables inline de l'aperçu (tuile « Personnalisé » : couleurs choisies). */
  previewVars?: Record<string, string> | undefined;
  /** Base d'aperçu de la tuile « Personnalisé » (texte, verre). */
  previewBase?: 'dark' | 'light' | undefined;
  accountName: string;
  accountStatus: string;
  composerLabel: string;
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
        data-theme={id === 'custom' ? previewBase : id}
        style={previewVars as React.CSSProperties | undefined}
        aria-hidden
        className={`theme-preview theme-conversation-preview ${
          selected ? 'border-blurple' : 'border-input group-hover:border-faint'
        }`}
      >
        <span className="theme-preview__rail theme-conversation-preview__rail">
          <span className="theme-conversation-preview__server theme-conversation-preview__server--home">
            A
          </span>
          <span className="theme-conversation-preview__server">S</span>
          <span className="theme-conversation-preview__server">+</span>
        </span>
        <span className="theme-preview__sidebar theme-conversation-preview__sidebar">
          <span className="theme-conversation-preview__workspace">Accord</span>
          <span className="theme-conversation-preview__channel theme-conversation-preview__channel--active">
            <span>#</span>
            <span className="theme-conversation-preview__channel-line" />
          </span>
          <span className="theme-conversation-preview__channel">
            <span>#</span>
            <span className="theme-conversation-preview__channel-line theme-conversation-preview__channel-line--short" />
          </span>
          <span className="theme-conversation-preview__account">
            <span className="theme-conversation-preview__account-avatar">Y</span>
            <span className="theme-conversation-preview__account-copy">
              <span>{accountName}</span>
              <span>{accountStatus}</span>
            </span>
          </span>
        </span>
        <span className="theme-preview__chat theme-conversation-preview__chat">
          <span className="theme-preview__motion" />
          <ThemeAtmosphere theme={id} preview />
          <span className="theme-conversation-preview__header">
            <span>#</span>
            <span className="theme-conversation-preview__channel-line" />
            <span className="theme-conversation-preview__header-status" />
          </span>
          <span className="theme-conversation-preview__messages">
            <span className="theme-conversation-preview__message">
              <span className="theme-conversation-preview__avatar theme-conversation-preview__avatar--one">
                M
              </span>
              <span className="theme-conversation-preview__message-copy">
                <span className="theme-conversation-preview__name">Maya</span>
                <span className="theme-conversation-preview__line" />
                <span className="theme-conversation-preview__line theme-conversation-preview__line--short" />
              </span>
            </span>
            <span className="theme-conversation-preview__message">
              <span className="theme-conversation-preview__avatar theme-conversation-preview__avatar--two">
                K
              </span>
              <span className="theme-conversation-preview__message-copy">
                <span className="theme-conversation-preview__name">Kai</span>
                <span className="theme-conversation-preview__line theme-conversation-preview__line--medium" />
              </span>
            </span>
          </span>
          <span className="theme-conversation-preview__composer">
            <span>+</span>
            <span>{composerLabel}</span>
          </span>
        </span>
        {selected && (
          <span className="absolute right-1 top-1 z-[4] flex h-4 w-4 items-center justify-center rounded-full bg-blurple text-white">
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
  const customTheme = useUi((s) => s.customTheme);
  const persoVars = useMemo(() => deriverVariables(customTheme), [customTheme]);

  const selectTheme = (id: Theme): void => {
    if (id !== theme) applyThemeWithTransition(id, setTheme);
  };

  const selectAt = (next: number): void => {
    const bounded = ((next % THEME_IDS.length) + THEME_IDS.length) % THEME_IDS.length;
    const id = THEME_IDS[bounded];
    if (id === undefined) return;
    selectTheme(id);
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
          onSelect={() => selectTheme(id)}
          buttonRef={(el) => {
            buttonRefs.current[index] = el;
          }}
          previewVars={id === 'custom' ? persoVars : undefined}
          previewBase={id === 'custom' ? customTheme.base : undefined}
          accountName={t.app.you}
          accountStatus={t.profil.online}
          composerLabel={interpolate(t.dm.placeholder, { name: t.app.you })}
        />
      ))}
    </div>
  );
}

export function AppearanceTab() {
  const t = useT();
  const theme = useUi((s) => s.theme);
  const customTheme = useUi((s) => s.customTheme);
  const density = useUi((s) => s.density);
  const fontUi = useUi((s) => s.fontUi);
  const setTheme = useUi((s) => s.setTheme);
  const setCustomTheme = useUi((s) => s.setCustomTheme);
  const setDensity = useUi((s) => s.setDensity);
  const setFontUi = useUi((s) => s.setFontUi);

  return (
    <div>
      <SettingsSection title={t.settings.theme}>
        <ThemeGallery theme={theme} setTheme={setTheme} t={t} />
      </SettingsSection>

      <SettingsSection
        title={t.settings.customThemeTitle}
        hint={t.settings.customThemeHint}
      >
        <CustomThemeEditor couleurs={customTheme} onChange={setCustomTheme} t={t} />
      </SettingsSection>

      <SettingsSection title={t.settings.fontUiTitle} hint={t.settings.fontUiHint}>
        <div className="flex flex-wrap gap-2">
          <OptionPill selected={fontUi === 'system'} onSelect={() => setFontUi('system')}>
            {t.settings.fontUiSystem}
          </OptionPill>
          <OptionPill
            selected={fontUi === 'rounded'}
            onSelect={() => setFontUi('rounded')}
          >
            {t.settings.fontUiRounded}
          </OptionPill>
          <OptionPill selected={fontUi === 'serif'} onSelect={() => setFontUi('serif')}>
            {t.settings.fontUiSerif}
          </OptionPill>
        </div>
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
