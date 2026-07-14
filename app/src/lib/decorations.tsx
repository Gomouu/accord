/**
 * Catalogue intégré de personnalisation de profil (façon Discord), rendu en
 * CSS/SVG pur — AUCUN asset image, AUCUN transfert de blob : seul un id ASCII
 * court (`[a-z0-9_-]`, ≤ 24) traverse le réseau (voir `CoreMsg::Profile`).
 *
 * Deux familles :
 *  - `AVATAR_DECORATIONS` : un cadre/anneau décoratif superposé autour d'un
 *    avatar circulaire de taille arbitraire (px). `render(size)` rend un calque
 *    `position:absolute` non cliquable, à déposer dans un conteneur `relative`.
 *  - `PROFILE_EFFECTS` : un fond animé de la carte de profil. `render()` rend
 *    un calque `absolute inset-0` derrière le contenu, animé par des keyframes
 *    compositeur (transform/opacity) définies dans `styles/global.css`, donc
 *    couverts d'office par les blocs `prefers-reduced-motion`.
 *
 * Les ids sont des clés opaques : `decorationById` / `effectById` rendent
 * `undefined` pour un id inconnu ou retiré (rendu gracieux — rien ne s'affiche),
 * et un id n'est JAMAIS interpolé dans une chaîne CSS/HTML : il sert uniquement
 * de clé de lookup dans ce catalogue statique.
 */

import type { CSSProperties, ReactNode } from 'react';

/** Libellé bilingue d'une entrée de catalogue (le projet est fr/en). */
export interface DecorationLabel {
  fr: string;
  en: string;
}

/**
 * Textes bilingues des sélecteurs de personnalisation. Regroupés ici (avec les
 * libellés du catalogue) tant qu'aucune clé i18n dédiée n'existe : voir le
 * rapport pour les clés `settings.*` recommandées à ajouter dans `i18n/`.
 */
export const DECORATION_UI_TEXT = {
  decorationTitle: { fr: "Décoration d'avatar", en: 'Avatar decoration' },
  decorationHint: {
    fr: 'Un cadre décoratif autour de ton avatar, visible par tes amis.',
    en: 'A decorative frame around your avatar, visible to your friends.',
  },
  effectTitle: { fr: 'Effet de profil', en: 'Profile effect' },
  effectHint: {
    fr: 'Un fond animé sur ta carte de profil.',
    en: 'An animated background on your profile card.',
  },
  none: { fr: 'Aucune', en: 'None' },
  saved: { fr: 'Personnalisation enregistrée', en: 'Personalization saved' },
} as const;

export interface AvatarDecoration {
  /** Identifiant filaire stable (`[a-z0-9_-]`, ≤ 24). */
  id: string;
  /** Libellé affiché dans le sélecteur (résolu selon la langue courante). */
  label: DecorationLabel;
  /**
   * Rend le calque de décoration superposé à un avatar circulaire de `size` px.
   * Non cliquable (`pointer-events:none`), positionné en absolu, mise à
   * l'échelle proportionnelle à `size`.
   */
  render: (size: number) => ReactNode;
}

export interface ProfileEffect {
  /** Identifiant filaire stable (`[a-z0-9_-]`, ≤ 24). */
  id: string;
  /** Libellé affiché dans le sélecteur. */
  label: DecorationLabel;
  /** Rend le calque de fond animé de la carte (absolu, derrière le contenu). */
  render: () => ReactNode;
}

// ---------------------------------------------------------------------------
// Décorations d'avatar
// ---------------------------------------------------------------------------

/** Calque de base d'une décoration : couvre l'avatar, non cliquable. */
function overlayStyle(extra?: CSSProperties): CSSProperties {
  return {
    position: 'absolute',
    inset: 0,
    borderRadius: '50%',
    pointerEvents: 'none',
    ...extra,
  };
}

/**
 * Anneau décoratif dégradé posé sur la circonférence de l'avatar : un dégradé
 * conique masqué en un fin liseré (bande externe `radial-gradient`). `spin`
 * anime une lente rotation (transform, compositeur — couverte par
 * `prefers-reduced-motion`).
 */
