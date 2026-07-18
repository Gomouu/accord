import type { ReactNode } from 'react';
import type { AvatarDecoration, ProfileEffect, ProfileFrame } from './decorations';

function Decoration({ className, children }: { className: string; children: ReactNode }) {
  return (
    <span
      aria-hidden
      data-testid="avatar-decoration"
      className={`avatar-decoration nature-decoration ${className}`}
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
      className={`profile-effect figurative-effect ${className}`}
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
      className={`profile-frame figurative-frame ${className}`}
    >
      <span className="figurative-frame__inner" />
      {children}
    </span>
  );
}

function FivePetal({ x, y, scale = 1 }: { x: number; y: number; scale?: number }) {
  return (
    <g
      className="nature-flower-anchor"
      transform={`translate(${x} ${y}) scale(${scale})`}
    >
      <g className="nature-flower">
        {[0, 72, 144, 216, 288].map((angle) => (
          <ellipse key={angle} cy="-5.5" rx="4.2" ry="7" transform={`rotate(${angle})`} />
        ))}
        <circle className="nature-flower__core" r="2.6" />
      </g>
    </g>
  );
}

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

function MangaImpact() {
  return (
    <Decoration className="avatar-decoration--manga-impact">
      <svg viewBox="0 0 120 120" className="avatar-decoration__svg">
        <path
          className="manga-burst"
          d="M60 2 67 18 78 5 80 23 98 13 91 31 114 29 98 43 118 51 98 60 117 72 94 76 105 96 84 91 82 115 68 98 58 118 51 98 35 113 37 91 14 99 25 78 3 73 23 61 2 51 24 44 7 29 29 30 22 10 42 22 45 3Z"
        />
        <path className="manga-bubble" d="M74 8h35v23H92l-9 10 2-10H74Z" />
        <path className="manga-ink" d="m14 19 7-4 4 7-8 5ZM101 91l10 4-5 9-9-6Z" />
      </svg>
    </Decoration>
  );
}

function ShojoRibbon() {
  return (
    <Decoration className="avatar-decoration--shojo-ribbon">
      <svg viewBox="0 0 120 120" className="avatar-decoration__svg">
        <path
          className="shojo-ribbon"
          d="M9 82C25 104 80 111 109 83l-8-8c-25 21-65 21-84 0Z"
        />
        <path className="shojo-ribbon-tail" d="m17 76-10 28 21-9M102 76l11 27-22-8" />
        <path className="shojo-lace" d="M17 27c21-22 67-25 88 0" />
        <FivePetal x={21} y={27} scale={0.9} />
        <FivePetal x={101} y={28} scale={0.9} />
        <FivePetal x={60} y={11} scale={0.68} />
      </svg>
      {[1, 2, 3, 4].map((index) => (
        <i key={index} className={`shojo-spark shojo-spark--${index}`} />
      ))}
    </Decoration>
  );
}

