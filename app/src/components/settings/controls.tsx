/** Briques partagées des onglets de paramètres : sections et pastilles. */

import { useEffect, useState, type ReactNode } from 'react';
import { interpolate } from '../../i18n';
import { profileColorCss } from '../../lib/color';
import { useT } from '../../stores/ui';

/** Section titrée d'un onglet (titre en petites capitales, façon Discord). */
export function SettingsSection({
  title,
  hint,
  children,
}: {
  title: string;
  hint?: string;
  children: ReactNode;
}) {
  return (
    <section aria-label={title} className="mb-8">
      <h3 className="mb-2 text-xs font-medium uppercase tracking-wide text-faint">
        {title}
      </h3>
      {hint !== undefined && <p className="mb-3 text-sm text-muted">{hint}</p>}
      {children}
    </section>
  );
}

/**
 * Rangée interrupteur : libellé + indice à gauche, bascule à droite.
 * `disabled` grise la rangée et bloque le clic (ex. « fermer réduit » tant
 * que la tray elle-même n'est pas activée) sans changer sa forme — le
 * réglage reste visible et lisible, juste inactivable.
 */
export function ToggleRow({
  label,
  hint,
  checked,
  disabled = false,
  onChange,
}: {
  label: string;
  hint?: string;
  checked: boolean;
  disabled?: boolean;
  onChange: (checked: boolean) => void;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-disabled={disabled}
      aria-label={label}
      disabled={disabled}
      onClick={() => onChange(!checked)}
      className="mb-2 flex w-full items-center justify-between gap-4 rounded-lg bg-sidebar px-4 py-3 text-left transition-colors duration-150 hover:bg-chat-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar disabled:cursor-not-allowed disabled:opacity-50 disabled:hover:bg-sidebar"
    >
      <span className="min-w-0">
        <span className="block text-sm font-medium text-header">{label}</span>
        {hint !== undefined && (
          <span className="mt-0.5 block text-xs text-muted">{hint}</span>
        )}
      </span>
      <span
        aria-hidden
        className={`relative h-6 w-10 shrink-0 rounded-full transition-colors duration-150 ${
          checked ? 'bg-green' : 'bg-input'
        }`}
      >
        <span
          className={`absolute left-0.5 top-0.5 h-5 w-5 rounded-full bg-white transition-transform duration-150 ${
            checked ? 'translate-x-4' : ''
          }`}
        />
      </span>
    </button>
  );
}

/**
 * Palette de couleurs partagée, façon Discord (blurple en tête) — même
 * préréglage pour le profil global et la bannière de serveur.
 */
export const PRESET_PROFILE_COLORS: readonly number[] = [
  0x5865f2, // Blurple
  0x3ba55c, // Vert
  0xfaa61a, // Or
  0xed4245, // Rouge
  0xeb459e, // Fuchsia
  0x9b59b6, // Violet
  0x1abc9c, // Sarcelle
  0x3498db, // Bleu
  0x99aab5, // Greyple
  0xffffff, // Blanc
];

/** Icône « ajouter une couleur personnalisée » (voir ICON SPEC, styles/global.css). */
function AddColorIcon() {
  return (
    <svg
      width="16"
      height="16"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
    >
      <line x1="12" y1="5" x2="12" y2="19" />
      <line x1="5" y1="12" x2="19" y2="12" />
    </svg>
  );
}

/** Icône « aucune couleur » (voir ICON SPEC, styles/global.css). */
function NoColorIcon() {
  return (
    <svg
      width="16"
      height="16"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
    >
      <circle cx="12" cy="12" r="9" />
      <line x1="5.5" y1="18.5" x2="18.5" y2="5.5" />
    </svg>
  );
}

/**
 * Rangée de pastilles de couleur : `PRESET_PROFILE_COLORS`, une pastille
 * « couleur personnalisée » (input natif `type=color`, engagée au blur pour
 * éviter de spammer l'appelant pendant le glisser dans le sélecteur du
 * système) et une pastille « aucune » qui efface la couleur. Partagée entre
 * les couleurs de profil (`AccountTab`) et la couleur de bannière de serveur
 * (`ServerProfileTab`) — même préréglage, même comportement.
 */
