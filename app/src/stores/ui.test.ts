/**
 * Tests des préférences d'apparence du store d'interface : application
 * immédiate sur la racine du document, persistance localStorage et
 * validation des valeurs restaurées.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
  useUi,
  THEME_IDS,
  FONT_SCALES,
  stepFontScale,
  SIDEBAR_WIDTH_DEFAULT,
  SIDEBAR_WIDTH_MIN,
  SIDEBAR_WIDTH_MAX,
  MEMBERS_WIDTH_DEFAULT,
  MEMBERS_WIDTH_MIN,
  MEMBERS_WIDTH_MAX,
} from './ui';

const root = document.documentElement;

beforeEach(() => {
  window.localStorage.clear();
  useUi.getState().setTheme('dark');
  useUi.getState().setDensity('comfortable');
  useUi.getState().setFontScale(100);
  window.localStorage.clear();
});

describe('useUi — thème', () => {
  it('applique le thème clair à la racine et le persiste', () => {
    useUi.getState().setTheme('light');

    expect(root.dataset.theme).toBe('light');
    expect(window.localStorage.getItem('accord.theme')).toBe('light');
    expect(useUi.getState().theme).toBe('light');
  });

  it('revient au thème sombre', () => {
    useUi.getState().setTheme('light');
    useUi.getState().setTheme('dark');

    expect(root.dataset.theme).toBe('dark');
    expect(window.localStorage.getItem('accord.theme')).toBe('dark');
  });

  it('applique et persiste chacun des thèmes de la galerie', () => {
    for (const id of THEME_IDS) {
      useUi.getState().setTheme(id);

      if (id === 'custom') {
        // Thème personnalisé : la racine porte la BASE (claire/sombre) et les
        // surfaces choisies sont posées en variables inline par-dessus.
        expect(root.dataset.theme).toBe(useUi.getState().customTheme.base);
        expect(root.style.getPropertyValue('--color-chat')).not.toBe('');
      } else {
        expect(root.dataset.theme).toBe(id);
        expect(root.style.getPropertyValue('--color-chat')).toBe('');
      }
      expect(window.localStorage.getItem('accord.theme')).toBe(id);
      expect(useUi.getState().theme).toBe(id);
    }
  });

  it('le thème personnalisé applique les couleurs choisies et réagit à leur changement', () => {
    useUi.getState().setTheme('custom');
    useUi.getState().setCustomTheme({
      fond: '#101020',
      panneaux: '#181828',
      accent: '#ff0080',
      base: 'dark',
    });
    expect(root.style.getPropertyValue('--color-chat')).toBe('16 16 32');
    expect(root.style.getPropertyValue('--color-blurple')).toBe('255 0 128');
    expect(window.localStorage.getItem('accord.theme.custom')).toContain('#ff0080');

    // Retour à un thème de la galerie : les variables inline sont retirées.
    useUi.getState().setTheme('dark');
    expect(root.style.getPropertyValue('--color-chat')).toBe('');
  });

  it('migre sans accroc une préférence historique (dark/light) persistée avant la galerie', async () => {
    // Valeurs stockées par une build antérieure à l'ajout des thèmes teintés :
    // toujours membres de l'union `Theme`, aucune réécriture n'est nécessaire.
    window.localStorage.setItem('accord.theme', 'light');

    vi.resetModules();
    const fresh = await import('./ui');

    expect(fresh.useUi.getState().theme).toBe('light');
    expect(root.dataset.theme).toBe('light');
  });

  it('replie sur le thème sombre pour un id de thème persisté inconnu', async () => {
    window.localStorage.setItem('accord.theme', 'sepia');

    vi.resetModules();
    const fresh = await import('./ui');

    expect(fresh.useUi.getState().theme).toBe('dark');
    expect(root.dataset.theme).toBe('dark');
  });
});

describe('useUi — densité', () => {
  it('applique la densité compacte à la racine et la persiste', () => {
    useUi.getState().setDensity('compact');

    expect(root.dataset.density).toBe('compact');
    expect(window.localStorage.getItem('accord.density')).toBe('compact');
    expect(useUi.getState().density).toBe('compact');
  });
});

describe('useUi — taille de police', () => {
  it('applique l’échelle en pourcentage sur la racine et la persiste', () => {
    useUi.getState().setFontScale(150);

    expect(root.style.fontSize).toBe('150%');
    expect(window.localStorage.getItem('accord.fontScale')).toBe('150');
    expect(useUi.getState().fontScale).toBe(150);
  });
});

describe('stepFontScale', () => {
  it('avance d’un palier dans chaque sens', () => {
    expect(stepFontScale(100, 1)).toBe(125);
    expect(stepFontScale(125, -1)).toBe(100);
  });

  it('reste borné aux extrêmes', () => {
    const min = FONT_SCALES[0];
    const max = FONT_SCALES[FONT_SCALES.length - 1];
    expect(stepFontScale(75, -1)).toBe(min);
    expect(stepFontScale(150, 1)).toBe(max);
  });
});

describe('useUi — zoom clavier', () => {
  it('agrandit, réduit et réinitialise l’échelle de police', () => {
    useUi.getState().setFontScale(100);
    useUi.getState().zoomIn();
    expect(useUi.getState().fontScale).toBe(125);
    useUi.getState().zoomOut();
    expect(useUi.getState().fontScale).toBe(100);
    useUi.getState().setFontScale(150);
    useUi.getState().zoomReset();
    expect(useUi.getState().fontScale).toBe(100);
  });
});

describe('useUi — langue', () => {
  it('persiste la langue choisie', () => {
    useUi.getState().setLang('en');

    expect(window.localStorage.getItem('accord.lang')).toBe('en');
    expect(useUi.getState().lang).toBe('en');

    useUi.getState().setLang('fr');
    expect(window.localStorage.getItem('accord.lang')).toBe('fr');
  });
});

describe('useUi — saut au message (jump)', () => {
  beforeEach(() => {
    useUi.setState({ view: { kind: 'friends' }, jump: null });
  });

  it('requestJump bascule la vue et incrémente le nonce à chaque appel', () => {
    const view = { kind: 'dm', peer: 'pair' } as const;
    useUi.getState().requestJump(view, 'm1');

    const first = useUi.getState().jump;
    expect(useUi.getState().view).toEqual(view);
    expect(first).toMatchObject({ view, msgId: 'm1' });

    useUi.getState().requestJump(view, 'm2');
    expect(useUi.getState().jump?.nonce).toBe((first?.nonce ?? 0) + 1);
  });

  it('clearJump consomme la demande de saut', () => {
    useUi.getState().requestJump({ kind: 'dm', peer: 'pair' }, 'm1');
    useUi.getState().clearJump();

    expect(useUi.getState().jump).toBeNull();
  });

  it('setView efface un saut en attente (navigation ordinaire)', () => {
    useUi.getState().requestJump({ kind: 'dm', peer: 'pair' }, 'm1');
    useUi.getState().setView({ kind: 'friends' });

    expect(useUi.getState().jump).toBeNull();
  });
});

describe('useUi — mémoire de navigation', () => {
  beforeEach(() => {
    useUi.setState({
      view: { kind: 'friends' },
      jump: null,
      lastChannelByServer: {},
      lastDmPeer: null,
    });
  });

  it('setView mémorise le dernier salon consulté par serveur et le persiste', () => {
    useUi.getState().setView({ kind: 'group', groupId: 'g1', channelId: 'c1' });

    expect(useUi.getState().lastChannelByServer).toEqual({ g1: 'c1' });
    expect(
      JSON.parse(window.localStorage.getItem('accord.nav.lastChannelByServer') ?? '{}'),
    ).toEqual({ g1: 'c1' });

    // Un second serveur s'ajoute sans écraser le premier.
    useUi.getState().setView({ kind: 'group', groupId: 'g2', channelId: 'c9' });
    expect(useUi.getState().lastChannelByServer).toEqual({ g1: 'c1', g2: 'c9' });
  });

  it('setView vers un salon null ne modifie pas la mémoire du serveur', () => {
    useUi.getState().setView({ kind: 'group', groupId: 'g1', channelId: 'c1' });
    useUi.getState().setView({ kind: 'group', groupId: 'g1', channelId: null });

    expect(useUi.getState().lastChannelByServer).toEqual({ g1: 'c1' });
  });

  it('setView mémorise le dernier pair de conversation privée et le persiste', () => {
    useUi.getState().setView({ kind: 'dm', peer: 'alice-pk' });

    expect(useUi.getState().lastDmPeer).toBe('alice-pk');
    expect(window.localStorage.getItem('accord.nav.lastDm')).toBe('alice-pk');
  });

  it('setView vers la vue amis ne modifie pas le dernier pair mémorisé', () => {
    useUi.getState().setView({ kind: 'dm', peer: 'alice-pk' });
    useUi.getState().setView({ kind: 'friends' });

    expect(useUi.getState().lastDmPeer).toBe('alice-pk');
  });

  it('requestJump mémorise aussi la navigation (recherche, épingle, citation)', () => {
    useUi.getState().requestJump({ kind: 'dm', peer: 'bob-pk' }, 'm1');

    expect(useUi.getState().lastDmPeer).toBe('bob-pk');
  });

  it('restaure la mémoire de navigation persistée au démarrage', async () => {
    window.localStorage.setItem(
      'accord.nav.lastChannelByServer',
      JSON.stringify({ g1: 'c1' }),
    );
    window.localStorage.setItem('accord.nav.lastDm', 'alice-pk');

    vi.resetModules();
    const fresh = await import('./ui');

    expect(fresh.useUi.getState().lastChannelByServer).toEqual({ g1: 'c1' });
    expect(fresh.useUi.getState().lastDmPeer).toBe('alice-pk');
  });

  it('replie sur une mémoire vide quand la valeur persistée est corrompue', async () => {
    window.localStorage.setItem(
      'accord.nav.lastChannelByServer',
      '{ceci n’est pas du json',
    );

    vi.resetModules();
    const fresh = await import('./ui');

    expect(fresh.useUi.getState().lastChannelByServer).toEqual({});
  });
});

describe('useUi — restauration au démarrage', () => {
  it('restaure les préférences persistées valides', async () => {
    window.localStorage.setItem('accord.theme', 'light');
    window.localStorage.setItem('accord.density', 'compact');
    window.localStorage.setItem('accord.fontScale', '125');

    vi.resetModules();
    const fresh = await import('./ui');

    expect(fresh.useUi.getState().theme).toBe('light');
    expect(fresh.useUi.getState().density).toBe('compact');
    expect(fresh.useUi.getState().fontScale).toBe(125);
    expect(root.dataset.theme).toBe('light');
    expect(root.dataset.density).toBe('compact');
    expect(root.style.fontSize).toBe('125%');
  });

  it('replie sur les défauts quand les valeurs persistées sont invalides', async () => {
    window.localStorage.setItem('accord.theme', 'fluo');
    window.localStorage.setItem('accord.density', 'serré');
    window.localStorage.setItem('accord.fontScale', '400');

    vi.resetModules();
    const fresh = await import('./ui');

    expect(fresh.useUi.getState().theme).toBe('dark');
    expect(fresh.useUi.getState().density).toBe('comfortable');
    expect(fresh.useUi.getState().fontScale).toBe(100);
    expect(root.dataset.theme).toBe('dark');
  });
});

describe('useUi — réduction des animations', () => {
  it('force l’attribut data-motion et le persiste (Activé)', () => {
    useUi.getState().setReducedMotion('on');

    expect(root.dataset.motion).toBe('reduce');
    expect(window.localStorage.getItem('accord.a11y.reducedMotion')).toBe('on');
    expect(useUi.getState().reducedMotion).toBe('on');
  });

  it('retire l’attribut pour Système et Désactivé', () => {
    useUi.getState().setReducedMotion('on');
    useUi.getState().setReducedMotion('off');

    expect(root.dataset.motion).toBeUndefined();
    expect(window.localStorage.getItem('accord.a11y.reducedMotion')).toBe('off');

    useUi.getState().setReducedMotion('on');
    useUi.getState().setReducedMotion('system');

    expect(root.dataset.motion).toBeUndefined();
  });

  it('restaure la préférence persistée au démarrage', async () => {
    window.localStorage.setItem('accord.a11y.reducedMotion', 'on');

    vi.resetModules();
    const fresh = await import('./ui');

    expect(fresh.useUi.getState().reducedMotion).toBe('on');
    expect(root.dataset.motion).toBe('reduce');
  });

  it('replie sur « système » quand la valeur persistée est invalide', async () => {
    window.localStorage.setItem('accord.a11y.reducedMotion', 'flou');

    vi.resetModules();
    const fresh = await import('./ui');

    expect(fresh.useUi.getState().reducedMotion).toBe('system');
  });
});

describe('useUi — saturation', () => {
  it('applique la variable CSS et la persiste', () => {
    useUi.getState().setSaturation(40);

    expect(root.style.getPropertyValue('--saturation')).toBe('40%');
    expect(window.localStorage.getItem('accord.a11y.saturation')).toBe('40');
    expect(useUi.getState().saturation).toBe(40);
  });

  it('borne aux limites [0, 100]', () => {
    useUi.getState().setSaturation(-20);
    expect(useUi.getState().saturation).toBe(0);

    useUi.getState().setSaturation(500);
    expect(useUi.getState().saturation).toBe(100);
  });
});

describe('useUi — texte & médias', () => {
  it('bascule et persiste l’aperçu des médias', () => {
    useUi.getState().setShowMediaPreviews(false);

    expect(useUi.getState().showMediaPreviews).toBe(false);
    expect(window.localStorage.getItem('accord.media.showPreviews')).toBe('false');
  });

  it('bascule et persiste la taille des émojis', () => {
    useUi.getState().setEmojiSize('large');

    expect(useUi.getState().emojiSize).toBe('large');
    expect(window.localStorage.getItem('accord.media.emojiSize')).toBe('large');
  });
});

describe('useUi — notifications (sons, natif, mode)', () => {
  it('bascule et persiste les interrupteurs maîtres', () => {
    useUi.getState().setNotifySoundEnabled(false);
    useUi.getState().setNotifyNative(false);

    expect(useUi.getState().notifySoundEnabled).toBe(false);
    expect(useUi.getState().notifyNative).toBe(false);
    expect(window.localStorage.getItem('accord.notify.soundEnabled')).toBe('false');
    expect(window.localStorage.getItem('accord.notify.native')).toBe('false');
  });

  it('bascule et persiste le mode de filtrage du son', () => {
    useUi.getState().setNotifySoundMode('mentionsOnly');

    expect(useUi.getState().notifySoundMode).toBe('mentionsOnly');
    expect(window.localStorage.getItem('accord.notify.soundMode')).toBe('mentionsOnly');
  });
});

describe('useUi — confidentialité (frappe, statut au démarrage)', () => {
  it('bascule et persiste l’indicateur de frappe', () => {
    useUi.getState().setTypingIndicatorEnabled(false);

    expect(useUi.getState().typingIndicatorEnabled).toBe(false);
    expect(window.localStorage.getItem('accord.privacy.typingIndicator')).toBe('false');
  });

  it('choisit puis efface la présence forcée au démarrage', () => {
    useUi.getState().setStartupPresence('online');
    expect(useUi.getState().startupPresence).toBe('online');
    expect(window.localStorage.getItem('accord.privacy.startupPresence')).toBe('online');

    useUi.getState().setStartupPresence(null);
    expect(useUi.getState().startupPresence).toBeNull();
  });

  it('replie sur null quand la valeur persistée est invalide', async () => {
    window.localStorage.setItem('accord.privacy.startupPresence', 'busy');

    vi.resetModules();
    const fresh = await import('./ui');

    expect(fresh.useUi.getState().startupPresence).toBeNull();
  });
});

describe('useUi — format de l’heure', () => {
  it('bascule et persiste le format des heures', () => {
    useUi.getState().setTimeFormat('12h');

    expect(useUi.getState().timeFormat).toBe('12h');
    expect(window.localStorage.getItem('accord.timeFormat')).toBe('12h');
  });

  it('replie sur « auto » quand rien n’est persisté', async () => {
    vi.resetModules();
    const fresh = await import('./ui');

    expect(fresh.useUi.getState().timeFormat).toBe('auto');
  });
});

describe('useUi — largeurs redimensionnables', () => {
  beforeEach(() => {
    window.localStorage.clear();
    useUi.getState().resetSidebarWidth();
    useUi.getState().resetMembersWidth();
    window.localStorage.clear();
  });

  it('expose les largeurs par défaut au démarrage', () => {
    expect(useUi.getState().sidebarWidth).toBe(SIDEBAR_WIDTH_DEFAULT);
    expect(useUi.getState().membersWidth).toBe(MEMBERS_WIDTH_DEFAULT);
  });

  it('setSidebarWidth applique et persiste une largeur valide', () => {
    useUi.getState().setSidebarWidth(300);

    expect(useUi.getState().sidebarWidth).toBe(300);
    expect(window.localStorage.getItem('accord.layout.sidebarWidth')).toBe('300');
  });

  it('setSidebarWidth borne en dessous du minimum', () => {
    useUi.getState().setSidebarWidth(SIDEBAR_WIDTH_MIN - 50);

    expect(useUi.getState().sidebarWidth).toBe(SIDEBAR_WIDTH_MIN);
  });

  it('setSidebarWidth borne au-dessus du maximum', () => {
    useUi.getState().setSidebarWidth(SIDEBAR_WIDTH_MAX + 50);

    expect(useUi.getState().sidebarWidth).toBe(SIDEBAR_WIDTH_MAX);
  });

  it('resetSidebarWidth restaure et persiste la largeur par défaut', () => {
    useUi.getState().setSidebarWidth(320);
    useUi.getState().resetSidebarWidth();

    expect(useUi.getState().sidebarWidth).toBe(SIDEBAR_WIDTH_DEFAULT);
    expect(window.localStorage.getItem('accord.layout.sidebarWidth')).toBe(
      String(SIDEBAR_WIDTH_DEFAULT),
    );
  });

  it('setMembersWidth applique, borne et persiste', () => {
    useUi.getState().setMembersWidth(260);
    expect(useUi.getState().membersWidth).toBe(260);
    expect(window.localStorage.getItem('accord.layout.membersWidth')).toBe('260');

    useUi.getState().setMembersWidth(MEMBERS_WIDTH_MIN - 10);
    expect(useUi.getState().membersWidth).toBe(MEMBERS_WIDTH_MIN);

    useUi.getState().setMembersWidth(MEMBERS_WIDTH_MAX + 10);
    expect(useUi.getState().membersWidth).toBe(MEMBERS_WIDTH_MAX);
  });

  it('resetMembersWidth restaure la largeur par défaut', () => {
    useUi.getState().setMembersWidth(200);
    useUi.getState().resetMembersWidth();

    expect(useUi.getState().membersWidth).toBe(MEMBERS_WIDTH_DEFAULT);
  });

  it('restaure une largeur persistée valide au démarrage', async () => {
    window.localStorage.setItem('accord.layout.sidebarWidth', '350');
    window.localStorage.setItem('accord.layout.membersWidth', '300');

    vi.resetModules();
    const fresh = await import('./ui');

    expect(fresh.useUi.getState().sidebarWidth).toBe(350);
    expect(fresh.useUi.getState().membersWidth).toBe(300);
  });

  it('replie sur la largeur par défaut quand la valeur persistée est invalide', async () => {
    window.localStorage.setItem('accord.layout.sidebarWidth', 'pas-un-nombre');

    vi.resetModules();
    const fresh = await import('./ui');

    expect(fresh.useUi.getState().sidebarWidth).toBe(SIDEBAR_WIDTH_DEFAULT);
  });

  it('borne une largeur persistée hors plage au démarrage', async () => {
    window.localStorage.setItem('accord.layout.membersWidth', '9999');

    vi.resetModules();
    const fresh = await import('./ui');

    expect(fresh.useUi.getState().membersWidth).toBe(MEMBERS_WIDTH_MAX);
  });
});

describe('useUi — sélecteur rapide', () => {
  beforeEach(() => {
    useUi.setState({ quickSwitcherOpen: false });
  });

  it('ouvre, ferme puis bascule l’état du sélecteur rapide', () => {
    expect(useUi.getState().quickSwitcherOpen).toBe(false);

    useUi.getState().openQuickSwitcher();
    expect(useUi.getState().quickSwitcherOpen).toBe(true);

    useUi.getState().closeQuickSwitcher();
    expect(useUi.getState().quickSwitcherOpen).toBe(false);

    useUi.getState().toggleQuickSwitcher();
    expect(useUi.getState().quickSwitcherOpen).toBe(true);
    useUi.getState().toggleQuickSwitcher();
    expect(useUi.getState().quickSwitcherOpen).toBe(false);
  });
});

describe('useUi — police d’interface', () => {
  it('applique et persiste chaque police, en variable CSS racine', () => {
    for (const f of ['system', 'rounded', 'serif'] as const) {
      useUi.getState().setFontUi(f);
      expect(document.documentElement.style.getPropertyValue('--font-ui')).not.toBe('');
      expect(window.localStorage.getItem('accord.appearance.fontUi')).toBe(f);
      expect(useUi.getState().fontUi).toBe(f);
    }
  });
});
