import type { CSSProperties } from 'react';
import { ProfileEffectLayer as Effect } from './decorationLayers';
import '../styles/profile-ambient-effects.css';
import '../styles/profile-essential-motion.css';

const STAR_STYLES = [
  { top: '13%', left: '7%' },
  { top: '21%', left: '33%', animationDelay: '-0.8s' },
  { top: '10%', left: '67%', animationDelay: '-1.7s' },
  { top: '31%', left: '87%', animationDelay: '-0.3s' },
  { top: '45%', left: '17%', animationDelay: '-2.1s' },
  { top: '53%', left: '49%', animationDelay: '-1.2s' },
  { top: '61%', left: '75%', animationDelay: '-2.5s' },
  { top: '78%', left: '24%', animationDelay: '-0.5s' },
  { top: '85%', left: '58%', animationDelay: '-1.5s' },
  { top: '73%', left: '92%', animationDelay: '-2.8s' },
  { top: '37%', left: '61%', animationDelay: '-1s' },
  { top: '92%', left: '8%', animationDelay: '-2.2s' },
] satisfies readonly CSSProperties[];

const PETAL_STYLES = [
  { left: '4%', animationDelay: '-1s' },
  { left: '18%', animationDelay: '-5.2s', animationDuration: '9.2s' },
  { left: '32%', animationDelay: '-2.8s', animationDuration: '7.4s' },
  { left: '47%', animationDelay: '-6.4s', animationDuration: '9.8s' },
  { left: '61%', animationDelay: '-0.3s', animationDuration: '8.6s' },
  { left: '74%', animationDelay: '-4.1s', animationDuration: '7.8s' },
  { left: '86%', animationDelay: '-7.2s', animationDuration: '10s' },
  { left: '94%', animationDelay: '-2s', animationDuration: '8.2s' },
] satisfies readonly CSSProperties[];

const MOTE_STYLES = [
  { left: '6%', animationDelay: '-1s' },
  { left: '17%', animationDelay: '-4.4s', animationDuration: '8.2s' },
  { left: '29%', animationDelay: '-2.5s', animationDuration: '6.4s' },
  { left: '41%', animationDelay: '-5.8s', animationDuration: '8.8s' },
  { left: '53%', animationDelay: '-0.2s' },
  { left: '64%', animationDelay: '-3.7s', animationDuration: '7.6s' },
  { left: '74%', animationDelay: '-6.2s', animationDuration: '9.1s' },
  { left: '84%', animationDelay: '-2s', animationDuration: '6.8s' },
  { left: '91%', animationDelay: '-4.9s', animationDuration: '8.4s' },
  { left: '97%', animationDelay: '-0.8s', animationDuration: '7.2s' },
] satisfies readonly CSSProperties[];

function AuroraEffect() {
  return (
    <Effect className="profile-effect--aurora">
      <span className="profile-effect__mesh profile-effect__mesh--one" />
      <span className="profile-effect__mesh profile-effect__mesh--two" />
      <span className="profile-effect__mesh profile-effect__mesh--three" />
      <span className="profile-effect__grain" />
    </Effect>
  );
}

function StarfieldEffect() {
  return (
    <Effect className="profile-effect--starfield">
      <svg
        viewBox="0 0 300 220"
        preserveAspectRatio="none"
        className="profile-effect__constellation"
      >
        <path d="M18 67 72 38 119 82 177 49 231 76 282 31" />
        <path d="M42 172 96 132 151 166 217 119 275 157" />
      </svg>
      {STAR_STYLES.map((style, index) => (
        <span key={index} className="profile-effect__star" style={style} />
      ))}
    </Effect>
  );
}

function PetalsEffect() {
  return (
    <Effect className="profile-effect--petals">
      {PETAL_STYLES.map((style, index) => (
        <span key={index} className="profile-effect__petal-track" style={style}>
          <i className="profile-effect__petal" />
        </span>
      ))}
      <span className="profile-effect__rose-light" />
    </Effect>
  );
}

function EmbersEffect() {
  return (
    <Effect className="profile-effect--embers">
      <span className="profile-effect__ember-arc profile-effect__ember-arc--one" />
      <span className="profile-effect__ember-arc profile-effect__ember-arc--two" />
      {MOTE_STYLES.map((style, index) => (
        <span key={index} className="profile-effect__mote" style={style} />
      ))}
    </Effect>
  );
}

export const DECORATION_RENDERERS = {
  aurora: AuroraEffect,
  starfield: StarfieldEffect,
  falling_petals: PetalsEffect,
  floating_particles: EmbersEffect,
} as const;