function ConicRing({
  size,
  gradient,
  spin = false,
}: {
  size: number;
  gradient: string;
  spin?: boolean;
}) {
  const stroke = Math.max(2, size * 0.08);
  const mask = `radial-gradient(farthest-side, transparent calc(100% - ${stroke}px), #000 calc(100% - ${stroke}px))`;
  return (
    <span
      aria-hidden
      style={overlayStyle({
        background: gradient,
        WebkitMask: mask,
        mask,
        ...(spin ? { animation: 'deco-ring-spin 7s linear infinite' } : {}),
      })}
    />
  );
}

/** Halo doux diffus (glow) autour de l'avatar, avec un fin liseré interne. */
function SoftGlow({ size }: { size: number }) {
  return (
    <span
      aria-hidden
      style={overlayStyle({
        boxShadow: `0 0 ${size * 0.16}px ${size * 0.05}px rgba(139, 124, 255, 0.55), inset 0 0 ${size * 0.07}px rgba(160, 150, 255, 0.55)`,
        border: `${Math.max(1.5, size * 0.035)}px solid rgba(168, 158, 255, 0.75)`,
      })}
    />
  );
}

/** Un feuillage de laurier (grappe de feuilles le long d'un arc). */
function LaurelBranch({ side }: { side: 'left' | 'right' }) {
  const flip = side === 'right';
  // Feuilles réparties le long de l'arc inférieur, du bas vers le côté.
  const leaves = [
    { cx: 50, cy: 93, r: 6.5, rot: flip ? 25 : -25 },
    { cx: 38, cy: 90, r: 6, rot: flip ? 45 : -45 },
    { cx: 28, cy: 84, r: 5.5, rot: flip ? 60 : -60 },
    { cx: 20, cy: 75, r: 5, rot: flip ? 78 : -78 },
    { cx: 15, cy: 64, r: 4.2, rot: flip ? 95 : -95 },
  ];
  return (
    <g transform={flip ? 'translate(100,0) scale(-1,1)' : undefined}>
      {leaves.map((l) => (
        <ellipse
          key={`${l.cx}-${l.cy}`}
          cx={l.cx}
          cy={l.cy}
          rx={l.r}
          ry={l.r * 0.5}
          fill="#e6c66e"
          stroke="#c39a3c"
          strokeWidth={0.6}
          transform={`rotate(${l.rot} ${l.cx} ${l.cy})`}
        />
      ))}
    </g>
  );
}

/** Couronne de laurier dorée + fin anneau or. */
function GoldenLaurel({ size }: { size: number }) {
  const stroke = Math.max(2, size * 0.06);
  const mask = `radial-gradient(farthest-side, transparent calc(100% - ${stroke}px), #000 calc(100% - ${stroke}px))`;
  return (
    <>
      <span
        aria-hidden
        style={overlayStyle({
          background:
            'conic-gradient(from 90deg, #f4d98a, #c39a3c, #f4d98a, #c39a3c, #f4d98a)',
          WebkitMask: mask,
          mask,
        })}
      />
      <svg
        aria-hidden
        viewBox="0 0 100 100"
        style={overlayStyle({ overflow: 'visible' })}
      >
        <LaurelBranch side="left" />
        <LaurelBranch side="right" />
        {/* Gemme centrale en bas. */}
        <circle
          cx="50"
          cy="97"
          r="3.4"
          fill="#f4d98a"
          stroke="#c39a3c"
          strokeWidth={0.8}
        />
      </svg>
    </>
  );
}

