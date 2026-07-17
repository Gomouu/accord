/**
 * Tests de l'écran « Choisis ton pseudo » : validation 2-32 caractères,
 * envoi du pseudo épuré, avatar optionnel (choix, retrait, envoi avant le
 * pseudo), bouton « Plus tard » et toast en cas d'échec.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { useSession } from '../stores/session';
import { useUi } from '../stores/ui';
import { ChooseNameScreen, RecoveryPhraseScreen } from './Onboarding';

// jsdom ne charge ni images ni canvas : on simule le chargement et l'encodage
// pour exercer le vrai recadreur (géométrie réelle, sortie déterministe).
function stubImage(largeur: number, hauteur: number): void {
  vi.spyOn(HTMLImageElement.prototype, 'naturalWidth', 'get').mockReturnValue(largeur);
  vi.spyOn(HTMLImageElement.prototype, 'naturalHeight', 'get').mockReturnValue(hauteur);
  vi.spyOn(HTMLImageElement.prototype, 'src', 'set').mockImplementation(function (
    this: HTMLImageElement,
  ) {
    setTimeout(() => this.onload?.(new Event('load')), 0);
  });
}

beforeEach(() => {
  useUi.setState({ lang: 'fr', toasts: [] });
  useSession.setState({
    askName: true,
    setName: vi.fn(async () => {}),
    setAvatar: vi.fn(async () => {}),
    skipNamePrompt: vi.fn(),
  });
  vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockReturnValue({
    drawImage: vi.fn(),
  } as unknown as CanvasRenderingContext2D);
  vi.spyOn(HTMLCanvasElement.prototype, 'toDataURL').mockReturnValue(
    'data:image/png;base64,QUJD',
  );
});

afterEach(() => {
  vi.restoreAllMocks();
});

function typeName(value: string): void {
  fireEvent.change(screen.getByRole('textbox', { name: 'Pseudo' }), {
    target: { value },
  });
}

describe('ChooseNameScreen — validation', () => {
  it('désactive l’envoi et signale un pseudo trop court', () => {
    render(<ChooseNameScreen />);
    typeName('a');

    expect(screen.getByRole('button', { name: 'C’est parti' })).toBeDisabled();
    expect(
      screen.getByText('Le pseudo doit faire entre 2 et 32 caractères'),
    ).toBeInTheDocument();
  });

  it('désactive l’envoi pour un pseudo trop long (33 caractères)', () => {
    render(<ChooseNameScreen />);
    typeName('a'.repeat(33));

    expect(screen.getByRole('button', { name: 'C’est parti' })).toBeDisabled();
  });

  it('désactive l’envoi tant que le champ est vide, sans message d’erreur', () => {
    render(<ChooseNameScreen />);

    expect(screen.getByRole('button', { name: 'C’est parti' })).toBeDisabled();
    expect(
      screen.queryByText('Le pseudo doit faire entre 2 et 32 caractères'),
    ).not.toBeInTheDocument();
  });
});

describe('ChooseNameScreen — actions', () => {
  it('envoie le pseudo épuré des espaces', async () => {
    const setName = vi.fn(async () => {});
    useSession.setState({ setName });
    render(<ChooseNameScreen />);

    typeName('  Alex  ');
    fireEvent.click(screen.getByRole('button', { name: 'C’est parti' }));

    await waitFor(() => expect(setName).toHaveBeenCalledWith('Alex'));
  });

  it('« Plus tard » écarte l’écran sans définir de pseudo', () => {
    const skipNamePrompt = vi.fn();
    const setName = vi.fn(async () => {});
    useSession.setState({ skipNamePrompt, setName });
    render(<ChooseNameScreen />);

    fireEvent.click(screen.getByRole('button', { name: 'Plus tard' }));

    expect(skipNamePrompt).toHaveBeenCalledTimes(1);
    expect(setName).not.toHaveBeenCalled();
  });

  it('affiche un toast d’erreur et redevient actionnable en cas d’échec', async () => {
    useSession.setState({
      setName: vi.fn(async () => Promise.reject(new Error('nope'))),
    });
    render(<ChooseNameScreen />);

    typeName('Alex');
    fireEvent.click(screen.getByRole('button', { name: 'C’est parti' }));

    await waitFor(() => {
      expect(useUi.getState().toasts.some((t) => t.kind === 'error')).toBe(true);
    });
    expect(screen.getByRole('button', { name: 'C’est parti' })).toBeEnabled();
  });
});

describe('RecoveryPhraseScreen — garde anti-perte', () => {
  const PHRASE =
    'alpha bravo charlie delta echo foxtrot golf hotel india juliett kilo lima';

  beforeEach(() => {
    useUi.setState({ lang: 'fr', toasts: [] });
    useSession.setState({ ackRecoveryPhrase: vi.fn() });
    // Mot-défi déterministe : index 0 → « alpha » (« mot n°1 »).
    vi.spyOn(Math, 'random').mockReturnValue(0);
  });

  it('bloque la confirmation tant que le mot-défi n’est pas retapé', () => {
    render(<RecoveryPhraseScreen phrase={PHRASE} />);
    const confirm = screen.getByRole('button', { name: 'Je les ai notés en lieu sûr' });
    expect(confirm).toBeDisabled();

    fireEvent.change(screen.getByRole('textbox'), { target: { value: 'mauvais' } });
    expect(screen.getByText('Ce mot ne correspond pas')).toBeInTheDocument();
    expect(confirm).toBeDisabled();

    fireEvent.change(screen.getByRole('textbox'), { target: { value: '  ALPHA  ' } });
    expect(screen.queryByText('Ce mot ne correspond pas')).not.toBeInTheDocument();
    expect(confirm).toBeEnabled();
  });

  it('confirme (ack) une fois le mot correct saisi', () => {
    const ack = vi.fn();
    useSession.setState({ ackRecoveryPhrase: ack });
    render(<RecoveryPhraseScreen phrase={PHRASE} />);

    fireEvent.change(screen.getByRole('textbox'), { target: { value: 'alpha' } });
    fireEvent.click(screen.getByRole('button', { name: 'Je les ai notés en lieu sûr' }));

    expect(ack).toHaveBeenCalledTimes(1);
  });

  it('copie la phrase dans le presse-papiers', async () => {
    const writeText = vi.fn((_text: string) => Promise.resolve());
    Object.assign(navigator, { clipboard: { writeText } });
    render(<RecoveryPhraseScreen phrase={PHRASE} />);

    fireEvent.click(screen.getByRole('button', { name: 'Copier' }));

    await waitFor(() => expect(writeText).toHaveBeenCalledOnce());
    expect(writeText.mock.calls[0]?.[0]).toContain('1. alpha');
  });
});

describe('ChooseNameScreen — avatar optionnel', () => {
  function chooseImage(): void {
    stubImage(800, 500);
    const input = screen.getByLabelText('Choisir une image', { selector: 'input' });
    fireEvent.change(input, {
      target: { files: [new File(['ABC'], 'moi.png', { type: 'image/png' })] },
    });
  }

  /** Choisit une image et valide le recadrage (recadreur → aperçu). */
  async function cropAndApply(): Promise<void> {
    chooseImage();
    // Le chargement passe par FileReader puis Image : attendre l'état prêt.
    const slider = await screen.findByRole('slider', { name: 'Zoom' });
    await waitFor(() => expect(slider).toBeEnabled());
    fireEvent.click(screen.getByRole('button', { name: 'Valider' }));
    await screen.findByRole('button', { name: 'Retirer l’image' });
  }

  it('ouvre le recadreur au choix d’une image', async () => {
    render(<ChooseNameScreen />);
    chooseImage();

    expect(
      await screen.findByRole('dialog', { name: 'Recadrer l’avatar' }),
    ).toBeInTheDocument();
  });

  it('affiche l’aperçu après recadrage, retirable', async () => {
    render(<ChooseNameScreen />);
    await cropAndApply();

    fireEvent.click(screen.getByRole('button', { name: 'Retirer l’image' }));
    expect(
      screen.queryByRole('button', { name: 'Retirer l’image' }),
    ).not.toBeInTheDocument();
  });

  it('publie l’avatar recadré puis le pseudo à l’envoi', async () => {
    const setName = vi.fn(async () => {});
    const setAvatar = vi.fn(async () => {});
    useSession.setState({ setName, setAvatar });
    render(<ChooseNameScreen />);

    typeName('Alex');
    await cropAndApply();
    fireEvent.click(screen.getByRole('button', { name: 'C’est parti' }));

    await waitFor(() => expect(setName).toHaveBeenCalledWith('Alex'));
    expect(setAvatar).toHaveBeenCalledWith('QUJD', 'image/png');
  });

  it('n’envoie pas d’avatar quand aucun n’est choisi', async () => {
    const setName = vi.fn(async () => {});
    const setAvatar = vi.fn(async () => {});
    useSession.setState({ setName, setAvatar });
    render(<ChooseNameScreen />);

    typeName('Alex');
    fireEvent.click(screen.getByRole('button', { name: 'C’est parti' }));

    await waitFor(() => expect(setName).toHaveBeenCalled());
    expect(setAvatar).not.toHaveBeenCalled();
  });
});
