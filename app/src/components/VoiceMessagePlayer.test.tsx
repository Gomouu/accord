/**
 * Lecteur de message vocal : progression du téléchargement puis lecteur,
 * état d'erreur (fichier introuvable / flux indécodable) relançable (bouton
 * réessayer), classification des rejets de `play()` (AbortError transitoire
 * vs vrai échec), durée relue du nom de pièce (`voice-12.4s.m4a`), bascule
 * lecture/pause, coordination « un seul lecteur actif à la fois »
 * (`stores/recorder.ts`). `HTMLMediaElement.play/pause` sont simulés : jsdom
 * ne les implémente pas.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Mock } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import type { FileAttachment } from '../lib/api';
import { useRecorder } from '../stores/recorder';
import { useUi } from '../stores/ui';
import { VoiceMessagePlayer } from './VoiceMessagePlayer';

vi.mock('../lib/files', () => ({
  lireFichier: vi.fn(),
  statutFichier: vi.fn(async () => ({
    known: false,
    complete: false,
    done: 0,
    total: 0,
  })),
  observerProgression: vi.fn(() => () => {}),
}));

import { lireFichier } from '../lib/files';

const lireMock = lireFichier as unknown as Mock;

function piece(over: Partial<FileAttachment> = {}): FileAttachment {
  return {
    merkle_root: 'ab'.repeat(32),
    name: 'voice-message.webm',
    size: 2048,
    mime: 'audio/webm;codecs=opus',
    ...over,
  };
}

let playSpy: Mock;

beforeEach(() => {
  useUi.setState({ lang: 'fr' });
  useRecorder.setState({ activePlayer: null });
  lireMock.mockReset();
  playSpy = vi.fn(() => Promise.resolve());
  vi.spyOn(window.HTMLMediaElement.prototype, 'play').mockImplementation(playSpy);
  vi.spyOn(window.HTMLMediaElement.prototype, 'pause').mockImplementation(() => {});
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe('VoiceMessagePlayer — chargement', () => {
  it('affiche la progression puis le lecteur une fois le blob prêt', async () => {
    lireMock.mockResolvedValueOnce('data:audio/webm;base64,AA==');
    render(<VoiceMessagePlayer piece={piece()} />);

    expect(
      await screen.findByRole('button', { name: 'Lire le message vocal' }),
    ).toBeInTheDocument();
  });

  it('affiche un état d’erreur si le fichier est introuvable, sans planter', async () => {
    lireMock.mockRejectedValueOnce(new Error('introuvable'));
    render(<VoiceMessagePlayer piece={piece()} />);

    expect(await screen.findByText('Message vocal indisponible')).toBeInTheDocument();
  });

  it('cycle la vitesse 1× → 1,5× → 2× → 1× et l’applique à l’audio', async () => {
    lireMock.mockResolvedValueOnce('data:audio/webm;base64,AA==');
    const { container } = render(<VoiceMessagePlayer piece={piece()} />);
    await screen.findByRole('button', { name: 'Lire le message vocal' });
    const audio = container.querySelector('audio') as HTMLAudioElement;
    const bouton = screen.getByRole('button', { name: 'Vitesse de lecture' });

    expect(bouton).toHaveTextContent('1×');
    expect(audio.playbackRate).toBe(1);

    fireEvent.click(bouton);
    expect(bouton).toHaveTextContent('1.5×');
    expect(audio.playbackRate).toBe(1.5);

    fireEvent.click(bouton);
    expect(bouton).toHaveTextContent('2×');
    expect(audio.playbackRate).toBe(2);

    fireEvent.click(bouton);
    expect(bouton).toHaveTextContent('1×');
    expect(audio.playbackRate).toBe(1);
  });

  it('bouton réessayer : relance lireFichier et rend le lecteur au succès', async () => {
    lireMock.mockRejectedValueOnce(new Error('introuvable'));
    lireMock.mockResolvedValueOnce('data:audio/webm;base64,AA==');
    render(<VoiceMessagePlayer piece={piece()} />);

    fireEvent.click(await screen.findByRole('button', { name: 'Réessayer la lecture' }));

    expect(
      await screen.findByRole('button', { name: 'Lire le message vocal' }),
    ).toBeInTheDocument();
    expect(lireMock).toHaveBeenCalledTimes(2);
    expect(screen.queryByText('Message vocal indisponible')).not.toBeInTheDocument();
  });

  it('relit la durée embarquée dans le nom de pièce quand le blob n’en a pas', async () => {
    lireMock.mockResolvedValueOnce('data:audio/webm;base64,AA==');
    render(<VoiceMessagePlayer piece={piece({ name: 'voice-7.5s.webm' })} />);
    await screen.findByRole('button', { name: 'Lire le message vocal' });

    // jsdom ne charge pas les métadonnées : la durée vient du nom (7,5 s → 0:07).
    expect(screen.getByText('0:00 / 0:07')).toBeInTheDocument();
  });
});

describe('VoiceMessagePlayer — lecture', () => {
  it('joue au clic, bascule l’icône en pause, et rappelle pause() au second clic', async () => {
    lireMock.mockResolvedValueOnce('data:audio/webm;base64,AA==');
    const { container } = render(<VoiceMessagePlayer piece={piece()} />);
    const bouton = await screen.findByRole('button', { name: 'Lire le message vocal' });

    fireEvent.click(bouton);
    expect(playSpy).toHaveBeenCalledTimes(1);

    const audio = container.querySelector('audio');
    expect(audio).not.toBeNull();
    fireEvent.play(audio as HTMLAudioElement);
    expect(
      await screen.findByRole('button', { name: 'Mettre en pause' }),
    ).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'Mettre en pause' }));
    expect(window.HTMLMediaElement.prototype.pause).toHaveBeenCalled();
  });

  it('retombe sur l’état d’erreur si le flux est indécodable, sans spinner infini', async () => {
    lireMock.mockResolvedValueOnce('data:audio/webm;base64,AA==');
    const { container } = render(<VoiceMessagePlayer piece={piece()} />);
    await screen.findByRole('button', { name: 'Lire le message vocal' });

    fireEvent.error(container.querySelector('audio') as HTMLAudioElement);

    expect(await screen.findByText('Message vocal indisponible')).toBeInTheDocument();
  });

  it('un play() rejeté avec AbortError (pause concurrente) ne condamne pas le lecteur', async () => {
    lireMock.mockResolvedValueOnce('data:audio/webm;base64,AA==');
    playSpy.mockImplementationOnce(() =>
      Promise.reject(new DOMException('interrupted by pause()', 'AbortError')),
    );
    render(<VoiceMessagePlayer piece={piece()} />);
    const bouton = await screen.findByRole('button', { name: 'Lire le message vocal' });

    fireEvent.click(bouton);
    await waitFor(() => expect(playSpy).toHaveBeenCalled());

    expect(
      screen.getByRole('button', { name: 'Lire le message vocal' }),
    ).toBeInTheDocument();
    expect(screen.queryByText('Message vocal indisponible')).not.toBeInTheDocument();
  });

  it('un play() rejeté pour flux non supporté bascule en erreur relançable', async () => {
    lireMock.mockResolvedValueOnce('data:audio/webm;base64,AA==');
    playSpy.mockImplementationOnce(() =>
      Promise.reject(new DOMException('unsupported', 'NotSupportedError')),
    );
    render(<VoiceMessagePlayer piece={piece()} />);

    fireEvent.click(await screen.findByRole('button', { name: 'Lire le message vocal' }));

    expect(await screen.findByText('Message vocal indisponible')).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: 'Réessayer la lecture' }),
    ).toBeInTheDocument();
  });

  it('ne joue jamais tout seul (pas d’autoPlay)', async () => {
    lireMock.mockResolvedValueOnce('data:audio/webm;base64,AA==');
    const { container } = render(<VoiceMessagePlayer piece={piece()} />);
    await screen.findByRole('button', { name: 'Lire le message vocal' });

    expect(container.querySelector('audio')).not.toHaveAttribute('autoplay');
    expect(playSpy).not.toHaveBeenCalled();
  });

  it('un seul lecteur actif à la fois : démarrer le second met le premier en pause', async () => {
    lireMock.mockResolvedValueOnce('data:audio/webm;base64,AA==');
    lireMock.mockResolvedValueOnce('data:audio/webm;base64,BB==');
    const { container } = render(
      <>
        <VoiceMessagePlayer piece={piece({ merkle_root: 'aa'.repeat(32) })} />
        <VoiceMessagePlayer piece={piece({ merkle_root: 'bb'.repeat(32) })} />
      </>,
    );
    const boutons = await screen.findAllByRole('button', {
      name: 'Lire le message vocal',
    });
    expect(boutons).toHaveLength(2);
    const audios = container.querySelectorAll('audio');
    expect(audios).toHaveLength(2);
    const pauseAudio1 = vi.spyOn(audios[0] as HTMLAudioElement, 'pause');

    fireEvent.click(boutons[0] as HTMLButtonElement);
    fireEvent.play(audios[0] as HTMLAudioElement);
    fireEvent.click(boutons[1] as HTMLButtonElement);

    await waitFor(() => expect(pauseAudio1).toHaveBeenCalled());
  });
});

describe('VoiceMessagePlayer — barre de progression accessible', () => {
  it('expose un slider nommé avec la position lisible (aria-valuetext)', async () => {
    lireMock.mockResolvedValueOnce('data:audio/webm;base64,AA==');
    render(<VoiceMessagePlayer piece={piece({ name: 'voice-12.4s.webm' })} />);

    const slider = await screen.findByRole('slider', {
      name: 'Progression du message vocal',
    });
    expect(slider).toHaveAttribute('aria-valuemax', '12');
    expect(slider).toHaveAttribute('aria-valuetext', '0:00 / 0:12');
  });
});
