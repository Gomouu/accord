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
import '../styles/profile-shonen-effects.css';

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

export const DECORATION_RENDERERS = {
  manga_impact: MangaImpact,
  shojo_ribbon: ShojoRibbon,
  shonen_panels: ShonenPanels,
  manga_panels: MangaPanelsEffect,
  shojo_roses: ShojoRosesEffect,
  shonen_impact: ShonenImpactEffect,
  manga_page: MangaPageFrame,
  shojo_lace: ShojoLaceFrame,
} as const;
