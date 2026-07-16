import type { ReactNode } from 'react';
import { EXTRA_AVATAR_DECORATIONS, EXTRA_PROFILE_EFFECTS } from './decorations-extra';

export interface DecorationLabel {
  fr: string;
  en: string;
}

export const DECORATION_UI_TEXT = {
  decorationTitle: { fr: "Décoration d'avatar", en: 'Avatar decoration' },
  decorationHint: {
    fr: 'Une signature visuelle visible partout où ton avatar apparaît.',
    en: 'A visual signature shown everywhere your avatar appears.',
  },
  effectTitle: { fr: 'Effet de profil', en: 'Profile effect' },
  effectHint: {
    fr: 'Une atmosphère animée à l’intérieur de ta carte de profil.',
    en: 'An animated atmosphere inside your profile card.',
  },
  frameTitle: { fr: 'Cadre de profil', en: 'Profile frame' },
  frameHint: {
    fr: 'Une composition animée qui habille tout le contour de ta carte.',
    en: 'An animated composition that dresses the full outline of your card.',
  },
  preview: { fr: 'Aperçu en direct', en: 'Live preview' },
  signature: { fr: 'Signature Accord', en: 'Accord signature' },
  none: { fr: 'Aucune', en: 'None' },
  saved: { fr: 'Personnalisation enregistrée', en: 'Personalization saved' },
} as const;

export interface AvatarDecoration {
  id: string;
  label: DecorationLabel;
  render: (size: number) => ReactNode;
}

export interface ProfileEffect {
  id: string;
  label: DecorationLabel;
  render: () => ReactNode;
}

export interface ProfileFrame {
  id: string;
  label: DecorationLabel;
  render: () => ReactNode;
}

function DecorationLayer({
  className,
  children,
}: {
  className: string;
  children: ReactNode;
}) {
  return (
    <span
      aria-hidden
      data-testid="avatar-decoration"
      className={`avatar-decoration ${className}`}
    >
      {children}
    </span>
  );
}

function PrismaticHalo() {
  return (
    <DecorationLayer className="avatar-decoration--halo">
      <span className="avatar-decoration__aura" />
      <span className="avatar-decoration__ring" />
      <span className="avatar-decoration__glint avatar-decoration__glint--north" />
      <span className="avatar-decoration__glint avatar-decoration__glint--south" />
    </DecorationLayer>
  );
}

function NeonEclipse() {
  return (
    <DecorationLayer className="avatar-decoration--eclipse">
      <span className="avatar-decoration__ring" />
      <span className="avatar-decoration__arc" />
      <span className="avatar-decoration__node avatar-decoration__node--one" />
      <span className="avatar-decoration__node avatar-decoration__node--two" />
    </DecorationLayer>
  );
}

function AuroraOrbit() {
  return (
    <DecorationLayer className="avatar-decoration--orbit">
      <span className="avatar-decoration__orbit avatar-decoration__orbit--outer" />
      <span className="avatar-decoration__orbit avatar-decoration__orbit--inner" />
      <span className="avatar-decoration__comet" />
    </DecorationLayer>
  );
}

const LAUREL_LEAVES = [
  { x: 26, y: 91, angle: -28, scale: 1 },
  { x: 18, y: 81, angle: -46, scale: 0.92 },
  { x: 13, y: 69, angle: -62, scale: 0.84 },
  { x: 11, y: 56, angle: -78, scale: 0.74 },
] as const;

function LaurelBranch({ mirrored = false }: { mirrored?: boolean }) {
  return (
    <g transform={mirrored ? 'translate(120 0) scale(-1 1)' : undefined}>
      <path
        d="M 28 98 C 12 87, 7 70, 12 48"
        fill="none"
        stroke="url(#laurel-stem)"
        strokeWidth="2"
        strokeLinecap="round"
      />
      {LAUREL_LEAVES.map((leaf) => (
        <ellipse
          key={leaf.y}
          cx={leaf.x}
          cy={leaf.y}
          rx={7 * leaf.scale}
          ry={3.2 * leaf.scale}
          fill="url(#laurel-leaf)"
          transform={`rotate(${leaf.angle} ${leaf.x} ${leaf.y})`}
        />
      ))}
    </g>
  );
}

