import { render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import {
  AVATAR_DECORATIONS,
  DECORATION_REGISTRY,
  PROFILE_EFFECTS,
  PROFILE_FRAMES,
  decorationById,
  effectById,
  frameById,
} from './decorations';

describe('personalization registry', () => {
  it('preserves every persisted ID and rejects duplicates', () => {
    expect(AVATAR_DECORATIONS.map((item) => item.id)).toEqual([
      'camellia_wreath',
      'wisteria_drape',
      'lotus_koi',
      'manga_impact',
      'shojo_ribbon',
      'shonen_panels',
      'soft_glow',
      'neon_ring',
      'aurora_ring',
      'golden_laurel',
      'sakura_arc',
      'pixel_crown',
      'moon_moths',
      'crystal_bloom',
      'ember_wings',
      'ocean_tide',
      'forest_spirit',
      'frost_shards',
      'heart_ribbon',
      'pixel_portal',
      'storm_halo',
      'galaxy_swirl',
      'clockwork',
      'butterfly_waltz',
      'rune_circle',
      'phoenix_plume',
    ]);
    expect(PROFILE_EFFECTS.map((item) => item.id)).toEqual([
      'sakura_garden',
      'wisteria_fireflies',
      'lotus_ripples',
      'manga_panels',
      'shojo_roses',
      'shonen_impact',
      'aurora',
      'starfield',
      'falling_petals',
      'floating_particles',
      'moon_clouds',
      'deep_sea',
      'soft_rain',
      'holo_grid',
      'fireflies',
      'snowfall',
      'ink_bloom',
      'cosmic_portal',
      'thunderstorm',
      'lava_flow',
      'code_rain',
      'light_beams',
      'confetti',
      'drifting_hearts',
    ]);
    expect(PROFILE_FRAMES.map((item) => item.id)).toEqual([
      'sakura_gate',
      'wisteria_arch',
      'lotus_lacquer',
      'manga_page',
      'shojo_lace',
      'lumen_bloom',
      'crystal_crown',
      'celestial_wings',
      'neon_circuit',
      'royal_gilt',
      'frost_veil',
      'emberforge',
      'wild_ivy',
    ]);

    const ids = [
      ...AVATAR_DECORATIONS.map((item) => item.id),
      ...PROFILE_EFFECTS.map((item) => item.id),
      ...PROFILE_FRAMES.map((item) => item.id),
    ];

    expect(new Set(ids).size).toBe(ids.length);
    for (const id of ids) {
      expect(id).toMatch(/^[a-z0-9_-]{1,24}$/);
    }
  });

  it('resolves known choices and ignores unknown identifiers', () => {
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

  it('maps every registry item to exactly one lazy renderer', async () => {
    const bundles = await Promise.all([
      import('./decorations-ambient'),
      import('./decorations-botanical'),
      import('./decorations-elemental'),
      import('./decorations-essentials'),
      import('./decorations-kinetic'),
      import('./decorations-manga'),
      import('./decorations-ornamental-frames'),
      import('./decorations-premium-frames'),
    ]);
    const rendererIds = bundles.flatMap((bundle) =>
      Object.keys(bundle.DECORATION_RENDERERS),
    );

    expect(rendererIds.sort()).toEqual(DECORATION_REGISTRY.map((item) => item.id).sort());
    expect(new Set(rendererIds).size).toBe(rendererIds.length);
  });

  it('renders every figurative family as decorative content', async () => {
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

    const decorations = await screen.findAllByTestId('avatar-decoration');
    const effects = await screen.findAllByTestId('profile-effect');
    for (const decoration of decorations) {
      expect(decoration).toHaveAttribute('aria-hidden', 'true');
    }
    for (const effect of effects) {
      expect(effect).toHaveAttribute('aria-hidden', 'true');
    }
    expect(decorations).toHaveLength(6);
    expect(effects).toHaveLength(6);
  });

  it('renders every card frame as distinct decorative content', async () => {
    render(
      <div>
        {PROFILE_FRAMES.map((frame) => (
          <span key={frame.id}>{frame.render()}</span>
        ))}
      </div>,
    );

    await waitFor(() => {
      expect(screen.getAllByTestId('profile-frame')).toHaveLength(PROFILE_FRAMES.length);
    });
    const frames = screen.getAllByTestId('profile-frame');
    expect(frames).toHaveLength(PROFILE_FRAMES.length);
    for (const frame of frames) {
      expect(frame).toHaveAttribute('aria-hidden', 'true');
    }
  });
});