function ShonenPanels() {
  return (
    <Decoration className="avatar-decoration--shonen-panels">
      <svg viewBox="0 0 120 120" className="avatar-decoration__svg">
        <path className="shonen-panel" d="M5 10h38L37 36H2Z" />
        <path className="shonen-panel shonen-panel--right" d="M79 4h36l3 36-32-7Z" />
        <path className="shonen-panel shonen-panel--bottom" d="m8 87 37-8 2 36H13Z" />
        <path
          className="shonen-speed"
          d="m2 58 26-5M5 68l27-2m86-11-28 4m27 9-25-1M58 2l1 27m9-26-3 25M57 118l2-27m10 25-4-24"
        />
        <circle className="shonen-dot" cx="105" cy="99" r="11" />
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

function MangaPanelsEffect() {
  return (
    <Effect className="profile-effect--manga-panels">
      <span className="figurative-effect__veil" />
      <span className="effect-manga-panel effect-manga-panel--one" />
      <span className="effect-manga-panel effect-manga-panel--two" />
      {Array.from({ length: 10 }, (_, index) => (
        <i key={index} className={`effect-speedline effect-speedline--${index + 1}`} />
      ))}
    </Effect>
  );
}

function ShojoRosesEffect() {
  return (
    <Effect className="profile-effect--shojo-roses">
      <span className="figurative-effect__veil" />
      <span className="effect-shojo-ribbon" />
      {Array.from({ length: 9 }, (_, index) => (
        <i
          key={index}
          className={`effect-shojo-spark effect-shojo-spark--${index + 1}`}
        />
      ))}
    </Effect>
  );
}

function ShonenImpactEffect() {
  return (
    <Effect className="profile-effect--shonen-impact">
      <span className="effect-shonen-burst" />
      {Array.from({ length: 14 }, (_, index) => (
        <i
          key={index}
          className={`effect-shonen-line effect-shonen-line--${index + 1}`}
        />
      ))}
      <span className="effect-shonen-ink" />
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

function MangaPageFrame() {
  return (
    <Frame className="profile-frame--manga-page">
      <svg
        viewBox="0 0 400 300"
        preserveAspectRatio="none"
        className="figurative-frame__full"
      >
        <path
          className="frame-manga-panel"
          d="M5 8h120l-18 58H3ZM278 4h117v80l-105-18ZM3 220l108-18 18 93H4ZM286 218l109-17v94H274Z"
        />
        <path
          className="frame-manga-line"
          d="M0 104 56 91M0 121l69-9m331-15-63 14m63 8-72 9M155 0l15 57M178 0l7 52m43-52-6 52m25-52-14 56M152 300l17-55m14 55 5-50m42 50-7-51m28 51-16-56"
        />
        <path className="frame-manga-bubble" d="M321 24h61v38h-27l-17 15 4-15h-21Z" />
      </svg>
    </Frame>
  );
}

function ShojoLaceFrame() {
  return (
    <Frame className="profile-frame--shojo-lace">
      <svg
        viewBox="0 0 400 130"
        preserveAspectRatio="none"
        className="figurative-frame__top"
      >
        <path className="frame-shojo-ribbon" d="M5 83C80 11 320 11 395 83" />
        <path
          className="frame-shojo-lace"
          d="M26 76c25-12 28-39 55-39s30 27 56 27 33-43 63-43 37 43 63 43 29-27 56-27 30 27 55 39"
        />
        {[54, 128, 200, 272, 346].map((x, index) => (
          <g key={x} transform={`translate(${x} ${index % 2 === 0 ? 42 : 25})`}>
            <g className={`frame-rose frame-rose--${index + 1}`}>
              <circle r="15" />
              <path d="M-11 0C-4-11 6-12 12-2 5 10-5 12-11 0Z" />
            </g>
          </g>
        ))}
      </svg>
      {Array.from({ length: 8 }, (_, index) => (
        <i key={index} className={`frame-shojo-spark frame-shojo-spark--${index + 1}`} />
      ))}
    </Frame>
  );
}

export const NATURE_MANGA_AVATAR_DECORATIONS = [
  {
    id: 'camellia_wreath',
    label: { fr: 'Camélias', en: 'Camellias' },
    render: () => <CamelliaWreath />,
  },
  {
    id: 'wisteria_drape',
    label: { fr: 'Glycines', en: 'Wisteria' },
    render: () => <WisteriaDrape />,
  },
  {
    id: 'lotus_koi',
    label: { fr: 'Lotus et koï', en: 'Lotus & Koi' },
    render: () => <LotusKoi />,
  },
  {
    id: 'manga_impact',
    label: { fr: 'Impact manga', en: 'Manga Impact' },
    render: () => <MangaImpact />,
  },
  {
    id: 'shojo_ribbon',
    label: { fr: 'Ruban shōjo', en: 'Shōjo Ribbon' },
    render: () => <ShojoRibbon />,
  },
  {
    id: 'shonen_panels',
    label: { fr: 'Cases shōnen', en: 'Shōnen Panels' },
    render: () => <ShonenPanels />,
  },
] satisfies readonly AvatarDecoration[];

export const NATURE_MANGA_PROFILE_EFFECTS = [
  {
    id: 'sakura_garden',
    label: { fr: 'Jardin de sakura', en: 'Sakura Garden' },
    render: () => <FallingPetalsEffect />,
  },
  {
    id: 'wisteria_fireflies',
    label: { fr: 'Nuit de glycines', en: 'Wisteria Night' },
    render: () => <WisteriaFirefliesEffect />,
  },
  {
    id: 'lotus_ripples',
    label: { fr: 'Bassin de lotus', en: 'Lotus Pond' },
    render: () => <LotusRipplesEffect />,
  },
  {
    id: 'manga_panels',
    label: { fr: 'Planche manga', en: 'Manga Page' },
    render: () => <MangaPanelsEffect />,
  },
  {
    id: 'shojo_roses',
    label: { fr: 'Roses shōjo', en: 'Shōjo Roses' },
    render: () => <ShojoRosesEffect />,
  },
  {
    id: 'shonen_impact',
    label: { fr: 'Impact shōnen', en: 'Shōnen Impact' },
    render: () => <ShonenImpactEffect />,
  },
] satisfies readonly ProfileEffect[];

export const NATURE_MANGA_PROFILE_FRAMES = [
  {
    id: 'sakura_gate',
    label: { fr: 'Portail sakura', en: 'Sakura Gate' },
    render: () => <SakuraGateFrame />,
  },
  {
    id: 'wisteria_arch',
    label: { fr: 'Arche de glycines', en: 'Wisteria Arch' },
    render: () => <WisteriaArchFrame />,
  },
  {
    id: 'lotus_lacquer',
    label: { fr: 'Laque aux lotus', en: 'Lotus Lacquer' },
    render: () => <LotusLacquerFrame />,
  },
  {
    id: 'manga_page',
    label: { fr: 'Planche encrée', en: 'Ink Page' },
    render: () => <MangaPageFrame />,
  },
  {
    id: 'shojo_lace',
    label: { fr: 'Dentelle shōjo', en: 'Shōjo Lace' },
    render: () => <ShojoLaceFrame />,
  },
] satisfies readonly ProfileFrame[];
