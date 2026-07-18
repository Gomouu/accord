import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { ThemeAtmosphere } from './ThemeAtmosphere';

describe('ThemeAtmosphere', () => {
  it.each(['sakura', 'wisteria', 'lotus', 'manga', 'shojo'] as const)(
    'rend la scène figurative %s',
    (theme) => {
      const { container } = render(<ThemeAtmosphere theme={theme} />);
      const atmosphere = container.querySelector('.theme-atmosphere');
      expect(atmosphere).toHaveAttribute('aria-hidden', 'true');
      expect(atmosphere).toHaveAttribute('data-scene', theme);
      expect(atmosphere?.querySelector('.theme-atmosphere__art')).not.toBeNull();
    },
  );

  it('ne rend aucune couche pour un thème sans scène figurative', () => {
    render(<ThemeAtmosphere theme="dark" />);
    expect(screen.queryByText(/./)).toBeNull();
  });
});
