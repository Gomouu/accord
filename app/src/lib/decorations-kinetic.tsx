import {
  AvatarDecorationLayer as Decoration,
  ProfileEffectLayer as Effect,
} from './decorationLayers';
import '../styles/profile-kinetic-avatars.css';
import '../styles/profile-kinetic-effects.css';
import '../styles/profile-kinetic-motion.css';

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

export const DECORATION_RENDERERS = {
  storm_halo: StormHalo,
  galaxy_swirl: GalaxySwirl,
  clockwork: Clockwork,
  butterfly_waltz: ButterflyWaltz,
  rune_circle: RuneCircle,
  phoenix_plume: PhoenixPlume,
  thunderstorm: Thunderstorm,
  lava_flow: LavaFlow,
  code_rain: CodeRain,
  light_beams: LightBeams,
  confetti: Confetti,
  drifting_hearts: DriftingHearts,
} as const;
