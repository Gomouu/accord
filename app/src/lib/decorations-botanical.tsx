import {
  FigurativeAvatarDecorationLayer as Decoration,
  FigurativeProfileEffectLayer as Effect,
  FigurativeProfileFrameLayer as Frame,
  FivePetal,
} from './decorationLayers';
import '../styles/profile-figurative-avatars.css';
import '../styles/profile-figurative-effects.css';
import '../styles/profile-figurative-frames.css';
import '../styles/profile-figurative-motion.css';

function CamelliaWreath() {
  return (
    <Decoration className="avatar-decoration--camellia-wreath">
      <svg viewBox="0 0 120 120" className="avatar-decoration__svg">
        <path className="nature-branch" d="M10 87C15 36 43 10 83 12c17 1 26 11 28 28" />
        <path
          className="nature-branch nature-branch--lower"
          d="M16 98c23 15 62 17 90-5"
        />
        <path className="nature-leaf" d="M20 64c-12-7-17 3-12 13 11 1 16-4 12-13Z" />
        <path className="nature-leaf" d="M86 16c4-13 16-11 19-1-6 9-13 9-19 1Z" />
        <path className="nature-leaf" d="M101 81c13-5 18 6 11 15-11-1-15-7-11-15Z" />
        <FivePetal x={20} y={43} scale={0.9} />
        <FivePetal x={43} y={18} scale={1.15} />
        <FivePetal x={78} y={14} scale={0.82} />
        <FivePetal x={103} y={43} scale={1.02} />
        <FivePetal x={95} y={91} scale={0.76} />
      </svg>
      {[1, 2, 3].map((index) => (
        <i
          key={index}
          className={`nature-falling-petal nature-falling-petal--${index}`}
        />
      ))}
    </Decoration>
  );
}

function WisteriaDrape() {
  const clusters = [19, 38, 61, 84, 103];
  return (
    <Decoration className="avatar-decoration--wisteria-drape">
      <svg viewBox="0 0 120 120" className="avatar-decoration__svg">
        <path className="wisteria-branch" d="M4 21C34 4 77 6 116 22" />
        {clusters.map((x, cluster) => (
          <g key={x} transform={`translate(${x} ${16 + (cluster % 2) * 3})`}>
            <g className={`wisteria-cluster wisteria-cluster--${cluster + 1}`}>
              {Array.from({ length: 5 }, (_, index) => (
                <ellipse
                  key={index}
                  cx={(index % 2) * 4 - 2}
                  cy={index * 7}
                  rx={5 - index * 0.45}
                  ry="5.5"
                />
              ))}
            </g>
          </g>
        ))}
      </svg>
      {[1, 2, 3, 4].map((index) => (
        <i key={index} className={`wisteria-firefly wisteria-firefly--${index}`} />
      ))}
    </Decoration>
  );
}

function LotusKoi() {
  return (
    <Decoration className="avatar-decoration--lotus-koi">
      <svg viewBox="0 0 120 120" className="avatar-decoration__svg">
        <ellipse className="lotus-water-ring" cx="60" cy="62" rx="53" ry="45" />
        <g className="lotus-koi lotus-koi--one">
          <path d="M22 50c11-10 24-4 28 7-10 8-23 7-28-7Z" />
          <path d="m22 50-11-8 2 14Z" />
          <circle cx="43" cy="53" r="1.2" />
        </g>
        <g className="lotus-koi lotus-koi--two">
          <path d="M98 75c-10 11-23 6-28-4 9-9 22-9 28 4Z" />
          <path d="m98 75 12 7-3-14Z" />
          <circle cx="77" cy="73" r="1.2" />
        </g>
        <g transform="translate(28 91)">
          <g className="lotus-bloom">
            <path d="M0 0C-8-13-15-10-13 1-8 8-3 8 0 0Z" />
            <path d="M0 0C8-13 15-10 13 1 8 8 3 8 0 0Z" />
            <path d="M0 2C-4-17 4-17 0 2Z" />
          </g>
        </g>
        <g transform="translate(89 29)">
          <g className="lotus-bloom lotus-bloom--small">
            <path d="M0 0C-8-13-15-10-13 1-8 8-3 8 0 0Z" />
            <path d="M0 0C8-13 15-10 13 1 8 8 3 8 0 0Z" />
            <path d="M0 2C-4-17 4-17 0 2Z" />
          </g>
        </g>
      </svg>
    </Decoration>
  );
}

