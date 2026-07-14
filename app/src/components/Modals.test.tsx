/**
 * Tests des modales : invitation de serveur façon Discord (consentement
 * explicite D-045 — seuls les amis non-membres sont proposés, le clic appelle
 * `groups.invite_create` sans fermer la modale, lien partageable auto-créé à
 * l'ouverture), création/rejoindre un serveur en deux onglets, et création de
 * sondage (D-048).
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';
import { act, fireEvent, render, screen } from '@testing-library/react';

vi.mock('../lib/client', () => ({
  rpc: {
    call: vi.fn(() => Promise.resolve({ messages: [] })),
    onEvent: vi.fn(() => () => {}),
    onStatus: vi.fn(() => () => {}),
  },
  api: {
    groupsInviteCreate: vi.fn(() => Promise.resolve({ invite_id: 'i1' })),
    groupsInviteLinkCreate: vi.fn(() =>
      Promise.resolve({ code: 'accord://invite/CODE123' }),
    ),
    groupsInviteLinkRedeem: vi.fn(() =>
      Promise.resolve({ ok: true, group_id: 'g9', group_name: 'Nouvelle Guilde' }),
    ),
    // `invite` (store) rafraîchit l'état après invite_create : le fixture doit
    // garder « Déjà membre » membre, sinon il réapparaîtrait dans la liste.
    groupsState: vi.fn(() =>
      Promise.resolve(groupStateFixture([{ pubkey: 'pk_membre', roles: [] }])),
    ),
    groupsSendPoll: vi.fn(() => Promise.resolve({ msg_id: 'm1', poll_id: 'p1' })),
  },
}));

import { api } from '../lib/client';
import type { Contact, GroupStateJson } from '../lib/api';
import { useFriends } from '../stores/friends';
import { useGroups } from '../stores/groups';
import { useUi } from '../stores/ui';
import { Modals } from './Modals';

function groupStateFixture(members: GroupStateJson['members'] = []): GroupStateJson {
  return {
    group_id: 'g1',
    name: 'Guilde',
    icon: null,
    founder: null,
    members,
    bans: [],
    channels: [],
    categories: [],
    roles: [],
    invites: [],
    my_permissions: 0,
  };
}

describe('InviteModal (Modals.tsx) — invitation par consentement (D-045)', () => {
  beforeEach(() => {
    useUi.setState({ lang: 'fr', modal: { kind: 'invite', groupId: 'g1' } });
    useFriends.setState({
      contacts: [
        { pubkey: 'pk_bob', display_name: 'Bob', state: 'friend' },
        { pubkey: 'pk_carole', display_name: 'Carole', state: 'friend' },
        { pubkey: 'pk_membre', display_name: 'Déjà membre', state: 'friend' },
      ] as unknown as Contact[],
    });
    useGroups.setState({
      states: { g1: groupStateFixture([{ pubkey: 'pk_membre', roles: [] }]) },
    });
    (api.groupsInviteCreate as unknown as ReturnType<typeof vi.fn>).mockClear();
    (api.groupsInviteLinkCreate as unknown as ReturnType<typeof vi.fn>)
      .mockClear()
      .mockResolvedValue({ code: 'accord://invite/CODE123' });
    (api.groupsState as unknown as ReturnType<typeof vi.fn>).mockClear();
  });

  /** Attend le lien auto-créé (évite les avertissements act sur l'effet). */
  async function attendreLien(): Promise<void> {
    expect(
      await screen.findByDisplayValue('accord://invite/CODE123'),
    ).toBeInTheDocument();
  }

  it('affiche le nom du serveur dans le titre et propose seulement les amis non-membres', async () => {
    render(<Modals />);

    expect(
      screen.getByRole('dialog', { name: 'Inviter des amis sur Guilde' }),
    ).toBeInTheDocument();
    expect(screen.getByText('Bob')).toBeInTheDocument();
    expect(screen.queryByText('Déjà membre')).not.toBeInTheDocument();
    await attendreLien();
  });

  it('appelle groups.invite_create au clic — jamais l’ancien force-join', async () => {
    render(<Modals />);

    fireEvent.click(screen.getByRole('button', { name: 'Inviter Bob' }));

    await vi.waitFor(() =>
      expect(api.groupsInviteCreate).toHaveBeenCalledWith('g1', 'pk_bob'),
    );
    await attendreLien();
  });

  it('le bouton devient « Invité ✓ » désactivé sans fermer la modale', async () => {
    render(<Modals />);

    fireEvent.click(screen.getByRole('button', { name: 'Inviter Bob' }));

    expect(
      await screen.findByRole('button', { name: 'Invitation envoyée à Bob' }),
    ).toBeDisabled();
    // La modale reste ouverte pour enchaîner d'autres invitations.
    expect(useUi.getState().modal).toEqual({ kind: 'invite', groupId: 'g1' });
    // Carole reste invitable.
    expect(screen.getByRole('button', { name: 'Inviter Carole' })).toBeInTheDocument();
    await attendreLien();
  });

  it('la confirmation est transitoire : on peut relancer l’invitation au même ami', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    try {
      render(<Modals />);

      fireEvent.click(screen.getByRole('button', { name: 'Inviter Bob' }));
      expect(
        await screen.findByRole('button', { name: 'Invitation envoyée à Bob' }),
      ).toBeInTheDocument();
      expect(api.groupsInviteCreate).toHaveBeenCalledTimes(1);

      // « Invité ✓ » n'est PAS un verrou définitif : le bouton redevient
      // « Inviter » (régression — auparavant il restait désactivé à vie).
      await act(async () => {
        vi.advanceTimersByTime(3000);
      });
      const reinviter = await screen.findByRole('button', { name: 'Inviter Bob' });

      // Une nouvelle invitation (à usage unique côté nœud) peut repartir.
      fireEvent.click(reinviter);
      await vi.waitFor(() => expect(api.groupsInviteCreate).toHaveBeenCalledTimes(2));
    } finally {
      vi.useRealTimers();
    }
  });

  it('filtre les amis via le champ de recherche', async () => {
    render(<Modals />);

    fireEvent.change(screen.getByLabelText('Rechercher des amis'), {
      target: { value: 'caro' },
    });

    expect(screen.getByText('Carole')).toBeInTheDocument();
    expect(screen.queryByText('Bob')).not.toBeInTheDocument();

    fireEvent.change(screen.getByLabelText('Rechercher des amis'), {
      target: { value: 'zzz' },
    });
    expect(
      screen.getByText('Aucun ami ne correspond à la recherche.'),
    ).toBeInTheDocument();
    await attendreLien();
  });

  it('crée automatiquement un lien par défaut (illimité, 7 jours) à l’ouverture', async () => {
    render(<Modals />);

    await attendreLien();
    expect(api.groupsInviteLinkCreate).toHaveBeenCalledWith('g1', 0, 168);
    expect(screen.getByRole('button', { name: 'Copier' })).toBeEnabled();
  });

  it('affiche « Réessayer » quand la création du lien échoue, puis récupère', async () => {
    (api.groupsInviteLinkCreate as unknown as ReturnType<typeof vi.fn>)
      .mockRejectedValueOnce(new Error('réseau'))
      .mockResolvedValueOnce({ code: 'accord://invite/CODE123' });

    render(<Modals />);

    expect(
      await screen.findByText('Impossible de créer le lien d’invitation.'),
    ).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'Réessayer' }));

    await attendreLien();
  });

  it('« Modifier le lien » déplie les sélecteurs usages/durée, repliés par défaut', async () => {
    render(<Modals />);
    await attendreLien();

    expect(screen.queryByLabelText('Utilisations')).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'Modifier le lien' }));

    expect(screen.getByLabelText('Utilisations')).toBeInTheDocument();
    expect(screen.getByLabelText('Expire')).toBeInTheDocument();

    // Générer un nouveau lien avec les valeurs choisies.
    fireEvent.change(screen.getByLabelText('Utilisations'), { target: { value: '5' } });
    fireEvent.click(screen.getByRole('button', { name: 'Générer un nouveau lien' }));
    await vi.waitFor(() =>
      expect(api.groupsInviteLinkCreate).toHaveBeenLastCalledWith('g1', 5, 168),
    );
  });
});

