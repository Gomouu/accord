/**
 * QR du numéro de sécurité : ce qu'il encode, et ce qui reste lisible quand la
 * génération échoue.
 *
 * `qrcode` dessine sur un canvas, indisponible sous jsdom : on le substitue,
 * l'encodage étant testé chez lui. Ce qui se joue ici, c'est la charge utile
 * qu'on lui donne — un QR qui n'encode pas le schéma attendu serait rejeté
 * comme étranger par le lecteur d'en face.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/react';

vi.mock('qrcode', () => ({
  toDataURL: vi.fn(() => Promise.resolve('data:image/png;base64,QR')),
}));

import { toDataURL } from 'qrcode';
import { fr } from '../i18n/fr';
import { useUi } from '../stores/ui';
import { SafetyQrCode } from './SafetyQrCode';

const DIGITS = '123450987612345098761234509876123450987612345098761234509876';

beforeEach(() => {
  useUi.setState({ lang: 'fr' });
  vi.mocked(toDataURL).mockClear();
  vi.mocked(toDataURL).mockResolvedValue('data:image/png;base64,QR');
});

describe('SafetyQrCode', () => {
  it('encode le numéro sous le schéma que le lecteur attend', async () => {
    // Arrange / Act
    render(<SafetyQrCode digits={DIGITS} />);

    // Assert
    const img = await screen.findByRole('img', { name: fr.friends.verifyQrAlt });
    expect(img).toHaveAttribute('src', 'data:image/png;base64,QR');
    expect(toDataURL).toHaveBeenCalledWith(`accord://safety/${DIGITS}`, {
      width: 200,
      margin: 1,
    });
  });

  it('ne casse rien quand la génération échoue', async () => {
    // Arrange — canvas indisponible : le QR ne s'affiche pas, mais la modale
    // garde ses chiffres au-dessus et son explication en dessous.
    vi.mocked(toDataURL).mockRejectedValue(new Error('canvas indisponible'));

    // Act
    render(<SafetyQrCode digits={DIGITS} />);

    // Assert
    expect(await screen.findByText(fr.friends.verifyQrHint)).toBeInTheDocument();
    expect(screen.queryByRole('img')).not.toBeInTheDocument();
  });
});
