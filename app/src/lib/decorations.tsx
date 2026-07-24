import { Suspense, lazy, type ComponentType, type ReactNode } from 'react';

export interface DecorationLabel {
  fr: string;
  en: string;
  es: string;
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
  decorationTitle: {
    fr: "Décoration d'avatar",
    en: 'Avatar decoration',
    es: 'Decoración de avatar',
  },
  decorationHint: {
    fr: 'Une signature visuelle visible partout où ton avatar apparaît.',
    en: 'A visual signature shown everywhere your avatar appears.',
    es: 'Una firma visual visible allí donde aparece tu avatar.',
  },
  effectTitle: { fr: 'Effet de profil', en: 'Profile effect', es: 'Efecto de perfil' },
  effectHint: {
    fr: 'Une atmosphère animée à l’intérieur de ta carte de profil.',
    en: 'An animated atmosphere inside your profile card.',
    es: 'Un ambiente animado dentro de tu tarjeta de perfil.',
  },
  frameTitle: { fr: 'Cadre de profil', en: 'Profile frame', es: 'Marco de perfil' },
  frameHint: {
    fr: 'Une composition animée qui habille tout le contour de ta carte.',
    en: 'An animated composition that dresses the full outline of your card.',
    es: 'Una composición animada que rodea todo el contorno de tu tarjeta.',
  },
  preview: { fr: 'Aperçu en direct', en: 'Live preview', es: 'Vista previa en directo' },
  signature: { fr: 'Signature Accord', en: 'Accord signature', es: 'Firma Accord' },
  none: { fr: 'Aucune', en: 'None', es: 'Ninguna' },
  saved: {
    fr: 'Personnalisation enregistrée',
    en: 'Personalization saved',
    es: 'Personalización guardada',
  },
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
  es: string,
): AvatarDecoration {
  return { id, category: 'avatar', label: { fr, en, es }, render: renderer(bundle, id) };
}

function effect(
  bundle: DecorationBundleKey,
  id: string,
  fr: string,
  en: string,
  es: string,
): ProfileEffect {
  return { id, category: 'effect', label: { fr, en, es }, render: renderer(bundle, id) };
}

function frame(
  bundle: DecorationBundleKey,
  id: string,
  fr: string,
  en: string,
  es: string,
): ProfileFrame {
  return { id, category: 'frame', label: { fr, en, es }, render: renderer(bundle, id) };
}

