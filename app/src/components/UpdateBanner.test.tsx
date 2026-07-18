/**
 * Bandeau de mise à jour : visibilité selon l'état du store, actions
 * installer/redémarrer/plus tard, respect de la version écartée.
 */

import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { downloadAndInstall, restartApp } from '../lib/updater';
import { useUpdater } from '../stores/updater';
import { UpdateBanner } from './UpdateBanner';

vi.mock('../lib/updater', () => ({
  RELEASES_URL: 'https://github.com/Gomouu/accord/releases/latest',
  checkForUpdate: vi.fn(),
  downloadAndInstall: vi.fn(),
  restartApp: vi.fn(),
}));

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

describe('UpdateBanner', () => {
  it('reste invisible sans mise à jour', () => {
    render(<UpdateBanner />);

    expect(screen.queryByRole('status')).not.toBeInTheDocument();
  });

  it('annonce la version disponible et lance l’installation', async () => {
    const user = userEvent.setup();
    useUpdater.setState({ status: 'available', version: '2.1.0' });
    mockInstall.mockResolvedValue(undefined);
    render(<UpdateBanner />);

    expect(screen.getByText('Version 2.1.0 available')).toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: 'Install' }));

    expect(mockInstall).toHaveBeenCalledOnce();
  });

  it('« Plus tard » masque le bandeau pour cette version', async () => {
    const user = userEvent.setup();
    useUpdater.setState({ status: 'available', version: '2.1.0' });
    render(<UpdateBanner />);

    await user.click(screen.getByRole('button', { name: 'Later' }));

    expect(screen.queryByRole('status')).not.toBeInTheDocument();
  });

  it('reste invisible pour une version déjà écartée', () => {
    useUpdater.setState({
      status: 'available',
      version: '2.1.0',
      dismissedVersion: '2.1.0',
    });
    render(<UpdateBanner />);

    expect(screen.queryByRole('status')).not.toBeInTheDocument();
  });

  it('affiche l’avancement du téléchargement', () => {
    useUpdater.setState({ status: 'downloading', version: '2.1.0', progress: 0.42 });
    render(<UpdateBanner />);

    expect(screen.getByText('Downloading… 42%')).toBeInTheDocument();
  });

  it('propose le redémarrage quand la mise à jour est prête', async () => {
    const user = userEvent.setup();
    useUpdater.setState({ status: 'ready', version: '2.1.0' });
    mockRestart.mockResolvedValue(undefined);
    render(<UpdateBanner />);

    expect(
      screen.getByText('Update ready — restart Accord to apply it.'),
    ).toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: 'Restart' }));

    expect(mockRestart).toHaveBeenCalledOnce();
  });

  it('permet de réessayer après un échec', async () => {
    const user = userEvent.setup();
    useUpdater.setState({
      status: 'error',
      version: '2.1.0',
      error: 'signature invalide',
    });
    mockInstall.mockResolvedValue(undefined);
    render(<UpdateBanner />);

    expect(screen.getByText('The update failed: signature invalide')).toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: 'Retry' }));

    expect(mockInstall).toHaveBeenCalledOnce();
  });
});