function SolarLaurel() {
  return (
    <DecorationLayer className="avatar-decoration--laurel">
      <svg viewBox="0 0 120 120" className="avatar-decoration__svg">
        <defs>
          <linearGradient id="laurel-stem" x1="0" y1="0" x2="1" y2="1">
            <stop stopColor="#fff1a8" />
            <stop offset="1" stopColor="#b66f18" />
          </linearGradient>
          <linearGradient id="laurel-leaf" x1="0" y1="0" x2="1" y2="1">
            <stop stopColor="#fff3b0" />
            <stop offset="0.45" stopColor="#e2ab45" />
            <stop offset="1" stopColor="#9d5b12" />
          </linearGradient>
          <radialGradient id="laurel-gem">
            <stop stopColor="#fffbd5" />
            <stop offset="0.45" stopColor="#ffcf61" />
            <stop offset="1" stopColor="#b35a16" />
          </radialGradient>
        </defs>
        <circle
          cx="60"
          cy="60"
          r="49"
          fill="none"
          stroke="url(#laurel-stem)"
          strokeWidth="2"
        />
        <LaurelBranch />
        <LaurelBranch mirrored />
        <path d="M52 104 60 98 68 104 60 112Z" fill="url(#laurel-gem)" />
      </svg>
    </DecorationLayer>
  );
}

const BLOSSOMS = [
  { x: 25, y: 24, scale: 0.82 },
  { x: 44, y: 12, scale: 1 },
  { x: 66, y: 10, scale: 0.76 },
  { x: 88, y: 21, scale: 0.92 },
] as const;

function SakuraCrest() {
  return (
    <DecorationLayer className="avatar-decoration--sakura">
      <svg viewBox="0 0 120 120" className="avatar-decoration__svg">
        <defs>
          <linearGradient id="sakura-branch" x1="0" y1="0" x2="1" y2="1">
            <stop stopColor="#7b3d54" />
            <stop offset="1" stopColor="#d48aa5" />
          </linearGradient>
          <linearGradient id="sakura-petal" x1="0" y1="0" x2="1" y2="1">
            <stop stopColor="#fff4fa" />
            <stop offset="1" stopColor="#ff86b6" />
          </linearGradient>
        </defs>
        <path
          d="M8 39 C35 7, 72 2, 111 34"
          fill="none"
          stroke="url(#sakura-branch)"
          strokeWidth="2.4"
          strokeLinecap="round"
        />
        {BLOSSOMS.map((blossom) => (
          <g
            key={blossom.x}
            transform={`translate(${blossom.x} ${blossom.y}) scale(${blossom.scale})`}
          >
            {[0, 72, 144, 216, 288].map((angle) => (
              <ellipse
                key={angle}
                cy="-5"
                rx="3.4"
                ry="5.5"
                fill="url(#sakura-petal)"
                transform={`rotate(${angle})`}
              />
            ))}
            <circle r="1.8" fill="#ffd86b" />
          </g>
        ))}
        <path
          d="M98 30 C108 39, 109 49, 106 58"
          fill="none"
          stroke="url(#sakura-branch)"
          strokeWidth="1.5"
        />
        <path
          d="M104 58 C110 56, 114 59, 114 64 C109 65, 105 63, 104 58Z"
          fill="url(#sakura-petal)"
        />
      </svg>
    </DecorationLayer>
  );
}