export const DECORATION_REGISTRY = [
  avatar('botanical', 'camellia_wreath', 'Camélias', 'Camellias', 'Camelias'),
  avatar('botanical', 'wisteria_drape', 'Glycines', 'Wisteria', 'Glicinas'),
  avatar('botanical', 'lotus_koi', 'Lotus et koï', 'Lotus & Koi', 'Lotos y kois'),
  avatar('manga', 'manga_impact', 'Impact manga', 'Manga Impact', 'Impacto manga'),
  avatar('manga', 'shojo_ribbon', 'Ruban shōjo', 'Shōjo Ribbon', 'Lazo shōjo'),
  avatar('manga', 'shonen_panels', 'Cases shōnen', 'Shōnen Panels', 'Viñetas shōnen'),
  avatar('essentials', 'soft_glow', 'Prisme', 'Prism', 'Prisma'),
  avatar('essentials', 'neon_ring', 'Éclipse', 'Eclipse', 'Eclipse'),
  avatar('essentials', 'aurora_ring', 'Orbite', 'Orbit', 'Órbita'),
  avatar('essentials', 'golden_laurel', 'Solaire', 'Solar', 'Solar'),
  avatar('essentials', 'sakura_arc', 'Sakura', 'Sakura', 'Sakura'),
  avatar('essentials', 'pixel_crown', 'Arcade', 'Arcade', 'Arcade'),
  avatar(
    'elemental',
    'moon_moths',
    'Papillons lunaires',
    'Moon Moths',
    'Polillas lunares',
  ),
  avatar(
    'elemental',
    'crystal_bloom',
    'Floraison cristal',
    'Crystal Bloom',
    'Floración de cristal',
  ),
  avatar('elemental', 'ember_wings', 'Ailes de braise', 'Ember Wings', 'Alas de brasa'),
  avatar('elemental', 'ocean_tide', 'Marée', 'Ocean Tide', 'Marea'),
  avatar(
    'elemental',
    'forest_spirit',
    'Esprit sylvestre',
    'Forest Spirit',
    'Espíritu del bosque',
  ),
  avatar(
    'elemental',
    'frost_shards',
    'Éclats polaires',
    'Frost Shards',
    'Esquirlas polares',
  ),
  avatar('elemental', 'heart_ribbon', 'Ruban cœur', 'Heart Ribbon', 'Lazo de corazón'),
  avatar('elemental', 'pixel_portal', 'Portail pixel', 'Pixel Portal', 'Portal pixel'),
  avatar('kinetic', 'storm_halo', 'Orage', 'Storm', 'Tormenta'),
  avatar('kinetic', 'galaxy_swirl', 'Galaxie', 'Galaxy', 'Galaxia'),
  avatar('kinetic', 'clockwork', 'Rouages', 'Clockwork', 'Engranajes'),
  avatar(
    'kinetic',
    'butterfly_waltz',
    'Valse de papillons',
    'Butterfly Waltz',
    'Vals de mariposas',
  ),
  avatar('kinetic', 'rune_circle', 'Cercle runique', 'Rune Circle', 'Círculo rúnico'),
  avatar('kinetic', 'phoenix_plume', 'Phénix', 'Phoenix', 'Fénix'),
  effect(
    'botanical',
    'sakura_garden',
    'Jardin de sakura',
    'Sakura Garden',
    'Jardín de sakura',
  ),
  effect(
    'botanical',
    'wisteria_fireflies',
    'Nuit de glycines',
    'Wisteria Night',
    'Noche de glicinas',
  ),
  effect(
    'botanical',
    'lotus_ripples',
    'Bassin de lotus',
    'Lotus Pond',
    'Estanque de lotos',
  ),
  effect('manga', 'manga_panels', 'Planche manga', 'Manga Page', 'Página manga'),
  effect('manga', 'shojo_roses', 'Roses shōjo', 'Shōjo Roses', 'Rosas shōjo'),
  effect('manga', 'shonen_impact', 'Impact shōnen', 'Shōnen Impact', 'Impacto shōnen'),
  effect('ambient', 'aurora', 'Aurore', 'Aurora', 'Aurora'),
  effect('ambient', 'starfield', 'Constellation', 'Constellation', 'Constelación'),
  effect('ambient', 'falling_petals', 'Pétales', 'Petals', 'Pétalos'),
  effect('ambient', 'floating_particles', 'Braises', 'Embers', 'Brasas'),
  effect('elemental', 'moon_clouds', 'Clair de lune', 'Moonlight', 'Claro de luna'),
  effect('elemental', 'deep_sea', 'Grand bleu', 'Deep Sea', 'Mar profundo'),
  effect('elemental', 'soft_rain', 'Pluie douce', 'Soft Rain', 'Lluvia suave'),
  effect('elemental', 'holo_grid', 'Hologramme', 'Hologram', 'Holograma'),
  effect('elemental', 'fireflies', 'Lucioles', 'Fireflies', 'Luciérnagas'),
  effect('elemental', 'snowfall', 'Neige', 'Snowfall', 'Nevada'),
  effect('elemental', 'ink_bloom', 'Encre vivante', 'Living Ink', 'Tinta viva'),
  effect(
    'elemental',
    'cosmic_portal',
    'Portail cosmique',
    'Cosmic Portal',
    'Portal cósmico',
  ),
  effect('kinetic', 'thunderstorm', "Ciel d'orage", 'Thunderstorm', 'Cielo de tormenta'),
  effect('kinetic', 'lava_flow', 'Lave', 'Lava Flow', 'Lava'),
  effect('kinetic', 'code_rain', 'Pluie de code', 'Code Rain', 'Lluvia de código'),
  effect('kinetic', 'light_beams', 'Faisceaux', 'Light Beams', 'Haces de luz'),
  effect('kinetic', 'confetti', 'Confettis', 'Confetti', 'Confeti'),
  effect(
    'kinetic',
    'drifting_hearts',
    'Cœurs flottants',
    'Drifting Hearts',
    'Corazones flotantes',
  ),
  frame('botanical', 'sakura_gate', 'Portail sakura', 'Sakura Gate', 'Portal sakura'),
  frame(
    'botanical',
    'wisteria_arch',
    'Arche de glycines',
    'Wisteria Arch',
    'Arco de glicinas',
  ),
  frame(
    'botanical',
    'lotus_lacquer',
    'Laque aux lotus',
    'Lotus Lacquer',
    'Laca de lotos',
  ),
  frame('manga', 'manga_page', 'Planche encrée', 'Ink Page', 'Página entintada'),
  frame('manga', 'shojo_lace', 'Dentelle shōjo', 'Shōjo Lace', 'Encaje shōjo'),
  frame(
    'premiumFrames',
    'lumen_bloom',
    'Jardin de lumière',
    'Lumen Garden',
    'Jardín de luz',
  ),
  frame(
    'premiumFrames',
    'crystal_crown',
    'Couronne de cristaux',
    'Crystal Crown',
    'Corona de cristales',
  ),
  frame(
    'premiumFrames',
    'celestial_wings',
    'Papillons célestes',
    'Celestial Wings',
    'Mariposas celestes',
  ),
  frame('premiumFrames', 'neon_circuit', 'Circuit néon', 'Neon Circuit', 'Circuito neón'),
  frame('ornamentalFrames', 'royal_gilt', 'Or royal', 'Royal Gilt', 'Oro real'),
  frame(
    'ornamentalFrames',
    'frost_veil',
    'Voile de givre',
    'Frost Veil',
    'Velo de escarcha',
  ),
  frame(
    'ornamentalFrames',
    'emberforge',
    'Forge ardente',
    'Emberforge',
    'Forja ardiente',
  ),
  frame('ornamentalFrames', 'wild_ivy', 'Lierre sauvage', 'Wild Ivy', 'Hiedra salvaje'),
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
