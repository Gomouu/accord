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

    expect(AVATAR_DECORATIONS).toHaveLength(26);
    expect(PROFILE_EFFECTS).toHaveLength(24);
    expect(PROFILE_FRAMES).toHaveLength(13);
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
    expect(decorationById('camellia_wreath')?.label.fr).toBe('Camélias');
    expect(effectById('manga_panels')?.label.en).toBe('Manga Page');
    expect(frameById('shojo_lace')?.label.fr).toBe('Dentelle shōjo');
    expect(effectById('lumen_bloom')).toBeUndefined();
    expect(decorationById('<style>')).toBeUndefined();
    expect(effectById('missing')).toBeUndefined();
    expect(frameById('missing')).toBeUndefined();
  });

  it('rend les nouvelles familles sans contenu interactif', () => {
    const decorationIds = [
      'camellia_wreath',
      'wisteria_drape',
      'lotus_koi',
      'manga_impact',
      'shojo_ribbon',
      'shonen_panels',
    ] as const;
    const effectIds = [
      'sakura_garden',
      'wisteria_fireflies',
      'lotus_ripples',
      'manga_panels',
      'shojo_roses',
      'shonen_impact',
    ] as const;

    render(
      <div>
        {decorationIds.map((id) => (
          <span key={id}>{decorationById(id)?.render(80)}</span>
        ))}
        {effectIds.map((id) => (
          <span key={id}>{effectById(id)?.render()}</span>
        ))}
      </div>,
    );

    for (const decoration of screen.getAllByTestId('avatar-decoration')) {
      expect(decoration).toHaveAttribute('aria-hidden', 'true');
    }
    for (const effect of screen.getAllByTestId('profile-effect')) {
      expect(effect).toHaveAttribute('aria-hidden', 'true');
    }
    expect(screen.getAllByTestId('avatar-decoration')).toHaveLength(6);
    expect(screen.getAllByTestId('profile-effect')).toHaveLength(6);
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
