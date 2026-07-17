import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import {
  AVATAR_DECORATIONS,
  PROFILE_EFFECTS,
  PROFILE_FRAMES,
  decorationById,
  effectById,
  frameById,
} from './decorations';

describe('catalogue de personnalisation', () => {
  it('expose des identifiants uniques et compatibles avec le protocole', () => {
    const ids = [
      ...AVATAR_DECORATIONS.map((item) => item.id),
      ...PROFILE_EFFECTS.map((item) => item.id),
      ...PROFILE_FRAMES.map((item) => item.id),
    ];

    expect(AVATAR_DECORATIONS).toHaveLength(20);
    expect(PROFILE_EFFECTS).toHaveLength(18);
    expect(PROFILE_FRAMES).toHaveLength(8);
    expect(new Set(ids).size).toBe(ids.length);
    for (const id of ids) {
      expect(id).toMatch(/^[a-z0-9_-]{1,24}$/);
    }
  });

  it('résout les nouveaux choix et ignore les identifiants inconnus', () => {
    expect(decorationById('moon_moths')?.label.fr).toBe('Papillons lunaires');
    expect(effectById('cosmic_portal')?.label.en).toBe('Cosmic Portal');
    expect(frameById('lumen_bloom')?.label.fr).toBe('Jardin de lumière');
    expect(decorationById('phoenix_plume')?.label.fr).toBe('Phénix');
    expect(effectById('code_rain')?.label.en).toBe('Code Rain');
    expect(frameById('wild_ivy')?.label.fr).toBe('Lierre sauvage');
    expect(effectById('lumen_bloom')).toBeUndefined();
    expect(decorationById('<style>')).toBeUndefined();
    expect(effectById('missing')).toBeUndefined();
    expect(frameById('missing')).toBeUndefined();
  });

  it('rend les nouvelles familles sans contenu interactif', () => {
    render(
      <div>
        {decorationById('crystal_bloom')?.render(80)}
        {effectById('fireflies')?.render()}
      </div>,
    );

    expect(screen.getByTestId('avatar-decoration')).toHaveAttribute(
      'aria-hidden',
      'true',
    );
    expect(screen.getByTestId('profile-effect')).toHaveAttribute('aria-hidden', 'true');
  });

  it('rend chaque cadre de carte comme contenu décoratif distinct', () => {
    render(
      <div>
        {PROFILE_FRAMES.map((frame) => (
          <span key={frame.id}>{frame.render()}</span>
        ))}
      </div>,
    );

    const frames = screen.getAllByTestId('profile-frame');
    expect(frames).toHaveLength(PROFILE_FRAMES.length);
    for (const frame of frames) {
      expect(frame).toHaveAttribute('aria-hidden', 'true');
    }
  });
});
