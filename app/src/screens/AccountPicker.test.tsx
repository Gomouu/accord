/**
 * Tests du sélecteur de comptes : affichage des comptes connus, ouverture
 * de l'invite de phrase de passe au clic, déverrouillage du bon compte à la
 * soumission, bascule vers « Ajouter un compte » / « Importer ».
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import type { AccountMeta } from '../lib/bridge';
import { useSession } from '../stores/session';
import { useUi } from '../stores/ui';
import { AccountPicker } from './AccountPicker';

const accounts: AccountMeta[] = [
  {
    id: 'a1',
    name: 'Alex',
    created_ms: 1,
    last_used_ms: 1_700_000_000_000,
    is_legacy: true,
    pubkey_short: 'deadbeef',
  },
  {
    id: 'a2',
    name: 'Compte pro',
    created_ms: 2,
    last_used_ms: 1_700_000_100_000,
    is_legacy: false,
    pubkey_short: null,
  },
];

beforeEach(() => {
  useUi.setState({ lang: 'fr' });
  useSession.setState({
    accounts,
    error: null,
    unlockAccount: vi.fn(async () => {}),
    createAccount: vi.fn(async () => {}),
    restoreAccount: vi.fn(async () => {}),
  });
});

describe('AccountPicker — liste', () => {
  it('affiche une ligne par compte connu', () => {
    render(<AccountPicker />);

    expect(screen.getByText('Alex')).toBeInTheDocument();
    expect(screen.getByText('Compte pro')).toBeInTheDocument();
  });

  it('affiche le préfixe de clé publique quand il est connu', () => {
    render(<AccountPicker />);

    expect(screen.getByText(/deadbeef/)).toBeInTheDocument();
  });

  it('borne la liste et garde le panneau accessible à faible hauteur', () => {
    const { container } = render(<AccountPicker />);

    expect(screen.getByRole('list')).toHaveClass('max-h-64', 'overflow-y-auto');
    expect(container.firstElementChild).toHaveClass('overflow-y-auto');
    expect(screen.getByRole('button', { name: 'Ajouter un compte' })).not.toHaveClass(
      'transition-all',
    );
  });
});

describe('AccountPicker — déverrouillage', () => {
  it('déverrouille le compte cliqué avec la phrase de passe saisie', async () => {
    const unlockAccount = vi.fn(async () => {});
    useSession.setState({ unlockAccount });
    render(<AccountPicker />);

    fireEvent.click(screen.getByRole('button', { name: 'Déverrouiller Alex' }));
    fireEvent.change(screen.getByLabelText('Phrase de passe'), {
      target: { value: 'phrase-de-passe-longue' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Déverrouiller' }));

    expect(unlockAccount).toHaveBeenCalledWith('a1', 'phrase-de-passe-longue');
  });

  it("n'ouvre l'invite que pour le compte cliqué", () => {
    render(<AccountPicker />);

    fireEvent.click(screen.getByRole('button', { name: 'Déverrouiller Alex' }));

    expect(screen.getByLabelText('Phrase de passe')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Déverrouiller Compte pro' }));
    // Une seule invite affichée à la fois : le champ précédent reste unique.
    expect(screen.getAllByLabelText('Phrase de passe')).toHaveLength(1);
  });

  it('affiche l’erreur du store (phrase de passe incorrecte)', () => {
    useSession.setState({ error: 'Phrase de passe incorrecte' });
    render(<AccountPicker />);

    expect(screen.getByText('Phrase de passe incorrecte')).toBeInTheDocument();
  });
});

describe('AccountPicker — ajout / import', () => {
  it('« Ajouter un compte » ouvre le formulaire de création câblé sur createAccount', () => {
    render(<AccountPicker />);

    fireEvent.click(screen.getByRole('button', { name: 'Ajouter un compte' }));

    expect(
      screen.getByRole('button', { name: 'Retour à la liste des comptes' }),
    ).toBeInTheDocument();
  });

  it('« Importer depuis une phrase de récupération » ouvre le formulaire de restauration', () => {
    render(<AccountPicker />);

    fireEvent.click(
      screen.getByRole('button', { name: 'Importer depuis une phrase de récupération' }),
    );

    expect(
      screen.getByRole('heading', { name: 'Restaurer une identité' }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: 'Retour à la liste des comptes' }),
    ).toBeInTheDocument();
  });
});
