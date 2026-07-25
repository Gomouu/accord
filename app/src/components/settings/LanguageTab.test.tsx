import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it } from 'vitest';
import { useUi } from '../../stores/ui';
import { LanguageTab } from './LanguageTab';

beforeEach(() => {
  window.localStorage.clear();
  useUi.setState({ lang: 'fr', timeFormat: 'auto' });
});

describe('LanguageTab', () => {
  it('propose les trois langues supportées', () => {
    render(<LanguageTab />);

    for (const label of ['Français', 'English', 'Español']) {
      expect(screen.getByText(label)).toBeInTheDocument();
    }
  });

  it('bascule l’interface en espagnol et affiche les libellés traduits', () => {
    render(<LanguageTab />);

    fireEvent.click(screen.getByText('Español'));

    expect(useUi.getState().lang).toBe('es');
    // Titres de sections rendus par le dictionnaire actif.
    expect(screen.getByText('Idioma')).toBeInTheDocument();
    expect(screen.getByText('Formato de hora')).toBeInTheDocument();
  });

  it('bascule l’interface en portugais et affiche les libellés traduits', () => {
    render(<LanguageTab />);

    fireEvent.click(screen.getByText('Português'));

    expect(useUi.getState().lang).toBe('pt');
    expect(screen.getByText('Idioma')).toBeInTheDocument();
    expect(screen.getByText('Formato da hora')).toBeInTheDocument();
  });

  it('bascule l’interface en allemand et affiche les libellés traduits', () => {
    render(<LanguageTab />);

    fireEvent.click(screen.getByText('Deutsch'));

    expect(useUi.getState().lang).toBe('de');
    expect(screen.getByText('Sprache')).toBeInTheDocument();
    expect(screen.getByText('Format der Uhrzeit')).toBeInTheDocument();
  });
});
