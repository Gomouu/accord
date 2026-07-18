/**
 * Store de mise à jour intégrée : cycle vérification → téléchargement →
 * redémarrage, traitement différencié des échecs (silencieux en automatique,
 * visibles en manuel) et écart de version (« Plus tard »).
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';
import { checkForUpdate, downloadAndInstall, restartApp } from '../lib/updater';
import { useUpdater } from './updater';

vi.mock('../lib/updater', () => ({
  RELEASES_URL: 'https://github.com/Gomouu/accord/releases/latest',
  checkForUpdate: vi.fn(),
  downloadAndInstall: vi.fn(),
  restartApp: vi.fn(),
}));

const mockCheck = vi.mocked(checkForUpdate);
const mockInstall = vi.mocked(downloadAndInstall);
const mockRestart = vi.mocked(restartApp);

beforeEach(() => {
  vi.clearAllMocks();
  useUpdater.setState({
    status: 'idle',
    version: null,
    notes: null,
    progress: null,
    error: null,
    dismissedVersion: null,
  });
});

describe('check', () => {
  it('passe à « à jour » quand aucune version plus récente n’existe', async () => {
    mockCheck.mockResolvedValue(null);

    await useUpdater.getState().check();

    expect(useUpdater.getState().status).toBe('upToDate');
    expect(useUpdater.getState().version).toBeNull();
  });

  it('expose la version et les notes quand une mise à jour est trouvée', async () => {
    mockCheck.mockResolvedValue({ version: '2.1.0', notes: 'Nouveautés' });

    await useUpdater.getState().check();

    const state = useUpdater.getState();
    expect(state.status).toBe('available');
    expect(state.version).toBe('2.1.0');
    expect(state.notes).toBe('Nouveautés');
  });

  it('échoue en silence lors d’une vérification automatique', async () => {
    mockCheck.mockRejectedValue(new Error('réseau coupé'));

    await useUpdater.getState().check();

    expect(useUpdater.getState().status).toBe('idle');
    expect(useUpdater.getState().error).toBeNull();
  });

  it('rend l’échec visible lors d’une vérification manuelle', async () => {
    mockCheck.mockRejectedValue(new Error('réseau coupé'));

    await useUpdater.getState().check(true);

    expect(useUpdater.getState().status).toBe('error');
    expect(useUpdater.getState().error).toBe('réseau coupé');
  });

  it('n’interrompt jamais un téléchargement en cours', async () => {
    useUpdater.setState({ status: 'downloading' });

    await useUpdater.getState().check();

    expect(mockCheck).not.toHaveBeenCalled();
    expect(useUpdater.getState().status).toBe('downloading');
  });
});

describe('install', () => {
  it('suit l’avancement puis passe à « prête »', async () => {
    useUpdater.setState({ status: 'available', version: '2.1.0' });
    mockInstall.mockImplementation(async (onProgress) => {
      onProgress(50, 200);
      expect(useUpdater.getState().progress).toBe(0.25);
      onProgress(200, 200);
    });

    await useUpdater.getState().install();

    expect(useUpdater.getState().status).toBe('ready');
    expect(useUpdater.getState().progress).toBe(1);
  });

  it('garde un avancement indéterminé quand la taille totale est inconnue', async () => {
    useUpdater.setState({ status: 'available', version: '2.1.0' });
    mockInstall.mockImplementation(async (onProgress) => {
      onProgress(50, null);
      expect(useUpdater.getState().progress).toBeNull();
    });

    await useUpdater.getState().install();

    expect(useUpdater.getState().status).toBe('ready');
  });

  it('bascule en erreur si l’installation échoue', async () => {
    useUpdater.setState({ status: 'available', version: '2.1.0' });
    mockInstall.mockRejectedValue(new Error('signature invalide'));

    await useUpdater.getState().install();

    expect(useUpdater.getState().status).toBe('error');
    expect(useUpdater.getState().error).toBe('signature invalide');
  });

  it('ne fait rien hors des états « disponible » et « erreur »', async () => {
    await useUpdater.getState().install();

    expect(mockInstall).not.toHaveBeenCalled();
    expect(useUpdater.getState().status).toBe('idle');
  });
});

describe('restart', () => {
  it('relance l’application', async () => {
    mockRestart.mockResolvedValue(undefined);

    await useUpdater.getState().restart();

    expect(mockRestart).toHaveBeenCalledOnce();
  });

  it('bascule en erreur si le redémarrage échoue', async () => {
    mockRestart.mockRejectedValue(new Error('relance impossible'));

    await useUpdater.getState().restart();

    expect(useUpdater.getState().status).toBe('error');
    expect(useUpdater.getState().error).toBe('relance impossible');
  });
});

describe('dismissBanner', () => {
  it('écarte la version proposée', () => {
    useUpdater.setState({ status: 'available', version: '2.1.0' });

    useUpdater.getState().dismissBanner();

    expect(useUpdater.getState().dismissedVersion).toBe('2.1.0');
  });

  it('conserve l’écart précédent quand aucune version n’est proposée', () => {
    useUpdater.setState({ dismissedVersion: '2.1.0' });

    useUpdater.getState().dismissBanner();

    expect(useUpdater.getState().dismissedVersion).toBe('2.1.0');
  });
});
