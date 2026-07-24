/**
 * Catalogue des raccourcis clavier.
 *
 * Une seule liste, lue par l'écran de référence. Un raccourci qui existe dans
 * le code mais pas ici est un raccourci que personne ne découvrira ; l'inverse
 * — listé mais non implémenté — est pire, il promet quelque chose de faux.
 * Le test `shortcuts.test.ts` compare cette liste à ce que le code câble
 * réellement.
 *
 * `mod` est remplacé par `⌘` sur macOS et `Ctrl` ailleurs au moment du rendu.
 */

import type { Dict } from '../i18n';

/** Regroupement d'affichage d'un raccourci. */
export type ShortcutSection =
  'navigation' | 'interface' | 'palette' | 'messaging' | 'voice';

export interface Shortcut {
  /** Clé du libellé dans `t.shortcuts`. */
  key: keyof Dict['shortcuts'];
  /** Section d'affichage. */
  section: ShortcutSection;
  /**
   * Combinaison, touches séparées par `|`. Le jeton `mod` devient `⌘`/`Ctrl`,
   * `enter` devient le libellé traduit de la touche Entrée.
   */
  combo: readonly string[];
}

export const SHORTCUTS: readonly Shortcut[] = [
  { key: 'quickSwitchLabel', section: 'navigation', combo: ['mod', 'K'] },
  { key: 'prevChannelLabel', section: 'navigation', combo: ['Alt', '↑'] },
  { key: 'nextChannelLabel', section: 'navigation', combo: ['Alt', '↓'] },
  { key: 'closeLabel', section: 'navigation', combo: ['esc'] },

  { key: 'zoomInLabel', section: 'interface', combo: ['mod', '+'] },
  { key: 'zoomOutLabel', section: 'interface', combo: ['mod', '-'] },
  { key: 'zoomResetLabel', section: 'interface', combo: ['mod', '0'] },

  { key: 'paletteMoveLabel', section: 'palette', combo: ['↑', '↓'] },
  { key: 'paletteEdgesLabel', section: 'palette', combo: ['Home', 'End'] },
  { key: 'paletteOpenLabel', section: 'palette', combo: ['enter'] },
  { key: 'paletteCycleLabel', section: 'palette', combo: ['Tab'] },
  { key: 'paletteCloseLabel', section: 'palette', combo: ['esc'] },

  { key: 'sendMessageLabel', section: 'messaging', combo: ['enter'] },
  { key: 'newLineLabel', section: 'messaging', combo: ['⇧', 'enter'] },
  { key: 'editLastLabel', section: 'messaging', combo: ['↑'] },
  { key: 'completionMoveLabel', section: 'messaging', combo: ['↑', '↓'] },
  { key: 'completionAcceptLabel', section: 'messaging', combo: ['Tab'] },
  { key: 'completionCancelLabel', section: 'messaging', combo: ['esc'] },

  { key: 'toggleMuteLabel', section: 'voice', combo: ['mod', '⇧', 'M'] },
];

/** Sections dans l'ordre d'affichage, avec la clé de leur titre. */
export const SHORTCUT_SECTIONS: readonly {
  id: ShortcutSection;
  titleKey: keyof Dict['shortcuts'];
}[] = [
  { id: 'navigation', titleKey: 'navigationSection' },
  { id: 'interface', titleKey: 'interfaceSection' },
  { id: 'palette', titleKey: 'paletteSection' },
  { id: 'messaging', titleKey: 'messagingSection' },
  { id: 'voice', titleKey: 'voiceSection' },
];

/** Rend une combinaison en touches affichables. */
export function renderCombo(
  combo: readonly string[],
  opts: { mod: string; enter: string; esc: string },
): string[] {
  return combo.map((token) => {
    if (token === 'mod') return opts.mod;
    if (token === 'enter') return opts.enter;
    if (token === 'esc') return opts.esc;
    return token;
  });
}
