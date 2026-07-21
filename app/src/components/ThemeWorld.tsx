import { useUi } from '../stores/ui';
import { ThemeAtmosphere } from './ThemeAtmosphere';

export function ThemeWorld() {
  const theme = useUi((state) => state.theme);

  return (
    <span aria-hidden data-theme-world={theme} className="theme-world">
      <ThemeAtmosphere theme={theme} />
      <span className="theme-world__legacy-motion" />
    </span>
  );
}