function ArcadeCrown() {
  return (
    <DecorationLayer className="avatar-decoration--crown">
      <svg viewBox="0 0 120 120" className="avatar-decoration__svg">
        <defs>
          <linearGradient id="crown-gold" x1="0" y1="0" x2="0" y2="1">
            <stop stopColor="#fff2a1" />
            <stop offset="0.5" stopColor="#f0b938" />
            <stop offset="1" stopColor="#9f5d12" />
          </linearGradient>
        </defs>
        <path
          d="M32 25 40 8 52 22 60 3 68 22 80 8 88 25 84 38H36Z"
          fill="url(#crown-gold)"
          stroke="#6f4010"
          strokeWidth="1.5"
          strokeLinejoin="round"
        />
        <path d="M38 31H82" stroke="#fff0a0" strokeWidth="2" strokeLinecap="round" />
        <rect
          x="56"
          y="25"
          width="8"
          height="8"
          rx="1.5"
          fill="#5eead4"
          stroke="#164e63"
          strokeWidth="1"
          transform="rotate(45 60 29)"
        />
        <path d="M17 82 24 89 17 96 10 89Z" fill="#61e7ff" />
        <path d="M103 82 110 89 103 96 96 89Z" fill="#ff6dac" />
      </svg>
    </DecorationLayer>
  );
}

export const AVATAR_DECORATIONS: readonly AvatarDecoration[] = [
  {
    id: 'soft_glow',
    label: { fr: 'Prisme', en: 'Prism' },
    render: () => <PrismaticHalo />,
  },
  {
    id: 'neon_ring',
    label: { fr: 'Éclipse', en: 'Eclipse' },
    render: () => <NeonEclipse />,
  },
  {
    id: 'aurora_ring',
    label: { fr: 'Orbite', en: 'Orbit' },
    render: () => <AuroraOrbit />,
  },
  {
    id: 'golden_laurel',
    label: { fr: 'Solaire', en: 'Solar' },
    render: () => <SolarLaurel />,
  },
  {
    id: 'sakura_arc',
    label: { fr: 'Sakura', en: 'Sakura' },
    render: () => <SakuraCrest />,
  },
  {
    id: 'pixel_crown',
    label: { fr: 'Arcade', en: 'Arcade' },
    render: () => <ArcadeCrown />,
  },
  ...EXTRA_AVATAR_DECORATIONS,
];

function EffectLayer({
  className,
  children,
}: {
  className: string;
  children: ReactNode;
}) {
  return (
    <span
      aria-hidden
      data-testid="profile-effect"
      className={`profile-effect ${className}`}
    >
      {children}
    </span>
  );
}

function AuroraEffect() {
  return (
    <EffectLayer className="profile-effect--aurora">
      <span className="profile-effect__mesh profile-effect__mesh--one" />
      <span className="profile-effect__mesh profile-effect__mesh--two" />
      <span className="profile-effect__mesh profile-effect__mesh--three" />
      <span className="profile-effect__grain" />
    </EffectLayer>
  );
}

function StarfieldEffect() {
  return (
    <EffectLayer className="profile-effect--starfield">
      <svg
        viewBox="0 0 300 220"
        preserveAspectRatio="none"
        className="profile-effect__constellation"
      >
        <path d="M18 67 72 38 119 82 177 49 231 76 282 31" />
        <path d="M42 172 96 132 151 166 217 119 275 157" />
      </svg>
      {Array.from({ length: 12 }, (_, index) => (
        <span
          key={index}
          className={`profile-effect__star profile-effect__star--${index + 1}`}
        />
      ))}
    </EffectLayer>
  );
}

function PetalsEffect() {
  return (
    <EffectLayer className="profile-effect--petals">
      {Array.from({ length: 8 }, (_, index) => (
        <span
          key={index}
          className={`profile-effect__petal-track profile-effect__petal-track--${index + 1}`}
        >
          <i className="profile-effect__petal" />
        </span>
      ))}
      <span className="profile-effect__rose-light" />
    </EffectLayer>
  );
}