/** Arc de fleurs de sakura sur le haut de l'avatar (mise à l'échelle SVG). */
function SakuraArc() {
  const petals = [0, 72, 144, 216, 288];
  const blossoms = [
    { cx: 24, cy: 16, s: 0.9 },
    { cx: 50, cy: 7, s: 1.15 },
    { cx: 76, cy: 16, s: 0.9 },
  ];
  return (
    <svg aria-hidden viewBox="0 0 100 100" style={overlayStyle({ overflow: 'visible' })}>
      {blossoms.map((b) => (
        <g key={`${b.cx}-${b.cy}`} transform={`translate(${b.cx} ${b.cy}) scale(${b.s})`}>
          {petals.map((deg) => (
            <ellipse
              key={deg}
              cx="0"
              cy="-6"
              rx="3.1"
              ry="5"
              fill="#ffc0d8"
              stroke="#ff8fb8"
              strokeWidth={0.5}
              transform={`rotate(${deg})`}
            />
          ))}
          <circle cx="0" cy="0" r="1.9" fill="#ffe38f" />
        </g>
      ))}
    </svg>
  );
}

/** Couronne « pixel » posée sur le haut de l'avatar (mise à l'échelle SVG). */
function PixelCrown() {
  // Damier de carrés dorés formant une couronne à trois pointes + gemmes.
  const u = 7; // unité pixel dans le viewBox 100.
  const gold = '#f4c542';
  const goldDark = '#c8952a';
  const cells: Array<{ x: number; y: number; c: string }> = [
    // Base.
    { x: 29, y: 14, c: gold },
    { x: 36, y: 14, c: gold },
    { x: 43, y: 14, c: gold },
    { x: 50, y: 14, c: gold },
    { x: 57, y: 14, c: gold },
    { x: 64, y: 14, c: gold },
    // Pointes.
    { x: 29, y: 7, c: goldDark },
    { x: 50, y: 4, c: goldDark },
    { x: 64, y: 7, c: goldDark },
    { x: 29, y: 0, c: gold },
    { x: 50, y: -3, c: gold },
    { x: 64, y: 0, c: gold },
  ];
  return (
    <svg aria-hidden viewBox="0 0 100 100" style={overlayStyle({ overflow: 'visible' })}>
      <g>
        {cells.map((cell) => (
          <rect
            key={`${cell.x}-${cell.y}`}
            x={cell.x}
            y={cell.y}
            width={u}
            height={u}
            fill={cell.c}
            stroke="#7a5a12"
            strokeWidth={0.4}
          />
        ))}
        {/* Gemmes sur les pointes. */}
        <rect x="31" y="2" width="3" height="3" fill="#4fd0e0" />
        <rect x="52" y="-1" width="3" height="3" fill="#ff5d8f" />
        <rect x="66" y="2" width="3" height="3" fill="#4fd0e0" />
      </g>
    </svg>
  );
}

/**
 * Catalogue des décorations d'avatar (~6). Ids stables, jamais renommés (ils
 * transitent sur le réseau) ; l'ordre fixe l'affichage dans le sélecteur.
 */
export const AVATAR_DECORATIONS: readonly AvatarDecoration[] = [
  {
    id: 'soft_glow',
    label: { fr: 'Halo doux', en: 'Soft glow' },
    render: (size) => <SoftGlow size={size} />,
  },
  {
    id: 'neon_ring',
    label: { fr: 'Anneau néon', en: 'Neon ring' },
    render: (size) => (
      <ConicRing
        size={size}
        gradient="conic-gradient(from 0deg, #ff3db8, #7a5cff, #22d3ee, #7a5cff, #ff3db8)"
      />
    ),
  },
  {
    id: 'aurora_ring',
    label: { fr: 'Anneau aurore', en: 'Aurora ring' },
    render: (size) => (
      <ConicRing
        size={size}
        gradient="conic-gradient(from 0deg, #34d399, #22d3ee, #a78bfa, #f472b6, #34d399)"
        spin
      />
    ),
  },
  {
    id: 'golden_laurel',
    label: { fr: 'Laurier doré', en: 'Golden laurel' },
    render: (size) => <GoldenLaurel size={size} />,
  },
  {
    id: 'sakura_arc',
    label: { fr: 'Arc de sakura', en: 'Sakura arc' },
    render: () => <SakuraArc />,
  },
  {
    id: 'pixel_crown',
    label: { fr: 'Couronne pixel', en: 'Pixel crown' },
    render: () => <PixelCrown />,
  },
];

