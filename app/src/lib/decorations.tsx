import { Suspense, lazy, type ComponentType, type ReactNode } from 'react';

export interface DecorationLabel {
  fr: string;
  en: string;
}

export type DecorationCategory = 'avatar' | 'effect' | 'frame';

interface DecorationRecord {
  id: string;
  category: DecorationCategory;
  label: DecorationLabel;
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

export const DECORATION_UI_TEXT = {
  decorationTitle: { fr: "Décoration d'avatar", en: 'Avatar decoration' },
  decorationHint: {
    fr: 'Une signature visuelle visible partout où ton avatar apparaît.',
    en: 'A visual signature shown everywhere your avatar appears.',
  },
  effectTitle: { fr: 'Effet de profil', en: 'Profile effect' },
  effectHint: {
    fr: 'Une atmosphère animée à l’intérieur de ta carte de profil.',
    en: 'An animated atmosphere inside your profile card.',
  },
  frameTitle: { fr: 'Cadre de profil', en: 'Profile frame' },
  frameHint: {
    fr: 'Une composition animée qui habille tout le contour de ta carte.',
    en: 'An animated composition that dresses the full outline of your card.',
  },
  preview: { fr: 'Aperçu en direct', en: 'Live preview' },
  signature: { fr: 'Signature Accord', en: 'Accord signature' },
  none: { fr: 'Aucune', en: 'None' },
  saved: { fr: 'Personnalisation enregistrée', en: 'Personalization saved' },
} as const;

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

function avatar(
  bundle: DecorationBundleKey,
  id: string,
  fr: string,
  en: string,
): AvatarDecoration {
  return { id, category: 'avatar', label: { fr, en }, render: renderer(bundle, id) };
}

function effect(
  bundle: DecorationBundleKey,
  id: string,
  fr: string,
  en: string,
): ProfileEffect {
  return { id, category: 'effect', label: { fr, en }, render: renderer(bundle, id) };
}

function frame(
  bundle: DecorationBundleKey,
  id: string,
  fr: string,
  en: string,
): ProfileFrame {
  return { id, category: 'frame', label: { fr, en }, render: renderer(bundle, id) };
}

export const DECORATION_REGISTRY = [
  avatar('botanical', 'camellia_wreath', 'Camélias', 'Camellias'),
  avatar('botanical', 'wisteria_drape', 'Glycines', 'Wisteria'),
  avatar('botanical', 'lotus_koi', 'Lotus et koï', 'Lotus & Koi'),
  avatar('manga', 'manga_impact', 'Impact manga', 'Manga Impact'),
  avatar('manga', 'shojo_ribbon', 'Ruban shōjo', 'Shōjo Ribbon'),
  avatar('manga', 'shonen_panels', 'Cases shōnen', 'Shōnen Panels'),
  avatar('essentials', 'soft_glow', 'Prisme', 'Prism'),
  avatar('essentials', 'neon_ring', 'Éclipse', 'Eclipse'),
  avatar('essentials', 'aurora_ring', 'Orbite', 'Orbit'),
  avatar('essentials', 'golden_laurel', 'Solaire', 'Solar'),
  avatar('essentials', 'sakura_arc', 'Sakura', 'Sakura'),
  avatar('essentials', 'pixel_crown', 'Arcade', 'Arcade'),
  avatar('elemental', 'moon_moths', 'Papillons lunaires', 'Moon Moths'),
  avatar('elemental', 'crystal_bloom', 'Floraison cristal', 'Crystal Bloom'),
  avatar('elemental', 'ember_wings', 'Ailes de braise', 'Ember Wings'),
  avatar('elemental', 'ocean_tide', 'Marée', 'Ocean Tide'),
  avatar('elemental', 'forest_spirit', 'Esprit sylvestre', 'Forest Spirit'),
  avatar('elemental', 'frost_shards', 'Éclats polaires', 'Frost Shards'),
  avatar('elemental', 'heart_ribbon', 'Ruban cœur', 'Heart Ribbon'),
  avatar('elemental', 'pixel_portal', 'Portail pixel', 'Pixel Portal'),
  avatar('kinetic', 'storm_halo', 'Orage', 'Storm'),
  avatar('kinetic', 'galaxy_swirl', 'Galaxie', 'Galaxy'),
  avatar('kinetic', 'clockwork', 'Rouages', 'Clockwork'),
  avatar('kinetic', 'butterfly_waltz', 'Valse de papillons', 'Butterfly Waltz'),
  avatar('kinetic', 'rune_circle', 'Cercle runique', 'Rune Circle'),
  avatar('kinetic', 'phoenix_plume', 'Phénix', 'Phoenix'),
  effect('botanical', 'sakura_garden', 'Jardin de sakura', 'Sakura Garden'),
  effect('botanical', 'wisteria_fireflies', 'Nuit de glycines', 'Wisteria Night'),
  effect('botanical', 'lotus_ripples', 'Bassin de lotus', 'Lotus Pond'),
  effect('manga', 'manga_panels', 'Planche manga', 'Manga Page'),
  effect('manga', 'shojo_roses', 'Roses shōjo', 'Shōjo Roses'),
  effect('manga', 'shonen_impact', 'Impact shōnen', 'Shōnen Impact'),
  effect('ambient', 'aurora', 'Aurore', 'Aurora'),
  effect('ambient', 'starfield', 'Constellation', 'Constellation'),
  effect('ambient', 'falling_petals', 'Pétales', 'Petals'),
  effect('ambient', 'floating_particles', 'Braises', 'Embers'),
  effect('elemental', 'moon_clouds', 'Clair de lune', 'Moonlight'),
  effect('elemental', 'deep_sea', 'Grand bleu', 'Deep Sea'),
  effect('elemental', 'soft_rain', 'Pluie douce', 'Soft Rain'),
  effect('elemental', 'holo_grid', 'Hologramme', 'Hologram'),
  effect('elemental', 'fireflies', 'Lucioles', 'Fireflies'),
  effect('elemental', 'snowfall', 'Neige', 'Snowfall'),
  effect('elemental', 'ink_bloom', 'Encre vivante', 'Living Ink'),
  effect('elemental', 'cosmic_portal', 'Portail cosmique', 'Cosmic Portal'),
  effect('kinetic', 'thunderstorm', "Ciel d'orage", 'Thunderstorm'),
  effect('kinetic', 'lava_flow', 'Lave', 'Lava Flow'),
  effect('kinetic', 'code_rain', 'Pluie de code', 'Code Rain'),
  effect('kinetic', 'light_beams', 'Faisceaux', 'Light Beams'),
  effect('kinetic', 'confetti', 'Confettis', 'Confetti'),
  effect('kinetic', 'drifting_hearts', 'Cœurs flottants', 'Drifting Hearts'),
  frame('botanical', 'sakura_gate', 'Portail sakura', 'Sakura Gate'),
  frame('botanical', 'wisteria_arch', 'Arche de glycines', 'Wisteria Arch'),
  frame('botanical', 'lotus_lacquer', 'Laque aux lotus', 'Lotus Lacquer'),
  frame('manga', 'manga_page', 'Planche encrée', 'Ink Page'),
  frame('manga', 'shojo_lace', 'Dentelle shōjo', 'Shōjo Lace'),
  frame('premiumFrames', 'lumen_bloom', 'Jardin de lumière', 'Lumen Garden'),
  frame('premiumFrames', 'crystal_crown', 'Couronne de cristaux', 'Crystal Crown'),
  frame('premiumFrames', 'celestial_wings', 'Papillons célestes', 'Celestial Wings'),
  frame('premiumFrames', 'neon_circuit', 'Circuit néon', 'Neon Circuit'),
  frame('ornamentalFrames', 'royal_gilt', 'Or royal', 'Royal Gilt'),
  frame('ornamentalFrames', 'frost_veil', 'Voile de givre', 'Frost Veil'),
  frame('ornamentalFrames', 'emberforge', 'Forge ardente', 'Emberforge'),
  frame('ornamentalFrames', 'wild_ivy', 'Lierre sauvage', 'Wild Ivy'),
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