function EmbersEffect() {
  return (
    <EffectLayer className="profile-effect--embers">
      <span className="profile-effect__ember-arc profile-effect__ember-arc--one" />
      <span className="profile-effect__ember-arc profile-effect__ember-arc--two" />
      {Array.from({ length: 10 }, (_, index) => (
        <span
          key={index}
          className={`profile-effect__mote profile-effect__mote--${index + 1}`}
        />
      ))}
    </EffectLayer>
  );
}

function FrameLayer({ className, children }: { className: string; children: ReactNode }) {
  return (
    <span
      aria-hidden
      data-testid="profile-frame"
      className={`profile-frame ${className}`}
    >
      {children}
    </span>
  );
}

const LUMEN_BLOOMS = [
  { x: 42, y: 77, scale: 0.72, rotate: -14 },
  { x: 83, y: 48, scale: 0.9, rotate: 8 },
  { x: 132, y: 31, scale: 0.66, rotate: -10 },
  { x: 200, y: 24, scale: 1.08, rotate: 0 },
  { x: 268, y: 31, scale: 0.66, rotate: 10 },
  { x: 317, y: 48, scale: 0.9, rotate: -8 },
  { x: 358, y: 77, scale: 0.72, rotate: 14 },
] as const;

function LumenBloom({
  x,
  y,
  scale,
  rotate,
}: {
  x: number;
  y: number;
  scale: number;
  rotate: number;
}) {
  return (
    <g transform={`translate(${x} ${y}) rotate(${rotate}) scale(${scale})`}>
      {[0, 60, 120, 180, 240, 300].map((angle) => (
        <ellipse
          key={angle}
          className="lumen-frame__petal"
          cy="-9"
          rx="4.8"
          ry="10"
          transform={`rotate(${angle})`}
        />
      ))}
      <circle className="lumen-frame__core" r="4.5" />
    </g>
  );
}

function LumenBloomFrame() {
  return (
    <FrameLayer className="profile-frame--lumen-bloom">
      <span className="lumen-frame__aura" />
      <span className="lumen-frame__rail lumen-frame__rail--left" />
      <span className="lumen-frame__rail lumen-frame__rail--right" />
      <svg
        viewBox="0 0 400 112"
        preserveAspectRatio="none"
        className="lumen-frame__crown lumen-frame__crown--top"
      >
        <path
          className="lumen-frame__vine lumen-frame__vine--echo"
          d="M8 94C54 93 67 53 106 48c42-6 54-31 94-31s52 25 94 31c39 5 52 45 98 46"
        />
        <path
          className="lumen-frame__vine"
          d="M8 94C54 93 67 53 106 48c42-6 54-31 94-31s52 25 94 31c39 5 52 45 98 46"
        />
        <path
          className="lumen-frame__filament"
          d="M26 96c20-18 26-40 36-69M374 96c-20-18-26-40-36-69M142 38c13-14 24-22 31-32M258 38c-13-14-24-22-31-32"
        />
        {LUMEN_BLOOMS.map((bloom) => (
          <LumenBloom key={bloom.x} {...bloom} />
        ))}
        <path className="lumen-frame__gem" d="m200 2 9 15-9 14-9-14Z" />
        <path
          className="lumen-frame__gem lumen-frame__gem--soft"
          d="m24 79 6 9-6 9-6-9Z"
        />
        <path
          className="lumen-frame__gem lumen-frame__gem--soft"
          d="m376 79 6 9-6 9-6-9Z"
        />
      </svg>
      <svg
        viewBox="0 0 400 92"
        preserveAspectRatio="none"
        className="lumen-frame__crown lumen-frame__crown--bottom"
      >
        <path
          className="lumen-frame__vine lumen-frame__vine--echo"
          d="M6 14c50 0 63 39 110 39 34 0 49 26 84 26s50-26 84-26c47 0 60-39 110-39"
        />
        <path
          className="lumen-frame__vine"
          d="M6 14c50 0 63 39 110 39 34 0 49 26 84 26s50-26 84-26c47 0 60-39 110-39"
        />
        <path
          className="lumen-frame__filament"
          d="M42 19c10 27 30 44 55 53M358 19c-10 27-30 44-55 53M171 69l29 20 29-20"
        />
        <g transform="translate(74 39) scale(.68)">
          <LumenBloom x={0} y={0} scale={1} rotate={-10} />
        </g>
        <g transform="translate(326 39) scale(.68)">
          <LumenBloom x={0} y={0} scale={1} rotate={10} />
        </g>
        <path className="lumen-frame__gem" d="m200 63 10 14-10 13-10-13Z" />
      </svg>
      {Array.from({ length: 8 }, (_, index) => (
        <i key={index} className={`lumen-frame__mote lumen-frame__mote--${index + 1}`} />
      ))}
    </FrameLayer>
  );
}