export function ColorSwatchPicker({
  label,
  value,
  busy,
  onPick,
}: {
  label: string;
  value: number | null;
  busy: boolean;
  onPick: (color: number | null) => void;
}) {
  const t = useT();
  // Repli sur le blurple de la palette (même valeur que le token design
  // system --color-blurple) quand aucune couleur personnalisée n'est encore
  // choisie — l'input natif `type=color` exige un littéral hexadécimal.
  const defaultCustomColor = profileColorCss(PRESET_PROFILE_COLORS[0]) ?? '#5865f2';
  const [customDraft, setCustomDraft] = useState<string>(
    () => profileColorCss(value) ?? defaultCustomColor,
  );

  // Resynchronise l'aperçu du sélecteur personnalisé quand la couleur
  // enregistrée change ailleurs (préréglage cliqué, effacement, etc.).
  useEffect(() => {
    setCustomDraft(profileColorCss(value) ?? defaultCustomColor);
  }, [value, defaultCustomColor]);

  const commitCustom = (): void => {
    const parsed = Number.parseInt(customDraft.slice(1), 16);
    if (Number.isFinite(parsed) && parsed !== value) onPick(parsed);
  };

  return (
    <div role="group" aria-label={label} className="flex flex-wrap items-center gap-2">
      {PRESET_PROFILE_COLORS.map((color) => {
        const hex = profileColorCss(color) ?? '#000000';
        const selected = value === color;
        return (
          <button
            key={color}
            type="button"
            disabled={busy}
            aria-pressed={selected}
            aria-label={interpolate(t.settings.colorSwatchLabel, { hex })}
            onClick={() => onPick(color)}
            style={{ backgroundColor: hex }}
            className={`h-9 w-9 shrink-0 rounded-full border border-input transition-transform duration-fast focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar disabled:opacity-50 ${
              selected
                ? 'ring-2 ring-header ring-offset-2 ring-offset-sidebar'
                : 'hover:scale-105'
            }`}
          />
        );
      })}
      <label
        title={t.settings.colorCustom}
        className="flex h-9 w-9 shrink-0 cursor-pointer items-center justify-center overflow-hidden rounded-full border-2 border-dashed border-faint bg-rail text-faint transition-colors duration-fast hover:text-norm has-[:focus-visible]:ring-2 has-[:focus-visible]:ring-blurple has-[:focus-visible]:ring-offset-2 has-[:focus-visible]:ring-offset-sidebar"
      >
        <input
          type="color"
          aria-label={t.settings.colorCustom}
          disabled={busy}
          value={customDraft}
          onChange={(e) => setCustomDraft(e.target.value)}
          onBlur={commitCustom}
          className="sr-only"
        />
        <AddColorIcon />
      </label>
      <button
        type="button"
        disabled={busy || value === null}
        aria-label={t.settings.colorNone}
        onClick={() => onPick(null)}
        className="flex h-9 w-9 shrink-0 items-center justify-center rounded-full border-2 border-faint text-faint transition-colors duration-fast hover:border-red hover:text-red focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar disabled:opacity-40"
      >
        <NoColorIcon />
      </button>
    </div>
  );
}

/** Pastille d'option exclusive (`aria-pressed` reflète la sélection). */
export function OptionPill({
  selected,
  onSelect,
  children,
}: {
  selected: boolean;
  onSelect: () => void;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      aria-pressed={selected}
      onClick={onSelect}
      className={`inline-flex min-h-9 items-center rounded-full px-3 py-1.5 text-center text-sm font-medium transition-colors duration-150 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar ${
        selected
          ? 'bg-blurple text-white'
          : 'bg-rail text-norm hover:bg-input hover:text-header'
      }`}
    >
      {children}
    </button>
  );
}
