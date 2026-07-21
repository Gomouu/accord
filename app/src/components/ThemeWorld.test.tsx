import { act, render } from '@testing-library/react';
import { beforeEach, describe, expect, it } from 'vitest';
import { useUi } from '../stores/ui';
import { ThemeWorld } from './ThemeWorld';

beforeEach(() => {
  useUi.setState({ theme: 'dark' });
});

describe('ThemeWorld', () => {
  it('renders a figurative scene across the global surface', () => {
    useUi.setState({ theme: 'wisteria' });
    const { container } = render(<ThemeWorld />);
    const world = container.querySelector('.theme-world');

    expect(world).toHaveAttribute('data-theme-world', 'wisteria');
    expect(world?.querySelector('[data-scene="wisteria"]')).not.toBeNull();
    expect(world?.querySelector('.theme-world__legacy-motion')).not.toBeNull();
  });

  it('follows active theme changes immediately', () => {
    useUi.setState({ theme: 'sakura' });
    const { container } = render(<ThemeWorld />);

    act(() => useUi.setState({ theme: 'lotus' }));

    expect(container.querySelector('.theme-world')).toHaveAttribute(
      'data-theme-world',
      'lotus',
    );
    expect(container.querySelector('[data-scene="sakura"]')).toBeNull();
    expect(container.querySelector('[data-scene="lotus"]')).not.toBeNull();
  });

  it('keeps the motion layer for non-figurative animated themes', () => {
    useUi.setState({ theme: 'nebula' });
    const { container } = render(<ThemeWorld />);

    expect(container.querySelector('.theme-world')).toHaveAttribute(
      'data-theme-world',
      'nebula',
    );
    expect(container.querySelector('.theme-atmosphere')).toBeNull();
    expect(container.querySelector('.theme-world__legacy-motion')).not.toBeNull();
  });
});
