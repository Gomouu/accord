/**
 * Lecteur de QR du numéro de sécurité : verdicts, chemins d'échec de la
 * caméra, et libération de celle-ci.
 *
 * Le décodeur (`jsqr`) et la caméra sont simulés — jsdom n'a ni l'un ni
 * l'autre, et ce qui compte ici n'est pas la qualité du décodage mais ce que
 * le composant *fait* de ce qu'on lui rend : un désaccord doit s'afficher tel
 * quel et arrêter la boucle, une lecture ratée ne doit jamais devenir une
 * concordance.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';

vi.mock('jsqr', () => ({ default: vi.fn(() => null) }));

import jsQR from 'jsqr';
import { fr } from '../i18n/fr';
import { buildSafetyQrPayload } from '../lib/safetyQr';
import { useUi } from '../stores/ui';
import { SafetyQrScanner } from './SafetyQrScanner';

const LOCAL = '123450987612345098761234509876123450987612345098761234509876';
const AUTRE = `${LOCAL.slice(0, -1)}5`;

const jsQRMock = vi.mocked(jsQR);

/** Piste vidéo factice : on vérifie qu'elle est bien coupée. */
let pistes: { stop: ReturnType<typeof vi.fn> }[] = [];

/** Installe une caméra qui accepte, et rend le flux factice. */
function cameraQuiAccepte(): ReturnType<typeof vi.fn> {
  pistes = [{ stop: vi.fn() }];
  const flux = { getTracks: () => pistes } as unknown as MediaStream;
  const getUserMedia = vi.fn(() => Promise.resolve(flux));
  Object.defineProperty(navigator, 'mediaDevices', {
    value: { getUserMedia },
    configurable: true,
  });
  return getUserMedia;
}

/** Installe une caméra qui rejette avec l'erreur DOM `nom`. */
function cameraQuiRefuse(nom: string): void {
  const erreur = new Error('refus');
  erreur.name = nom;
  Object.defineProperty(navigator, 'mediaDevices', {
    value: { getUserMedia: vi.fn(() => Promise.reject(erreur)) },
    configurable: true,
  });
}

/** Fait décoder `charge` par jsQR (ou rien du tout si `null`). */
function decode(charge: string | null): void {
  jsQRMock.mockImplementation(
    () => (charge === null ? null : { data: charge }) as ReturnType<typeof jsQR>,
  );
}

beforeEach(() => {
  useUi.setState({ lang: 'fr' });
  jsQRMock.mockReset();
  decode(null);

  // jsdom n'implémente ni la lecture vidéo ni le canevas 2D : on fournit le
  // minimum dont la boucle d'analyse a besoin pour tourner.
  HTMLMediaElement.prototype.play = vi.fn(() => Promise.resolve());
  Object.defineProperty(HTMLVideoElement.prototype, 'videoWidth', {
    configurable: true,
    get: () => 640,
  });
  Object.defineProperty(HTMLVideoElement.prototype, 'videoHeight', {
    configurable: true,
    get: () => 480,
  });
  HTMLCanvasElement.prototype.getContext = vi.fn(() => ({
    drawImage: vi.fn(),
    getImageData: (_x: number, _y: number, w: number, h: number) => ({
      data: new Uint8ClampedArray(w * h * 4),
      width: w,
      height: h,
    }),
  })) as unknown as HTMLCanvasElement['getContext'];
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe('SafetyQrScanner', () => {
  it('annonce l’absence de caméra sans masquer la comparaison manuelle', async () => {
    // Arrange — runtime sans `mediaDevices` du tout.
    Object.defineProperty(navigator, 'mediaDevices', {
      value: undefined,
      configurable: true,
    });

    // Act
    render(<SafetyQrScanner localDigits={LOCAL} />);

    // Assert
    expect(await screen.findByRole('alert')).toHaveTextContent(
      fr.friends.verifyScanNoCamera,
    );
  });

  it('annonce un refus d’autorisation distinctement d’une caméra absente', async () => {
    // Arrange
    cameraQuiRefuse('NotAllowedError');

    // Act
    render(<SafetyQrScanner localDigits={LOCAL} />);

    // Assert
    expect(await screen.findByRole('alert')).toHaveTextContent(
      fr.friends.verifyScanDenied,
    );
  });

  it('traite une caméra introuvable comme indisponible, pas comme un refus', async () => {
    // Arrange
    cameraQuiRefuse('NotFoundError');

    // Act
    render(<SafetyQrScanner localDigits={LOCAL} />);

    // Assert
    expect(await screen.findByRole('alert')).toHaveTextContent(
      fr.friends.verifyScanNoCamera,
    );
  });

  it('conclut à la concordance et relâche la caméra', async () => {
    // Arrange
    cameraQuiAccepte();
    decode(buildSafetyQrPayload(LOCAL));

    // Act
    render(<SafetyQrScanner localDigits={LOCAL} />);

    // Assert
    expect(await screen.findByRole('alert')).toHaveTextContent(
      fr.friends.verifyScanMatch,
    );
    // Le voyant de la caméra doit s'éteindre avec le scan.
    await waitFor(() => expect(pistes[0]?.stop).toHaveBeenCalled());
  });

  it('🔒 dit le désaccord et ne réessaie pas jusqu’à tomber sur une concordance', async () => {
    // Arrange — le premier tour décode un numéro différent d'un seul chiffre ;
    // tous les suivants décoderaient le bon. Si la boucle continuait, l'écran
    // basculerait sur « identiques » — c'est précisément ce qu'on interdit.
    cameraQuiAccepte();
    let tour = 0;
    jsQRMock.mockImplementation(() => {
      tour += 1;
      const charge = buildSafetyQrPayload(tour === 1 ? AUTRE : LOCAL);
      return { data: charge } as ReturnType<typeof jsQR>;
    });

    // Act
    render(<SafetyQrScanner localDigits={LOCAL} />);
    const alerte = await screen.findByRole('alert');

    // Assert — désaccord affiché tel quel…
    expect(alerte).toHaveTextContent(fr.friends.verifyScanMismatch);
    // …et la boucle est bien arrêtée : rien ne se redécode ensuite.
    const appels = jsQRMock.mock.calls.length;
    await new Promise((resolve) => setTimeout(resolve, 500));
    expect(jsQRMock.mock.calls.length).toBe(appels);
    expect(screen.getByRole('alert')).toHaveTextContent(fr.friends.verifyScanMismatch);
    expect(screen.queryByText(fr.friends.verifyScanMatch)).not.toBeInTheDocument();
  });

  it('signale un QR étranger sans rien conclure, et continue à scanner', async () => {
    // Arrange — un lien d'ami est un QR valide, mais pas un numéro de sécurité.
    cameraQuiAccepte();
    decode('accord://friend/LION-FORET-PLAGE-NUAGE-TIGRE-OCEAN-0042');

    // Act
    render(<SafetyQrScanner localDigits={LOCAL} />);

    // Assert — un statut, pas un verdict.
    expect(await screen.findByRole('status')).toHaveTextContent(
      fr.friends.verifyScanForeign,
    );
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
    expect(screen.getByLabelText(fr.friends.verifyScanPreview)).toBeInTheDocument();
  });

  it('coupe la caméra au démontage', async () => {
    // Arrange
    cameraQuiAccepte();
    const { unmount } = render(<SafetyQrScanner localDigits={LOCAL} />);
    await screen.findByLabelText(fr.friends.verifyScanPreview);
    await waitFor(() => expect(jsQRMock).toHaveBeenCalled());

    // Act
    unmount();

    // Assert
    expect(pistes[0]?.stop).toHaveBeenCalled();
  });
});