describe('CreateGroupModal (Modals.tsx) — créer ou rejoindre en deux onglets', () => {
  beforeEach(() => {
    useUi.setState({ lang: 'fr', modal: { kind: 'createGroup' } });
    useGroups.setState({ loadList: vi.fn(() => Promise.resolve()) });
    (api.groupsInviteLinkRedeem as unknown as ReturnType<typeof vi.fn>)
      .mockClear()
      .mockResolvedValue({ ok: true, group_id: 'g9', group_name: 'Nouvelle Guilde' });
  });

  it('affiche l’onglet Créer par défaut, avec le formulaire de nom', () => {
    render(<Modals />);

    const dialog = screen.getByRole('dialog', { name: 'Créer votre groupe' });
    expect(dialog).toHaveClass('flex', 'max-h-[calc(100vh-2rem)]', 'overflow-hidden');
    expect(dialog.querySelector('.overflow-y-auto')).not.toBeNull();
    expect(screen.getByRole('tab', { name: 'Créer un serveur' })).toHaveAttribute(
      'aria-selected',
      'true',
    );
    expect(screen.getByLabelText('Nom du groupe')).toBeInTheDocument();
    expect(screen.queryByLabelText('accord://invite/…')).not.toBeInTheDocument();
  });

  it('l’onglet « Rejoindre avec un lien » affiche le champ de lien', () => {
    render(<Modals />);

    fireEvent.click(screen.getByRole('tab', { name: 'Rejoindre avec un lien' }));

    expect(screen.getByLabelText('accord://invite/…')).toBeInTheDocument();
    expect(screen.queryByLabelText('Nom du groupe')).not.toBeInTheDocument();
    expect(
      screen.getByRole('dialog', { name: 'Rejoindre un serveur' }),
    ).toBeInTheDocument();
  });

  it('rejoint via groups.invite_link_redeem puis ferme la modale', async () => {
    render(<Modals />);

    fireEvent.click(screen.getByRole('tab', { name: 'Rejoindre avec un lien' }));
    fireEvent.change(screen.getByLabelText('accord://invite/…'), {
      target: { value: 'accord://invite/CODE123' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Rejoindre' }));

    await vi.waitFor(() =>
      expect(api.groupsInviteLinkRedeem).toHaveBeenCalledWith('accord://invite/CODE123'),
    );
    await vi.waitFor(() => expect(useUi.getState().modal).toBeNull());
  });

  it('refuse un lien mal formé sans appeler le nœud', () => {
    render(<Modals />);

    fireEvent.click(screen.getByRole('tab', { name: 'Rejoindre avec un lien' }));
    fireEvent.change(screen.getByLabelText('accord://invite/…'), {
      target: { value: 'https://pas-un-lien' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Rejoindre' }));

    expect(api.groupsInviteLinkRedeem).not.toHaveBeenCalled();
  });
});

describe('CreatePollModal (Modals.tsx) — création de sondage (D-048)', () => {
  beforeEach(() => {
    useUi.setState({
      lang: 'fr',
      modal: { kind: 'createPoll', groupId: 'g1', channelId: 'c1' },
    });
    useGroups.setState({ states: { g1: groupStateFixture() } });
    (api.groupsSendPoll as unknown as ReturnType<typeof vi.fn>).mockClear();
  });

  function fillOption(index: number, value: string): void {
    fireEvent.change(screen.getByPlaceholderText(`Option ${index}`), {
      target: { value },
    });
  }

  it('démarre avec 2 options et Créer désactivé (question et options vides)', () => {
    render(<Modals />);

    expect(screen.getByPlaceholderText('Option 1')).toBeInTheDocument();
    expect(screen.getByPlaceholderText('Option 2')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Créer le sondage' })).toBeDisabled();
  });

  it('reste désactivé avec une seule option renseignée', () => {
    render(<Modals />);
    fireEvent.change(screen.getByLabelText('Question'), {
      target: { value: 'Pizza ou sushis ?' },
    });
    fillOption(1, 'Pizza');

    expect(screen.getByRole('button', { name: 'Créer le sondage' })).toBeDisabled();
  });

  it('s’active une fois la question et 2 options valides renseignées, puis crée le sondage', async () => {
    render(<Modals />);
    fireEvent.change(screen.getByLabelText('Question'), {
      target: { value: 'Pizza ou sushis ?' },
    });
    fillOption(1, 'Pizza');
    fillOption(2, 'Sushis');

    const createButton = screen.getByRole('button', { name: 'Créer le sondage' });
    expect(createButton).toBeEnabled();
    fireEvent.click(createButton);

    await vi.waitFor(() =>
      expect(api.groupsSendPoll).toHaveBeenCalledWith('g1', 'c1', 'Pizza ou sushis ?', [
        'Pizza',
        'Sushis',
      ]),
    );
  });

  it('permet d’ajouter des options jusqu’à 10, puis masque « Ajouter une option »', () => {
    render(<Modals />);
    const addButton = screen.getByRole('button', { name: /Ajouter une option/ });

    for (let i = 0; i < 8; i += 1) fireEvent.click(addButton);

    expect(screen.getByPlaceholderText('Option 10')).toBeInTheDocument();
    expect(
      screen.queryByRole('button', { name: /Ajouter une option/ }),
    ).not.toBeInTheDocument();
  });

  it('ne permet pas de retirer une option sous 2 (bouton de retrait absent à 2 options)', () => {
    render(<Modals />);

    expect(screen.queryByLabelText(/Retirer l.option/)).not.toBeInTheDocument();
  });

  it('retirer une option au-delà de 2 fait disparaître sa rangée', () => {
    render(<Modals />);
    fireEvent.click(screen.getByRole('button', { name: /Ajouter une option/ }));
    expect(screen.getByPlaceholderText('Option 3')).toBeInTheDocument();

    fireEvent.click(screen.getByLabelText('Retirer l’option 3'));

    expect(screen.queryByPlaceholderText('Option 3')).not.toBeInTheDocument();
  });

  it('reste désactivé au-delà de la borne de question (300 octets UTF-8)', () => {
    render(<Modals />);
    fireEvent.change(screen.getByLabelText('Question'), {
      target: { value: 'a'.repeat(301) },
    });
    fillOption(1, 'Pizza');
    fillOption(2, 'Sushis');

    expect(screen.getByRole('button', { name: 'Créer le sondage' })).toBeDisabled();
  });

  it('désactive Créer et affiche l’indication au plafond de 25 sondages par groupe', () => {
    const polls = Array.from({ length: 25 }, (_, i) => ({
      poll_id: `p${i}`,
      author: 'moi',
      closed: false,
      counts: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
      total_votes: 0,
      my_vote: null,
    }));
    useGroups.setState({ states: { g1: { ...groupStateFixture(), polls } } });
    render(<Modals />);
    fireEvent.change(screen.getByLabelText('Question'), {
      target: { value: 'Pizza ou sushis ?' },
    });
    fillOption(1, 'Pizza');
    fillOption(2, 'Sushis');

    expect(screen.getByText('25 sondages au maximum par groupe.')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Créer le sondage' })).toBeDisabled();
  });
});

describe('Modales — accessibilité (focus, onglets, noms accessibles)', () => {
  it('la modale est un dialog nommé par son titre (aria-labelledby)', () => {
    useUi.setState({ lang: 'fr', modal: { kind: 'createGroup' } });
    render(<Modals />);

    const dialog = screen.getByRole('dialog', { name: 'Créer votre groupe' });
    expect(dialog).toHaveAttribute('aria-modal', 'true');
    expect(dialog).toHaveAttribute('aria-labelledby');
  });

  it('Échap ferme la modale', async () => {
    useUi.setState({ lang: 'fr', modal: { kind: 'createGroup' } });
    render(<Modals />);

    fireEvent.keyDown(window, { key: 'Escape' });

    // Fermeture différée (animation de sortie) : le démontage suit l'animation.
    await vi.waitFor(() => expect(useUi.getState().modal).toBeNull());
  });

  it('les onglets Créer/Rejoindre basculent aux flèches, focus compris', () => {
    useUi.setState({ lang: 'fr', modal: { kind: 'createGroup' } });
    render(<Modals />);

    const creer = screen.getByRole('tab', { name: 'Créer un serveur' });
    const rejoindre = screen.getByRole('tab', { name: 'Rejoindre avec un lien' });
    expect(creer).toHaveAttribute('tabindex', '0');
    expect(rejoindre).toHaveAttribute('tabindex', '-1');

    fireEvent.keyDown(screen.getByRole('tablist'), { key: 'ArrowRight' });

    expect(rejoindre).toHaveAttribute('aria-selected', 'true');
    expect(rejoindre).toHaveFocus();
    expect(screen.getByRole('tabpanel')).toBeInTheDocument();

    fireEvent.keyDown(screen.getByRole('tablist'), { key: 'ArrowLeft' });
    expect(creer).toHaveAttribute('aria-selected', 'true');
    expect(creer).toHaveFocus();
  });

  it('chaque bouton d’invitation nomme l’ami visé', async () => {
    useFriends.setState({
      contacts: [
        { pubkey: 'pk_alice', display_name: 'Alice', state: 'friend' },
      ] as unknown as Contact[],
    });
    useGroups.setState({ states: { g1: groupStateFixture() } });
    useUi.setState({ lang: 'fr', modal: { kind: 'invite', groupId: 'g1' } });
    render(<Modals />);

    expect(
      await screen.findByRole('button', { name: 'Inviter Alice' }),
    ).toBeInTheDocument();
  });
});