const CRYSTAL_SHARDS = [
  { x: 12, width: 24, height: 44, tilt: -9, tone: 'violet' },
  { x: 34, width: 30, height: 66, tilt: -5, tone: 'blue' },
  { x: 62, width: 22, height: 48, tilt: 8, tone: 'pink' },
  { x: 95, width: 28, height: 72, tilt: -4, tone: 'violet' },
  { x: 128, width: 20, height: 45, tilt: 7, tone: 'blue' },
  { x: 252, width: 20, height: 45, tilt: -7, tone: 'blue' },
  { x: 277, width: 28, height: 72, tilt: 4, tone: 'violet' },
  { x: 316, width: 22, height: 48, tilt: -8, tone: 'pink' },
  { x: 336, width: 30, height: 66, tilt: 5, tone: 'blue' },
  { x: 364, width: 24, height: 44, tilt: 9, tone: 'violet' },
] as const;

function CrystalCrown({ bottom = false }: { bottom?: boolean }) {
  return (
    <svg
      viewBox="0 0 400 104"
      preserveAspectRatio="none"
      className={`crystal-frame__crown crystal-frame__crown--${bottom ? 'bottom' : 'top'}`}
    >
      <g transform={bottom ? 'translate(0 104) scale(1 -1)' : undefined}>
        <path className="crystal-frame__ridge" d="M5 94H395" />
        {CRYSTAL_SHARDS.map((shard) => (
          <g
            key={`${shard.x}-${shard.height}`}
            transform={`rotate(${shard.tilt} ${shard.x + shard.width / 2} 94)`}
          >
            <path
              className={`crystal-frame__shard crystal-frame__shard--${shard.tone}`}
              d={`M${shard.x} 94 ${shard.x + shard.width * 0.48} ${94 - shard.height} ${shard.x + shard.width} 94Z`}
            />
            <path
              className="crystal-frame__facet"
              d={`M${shard.x + shard.width * 0.48} ${94 - shard.height} ${shard.x + shard.width * 0.58} 94 ${shard.x + shard.width} 94Z`}
            />
          </g>
        ))}
        <path
          className="crystal-frame__arc"
          d="M4 94C62 67 115 88 154 76c34-11 58-11 92 0 39 12 92-9 150 18"
        />
      </g>
    </svg>
  );
}

function CrystalCrownFrame() {
  return (
    <FrameLayer className="profile-frame--crystal-crown">
      <span className="crystal-frame__aura" />
      <span className="crystal-frame__rail crystal-frame__rail--left" />
      <span className="crystal-frame__rail crystal-frame__rail--right" />
      <CrystalCrown />
      <CrystalCrown bottom />
      {Array.from({ length: 7 }, (_, index) => (
        <i
          key={index}
          className={`crystal-frame__spark crystal-frame__spark--${index + 1}`}
        />
      ))}
    </FrameLayer>
  );
}