// ---------------------------------------------------------------------------
// Effets de profil (fond animé de la carte)
// ---------------------------------------------------------------------------

/** Conteneur de calque d'effet : couvre la carte, derrière le contenu. */
function effectLayer(children: ReactNode, extra?: CSSProperties): ReactNode {
  return (
    <span
      aria-hidden
      style={{
        position: 'absolute',
        inset: 0,
        overflow: 'hidden',
        pointerEvents: 'none',
        // Adapte les particules au thème : couleur de texte forte (sombre sur
        // clair, claire sur sombre) via un jeton design system.
        color: 'rgb(var(--color-header))',
        ...extra,
      }}
    >
      {children}
    </span>
  );
}

/** Aurore : nappes de dégradé radial qui dérivent lentement. */
function AuroraEffect(): ReactNode {
  const blobs: CSSProperties[] = [
    {
      top: '-30%',
      left: '-20%',
      width: '80%',
      height: '90%',
      background: 'radial-gradient(circle, rgba(52,211,153,0.45), transparent 65%)',
      animationDelay: '0s',
    },
    {
      top: '-10%',
      left: '40%',
      width: '75%',
      height: '85%',
      background: 'radial-gradient(circle, rgba(167,139,250,0.45), transparent 65%)',
      animationDelay: '-4s',
    },
    {
      top: '20%',
      left: '10%',
      width: '70%',
      height: '80%',
      background: 'radial-gradient(circle, rgba(34,211,238,0.4), transparent 65%)',
      animationDelay: '-8s',
    },
  ];
  return effectLayer(
    blobs.map((style) => (
      <span
        key={`${style.top}-${style.left}`}
        style={{
          position: 'absolute',
          borderRadius: '50%',
          filter: 'blur(14px)',
          animation: 'deco-aurora-drift 12s ease-in-out infinite',
          ...style,
        }}
      />
    )),
  );
}

/** Ciel étoilé : petites étoiles qui scintillent (opacity). */
function StarfieldEffect(): ReactNode {
  const stars = [
    { top: '15%', left: '12%', s: 2.5, d: '0s' },
    { top: '28%', left: '72%', s: 2, d: '-0.6s' },
    { top: '55%', left: '30%', s: 3, d: '-1.2s' },
    { top: '70%', left: '85%', s: 2, d: '-0.3s' },
    { top: '40%', left: '55%', s: 2.5, d: '-1.6s' },
    { top: '80%', left: '18%', s: 2, d: '-0.9s' },
    { top: '18%', left: '45%', s: 2, d: '-2s' },
    { top: '62%', left: '65%', s: 2.5, d: '-1.4s' },
    { top: '35%', left: '20%', s: 1.8, d: '-0.5s' },
    { top: '85%', left: '52%', s: 2, d: '-1.8s' },
    { top: '48%', left: '90%', s: 2.2, d: '-1s' },
    { top: '25%', left: '88%', s: 1.8, d: '-2.2s' },
  ];
  return effectLayer(
    stars.map((star) => (
      <span
        key={`${star.top}-${star.left}`}
        style={{
          position: 'absolute',
          top: star.top,
          left: star.left,
          width: star.s,
          height: star.s,
          borderRadius: '50%',
          background: 'currentColor',
          opacity: 0.7,
          animation: 'deco-twinkle 2.6s ease-in-out infinite',
          animationDelay: star.d,
        }}
      />
    )),
    { opacity: 0.85 },
  );
}

