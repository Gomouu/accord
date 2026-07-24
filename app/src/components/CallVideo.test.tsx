/**
 * Vidéo d'appel (v5 écran, v6 caméra) : panneau visible seulement en appel
 * actif, boutons désactivés proprement quand le runtime ne supporte pas la
 * source, vues distantes liées aux décodeurs.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Mock } from 'vitest';
import { render, screen } from '@testing-library/react';
import { useCalls } from '../stores/calls';
import { useUi } from '../stores/ui';
import { CallVideo } from './CallVideo';

vi.mock('../lib/mediaController', () => ({
  videoSourceSupported: vi.fn(() => true),
  remotePlayback: { screen: { attach: vi.fn() }, camera: { attach: vi.fn() } },
  localPreviewStream: vi.fn(() => null),
  startLocalStream: vi.fn(),
  stopLocalStream: vi.fn(),
  stopAllLocal: vi.fn(),
  resetRemote: vi.fn(),
  resetAllRemote: vi.fn(),
  pushRemoteFrame: vi.fn(),
}));

import { remotePlayback, videoSourceSupported } from '../lib/mediaController';

const supportedMock = videoSourceSupported as unknown as Mock;
const attachScreen = remotePlayback.screen.attach as unknown as Mock;
const attachCamera = remotePlayback.camera.attach as unknown as Mock;

beforeEach(() => {
  useUi.setState({ lang: 'fr' });
  useCalls.setState({
    phase: 'idle',
    peer: null,
    callId: null,
    sincePhaseMs: null,
    localSharing: false,
    remoteSharing: false,
    localCamera: false,
    remoteCamera: false,
  });
  supportedMock.mockReturnValue(true);
  attachScreen.mockReset();
  attachCamera.mockReset();
});

describe('CallVideo', () => {
  it('ne rend rien hors appel actif', () => {
    const { container } = render(<CallVideo />);
    expect(container).toBeEmptyDOMElement();
  });

  it('propose caméra et partage d’écran en appel actif', () => {
    useCalls.setState({ phase: 'active' });
    render(<CallVideo />);

    expect(screen.getByRole('button', { name: 'Activer la caméra' })).toBeEnabled();
    expect(screen.getByRole('button', { name: 'Partager l’écran' })).toBeEnabled();
  });

  it('désactive les boutons et l’explique quand le runtime ne supporte pas', () => {
    supportedMock.mockReturnValue(false);
    useCalls.setState({ phase: 'active' });
    render(<CallVideo />);

    const camera = screen.getByRole('button', { name: 'Activer la caméra' });
    expect(camera).toBeDisabled();
    expect(camera).toHaveAttribute('title', 'Caméra indisponible sur cet appareil');
    expect(screen.getByRole('button', { name: 'Partager l’écran' })).toBeDisabled();
  });

  it('bascule les libellés quand les deux flux locaux sont actifs', () => {
    useCalls.setState({ phase: 'active', localSharing: true, localCamera: true });
    render(<CallVideo />);

    expect(screen.getByRole('button', { name: 'Couper la caméra' })).toHaveAttribute(
      'aria-pressed',
      'true',
    );
    expect(screen.getByRole('button', { name: 'Arrêter le partage' })).toHaveAttribute(
      'aria-pressed',
      'true',
    );
    // Aperçu local de sa propre caméra.
    expect(screen.getByLabelText('Votre caméra')).toBeInTheDocument();
  });

  it('montre la vue écran distante et la lie à son décodeur', () => {
    useCalls.setState({ phase: 'active', remoteSharing: true });
    render(<CallVideo />);

    expect(screen.getByLabelText('Écran partagé')).toBeInTheDocument();
    expect(attachScreen).toHaveBeenCalledTimes(1);
    expect(attachCamera).not.toHaveBeenCalled();
  });

  it('affiche les deux vues distantes simultanément (caméra + écran)', () => {
    useCalls.setState({ phase: 'active', remoteSharing: true, remoteCamera: true });
    render(<CallVideo />);

    expect(screen.getByLabelText('Caméra')).toBeInTheDocument();
    expect(screen.getByLabelText('Écran partagé')).toBeInTheDocument();
    expect(attachCamera).toHaveBeenCalledTimes(1);
    expect(attachScreen).toHaveBeenCalledTimes(1);
  });
});
