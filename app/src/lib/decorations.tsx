import { Suspense, lazy, type ComponentType, type ReactNode } from 'react';

export type DecorationCategory = 'avatar' | 'effect' | 'frame';

interface DecorationRecord {
  id: string;
  category: DecorationCategory;
}

export interface AvatarDecoration extends DecorationRecord {
  category: 'avatar';
  render: (size: number) => ReactNode;
}

export interface ProfileEffect extends DecorationRecord {
  category: 'effect';
  render: () => ReactNode;
}

export interface ProfileFrame extends DecorationRecord {
  category: 'frame';
  render: () => ReactNode;
}

export type DecorationRegistryItem = AvatarDecoration | ProfileEffect | ProfileFrame;

type DecorationBundleKey =
  | 'ambient'
  | 'botanical'
  | 'elemental'
  | 'essentials'
  | 'kinetic'
  | 'manga'
  | 'ornamentalFrames'
  | 'premiumFrames';
type DecorationBundle = {
  DECORATION_RENDERERS: Record<string, ComponentType>;
};

const BUNDLE_LOADERS: Record<DecorationBundleKey, () => Promise<DecorationBundle>> = {
  ambient: () => import('./decorations-ambient'),
  botanical: () => import('./decorations-botanical'),
  elemental: () => import('./decorations-elemental'),
  essentials: () => import('./decorations-essentials'),
  kinetic: () => import('./decorations-kinetic'),
  manga: () => import('./decorations-manga'),
  ornamentalFrames: () => import('./decorations-ornamental-frames'),
  premiumFrames: () => import('./decorations-premium-frames'),
};

const RENDERERS = new Map<string, ComponentType>();

function renderer(bundle: DecorationBundleKey, id: string): () => ReactNode {
  const cacheKey = `${bundle}:${id}`;
  let Renderer = RENDERERS.get(cacheKey);
  if (Renderer === undefined) {
    Renderer = lazy(async () => {
      const loaded = await BUNDLE_LOADERS[bundle]();
      const Component = loaded.DECORATION_RENDERERS[id];
      if (Component === undefined) throw new Error(`Unknown decoration renderer: ${id}`);
      return { default: Component };
    });
    RENDERERS.set(cacheKey, Renderer);
  }
  return () => (
    <Suspense fallback={null}>
      <Renderer />
    </Suspense>
  );
}

function avatar(bundle: DecorationBundleKey, id: string): AvatarDecoration {
  return { id, category: 'avatar', render: renderer(bundle, id) };
}

function effect(bundle: DecorationBundleKey, id: string): ProfileEffect {
  return { id, category: 'effect', render: renderer(bundle, id) };
}

function frame(bundle: DecorationBundleKey, id: string): ProfileFrame {
  return { id, category: 'frame', render: renderer(bundle, id) };
}

export const DECORATION_REGISTRY = [
  avatar('botanical', 'camellia_wreath'),
  avatar('botanical', 'wisteria_drape'),
  avatar('botanical', 'lotus_koi'),
  avatar('manga', 'manga_impact'),
  avatar('manga', 'shojo_ribbon'),
  avatar('manga', 'shonen_panels'),
  avatar('essentials', 'soft_glow'),
  avatar('essentials', 'neon_ring'),
  avatar('essentials', 'aurora_ring'),
  avatar('essentials', 'golden_laurel'),
  avatar('essentials', 'sakura_arc'),
  avatar('essentials', 'pixel_crown'),
  avatar('elemental', 'moon_moths'),
  avatar('elemental', 'crystal_bloom'),
  avatar('elemental', 'ember_wings'),
  avatar('elemental', 'ocean_tide'),
  avatar('elemental', 'forest_spirit'),
  avatar('elemental', 'frost_shards'),
  avatar('elemental', 'heart_ribbon'),
  avatar('elemental', 'pixel_portal'),
  avatar('kinetic', 'storm_halo'),
  avatar('kinetic', 'galaxy_swirl'),
  avatar('kinetic', 'clockwork'),
  avatar('kinetic', 'butterfly_waltz'),
  avatar('kinetic', 'rune_circle'),
  avatar('kinetic', 'phoenix_plume'),
  effect('botanical', 'sakura_garden'),
  effect('botanical', 'wisteria_fireflies'),
  effect('botanical', 'lotus_ripples'),
  effect('manga', 'manga_panels'),
  effect('manga', 'shojo_roses'),
  effect('manga', 'shonen_impact'),
  effect('ambient', 'aurora'),
  effect('ambient', 'starfield'),
  effect('ambient', 'falling_petals'),
  effect('ambient', 'floating_particles'),
  effect('elemental', 'moon_clouds'),
  effect('elemental', 'deep_sea'),
  effect('elemental', 'soft_rain'),
  effect('elemental', 'holo_grid'),
  effect('elemental', 'fireflies'),
  effect('elemental', 'snowfall'),
  effect('elemental', 'ink_bloom'),
  effect('elemental', 'cosmic_portal'),
  effect('kinetic', 'thunderstorm'),
  effect('kinetic', 'lava_flow'),
  effect('kinetic', 'code_rain'),
  effect('kinetic', 'light_beams'),
  effect('kinetic', 'confetti'),
  effect('kinetic', 'drifting_hearts'),
  frame('botanical', 'sakura_gate'),
  frame('botanical', 'wisteria_arch'),
  frame('botanical', 'lotus_lacquer'),
  frame('manga', 'manga_page'),
  frame('manga', 'shojo_lace'),
  frame('premiumFrames', 'lumen_bloom'),
  frame('premiumFrames', 'crystal_crown'),
  frame('premiumFrames', 'celestial_wings'),
  frame('premiumFrames', 'neon_circuit'),
  frame('ornamentalFrames', 'royal_gilt'),
  frame('ornamentalFrames', 'frost_veil'),
  frame('ornamentalFrames', 'emberforge'),
  frame('ornamentalFrames', 'wild_ivy'),
] as const satisfies readonly DecorationRegistryItem[];

function hasCategory<C extends DecorationCategory>(category: C) {
  return (
    item: DecorationRegistryItem,
  ): item is Extract<DecorationRegistryItem, { category: C }> =>
    item.category === category;
}

export const AVATAR_DECORATIONS = DECORATION_REGISTRY.filter(hasCategory('avatar'));
export const PROFILE_EFFECTS = DECORATION_REGISTRY.filter(hasCategory('effect'));
export const PROFILE_FRAMES = DECORATION_REGISTRY.filter(hasCategory('frame'));

const DECORATION_BY_ID = new Map(DECORATION_REGISTRY.map((item) => [item.id, item]));

export function decorationById(
  id: string | null | undefined,
): AvatarDecoration | undefined {
  const item = id == null ? undefined : DECORATION_BY_ID.get(id);
  return item?.category === 'avatar' ? item : undefined;
}

export function effectById(id: string | null | undefined): ProfileEffect | undefined {
  const item = id == null ? undefined : DECORATION_BY_ID.get(id);
  return item?.category === 'effect' ? item : undefined;
}

export function frameById(id: string | null | undefined): ProfileFrame | undefined {
  const item = id == null ? undefined : DECORATION_BY_ID.get(id);
  return item?.category === 'frame' ? item : undefined;
}

export async function preloadDecorations(): Promise<void> {
  await Promise.all(Object.values(BUNDLE_LOADERS).map(async (load) => load()));
}