/** Pétales : chute de pétales roses avec léger balancement (transform). */
function PetalsEffect(): ReactNode {
  const petals = [
    { left: '10%', d: '0s', dur: '7s', s: 1 },
    { left: '25%', d: '-2s', dur: '8s', s: 0.8 },
    { left: '40%', d: '-4s', dur: '6.5s', s: 1.1 },
    { left: '55%', d: '-1s', dur: '7.5s', s: 0.9 },
    { left: '70%', d: '-3s', dur: '8.5s', s: 1 },
    { left: '85%', d: '-5s', dur: '7s', s: 0.85 },
    { left: '18%', d: '-6s', dur: '9s', s: 0.7 },
    { left: '62%', d: '-2.5s', dur: '6s', s: 1.05 },
  ];
  return effectLayer(
    petals.map((p) => (
      <span
        key={`${p.left}-${p.d}`}
        style={{
          position: 'absolute',
          top: '-12%',
          left: p.left,
          width: 8 * p.s,
          height: 6 * p.s,
          borderRadius: '60% 0 60% 0',
          background: 'rgba(255, 170, 205, 0.75)',
          animation: `deco-petal-fall ${p.dur} linear infinite`,
          animationDelay: p.d,
        }}
      />
    )),
  );
}

/** Particules : fines lucioles qui montent en s'estompant (transform+opacity). */
function ParticlesEffect(): ReactNode {
  const dots = [
    { left: '8%', d: '0s', dur: '6s', s: 3 },
    { left: '20%', d: '-1.5s', dur: '7s', s: 2 },
    { left: '33%', d: '-3s', dur: '6.5s', s: 2.5 },
    { left: '46%', d: '-0.8s', dur: '7.5s', s: 2 },
    { left: '58%', d: '-2.2s', dur: '6s', s: 3 },
    { left: '70%', d: '-4s', dur: '8s', s: 2 },
    { left: '82%', d: '-1s', dur: '7s', s: 2.5 },
    { left: '92%', d: '-3.5s', dur: '6.5s', s: 2 },
    { left: '14%', d: '-5s', dur: '8s', s: 2 },
    { left: '64%', d: '-2.8s', dur: '6.8s', s: 2.5 },
  ];
  return effectLayer(
    dots.map((dot) => (
      <span
        key={`${dot.left}-${dot.d}`}
        style={{
          position: 'absolute',
          bottom: '-8%',
          left: dot.left,
          width: dot.s,
          height: dot.s,
          borderRadius: '50%',
          background: 'currentColor',
          opacity: 0.5,
          animation: `deco-particle-float ${dot.dur} ease-in infinite`,
          animationDelay: dot.d,
        }}
      />
    )),
    { opacity: 0.7 },
  );
}

/**
 * Catalogue des effets de profil (~4). Ids stables, jamais renommés (filaires).
 */
export const PROFILE_EFFECTS: readonly ProfileEffect[] = [
  {
    id: 'aurora',
    label: { fr: 'Aurore boréale', en: 'Aurora' },
    render: () => <AuroraEffect />,
  },
  {
    id: 'starfield',
    label: { fr: 'Ciel étoilé', en: 'Starfield' },
    render: () => <StarfieldEffect />,
  },
  {
    id: 'falling_petals',
    label: { fr: 'Pétales', en: 'Falling petals' },
    render: () => <PetalsEffect />,
  },
  {
    id: 'floating_particles',
    label: { fr: 'Lucioles', en: 'Floating particles' },
    render: () => <ParticlesEffect />,
  },
];

// ---------------------------------------------------------------------------
// Résolution d'id (rendu gracieux : `undefined` si inconnu/retiré)
// ---------------------------------------------------------------------------

const DECORATION_BY_ID = new Map(AVATAR_DECORATIONS.map((d) => [d.id, d]));
const EFFECT_BY_ID = new Map(PROFILE_EFFECTS.map((e) => [e.id, e]));

/** Décoration d'avatar pour un id, ou `undefined` (id inconnu/retiré/`null`). */
export function decorationById(
  id: string | null | undefined,
): AvatarDecoration | undefined {
  if (id === null || id === undefined) return undefined;
  return DECORATION_BY_ID.get(id);
}

/** Effet de profil pour un id, ou `undefined` (id inconnu/retiré/`null`). */
export function effectById(id: string | null | undefined): ProfileEffect | undefined {
  if (id === null || id === undefined) return undefined;
  return EFFECT_BY_ID.get(id);
}
