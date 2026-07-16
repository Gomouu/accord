/**
 * Tests des pièces jointes dans le fil de messages : vignette d'image
 * (progression puis aperçu, plein écran), carte de fichier téléchargeable
 * (petit fichier via `lireFichier`, gros fichier via le sélecteur natif +
 * `files.save`, tous deux téléchargeables) et message sans texte (pièces seules).
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Mock } from 'vitest';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import type { FileAttachment } from '../lib/api';
import { MAX_TAILLE_PIECE } from '../lib/attachments';
import { useSession } from '../stores/session';
import { useUi } from '../stores/ui';
import { MessageList, type DisplayMessage } from './MessageList';

vi.mock('../lib/files', () => ({
  FILE_WAIT_TIMEOUT_MS: 30_000,
  lireFichier: vi.fn(),
  statutFichier: vi.fn(async () => ({
    known: false,
    complete: false,
    done: 0,
    total: 0,
  })),
  observerProgression: vi.fn(() => () => {}),
}));

// Sélecteur natif Tauri : `open`/`save` interceptés (import statique et
// dynamique résolvent le même mock). Le chemin gros fichier n'est emprunté
// qu'avec `__TAURI_INTERNALS__` présent (voir le test dédié).
vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: vi.fn(),
  save: vi.fn(),
}));

import { lireFichier, observerProgression, statutFichier } from '../lib/files';
import { save } from '@tauri-apps/plugin-dialog';
import { api } from '../lib/client';

const lireMock = lireFichier as unknown as Mock;
const statutMock = statutFichier as unknown as Mock;
const observerMock = observerProgression as unknown as Mock;
const saveMock = save as unknown as Mock;

const BASE_MS = new Date('2026-07-08T10:00:00').getTime();

function piece(over: Partial<FileAttachment> = {}): FileAttachment {
  return {
    merkle_root: 'ab'.repeat(32),
    name: 'photo.png',
    size: 2048,
    mime: 'image/png',
    ...over,
  };
}

function message(attachments: FileAttachment[], text = 'regarde'): DisplayMessage {
  return {
    msg_id: 'm1',
    author: 'aabbccddee',
    sent_ms: BASE_MS,
    deleted: false,
    body: { type: 'text', text, reply_to: null, attachments: attachments.length },
    edited: null,
    attachments,
  };
}

beforeEach(() => {
  useUi.setState({ lang: 'fr' });
  useUi.getState().setShowMediaPreviews(true);
  useSession.setState({ self: null });
  lireMock.mockReset();
  statutMock.mockClear();
  observerMock.mockClear();
});

describe('Pièces jointes — vignette d’image', () => {
  it('affiche la vignette une fois le blob lu (hint = expéditeur)', async () => {
    lireMock.mockResolvedValueOnce('blob:image');
    render(<MessageList messages={[message([piece()])]} />);

    expect(await screen.findByAltText('photo.png')).toHaveAttribute('src', 'blob:image');
    expect(lireMock).toHaveBeenCalledWith('ab'.repeat(32), 'aabbccddee');
  });

  it('montre la progression pendant le téléchargement', async () => {
    lireMock.mockReturnValueOnce(new Promise(() => {}));
    statutMock.mockResolvedValueOnce({ known: true, complete: false, done: 1, total: 4 });
    render(<MessageList messages={[message([piece()])]} />);

    expect(await screen.findByText('Téléchargement… 25 %')).toBeInTheDocument();
    expect(observerMock).toHaveBeenCalledWith('ab'.repeat(32), expect.any(Function));
  });

  it('ouvre le plein écran au clic et le ferme par Échap', async () => {
    lireMock.mockResolvedValueOnce('blob:image');
    render(<MessageList messages={[message([piece()])]} />);

    fireEvent.click(await screen.findByRole('button', { name: 'Agrandir photo.png' }));
    expect(screen.getByRole('dialog', { name: 'photo.png' })).toBeInTheDocument();

    fireEvent.keyDown(window, { key: 'Escape' });
    // Fermeture différée (animation de sortie) : le dialogue se démonte après
    // l'animation, pas de façon synchrone.
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
  });

  it('signale une image indisponible sans casser le fil', async () => {
    lireMock.mockRejectedValueOnce(new Error('introuvable'));
    render(<MessageList messages={[message([piece()])]} />);

    expect(await screen.findByText('Image indisponible')).toBeInTheDocument();
  });

  it('signale une image illisible après son chargement', async () => {
    lireMock.mockResolvedValueOnce('blob:image-invalide');
    render(<MessageList messages={[message([piece()])]} />);

    fireEvent.error(await screen.findByAltText('photo.png'));

    expect(await screen.findByText('Image indisponible')).toBeInTheDocument();
  });

  it('rend un message sans texte (pièces jointes seules)', async () => {
    lireMock.mockResolvedValueOnce('blob:image');
    render(<MessageList messages={[message([piece()], '')]} />);

    expect(await screen.findByAltText('photo.png')).toBeInTheDocument();
    expect(screen.getByText('aabbcc')).toBeInTheDocument();
  });
});

describe('Pièces jointes — carte de fichier', () => {
  it('affiche nom, taille lisible et bouton de téléchargement', () => {
    render(
      <MessageList
        messages={[message([piece({ name: 'doc.pdf', mime: 'application/pdf' })])]}
      />,
    );

    expect(screen.getByText('doc.pdf')).toBeInTheDocument();
    expect(screen.getByText('2 Ko')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Télécharger doc.pdf' })).toBeEnabled();
    // Pas de lecture avant le clic : le téléchargement est à la demande.
    expect(lireMock).not.toHaveBeenCalled();
  });

  it('télécharge via lireFichier au clic (lien download)', async () => {
    lireMock.mockResolvedValueOnce('blob:doc');
    const clickSpy = vi
      .spyOn(HTMLAnchorElement.prototype, 'click')
      .mockImplementation(() => {});
    render(
      <MessageList
        messages={[message([piece({ name: 'doc.pdf', mime: 'application/pdf' })])]}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: 'Télécharger doc.pdf' }));

    await waitFor(() => expect(clickSpy).toHaveBeenCalledTimes(1));
    expect(lireMock).toHaveBeenCalledWith('ab'.repeat(32), 'aabbccddee');
    clickSpy.mockRestore();
  });

  it('garde le téléchargement actif au-delà de 8 Mio (plus d’avertissement 8 Mio)', () => {
    render(
      <MessageList
        messages={[
          message([
            piece({
              name: 'enorme.zip',
              mime: 'application/zip',
              size: MAX_TAILLE_PIECE + 1,
            }),
          ]),
        ]}
      />,
    );

    expect(screen.getByRole('button', { name: 'Télécharger enorme.zip' })).toBeEnabled();
    expect(screen.queryByText(/8 Mio/)).not.toBeInTheDocument();
  });

  it('télécharge un gros fichier via le sélecteur natif puis files.save (Tauri)', async () => {
    (window as unknown as { __TAURI_INTERNALS__?: object }).__TAURI_INTERNALS__ = {};
    saveMock.mockResolvedValueOnce('/tmp/enorme.zip');
    const saveSpy = vi.spyOn(api, 'filesSave').mockResolvedValue();
    try {
      render(
        <MessageList
          messages={[
            message([
              piece({
                name: 'enorme.zip',
                mime: 'application/zip',
                size: MAX_TAILLE_PIECE + 1,
              }),
            ]),
          ]}
        />,
      );

      fireEvent.click(screen.getByRole('button', { name: 'Télécharger enorme.zip' }));

      await waitFor(() =>
        expect(saveSpy).toHaveBeenCalledWith('ab'.repeat(32), '/tmp/enorme.zip'),
      );
      expect(saveMock).toHaveBeenCalledWith({ defaultPath: 'enorme.zip' });
    } finally {
      saveSpy.mockRestore();
      saveMock.mockReset();
      delete (window as unknown as { __TAURI_INTERNALS__?: object }).__TAURI_INTERNALS__;
    }
  });

  it('replie une image trop volumineuse en carte, téléchargement toujours actif', () => {
    render(<MessageList messages={[message([piece({ size: MAX_TAILLE_PIECE + 1 })])]} />);

    expect(screen.queryByAltText('photo.png')).not.toBeInTheDocument();
    expect(screen.getByText(/Trop volumineux pour l’aperçu/)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Télécharger photo.png' })).toBeEnabled();
  });
});

describe('Pièces jointes — message vocal', () => {
  it('rend le lecteur vocal pour une pièce audio, même aperçu désactivé', async () => {
    useUi.getState().setShowMediaPreviews(false);
    lireMock.mockResolvedValueOnce('data:audio/webm;base64,AA==');
    render(
      <MessageList
        messages={[
          message([
            piece({ name: 'voice-message.webm', mime: 'audio/webm;codecs=opus' }),
          ]),
        ]}
      />,
    );

    expect(
      await screen.findByRole('button', { name: 'Lire le message vocal' }),
    ).toBeInTheDocument();
    // Pas de carte de fichier générique pour l'audio : c'est le lecteur qui rend la pièce.
    expect(screen.queryByText('voice-message.webm')).not.toBeInTheDocument();
  });

  it('replie un message vocal trop volumineux en carte de fichier', () => {
    render(
      <MessageList
        messages={[
          message([
            piece({
              name: 'voice-message.webm',
              mime: 'audio/webm;codecs=opus',
              size: MAX_TAILLE_PIECE + 1,
            }),
          ]),
        ]}
      />,
    );

    expect(screen.getByText('voice-message.webm')).toBeInTheDocument();
    expect(
      screen.queryByRole('button', { name: 'Lire le message vocal' }),
    ).not.toBeInTheDocument();
    expect(screen.getByText(/Trop volumineux pour l’aperçu/)).toBeInTheDocument();
  });
});

describe('Pièces jointes — réglage « Aperçu des images et médias »', () => {
  it('replie une image de taille normale en carte de fichier quand l’aperçu est désactivé', () => {
    useUi.getState().setShowMediaPreviews(false);
    render(<MessageList messages={[message([piece()])]} />);

    expect(screen.queryByAltText('photo.png')).not.toBeInTheDocument();
    expect(screen.getByText('photo.png')).toBeInTheDocument();
    // Ce n'est pas une image "trop volumineuse" : pas de mention trompeuse.
    expect(screen.queryByText(/Trop volumineux pour l’aperçu/)).not.toBeInTheDocument();
    expect(lireMock).not.toHaveBeenCalled();
  });

  it('affiche de nouveau la vignette une fois l’aperçu réactivé', async () => {
    lireMock.mockResolvedValueOnce('blob:image');
    useUi.getState().setShowMediaPreviews(false);
    render(<MessageList messages={[message([piece()])]} />);
    expect(screen.queryByAltText('photo.png')).not.toBeInTheDocument();

    act(() => {
      useUi.getState().setShowMediaPreviews(true);
    });

    expect(await screen.findByAltText('photo.png')).toBeInTheDocument();
  });
});
