/**
 * Thème personnalisé : l'utilisateur choisit trois couleurs (fond de
 * conversation, panneaux latéraux, accent) et une base claire ou sombre ; le
 * reste de la palette (survols, champs, rail, infobulles, accent survolé) est
 * DÉRIVÉ par ajustement de luminosité, puis posé en variables CSS inline sur
 * la racine — par-dessus le bloc `[data-theme]` de la base, qui fournit les
 * valeurs saines (color-scheme, verre, barres de défilement) non couvertes.
 */

/** Couleurs choisies par l'utilisateur (hex `#rrggbb`) + base de repli. */
export interface CouleursPerso {
  /** Fond de la zone de conversation. */
  fond: string;
  /** Fond des panneaux latéraux (serveurs, membres, réglages). */
  panneaux: string;
  /** Couleur d'accent (boutons, liens, anneaux de focus). */
  accent: string;
  /** Base héritée pour tout ce qui n'est pas dérivé (texte, verre…). */
  base: 'dark' | 'light';
}

/** Thème personnalisé par défaut : sombre, proche du thème historique. */
export const PERSO_DEFAUT: CouleursPerso = {
  fond: '#313338',
  panneaux: '#2b2d31',
  accent: '#5865f2',
  base: 'dark',
};

/** Décompose un hex `#rrggbb` en composantes 0-255, ou `null` si invalide. */
export function hexVersRgb(hex: string): [number, number, number] | null {
  const m = /^#([0-9a-f]{6})$/i.exec(hex.trim());
  if (m === null || m[1] === undefined) return null;
  const n = parseInt(m[1], 16);
  return [(n >> 16) & 0xff, (n >> 8) & 0xff, n & 0xff];
}

/** Triplet CSS `r g b` attendu par les variables `--color-*` du thème. */
export function hexVersTriplet(hex: string): string | null {
  const rgb = hexVersRgb(hex);
  if (rgb === null) return null;
  return `${rgb[0]} ${rgb[1]} ${rgb[2]}`;
}

/**
 * Ajuste la luminosité d'une couleur : `delta` en unités 0-255, appliqué à
 * chaque composante avec bornage. Positif = éclaircit, négatif = assombrit.
 */
export function ajusterLuminosite(hex: string, delta: number): string {
  const rgb = hexVersRgb(hex);
  if (rgb === null) return hex;
  const c = rgb.map((v) => Math.min(255, Math.max(0, v + delta)));
  return `#${c.map((v) => v.toString(16).padStart(2, '0')).join('')}`;
}

/**
 * Palette dérivée des trois couleurs choisies, sous forme de variables CSS.
 * Les dérivations suivent le motif des thèmes intégrés : survol légèrement
 * décalé du fond, champ de saisie plus marqué, rail plus sombre que les
 * panneaux, infobulle presque noire (ou blanche en base claire).
 */
export function deriverVariables(c: CouleursPerso): Record<string, string> {
  const sens = c.base === 'dark' ? 1 : -1;
  const vars: Record<string, string | null> = {
    '--color-chat': hexVersTriplet(c.fond),
    '--color-chat-hover': hexVersTriplet(ajusterLuminosite(c.fond, -6 * sens)),
    '--color-input': hexVersTriplet(ajusterLuminosite(c.fond, 10 * sens)),
    '--color-modal': hexVersTriplet(c.fond),
    '--color-sidebar': hexVersTriplet(c.panneaux),
    '--color-rail': hexVersTriplet(ajusterLuminosite(c.panneaux, -12 * sens)),
    '--color-tooltip': hexVersTriplet(
      c.base === 'dark' ? ajusterLuminosite(c.panneaux, -24) : '#111214',
    ),
    '--color-blurple': hexVersTriplet(c.accent),
    '--color-blurple-hover': hexVersTriplet(ajusterLuminosite(c.accent, -20)),
  };
  const sortie: Record<string, string> = {};
  for (const [cle, valeur] of Object.entries(vars)) {
    if (valeur !== null) sortie[cle] = valeur;
  }
  return sortie;
}

/**
 * Encode un thème personnalisé en CODE PARTAGEABLE : `accord-theme:` suivi du
 * JSON encodé en base64url. Compact, collable dans un message, sans dépendance
 * à un fichier — un ami peut l'importer d'un copier-coller.
 */
export function exporterTheme(c: CouleursPerso): string {
  const json = JSON.stringify({ f: c.fond, p: c.panneaux, a: c.accent, b: c.base });
  const b64 = btoa(unescape(encodeURIComponent(json)))
    .replace(/\+/g, '-')
    .replace(/\//g, '_')
    .replace(/=+$/, '');
  return `accord-theme:${b64}`;
}

/**
 * Décode un code de thème produit par [`exporterTheme`], ou `null` si le code
 * est invalide (préfixe, base64 ou couleurs incorrects). Tolérant : accepte le
 * code avec ou sans espaces autour.
 */
export function importerTheme(code: string): CouleursPerso | null {
  const trimmed = code.trim();
  const brut = trimmed.startsWith('accord-theme:')
    ? trimmed.slice('accord-theme:'.length)
    : trimmed;
  const b64 = brut.replace(/-/g, '+').replace(/_/g, '/');
  let json: string;
  try {
    json = decodeURIComponent(escape(atob(b64)));
  } catch {
    return null;
  }
  let lu: { f?: unknown; p?: unknown; a?: unknown; b?: unknown };
  try {
    lu = JSON.parse(json);
  } catch {
    return null;
  }
  const valide = (h: unknown): h is string => typeof h === 'string' && hexVersRgb(h) !== null;
  if (!valide(lu.f) || !valide(lu.p) || !valide(lu.a)) return null;
  return {
    fond: lu.f,
    panneaux: lu.p,
    accent: lu.a,
    base: lu.b === 'light' ? 'light' : 'dark',
  };
}

/** Variables posées par `appliquerThemePerso` (retirées à la désactivation). */
const VARS_PERSO = [
  '--color-chat',
  '--color-chat-hover',
  '--color-input',
  '--color-modal',
  '--color-sidebar',
  '--color-rail',
  '--color-tooltip',
  '--color-blurple',
  '--color-blurple-hover',
] as const;

/**
 * Pose (ou retire, avec `null`) les variables du thème personnalisé sur la
 * racine du document. L'appelant règle `data-theme` sur la BASE choisie
 * (`dark`/`light`) avant l'appel — les inline priment sur le bloc de base.
 */
export function appliquerThemePerso(c: CouleursPerso | null): void {
  const style = document.documentElement.style;
  if (c === null) {
    for (const v of VARS_PERSO) style.removeProperty(v);
    return;
  }
  const vars = deriverVariables(c);
  for (const v of VARS_PERSO) {
    const valeur = vars[v];
    if (valeur !== undefined) style.setProperty(v, valeur);
    else style.removeProperty(v);
  }
}
