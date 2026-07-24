/**
 * Panneau de partage d'écran (v5) : visible seulement en appel actif, bouton
 * de partage désactivé proprement quand le runtime ne supporte pas la capture,
 * et visionneuse du partage distant liée au décodeur.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Mock } from 'vitest';
import { render, screen } from '@testing-library/react';
import { useCalls } from '../stores/calls';
import { useUi } from '../stores/ui';
import { ScreenShare } from './ScreenShare';

vi.mock('../lib/screenController', () => ({
  screenShareSupported: vi.fn(() => true),
  remotePlayback: { attach: vi.fn() },
  startLocalShare: vi.fn(),
  stopLocalShare: vi.fn(),
  resetRemote: vi.fn(),
  pushRemoteFrame: vi.fn(),
}));

import { remotePlayback, screenShareSupported } from '../lib/screenController';

const supportedMock = screenShareSupported as unknown as Mock;
const attachMock = remotePlayback.attach as unknown as Mock;

beforeEach(() => {
  useUi.setState({ lang: 'fr' });
  useCalls.setState({
    phase: 'idle',
    peer: null,
    callId: null,
    sincePhaseMs: null,
    localSharing: false,
    remoteSharing: false,
  });
  supportedMock.mockReturnValue(true);
  attachMock.mockReset();
});

describe('ScreenShare', () => {
  it('ne rend rien hors appel actif', () => {
    const { container } = render(<ScreenShare />);
    expect(container).toBeEmptyDOMElement();
  });

  it('affiche le bouton de partage en appel actif (runtime supporté)', () => {
    useCalls.setState({ phase: 'active' });
    render(<ScreenShare />);

    const button = screen.getByRole('button', { name: 'Partager l’écran' });
    expect(button).toBeEnabled();
    expect(button).toHaveAttribute('aria-pressed', 'false');
  });

  it('désactive le bouton et l’explique quand le runtime ne supporte pas', () => {
    supportedMock.mockReturnValue(false);
    useCalls.setState({ phase: 'active' });
    render(<ScreenShare />);

    const button = screen.getByRole('button', { name: 'Partager l’écran' });
    expect(button).toBeDisabled();
    expect(button).toHaveAttribute(
      'title',
      'Partage d’écran indisponible sur cet appareil',
    );
  });

  it('bascule le libellé du bouton quand on partage déjà', () => {
    useCalls.setState({ phase: 'active', localSharing: true });
    render(<ScreenShare />);

    const button = screen.getByRole('button', { name: 'Arrêter le partage' });
    expect(button).toHaveAttribute('aria-pressed', 'true');
  });

  it('montre la visionneuse et la lie au décodeur quand le pair partage', () => {
    useCalls.setState({ phase: 'active', remoteSharing: true });
    render(<ScreenShare />);

    // Le canevas de la visionneuse porte le libellé accessible et est branché.
    expect(screen.getByLabelText('Écran partagé')).toBeInTheDocument();
    expect(attachMock).toHaveBeenCalledTimes(1);
  });
});
