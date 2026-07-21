import { OrnamentalProfileFrameLayer as Frame } from './decorationLayers';
import '../styles/profile-kinetic-motion.css';
import '../styles/profile-ornamental-frames.css';

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

export const DECORATION_RENDERERS = {
  royal_gilt: RoyalGiltFrame,
  frost_veil: FrostVeilFrame,
  emberforge: EmberforgeFrame,
  wild_ivy: WildIvyFrame,
} as const;
