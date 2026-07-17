/**
 * Troisième vague du catalogue de personnalisation : décorations d'avatar
 * franchement animées, effets d'ambiance et nouveaux cadres de carte.
 * Styles associés : styles/profile-personalization-more.css.
 */

import type { ReactNode } from 'react';
import type { AvatarDecoration, ProfileEffect, ProfileFrame } from './decorations';

function Decoration({ className, children }: { className: string; children: ReactNode }) {
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

function Effect({ className, children }: { className: string; children: ReactNode }) {
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

function Frame({ className, children }: { className: string; children: ReactNode }) {
  return (
    <span
      aria-hidden
      data-testid="profile-frame"
      className={`profile-frame more-frame ${className}`}
    >
      <span className="more-frame__aura" />
      <span className="more-frame__rail more-frame__rail--left" />
      <span className="more-frame__rail more-frame__rail--right" />
      {children}
      {Array.from({ length: 8 }, (_, index) => (
        <i key={index} className={`more-frame__mote more-frame__mote--${index + 1}`} />
      ))}
    </span>
  );
}

// ─── Décorations d'avatar ───

function StormHalo() {
  return (
    <Decoration className="avatar-decoration--storm-halo">
      <svg viewBox="0 0 120 120" className="avatar-decoration__svg">
        <circle className="more-storm-ring" cx="60" cy="60" r="49" />
        <path
          className="more-bolt more-bolt--one"
          d="m28 18 8 10-6 2 9 12-13-8 6-3-9-9Z"
        />
        <path
          className="more-bolt more-bolt--two"
          d="m96 34 5 9-5 1 6 10-10-7 5-2-6-8Z"
        />
        <path
          className="more-bolt more-bolt--three"
          d="m20 78 6 8-4 1 5 9-9-6 4-2-5-7Z"
        />
      </svg>
    </Decoration>
  );
}

function GalaxySwirl() {
  return (
    <Decoration className="avatar-decoration--galaxy">
      <svg viewBox="0 0 120 120" className="avatar-decoration__svg">
        <g className="more-galaxy-orbit more-galaxy-orbit--slow">
          <ellipse cx="60" cy="60" rx="54" ry="22" transform="rotate(-24 60 60)" />
          <circle className="more-galaxy-star" cx="12" cy="43" r="2.6" />
        </g>
        <g className="more-galaxy-orbit more-galaxy-orbit--fast">
          <ellipse cx="60" cy="60" rx="52" ry="18" transform="rotate(28 60 60)" />
          <circle className="more-galaxy-star" cx="108" cy="79" r="2.2" />
        </g>
        {[
          [22, 22],
          [98, 24],
          [104, 96],
          [18, 98],
        ].map(([x, y], index) => (
          <circle
            key={`${x}-${y}`}
            className={`more-galaxy-twinkle more-galaxy-twinkle--${index + 1}`}
            cx={x}
            cy={y}
            r="1.7"
          />
        ))}
      </svg>
    </Decoration>
  );
}

function Clockwork() {
  return (
    <Decoration className="avatar-decoration--clockwork">
      <svg viewBox="0 0 120 120" className="avatar-decoration__svg">
        <circle className="more-gear more-gear--teeth" cx="60" cy="60" r="51" />
        <circle className="more-gear more-gear--rim" cx="60" cy="60" r="46" />
        <g className="more-gear-small">
          <circle className="more-gear more-gear--small-teeth" cx="98" cy="22" r="11" />
          <circle className="more-gear more-gear--axle" cx="98" cy="22" r="3.4" />
        </g>
      </svg>
    </Decoration>
  );
}

function ButterflyWaltz() {
  return (
    <Decoration className="avatar-decoration--butterflies">
      <svg viewBox="0 0 120 120" className="avatar-decoration__svg">
        <circle className="more-flutter-path" cx="60" cy="60" r="52" />
        <g className="more-flutter-orbit">
          <g className="more-butterfly" transform="translate(60 8)">
            <path
              className="more-butterfly__wing"
              d="M0 0C-9-10-20-6-16 3c3 6 10 5 16 1"
            />
            <path
              className="more-butterfly__wing more-butterfly__wing--right"
              d="M0 0C9-10 20-6 16 3c-3 6-10 5-16 1"
            />
            <ellipse className="more-butterfly__body" cx="0" cy="2" rx="1.6" ry="6" />
          </g>
        </g>
        <g className="more-flutter-orbit more-flutter-orbit--reverse">
          <g
            className="more-butterfly more-butterfly--small"
            transform="translate(60 112)"
          >
            <path className="more-butterfly__wing" d="M0 0C-7-8-16-5-13 2c2 5 8 4 13 1" />
            <path
              className="more-butterfly__wing more-butterfly__wing--right"
              d="M0 0C7-8 16-5 13 2c-2 5-8 4-13 1"
            />
            <ellipse className="more-butterfly__body" cx="0" cy="1.5" rx="1.3" ry="4.8" />
          </g>
        </g>
      </svg>
    </Decoration>
  );
}

function RuneCircle() {
  return (
    <Decoration className="avatar-decoration--runes">
      <svg viewBox="0 0 120 120" className="avatar-decoration__svg">
        <circle className="more-rune-ring" cx="60" cy="60" r="50" />
        {[
          { d: 'M60 2v12M55 8l10 3', angle: 0 },
          { d: 'M60 2v12m-5-3 10-4', angle: 72 },
          { d: 'M60 3v10l5-5m-10 0 5 5', angle: 144 },
          { d: 'M60 2v12m-4-9h8', angle: 216 },
          { d: 'M56 4l8 9m0-9-8 9', angle: 288 },
        ].map((rune, index) => (
          <path
            key={rune.angle}
            className={`more-rune more-rune--${index + 1}`}
            d={rune.d}
            transform={`rotate(${rune.angle} 60 60)`}
          />
        ))}
      </svg>
    </Decoration>
  );
}

function PhoenixPlume() {
  return (
    <Decoration className="avatar-decoration--phoenix">
      <svg viewBox="0 0 120 120" className="avatar-decoration__svg">
        <path
          className="more-plume more-plume--one"
          d="M32 104c-6-12-2-22 6-30-1 10 3 14 8 18-4 8-8 12-14 12Z"
        />
        <path
          className="more-plume more-plume--two"
          d="M60 112c-8-14-4-26 4-36 0 12 5 17 11 22-4 9-8 14-15 14Z"
        />
        <path
          className="more-plume more-plume--three"
          d="M88 104c6-12 2-22-6-30 1 10-3 14-8 18 4 8 8 12 14 12Z"
        />
        {[
          [30, 96],
          [52, 104],
          [68, 106],
          [86, 98],
          [60, 100],
        ].map(([x, y], index) => (
          <circle
            key={`${x}-${y}`}
            className={`more-ember more-ember--${index + 1}`}
            cx={x}
            cy={y}
            r="2"
          />
        ))}
      </svg>
    </Decoration>
  );
}

// ─── Effets de profil ───

function Thunderstorm() {
  return (
    <Effect className="profile-effect--thunderstorm">
      <span className="more-storm-cloud more-storm-cloud--one" />
      <span className="more-storm-cloud more-storm-cloud--two" />
      <span className="more-storm-flash" />
      {Array.from({ length: 8 }, (_, index) => (
        <i key={index} className={`more-storm-drop more-storm-drop--${index + 1}`} />
      ))}
    </Effect>
  );
}

function LavaFlow() {
  return (
    <Effect className="profile-effect--lava-flow">
      <span className="more-lava-blob more-lava-blob--one" />
      <span className="more-lava-blob more-lava-blob--two" />
      <span className="more-lava-blob more-lava-blob--three" />
      <span className="more-lava-glow" />
    </Effect>
  );
}

function CodeRain() {
  return (
    <Effect className="profile-effect--code-rain">
      {Array.from({ length: 9 }, (_, index) => (
        <i key={index} className={`more-code more-code--${index + 1}`} />
      ))}
      <span className="more-code-haze" />
    </Effect>
  );
}

function LightBeams() {
  return (
    <Effect className="profile-effect--light-beams">
      <span className="more-beam-wheel" />
      <span className="more-beam-wheel more-beam-wheel--reverse" />
      <span className="more-beam-core" />
    </Effect>
  );
}

function Confetti() {
  return (
    <Effect className="profile-effect--confetti">
      {Array.from({ length: 12 }, (_, index) => (
        <i key={index} className={`more-confetti more-confetti--${index + 1}`} />
      ))}
    </Effect>
  );
}

function DriftingHearts() {
  return (
    <Effect className="profile-effect--hearts">
      {Array.from({ length: 8 }, (_, index) => (
        <i key={index} className={`more-heart more-heart--${index + 1}`} />
      ))}
      <span className="more-heart-glow" />
    </Effect>
  );
}

// ─── Cadres de profil ───

function RoyalGiltFrame() {
  return (
    <Frame className="profile-frame--royal-gilt">
      <svg
        viewBox="0 0 400 112"
        preserveAspectRatio="none"
        className="more-frame__crown more-frame__crown--top"
      >
        <path
          className="more-gilt__scroll"
          d="M8 96C56 92 74 56 118 50c34-5 48-28 82-28s48 23 82 28c44 6 62 42 110 46"
        />
        <path
          className="more-gilt__scroll more-gilt__scroll--echo"
          d="M24 100c40-8 52-44 92-52 28-6 42-22 84-22s56 16 84 22c40 8 52 44 92 52"
        />
        <path className="more-gilt__gem" d="m200 6 11 16-11 15-11-15Z" />
        <path
          className="more-gilt__gem more-gilt__gem--soft"
          d="m130 32 7 10-7 10-7-10Z"
        />
        <path
          className="more-gilt__gem more-gilt__gem--soft"
          d="m270 32 7 10-7 10-7-10Z"
        />
        <path
          className="more-gilt__curl"
          d="M52 84c-10-14 2-26 14-22-8 4-10 12-4 18M348 84c10-14-2-26-14-22 8 4 10 12 4 18"
        />
      </svg>
      <svg
        viewBox="0 0 400 82"
        preserveAspectRatio="none"
        className="more-frame__crown more-frame__crown--bottom"
      >
        <path
          className="more-gilt__scroll"
          d="M8 12c48 4 66 40 110 46 34 5 48 18 82 18s48-13 82-18c44-6 62-42 110-46"
        />
        <path className="more-gilt__gem" d="m200 46 10 14-10 13-10-13Z" />
        <path
          className="more-gilt__curl"
          d="M96 46c-8 12 2 22 12 19-7-4-8-10-3-15M304 46c8 12-2 22-12 19 7-4 8-10 3-15"
        />
      </svg>
    </Frame>
  );
}

function FrostVeilFrame() {
  return (
    <Frame className="profile-frame--frost-veil">
      <svg
        viewBox="0 0 400 112"
        preserveAspectRatio="none"
        className="more-frame__crown more-frame__crown--top"
      >
        <path
          className="more-frost__branch"
          d="M4 92C64 84 96 44 140 36c40-8 50-24 60-32 10 8 20 24 60 32 44 8 76 48 136 56"
        />
        {[
          { x: 78, y: 64, s: 1 },
          { x: 140, y: 36, s: 0.8 },
          { x: 200, y: 16, s: 1.15 },
          { x: 260, y: 36, s: 0.8 },
          { x: 322, y: 64, s: 1 },
        ].map((flake, index) => (
          <g
            key={flake.x}
            className={`more-frost__flake more-frost__flake--${index + 1}`}
            transform={`translate(${flake.x} ${flake.y}) scale(${flake.s})`}
          >
            <path d="M0-11V11M-9.5-5.5 9.5 5.5M-9.5 5.5 9.5-5.5" />
            <path d="M0-11-3-7m3-4 3 4M0 11l-3-4m3 4 3-4" />
          </g>
        ))}
        <path
          className="more-frost__shard"
          d="m36 92 8-22 8 20-7 14Zm320 0-8-22-8 20 7 14Z"
        />
      </svg>
      <svg
        viewBox="0 0 400 82"
        preserveAspectRatio="none"
        className="more-frame__crown more-frame__crown--bottom"
      >
        <path
          className="more-frost__branch"
          d="M4 16c60 8 92 44 136 52 28 5 40 8 60 8s32-3 60-8c44-8 76-44 136-52"
        />
        <g
          className="more-frost__flake more-frost__flake--6"
          transform="translate(200 58)"
        >
          <path d="M0-10V10M-8.5-5 8.5 5M-8.5 5 8.5-5" />
        </g>
        <path
          className="more-frost__shard"
          d="m96 30 7 18-7 12-7-12Zm208 0-7 18 7 12 7-12Z"
        />
      </svg>
    </Frame>
  );
}

function EmberforgeFrame() {
  return (
    <Frame className="profile-frame--emberforge">
      <svg
        viewBox="0 0 400 112"
        preserveAspectRatio="none"
        className="more-frame__crown more-frame__crown--top"
      >
        <path
          className="more-forge__arc more-forge__arc--melt"
          d="M4 94C60 88 84 52 126 44c40-8 48-26 74-26s34 18 74 26c42 8 66 44 122 50"
        />
        <path
          className="more-forge__arc"
          d="M18 98C70 90 94 58 132 50c38-8 44-22 68-22s30 14 68 22c38 8 62 40 114 48"
        />
        <path
          className="more-forge__drip"
          d="M120 52c0 8-3 10-3 16 0 4 6 4 6 0 0-6-3-8-3-16Zm160 0c0 8 3 10 3 16 0 4-6 4-6 0 0-6 3-8 3-16ZM200 22c0 9-3 11-3 18 0 4 6 4 6 0 0-7-3-9-3-18Z"
        />
      </svg>
      <svg
        viewBox="0 0 400 82"
        preserveAspectRatio="none"
        className="more-frame__crown more-frame__crown--bottom"
      >
        <path
          className="more-forge__arc more-forge__arc--melt"
          d="M4 12c56 6 80 42 122 50 40 8 48 16 74 16s34-8 74-16c42-8 66-44 122-50"
        />
        <path className="more-forge__core" d="m200 42 12 14-12 14-12-14Z" />
      </svg>
    </Frame>
  );
}

function WildIvyFrame() {
  return (
    <Frame className="profile-frame--wild-ivy">
      <svg
        viewBox="0 0 400 112"
        preserveAspectRatio="none"
        className="more-frame__crown more-frame__crown--top"
      >
        <path
          className="more-ivy__vine"
          d="M6 96C58 92 76 54 116 46c40-8 52-28 84-28s44 20 84 28c40 8 58 46 110 50"
        />
        {[
          { x: 74, y: 68, a: -32 },
          { x: 124, y: 44, a: -18 },
          { x: 200, y: 20, a: 0 },
          { x: 276, y: 44, a: 18 },
          { x: 326, y: 68, a: 32 },
        ].map((leaf, index) => (
          <g
            key={leaf.x}
            className={`more-ivy__sprig more-ivy__sprig--${index + 1}`}
            transform={`translate(${leaf.x} ${leaf.y}) rotate(${leaf.a})`}
          >
            <path className="more-ivy__leaf" d="M0 0C-10-4-12-14-6-18c6-4 12 2 10 12Z" />
            <path
              className="more-ivy__leaf more-ivy__leaf--right"
              d="M0 0c10-4 12-14 6-18-6-4-12 2-10 12Z"
            />
            <circle className="more-ivy__bud" cy="-4" r="2.2" />
          </g>
        ))}
      </svg>
      <svg
        viewBox="0 0 400 82"
        preserveAspectRatio="none"
        className="more-frame__crown more-frame__crown--bottom"
      >
        <path
          className="more-ivy__vine"
          d="M6 14c52 4 70 42 110 50 40 8 52 14 84 14s44-6 84-14c40-8 58-46 110-50"
        />
        <g
          className="more-ivy__sprig more-ivy__sprig--6"
          transform="translate(200 60) rotate(180)"
        >
          <path className="more-ivy__leaf" d="M0 0C-9-4-11-12-5-16c5-3 10 2 8 11Z" />
          <path
            className="more-ivy__leaf more-ivy__leaf--right"
            d="M0 0c9-4 11-12 5-16-5-3-10 2-8 11Z"
          />
          <circle className="more-ivy__bud" cy="-3.5" r="2" />
        </g>
      </svg>
    </Frame>
  );
}

export const MORE_AVATAR_DECORATIONS = [
  {
    id: 'storm_halo',
    label: { fr: 'Orage', en: 'Storm' },
    render: () => <StormHalo />,
  },
  {
    id: 'galaxy_swirl',
    label: { fr: 'Galaxie', en: 'Galaxy' },
    render: () => <GalaxySwirl />,
  },
  {
    id: 'clockwork',
    label: { fr: 'Rouages', en: 'Clockwork' },
    render: () => <Clockwork />,
  },
  {
    id: 'butterfly_waltz',
    label: { fr: 'Valse de papillons', en: 'Butterfly Waltz' },
    render: () => <ButterflyWaltz />,
  },
  {
    id: 'rune_circle',
    label: { fr: 'Cercle runique', en: 'Rune Circle' },
    render: () => <RuneCircle />,
  },
  {
    id: 'phoenix_plume',
    label: { fr: 'Phénix', en: 'Phoenix' },
    render: () => <PhoenixPlume />,
  },
] as const satisfies readonly AvatarDecoration[];

export const MORE_PROFILE_EFFECTS = [
  {
    id: 'thunderstorm',
    label: { fr: "Ciel d'orage", en: 'Thunderstorm' },
    render: () => <Thunderstorm />,
  },
  {
    id: 'lava_flow',
    label: { fr: 'Lave', en: 'Lava Flow' },
    render: () => <LavaFlow />,
  },
  {
    id: 'code_rain',
    label: { fr: 'Pluie de code', en: 'Code Rain' },
    render: () => <CodeRain />,
  },
  {
    id: 'light_beams',
    label: { fr: 'Faisceaux', en: 'Light Beams' },
    render: () => <LightBeams />,
  },
  {
    id: 'confetti',
    label: { fr: 'Confettis', en: 'Confetti' },
    render: () => <Confetti />,
  },
  {
    id: 'drifting_hearts',
    label: { fr: 'Cœurs flottants', en: 'Drifting Hearts' },
    render: () => <DriftingHearts />,
  },
] as const satisfies readonly ProfileEffect[];

export const MORE_PROFILE_FRAMES = [
  {
    id: 'royal_gilt',
    label: { fr: 'Or royal', en: 'Royal Gilt' },
    render: () => <RoyalGiltFrame />,
  },
  {
    id: 'frost_veil',
    label: { fr: 'Voile de givre', en: 'Frost Veil' },
    render: () => <FrostVeilFrame />,
  },
  {
    id: 'emberforge',
    label: { fr: 'Forge ardente', en: 'Emberforge' },
    render: () => <EmberforgeFrame />,
  },
  {
    id: 'wild_ivy',
    label: { fr: 'Lierre sauvage', en: 'Wild Ivy' },
    render: () => <WildIvyFrame />,
  },
] as const satisfies readonly ProfileFrame[];