const CELESTIAL_MOTHS = [
  { x: 42, y: 72, scale: 0.72, rotate: -16 },
  { x: 126, y: 42, scale: 0.94, rotate: -8 },
  { x: 200, y: 25, scale: 1.08, rotate: 0 },
  { x: 274, y: 42, scale: 0.94, rotate: 8 },
  { x: 358, y: 72, scale: 0.72, rotate: 16 },
] as const;

function CelestialMoth({
  x,
  y,
  scale,
  rotate,
  index,
}: {
  x: number;
  y: number;
  scale: number;
  rotate: number;
  index: number;
}) {
  return (
    <g
      className={`celestial-frame__moth celestial-frame__moth--${index + 1}`}
      transform={`translate(${x} ${y}) rotate(${rotate}) scale(${scale})`}
    >
      <path
        className="celestial-frame__wing celestial-frame__wing--left"
        d="M-3 1C-12-22-35-25-39-8c13 1 24 8 34 20-13-4-25 0-30 12 15 5 28 1 35-9Z"
      />
      <path
        className="celestial-frame__wing celestial-frame__wing--right"
        d="M3 1C12-22 35-25 39-8 26-7 15 8 5 12c13-4 25 0 30 12-15 5-28 1-35-9Z"
      />
      <ellipse className="celestial-frame__body" rx="3.3" ry="14" />
      <path
        className="celestial-frame__antenna"
        d="M-1-12C-7-20-11-19-13-16M1-12c6-8 10-7 12-4"
      />
    </g>
  );
}

function CelestialWingsFrame() {
  return (
    <FrameLayer className="profile-frame--celestial-wings">
      <span className="celestial-frame__aura" />
      <span className="celestial-frame__rail celestial-frame__rail--left" />
      <span className="celestial-frame__rail celestial-frame__rail--right" />
      <svg
        viewBox="0 0 400 112"
        preserveAspectRatio="none"
        className="celestial-frame__crown celestial-frame__crown--top"
      >
        <path
          className="celestial-frame__orbit"
          d="M4 94C56 91 71 58 109 54c38-4 52-38 91-38s53 34 91 38c38 4 53 37 105 40"
        />
        <path
          className="celestial-frame__orbit celestial-frame__orbit--echo"
          d="M25 98C82 61 121 79 160 47 178 32 188 17 200 3c12 14 22 29 40 44 39 32 78 14 135 51"
        />
        {CELESTIAL_MOTHS.map((moth, index) => (
          <CelestialMoth key={moth.x} {...moth} index={index} />
        ))}
      </svg>
      <svg
        viewBox="0 0 400 82"
        preserveAspectRatio="none"
        className="celestial-frame__crown celestial-frame__crown--bottom"
      >
        <path
          className="celestial-frame__orbit"
          d="M4 9c60 2 84 38 136 38 24 0 40 20 60 28 20-8 36-28 60-28 52 0 76-36 136-38"
        />
        <path
          className="celestial-frame__moon"
          d="M187 59c13 15 28 9 31-6 3 20-12 31-27 22-6-4-8-10-4-16Z"
        />
      </svg>
      {Array.from({ length: 10 }, (_, index) => (
        <i
          key={index}
          className={`celestial-frame__star celestial-frame__star--${index + 1}`}
        />
      ))}
    </FrameLayer>
  );
}

