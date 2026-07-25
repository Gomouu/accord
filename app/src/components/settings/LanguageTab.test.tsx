import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it } from 'vitest';
import { LANGS, type Lang } from '../../i18n';
import { settingsDictionaries } from '../../i18n/all';
import { useUi } from '../../stores/ui';
import { LanguageTab } from './LanguageTab';

beforeEach(() => {
  window.localStorage.clear();
  useUi.setState({ lang: 'fr', timeFormat: 'auto' });
});

/** Nom natif d'une langue, tel que l'onglet doit l'afficher. */
function nativeName(lang: Lang): string {
  const names: Record<Lang, string> = settingsDictionaries.fr.settings.languageNames;
  return names[lang];
}

describe('LanguageTab', () => {
  it('propose toutes les langues déclarées, sous leur nom natif', () => {
    render(<LanguageTab />);

    for (const lang of LANGS) {
      expect(screen.getByText(nativeName(lang))).toBeInTheDocument();
    }
  });

  // Une langue ajoutée est couverte sans toucher au test : c'est le
  // dictionnaire lui-même qui fournit les libellés attendus, plutôt qu'une
  // traduction recopiée ici — laquelle finirait par mentir sur la vraie.
  it.each(LANGS)('bascule l’interface en « %s » et rend son dictionnaire', (lang) => {
    render(<LanguageTab />);

    fireEvent.click(screen.getByText(nativeName(lang)));

    expect(useUi.getState().lang).toBe(lang);
    const attendu = settingsDictionaries[lang].settings;
    expect(screen.getByText(attendu.language)).toBeInTheDocument();
    expect(screen.getByText(attendu.timeFormatTitle)).toBeInTheDocument();
  });
});
