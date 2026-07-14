import type { Config } from 'tailwindcss';

/**
 * Tokens sémantiques calqués sur Discord (fidélité visuelle, SPEC §10) : les
 * composants ne manipulent jamais de valeurs hexadécimales. Les valeurs
 * vivent dans des variables CSS (styles/global.css) déclinées par thème
 * (sombre par défaut, clair via `data-theme="light"` sur la racine) ; le
 * format `rgb(var(--…) / <alpha-value>)` préserve les modificateurs
 * d'opacité Tailwind (`bg-rail/60`).
 */
function themed(variable: string): string {
  return `rgb(var(${variable}) / <alpha-value>)`;
}

export default {
  content: ['./index.html', './src/**/*.{ts,tsx}'],
  theme: {
    extend: {
      colors: {
        // Surfaces, de la plus profonde à la plus claire.
        rail: themed('--color-rail'),
        sidebar: themed('--color-sidebar'),
        chat: themed('--color-chat'),
        'chat-hover': themed('--color-chat-hover'),
        input: themed('--color-input'),
        modal: themed('--color-modal'),
        tooltip: themed('--color-tooltip'),
        // Texte.
        norm: themed('--color-norm'),
        muted: themed('--color-muted'),
        faint: themed('--color-faint'),
        header: themed('--color-header'),
        // Accents.
        blurple: themed('--color-blurple'),
        'blurple-hover': themed('--color-blurple-hover'),
        green: themed('--color-green'),
        red: themed('--color-red'),
        yellow: themed('--color-yellow'),
        link: themed('--color-link'),
        'on-green': themed('--color-on-green'),
        'on-red': themed('--color-on-red'),
      },
      fontFamily: {
        // Native system stack (no bundled/CDN font — CSP forbids external
        // hosts): each OS renders its own default UI typeface, matching the
        // clean, standard chat-app feel users expect.
        sans: [
          '-apple-system',
          'BlinkMacSystemFont',
          '"Segoe UI"',
          'Roboto',
          '"Helvetica Neue"',
          'Arial',
          'sans-serif',
        ],
        mono: ['SF Mono', 'Consolas', 'Liberation Mono', 'monospace'],
      },
      borderRadius: {
        server: '16px',
        // Discord 2025 × Liquid Glass radius scale (styles/global.css) —
        // remaps Tailwind's built-in sm/md/lg/xl keys; every `rounded-*`
        // usage in the app now resolves through these tokens.
        xs: 'var(--radius-xs)',
        sm: 'var(--radius-sm)',
        md: 'var(--radius-md)',
        lg: 'var(--radius-lg)',
        xl: 'var(--radius-xl)',
      },
      boxShadow: {
        elevation: 'var(--shadow-elevation)',
        modal: 'var(--shadow-modal)',
        // Elevation tokens (styles/global.css): 1 = subtle, 2 = floating
        // (dual shadow), 3 = modal-depth.
        1: 'var(--shadow-1)',
        2: 'var(--shadow-2)',
        3: 'var(--shadow-3)',
      },
      // Tokens de mouvement (styles/global.css) : mêmes valeurs partout,
      // jamais de durée/courbe codée en dur dans un composant.
      transitionDuration: {
        instant: 'var(--duration-instant)',
        fast: 'var(--duration-fast)',
        normal: 'var(--duration-normal)',
      },
      transitionTimingFunction: {
        expo: 'var(--ease-out)',
        spring: 'var(--ease-spring)',
      },
    },
  },
  plugins: [],
} satisfies Config;
