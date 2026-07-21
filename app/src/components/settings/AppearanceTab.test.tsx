import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { PERSO_DEFAUT } from '../../lib/customTheme';
import { THEME_IDS, useUi } from '../../stores/ui';
import { AppearanceTab } from './AppearanceTab';

beforeEach(() => {
  Object.defineProperty(document, 'startViewTransition', {
    configurable: true,
    value: undefined,
    writable: true,
  });
  delete document.documentElement.dataset.themeTransition;
  window.localStorage.clear();
  useUi.setState({ lang: 'fr', customTheme: PERSO_DEFAUT });
  useUi.getState().setTheme('dark');
  useUi.getState().setDensity('comfortable');
  useUi.getState().setReducedMotion('system');
  window.localStorage.clear();
});

describe('AppearanceTab theme gallery', () => {
  it('renders a complete conversation for every theme', () => {
    render(<AppearanceTab />);

    const cards = screen.getAllByRole('radio');
    expect(cards).toHaveLength(THEME_IDS.length);
    for (const card of cards) {
      expect(card.querySelectorAll('.theme-conversation-preview')).toHaveLength(1);
      expect(card.querySelectorAll('.theme-conversation-preview__message')).toHaveLength(
        2,
      );
      expect(
        card.querySelector('.theme-conversation-preview__composer'),
      ).toHaveTextContent('@vous');
    }
  });

  it('keeps figurative artwork inside its conversation preview', () => {
    render(<AppearanceTab />);

    const wisteria = screen.getByRole('radio', { name: 'Glycines nocturnes' });
    expect(
      wisteria.querySelector(
        '[data-theme="wisteria"] .theme-atmosphere[data-scene="wisteria"]',
      ),
    ).toBeInTheDocument();
  });

  it('uses a view transition when selecting a theme', async () => {
    const startViewTransition = vi.fn((update: () => void) => {
      update();
      return { finished: Promise.resolve() };
    });
    Object.defineProperty(document, 'startViewTransition', {
      configurable: true,
      value: startViewTransition,
      writable: true,
    });
    render(<AppearanceTab />);

    fireEvent.click(screen.getByRole('radio', { name: 'Clair' }));

    expect(startViewTransition).toHaveBeenCalledOnce();
    expect(document.documentElement.dataset.theme).toBe('light');
    await startViewTransition.mock.results[0]?.value.finished;
    await Promise.resolve();
    expect(document.documentElement).not.toHaveAttribute('data-theme-transition');
  });

  it('skips the transition when reduced motion is enabled', () => {
    const startViewTransition = vi.fn((update: () => void) => {
      update();
      return { finished: Promise.resolve() };
    });
    Object.defineProperty(document, 'startViewTransition', {
      configurable: true,
      value: startViewTransition,
      writable: true,
    });
    useUi.getState().setReducedMotion('on');
    render(<AppearanceTab />);

    fireEvent.click(screen.getByRole('radio', { name: 'Clair' }));

    expect(startViewTransition).not.toHaveBeenCalled();
    expect(document.documentElement.dataset.theme).toBe('light');
  });
});
