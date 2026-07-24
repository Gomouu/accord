/**
 * Grille vidéo d'appel : visible en appel actif ou en salon vocal, une tuile
 * par flux reçu, épinglage, repli en avatars, boutons désactivés proprement
 * quand le runtime ne supporte pas la source.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Mock } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useCalls } from '../stores/calls';
import { useFriends } from '../stores/friends';
import { useUi } from '../stores/ui';
import { useVoice } from '../stores/voice';
import { CallVideo } from './CallVideo';

vi.mock('../lib/mediaController', () => ({
  videoSourceSupported: vi.fn(() => true),
  playbackFor: vi.fn(() => ({ attach: vi.fn() })),
  localPreviewStream: vi.fn(() => null),
  startLocalStream: vi.fn(),
  stopLocalStream: vi.fn(),
  stopAllLocal: vi.fn(),
  resetRemote: vi.fn(),
  resetAllRemote: vi.fn(),
  pushRemoteFrame: vi.fn(),
}));

import { playbackFor, videoSourceSupported } from '../lib/mediaController';

const supportedMock = videoSourceSupported as unknown as Mock;
const playbackForMock = playbackFor as unknown as Mock;

/** Contact minimal : la grille lit le nom et l'avatar depuis le carnet. */
function contact(pubkey: string, name: string) {
  return {
    pubkey,
    node_id: pubkey,
    friend_code: pubkey,
    display_name: name,
    state: 'friend' as const,
    added_ms: 0,
    last_seen_ms: 0,
    online: true,
    avatar: null,
    banner: null,
    bio: null,
    status_text: null,
    avatar_decoration: null,
    unread: 0,
  };
}

/** Participant silencieux : la grille ne lit que la présence, pas l'audio. */
function participant() {
  return {
    speaking: false,
    muted: false,
    deafened: false,
    volume: 100,
    serverMuted: false,
    serverDeafened: false,
    prioritySpeaker: false,
  };
}

beforeEach(() => {
  useUi.setState({ lang: 'fr' });
  useCalls.setState({
    phase: 'idle',
    peer: null,
    callId: null,
    sincePhaseMs: null,
    localSharing: false,
    localCamera: false,
    remoteVideo: {},
  });
  useVoice.setState({ active: null, participants: new Map() });
  useFriends.setState({
    contacts: [contact('alice', 'Alice'), contact('bob', 'Bob')],
  });
  supportedMock.mockReturnValue(true);
  playbackForMock.mockClear();
  playbackForMock.mockImplementation(() => ({ attach: vi.fn() }));
});

describe('CallVideo', () => {
  it('ne rend rien hors appel actif et hors salon vocal', () => {
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
    expect(screen.getByLabelText('Votre caméra')).toBeInTheDocument();
  });

  it('nomme chaque tuile d’après son émetteur', () => {
    useCalls.setState({
      phase: 'active',
      remoteVideo: { alice: { screen: true, camera: false } },
    });
    render(<CallVideo />);

    // Le nom, pas un libellé générique : à plusieurs émetteurs, « Écran
    // partagé » ne dirait pas de qui.
    expect(screen.getByLabelText(/^Alice — /)).toBeInTheDocument();
    expect(playbackForMock).toHaveBeenCalledWith('alice', 'screen');
  });

  it('affiche une tuile par flux, y compris deux flux d’une même personne', () => {
    useCalls.setState({
      phase: 'active',
      remoteVideo: { alice: { screen: true, camera: true } },
    });
    render(<CallVideo />);

    expect(screen.getByLabelText('Alice')).toBeInTheDocument();
    expect(screen.getByLabelText(/^Alice — /)).toBeInTheDocument();
    expect(playbackForMock).toHaveBeenCalledWith('alice', 'camera');
    expect(playbackForMock).toHaveBeenCalledWith('alice', 'screen');
  });

  it('sépare les flux de deux émetteurs simultanés', () => {
    // Le cas que l'ancienne mise en page ne savait pas rendre : une visionneuse
    // unique par source aurait entrelacé les deux flux.
    useCalls.setState({
      phase: 'active',
      remoteVideo: {
        alice: { screen: false, camera: true },
        bob: { screen: false, camera: true },
      },
    });
    render(<CallVideo />);

    expect(screen.getByLabelText('Alice')).toBeInTheDocument();
    expect(screen.getByLabelText('Bob')).toBeInTheDocument();
    expect(playbackForMock).toHaveBeenCalledWith('alice', 'camera');
    expect(playbackForMock).toHaveBeenCalledWith('bob', 'camera');
  });

  it('épingle un flux et n’affiche plus que lui, puis revient à la grille', async () => {
    const user = userEvent.setup();
    useCalls.setState({
      phase: 'active',
      remoteVideo: {
        alice: { screen: false, camera: true },
        bob: { screen: false, camera: true },
      },
    });
    render(<CallVideo />);

    const [epingler] = screen.getAllByRole('button', { name: 'Épingler' });
    await user.click(epingler!);

    expect(screen.queryByLabelText('Bob')).not.toBeInTheDocument();
    expect(screen.getByLabelText('Alice')).toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: 'Retirer l’épingle' }));
    expect(screen.getByLabelText('Bob')).toBeInTheDocument();
  });

  it('libère l’épingle quand le flux épinglé s’arrête', async () => {
    const user = userEvent.setup();
    useCalls.setState({
      phase: 'active',
      remoteVideo: {
        alice: { screen: false, camera: true },
        bob: { screen: false, camera: true },
      },
    });
    const { rerender } = render(<CallVideo />);
    await user.click(screen.getAllByRole('button', { name: 'Épingler' })[0]!);

    // Alice coupe sa caméra : figer une tuile morte serait pire que rien.
    useCalls.setState({ remoteVideo: { bob: { screen: false, camera: true } } });
    rerender(<CallVideo />);

    expect(screen.getByLabelText('Bob')).toBeInTheDocument();
  });

  it('se replie sur les avatars en salon vocal quand personne ne diffuse', () => {
    useVoice.setState({
      active: { groupId: 'g', channelId: 'c', muted: false, isCall: false },
      participants: new Map([
        ['alice', participant()],
        ['bob', participant()],
      ]),
    });
    render(<CallVideo />);

    // Pas de canevas noirs vides : les personnes présentes restent visibles.
    expect(
      screen.getByRole('group', { name: 'Participants en vidéo' }),
    ).toBeInTheDocument();
    expect(screen.getByText('Alice')).toBeInTheDocument();
    expect(screen.getByText('Bob')).toBeInTheDocument();
    expect(playbackForMock).not.toHaveBeenCalled();
  });
});
