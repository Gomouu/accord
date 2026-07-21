import { ProfileFrameLayer as CoreFrame } from './decorationLayers';

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
    <CoreFrame className="profile-frame--lumen-bloom">
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
    </CoreFrame>
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
    <CoreFrame className="profile-frame--crystal-crown">
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
    </CoreFrame>
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
    <CoreFrame className="profile-frame--celestial-wings">
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
    </CoreFrame>
  );
}

function TechCircuitFrame() {
  return (
    <CoreFrame className="profile-frame--neon-circuit">
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
    </CoreFrame>
  );
}

export const DECORATION_RENDERERS = {
  lumen_bloom: LumenBloomFrame,
  crystal_crown: CrystalCrownFrame,
  celestial_wings: CelestialWingsFrame,
  neon_circuit: TechCircuitFrame,
} as const;