function TechCircuitFrame() {
  return (
    <FrameLayer className="profile-frame--neon-circuit">
      <span className="tech-frame__aura" />
      <span className="tech-frame__corner tech-frame__corner--tl" />
      <span className="tech-frame__corner tech-frame__corner--tr" />
      <span className="tech-frame__corner tech-frame__corner--bl" />
      <span className="tech-frame__corner tech-frame__corner--br" />
      <span className="tech-frame__rail tech-frame__rail--left" />
      <span className="tech-frame__rail tech-frame__rail--right" />
      <svg
        viewBox="0 0 400 82"
        preserveAspectRatio="none"
        className="tech-frame__circuit tech-frame__circuit--top"
      >
        <path
          className="tech-frame__trace tech-frame__trace--wide"
          d="M4 68h54l18-24h60l17-22h94l17 22h60l18 24h54"
        />
        <path
          className="tech-frame__trace"
          d="M8 76h81l13-18h67l12-17h38l12 17h67l13 18h81"
        />
        <path
          className="tech-frame__trace tech-frame__trace--pulse"
          d="M64 68h54l19-27h126l19 27h54"
        />
        {[64, 102, 137, 181, 219, 263, 298, 336].map((x) => (
          <circle
            key={x}
            className="tech-frame__node"
            cx={x}
            cy={x === 181 || x === 219 ? 41 : x === 137 || x === 263 ? 44 : 68}
            r="4"
          />
        ))}
      </svg>
      <svg
        viewBox="0 0 400 72"
        preserveAspectRatio="none"
        className="tech-frame__circuit tech-frame__circuit--bottom"
      >
        <path
          className="tech-frame__trace tech-frame__trace--wide"
          d="M4 9h74l18 22h63l17 24h48l17-24h63l18-22h74"
        />
        <path
          className="tech-frame__trace tech-frame__trace--pulse"
          d="M36 18h80l18 25h132l18-25h80"
        />
        <path className="tech-frame__core" d="m200 43 13 13-13 13-13-13Z" />
      </svg>
      {Array.from({ length: 8 }, (_, index) => (
        <i key={index} className={`tech-frame__pixel tech-frame__pixel--${index + 1}`} />
      ))}
    </FrameLayer>
  );
}

export const PROFILE_FRAMES: readonly ProfileFrame[] = [
  {
    id: 'lumen_bloom',
    label: { fr: 'Jardin de lumière', en: 'Lumen Garden' },
    render: () => <LumenBloomFrame />,
  },
  {
    id: 'crystal_crown',
    label: { fr: 'Couronne de cristaux', en: 'Crystal Crown' },
    render: () => <CrystalCrownFrame />,
  },
  {
    id: 'celestial_wings',
    label: { fr: 'Papillons célestes', en: 'Celestial Wings' },
    render: () => <CelestialWingsFrame />,
  },
  {
    id: 'neon_circuit',
    label: { fr: 'Circuit néon', en: 'Neon Circuit' },
    render: () => <TechCircuitFrame />,
  },
];

export const PROFILE_EFFECTS: readonly ProfileEffect[] = [
  { id: 'aurora', label: { fr: 'Aurore', en: 'Aurora' }, render: () => <AuroraEffect /> },
  {
    id: 'starfield',
    label: { fr: 'Constellation', en: 'Constellation' },
    render: () => <StarfieldEffect />,
  },
  {
    id: 'falling_petals',
    label: { fr: 'Pétales', en: 'Petals' },
    render: () => <PetalsEffect />,
  },
  {
    id: 'floating_particles',
    label: { fr: 'Braises', en: 'Embers' },
    render: () => <EmbersEffect />,
  },
  ...EXTRA_PROFILE_EFFECTS,
];

const DECORATION_BY_ID = new Map(AVATAR_DECORATIONS.map((item) => [item.id, item]));
const EFFECT_BY_ID = new Map(PROFILE_EFFECTS.map((item) => [item.id, item]));
const FRAME_BY_ID = new Map(PROFILE_FRAMES.map((item) => [item.id, item]));

export function decorationById(
  id: string | null | undefined,
): AvatarDecoration | undefined {
  return id == null ? undefined : DECORATION_BY_ID.get(id);
}

export function effectById(id: string | null | undefined): ProfileEffect | undefined {
  return id == null ? undefined : EFFECT_BY_ID.get(id);
}

export function frameById(id: string | null | undefined): ProfileFrame | undefined {
  return id == null ? undefined : FRAME_BY_ID.get(id);
}
