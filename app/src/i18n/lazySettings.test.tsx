/**
 * Chargement paresseux de l'extension de réglages — le chemin que la suite de
 * tests ne voit jamais autrement.
 *
 * `src/test/setup.ts` enregistre les dix extensions d'emblée pour qu'aucun
 * rendu d'onglet ne suspende : c'est commode, mais ça masque exactement ce que
 * fait la production, où l'extension descend en chunk séparé. Ces tests
 * repartent donc d'un registre de modules neuf (`vi.resetModules`), dans lequel
 * rien n'est encore enregistré.
 */

import { Suspense } from 'react';
import { render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

/** Une instance neuve du module i18n, dont les caches sont vides. */
async function i18nNeuf(): Promise<typeof import('./index')> {
  vi.resetModules();
  return await import('./index');
}

beforeEach(() => {
  window.localStorage.clear();
});

describe('extension de réglages', () => {
  it('est absente avant chargement, présente après', async () => {
    const i18n = await i18nNeuf();

    expect(i18n.settingsDictionary('fr')).toBeNull();

    const dict = await i18n.loadSettingsDict('fr');

    expect(dict.settings.autoLockOff).toBe('Désactivé');
    expect(i18n.settingsDictionary('fr')).toBe(dict);
  });

  it('rend la même promesse à deux appels concurrents', async () => {
    const i18n = await i18nNeuf();

    // 🔒 L'invariant dont dépend `useSettingsT` : il jette cette promesse à
    // React à chaque rendu suspendu. Une promesse neuve à chaque appel
    // relancerait un téléchargement par rendu, sans jamais converger.
    const premier = i18n.loadSettingsDict('en');
    const second = i18n.loadSettingsDict('en');

    expect(premier).toBe(second);
    await premier;
  });

  it('charge bien la langue demandée, pas le français', async () => {
    const i18n = await i18nNeuf();

    const dict = await i18n.loadSettingsDict('de');

    expect(dict.settings.autoLockOff).toBe('Aus');
    // Le français n'a pas été tiré au passage : chaque langue a son chunk.
    expect(i18n.settingsDictionary('fr')).toBeNull();
  });

  it('affiche le repli de Suspense, puis le libellé une fois l’extension là', async () => {
    vi.resetModules();
    const { useSettingsT, useUi } = await import('../stores/ui');
    // La langue initiale suit `navigator.language`, que jsdom fixe à l'anglais :
    // on la force pour que le libellé attendu ne dépende pas de l'environnement.
    useUi.setState({ lang: 'fr' });

    function Sonde() {
      const ts = useSettingsT();
      return <span>{ts.settings.autoLockOff}</span>;
    }

    render(
      <Suspense fallback={<span>chargement</span>}>
        <Sonde />
      </Suspense>,
    );

    // Le rendu a suspendu : c'est le repli qui est à l'écran, pas le libellé.
    expect(screen.getByText('chargement')).toBeInTheDocument();

    expect(await screen.findByText('Désactivé')).toBeInTheDocument();
    expect(screen.queryByText('chargement')).not.toBeInTheDocument();
  });
});
