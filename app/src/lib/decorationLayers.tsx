import type { ReactNode } from 'react';

type LayerProps = {
  className: string;
  children: ReactNode;
};

export function AvatarDecorationLayer({ className, children }: LayerProps) {
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

export function ProfileEffectLayer({ className, children }: LayerProps) {
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

export function ProfileFrameLayer({ className, children }: LayerProps) {
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

export function OrnamentalProfileFrameLayer({ className, children }: LayerProps) {
  return (
    <ProfileFrameLayer className={`more-frame ${className}`}>
      <span className="more-frame__aura" />
      <span className="more-frame__rail more-frame__rail--left" />
      <span className="more-frame__rail more-frame__rail--right" />
      {children}
      {Array.from({ length: 8 }, (_, index) => (
        <i key={index} className={`more-frame__mote more-frame__mote--${index + 1}`} />
      ))}
    </ProfileFrameLayer>
  );
}

export function FigurativeAvatarDecorationLayer({ className, children }: LayerProps) {
  return (
    <AvatarDecorationLayer className={`nature-decoration ${className}`}>
      {children}
    </AvatarDecorationLayer>
  );
}

export function FigurativeProfileEffectLayer({ className, children }: LayerProps) {
  return (
    <ProfileEffectLayer className={`figurative-effect ${className}`}>
      {children}
    </ProfileEffectLayer>
  );
}

export function FigurativeProfileFrameLayer({ className, children }: LayerProps) {
  return (
    <ProfileFrameLayer className={`figurative-frame ${className}`}>
      <span className="figurative-frame__inner" />
      {children}
    </ProfileFrameLayer>
  );
}

export function FivePetal({ x, y, scale = 1 }: { x: number; y: number; scale?: number }) {
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
