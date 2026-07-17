/**
 * Tests de la modale QR du lien d'ami : rendu (dialogue, image QR, lien en
 * clair, hint de scan) et fermetures (Échap, bouton Fermer).
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';

// La génération réelle dessine sur un canvas, indisponible sous jsdom : on
// substitue une data-URL stable, l'encodage étant testé côté `qrcode`.
vi.mock('qrcode', () => ({
  toDataURL: vi.fn(() => Promise.resolve('data:image/png;base64,QR')),
}));

import { toDataURL } from 'qrcode';
import { fr } from '../i18n/fr';
import { useUi } from '../stores/ui';
import { FriendQrModal } from './FriendQrModal';

const LINK = 'accord://friend/LION-FORET-PLAGE-NUAGE-TIGRE-OCEAN-0042';

beforeEach(() => {
  useUi.setState({ lang: 'fr' });
  vi.mocked(toDataURL).mockClear();
});

describe('FriendQrModal', () => {
  it('rend un dialogue titré avec le QR du lien, le hint et le lien en clair', async () => {
    // Arrange / Act
    render(<FriendQrModal link={LINK} onClose={vi.fn()} />);

    // Assert — dialogue accessible et contenu attendu.
    expect(screen.getByRole('dialog', { name: fr.friends.qrTitle })).toBeInTheDocument();
    expect(screen.getByText(fr.friends.qrHint)).toBeInTheDocument();
    expect(screen.getByText(LINK)).toBeInTheDocument();
    const img = await screen.findByRole('img', { name: fr.friends.qrTitle });
    expect(img).toHaveAttribute('src', 'data:image/png;base64,QR');
    // Le QR encode bien le lien complet, ~240 px avec une marge de 1 module.
    expect(toDataURL).toHaveBeenCalledWith(LINK, { width: 240, margin: 1 });
  });

  it('ferme sur Échap', async () => {
    // Arrange — attendre le QR pour purger la mise à jour d'état asynchrone.
    const onClose = vi.fn();
    render(<FriendQrModal link={LINK} onClose={onClose} />);
    await screen.findByRole('img', { name: fr.friends.qrTitle });

    // Act
    fireEvent.keyDown(window, { key: 'Escape' });

    // Assert
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('ferme via le bouton Fermer', async () => {
    // Arrange — attendre le QR pour purger la mise à jour d'état asynchrone.
    const onClose = vi.fn();
    render(<FriendQrModal link={LINK} onClose={onClose} />);
    await screen.findByRole('img', { name: fr.friends.qrTitle });

    // Act
    fireEvent.click(screen.getByRole('button', { name: fr.app.close }));

    // Assert
    expect(onClose).toHaveBeenCalledTimes(1);
  });
});