function FallingPetalsEffect() {
  return (
    <Effect className="profile-effect--sakura-garden">
      <span className="figurative-effect__veil" />
      {Array.from({ length: 14 }, (_, index) => (
        <i key={index} className={`figurative-petal figurative-petal--${index + 1}`} />
      ))}
    </Effect>
  );
}

function WisteriaFirefliesEffect() {
  return (
    <Effect className="profile-effect--wisteria-fireflies">
      <span className="figurative-effect__veil" />
      <span className="effect-wisteria effect-wisteria--left" />
      <span className="effect-wisteria effect-wisteria--right" />
      {Array.from({ length: 10 }, (_, index) => (
        <i key={index} className={`effect-firefly effect-firefly--${index + 1}`} />
      ))}
    </Effect>
  );
}

function LotusRipplesEffect() {
  return (
    <Effect className="profile-effect--lotus-ripples">
      <span className="figurative-effect__veil" />
      {[1, 2, 3, 4].map((index) => (
        <i key={index} className={`effect-ripple effect-ripple--${index}`} />
      ))}
      <i className="effect-koi effect-koi--one" />
      <i className="effect-koi effect-koi--two" />
    </Effect>
  );
}

function SakuraGateFrame() {
  return (
    <Frame className="profile-frame--sakura-gate">
      <svg
        viewBox="0 0 400 120"
        preserveAspectRatio="none"
        className="figurative-frame__top"
      >
        <path
          className="frame-gate"
          d="M20 72h360M48 58h304L332 42H68ZM74 70v48m252-48v48"
        />
        <path
          className="frame-branch"
          d="M5 94C62 38 119 20 188 28M395 92c-50-50-99-64-153-60"
        />
        {[42, 96, 150, 252, 306, 360].map((x, index) => (
          <circle
            key={x}
            className={`frame-blossom frame-blossom--${index + 1}`}
            cx={x}
            cy={index % 2 === 0 ? 47 : 31}
            r="7"
          />
        ))}
      </svg>
      {Array.from({ length: 7 }, (_, index) => (
        <i key={index} className={`frame-petal frame-petal--${index + 1}`} />
      ))}
    </Frame>
  );
}

function WisteriaArchFrame() {
  return (
    <Frame className="profile-frame--wisteria-arch">
      <svg
        viewBox="0 0 400 130"
        preserveAspectRatio="none"
        className="figurative-frame__top"
      >
        <path className="frame-wisteria-vine" d="M5 80C76 8 323 8 395 80" />
        {[28, 72, 118, 164, 212, 260, 308, 354].map((x, index) => (
          <path
            key={x}
            className={`frame-wisteria-bloom frame-wisteria-bloom--${index + 1}`}
            d={`M${x} ${35 + (index % 2) * 8}c-10 18-7 40 0 58 8-18 10-40 0-58Z`}
          />
        ))}
      </svg>
      {Array.from({ length: 8 }, (_, index) => (
        <i key={index} className={`frame-firefly frame-firefly--${index + 1}`} />
      ))}
    </Frame>
  );
}

function LotusLacquerFrame() {
  return (
    <Frame className="profile-frame--lotus-lacquer">
      <svg
        viewBox="0 0 400 100"
        preserveAspectRatio="none"
        className="figurative-frame__bottom"
      >
        <path
          className="frame-water"
          d="M4 68c52-23 107 7 153-8 29-10 57-10 86 0 46 15 101-15 153 8"
        />
        {[48, 116, 284, 352].map((x, index) => (
          <g key={x} transform={`translate(${x} 58)`}>
            <g className={`frame-lotus frame-lotus--${index + 1}`}>
              <path d="M0 0c-16-25-29-14-20 7C-12 17-5 13 0 0Z" />
              <path d="M0 0c16-25 29-14 20 7C12 17 5 13 0 0Z" />
              <path d="M0 2C-9-31 9-31 0 2Z" />
            </g>
          </g>
        ))}
        <path
          className="frame-koi"
          d="M172 71c19-14 39-8 48 7-18 13-38 11-48-7Zm0 0-18-12 3 22Z"
        />
      </svg>
    </Frame>
  );
}

export const DECORATION_RENDERERS = {
  camellia_wreath: CamelliaWreath,
  wisteria_drape: WisteriaDrape,
  lotus_koi: LotusKoi,
  sakura_garden: FallingPetalsEffect,
  wisteria_fireflies: WisteriaFirefliesEffect,
  lotus_ripples: LotusRipplesEffect,
  sakura_gate: SakuraGateFrame,
  wisteria_arch: WisteriaArchFrame,
  lotus_lacquer: LotusLacquerFrame,
} as const;
