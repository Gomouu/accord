/**
 * Éléments d'interface partagés entre les écrans d'accueil (Onboarding,
 * sélecteur de comptes) : panneau centré façon verre liquide sur fond
 * ambiant, champ de saisie au focus doux, bouton primaire — un seul endroit
 * pour le contrat visuel de ces écrans (voir DESIGN SYSTEM, styles/global.css).
 */

import type { ReactNode } from 'react';
import { useT } from '../stores/ui';
import './onboarding.css';

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
    <div className="onboarding-shell overflow-y-auto">
      <div className="onboarding-haze" aria-hidden="true">
        <span />
        <span />
        <span />
      </div>
      <div
        className={`onboarding-frame modal-panel-enter ${
          wide ? 'onboarding-frame-wide' : ''
        }`}
      >
        <aside className="onboarding-brand" aria-hidden="true">
          <div className="onboarding-wordmark">
            <span className="onboarding-logo-shell">
              <img src={LOGO_URL} alt="" width={40} height={40} />
            </span>
            <span>{t.app.name}</span>
          </div>
          <div className="onboarding-signal">
            <span className="onboarding-signal-ring onboarding-signal-ring-one" />
            <span className="onboarding-signal-ring onboarding-signal-ring-two" />
            <span className="onboarding-signal-ring onboarding-signal-ring-three" />
            <span className="onboarding-signal-core">
              <span />
              <span />
              <span />
            </span>
            <span className="onboarding-signal-node onboarding-signal-node-one" />
            <span className="onboarding-signal-node onboarding-signal-node-two" />
            <span className="onboarding-signal-node onboarding-signal-node-three" />
          </div>
          <div className="onboarding-brand-copy">
            <span className="onboarding-brand-line" />
            <p>{t.onboarding.tagline}</p>
          </div>
        </aside>
        <main className="onboarding-panel">
          <div className="onboarding-panel-content">{children}</div>
        </main>
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
    <label className="onboarding-field mb-4 block">
      <span className="onboarding-field-label mb-1.5 block text-xs font-semibold uppercase tracking-wide text-muted">
        {label}
      </span>
      <input
        type={type}
        value={value}
        placeholder={placeholder}
        onChange={(e) => onChange(e.target.value)}
        className="onboarding-field-input min-h-11 w-full rounded-md border border-transparent bg-input px-3 py-2.5 text-norm placeholder-faint outline-none transition-colors duration-fast focus:border-blurple/50"
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
      className="onboarding-primary min-h-11 w-full rounded-lg bg-blurple py-2.5 font-medium text-white shadow-sm transition-[transform,background-color,box-shadow] duration-fast hover:-translate-y-px hover:bg-blurple-hover hover:shadow-md focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-modal disabled:pointer-events-none disabled:opacity-50 active:translate-y-0 active:scale-[0.98]"
    >
      {label}
    </button>
  );
}
