/**
 * Éléments d'interface partagés entre les écrans d'accueil (Onboarding,
 * sélecteur de comptes) : panneau centré façon verre liquide sur fond
 * ambiant, champ de saisie au focus doux, bouton primaire — un seul endroit
 * pour le contrat visuel de ces écrans (voir DESIGN SYSTEM, styles/global.css).
 */

import type { ReactNode } from 'react';
import { useT } from '../stores/ui';

/**
 * Panneau centré sur fond ambiant (`.app-ambient`, comme `AppShell`) :
 * verre liquide (`.glass`), coins `rounded-xl`, élévation `shadow-3`,
 * entrée animée (`modal-panel-enter`). `wide` élargit le panneau pour le
 * sélecteur de comptes (plusieurs lignes) plutôt que les formulaires.
 */
/** Logo de l'app (asset empaqueté par Vite). */
const LOGO_URL = new URL('../assets/accord-logo.svg', import.meta.url).href;

export function Card({
  children,
  wide = false,
}: {
  children: ReactNode;
  wide?: boolean;
}) {
  const t = useT();
  return (
    <div className="app-ambient flex h-full flex-col items-center overflow-y-auto bg-rail px-4 py-6">
      <div
        className={`modal-panel-enter glass my-auto w-full shrink-0 ${
          wide ? 'max-w-[min(520px,94vw)]' : 'max-w-[min(440px,94vw)]'
        } rounded-xl p-8 shadow-3`}
      >
        <img
          src={LOGO_URL}
          alt=""
          aria-hidden
          width={64}
          height={64}
          className="mx-auto mb-3 h-16 w-16"
        />
        <div className="mb-6 text-center text-xs font-bold uppercase tracking-[0.2em] text-blurple">
          {t.app.name}
        </div>
        {children}
      </div>
    </div>
  );
}

export function Field({
  label,
  type = 'password',
  value,
  onChange,
  placeholder,
}: {
  label: string;
  type?: 'password' | 'text';
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
}) {
  return (
    <label className="mb-4 block">
      <span className="mb-1.5 block text-xs font-semibold uppercase tracking-wide text-muted">
        {label}
      </span>
      <input
        type={type}
        value={value}
        placeholder={placeholder}
        onChange={(e) => onChange(e.target.value)}
        className="w-full rounded-md border border-transparent bg-input px-3 py-2.5 text-norm placeholder-faint outline-none transition-colors duration-fast focus:border-blurple/50"
      />
    </label>
  );
}

export function PrimaryButton({
  label,
  disabled,
  onClick,
}: {
  label: string;
  disabled?: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      disabled={disabled ?? false}
      onClick={onClick}
      className="w-full rounded-lg bg-blurple py-2.5 font-medium text-white shadow-sm transition-[transform,background-color,box-shadow] duration-fast hover:-translate-y-px hover:bg-blurple-hover hover:shadow-md focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-modal disabled:pointer-events-none disabled:opacity-50 active:translate-y-0 active:scale-[0.98]"
    >
      {label}
    </button>
  );
}
