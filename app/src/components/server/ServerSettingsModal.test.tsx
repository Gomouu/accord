/**
 * Tests des paramètres du serveur : structure et onglets, contrôles gouvernés
 * par my_permissions (ADMIN voit tout, un simple membre est en lecture
 * seule), hiérarchie des rôles, expulsion/bannissement confirmés, débannir,
 * et « Quitter le serveur » (refusé au fondateur accompagné).
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Mock } from 'vitest';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';

vi.mock('../../lib/client', () => ({
  rpc: {
    call: vi.fn(),
    onEvent: vi.fn(() => () => {}),
    onStatus: vi.fn(() => () => {}),
  },
  api: {
    filesRead: vi.fn(),
    groupsState: vi.fn(),
    groupsList: vi.fn(),
    groupsRename: vi.fn(),
    groupsSetIcon: vi.fn(),
    groupsSetTopic: vi.fn(),
    groupsChannelAdd: vi.fn(),
    groupsChannelEdit: vi.fn(),
    groupsChannelPerms: vi.fn(),
    groupsChannelDel: vi.fn(),
    groupsCategoryAdd: vi.fn(),
    groupsCategoryEdit: vi.fn(),
    groupsCategoryDel: vi.fn(),
    groupsAudit: vi.fn(),
    groupsKick: vi.fn(),
    groupsBan: vi.fn(),
    groupsUnban: vi.fn(),
    groupsLeave: vi.fn(),
    groupsRoleAdd: vi.fn(),
    groupsRoleEdit: vi.fn(),
    groupsRoleDel: vi.fn(),
    groupsRoleAssign: vi.fn(),
    groupsRoleUnassign: vi.fn(),
  },
}));

import { api } from '../../lib/client';
import type { GroupStateJson, SelfProfile } from '../../lib/api';
import { useFriends } from '../../stores/friends';
import { useGroups } from '../../stores/groups';
import { useSession } from '../../stores/session';
import { useUi } from '../../stores/ui';
import { ServerSettingsModal } from './ServerSettingsModal';

const stateMock = api.groupsState as unknown as Mock;
const renameMock = api.groupsRename as unknown as Mock;
const kickMock = api.groupsKick as unknown as Mock;
const unbanMock = api.groupsUnban as unknown as Mock;
const leaveMock = api.groupsLeave as unknown as Mock;
const channelEditMock = api.groupsChannelEdit as unknown as Mock;
const channelPermsMock = api.groupsChannelPerms as unknown as Mock;
const categoryEditMock = api.groupsCategoryEdit as unknown as Mock;
const categoryDelMock = api.groupsCategoryDel as unknown as Mock;
const roleEditMock = api.groupsRoleEdit as unknown as Mock;
const auditMock = api.groupsAudit as unknown as Mock;

const moi: SelfProfile = {
  node_id: 'n-moi',
  pubkey: 'moi',
  friend_code: 'accord-moi',
  name: null,
  bio: null,
  avatar: null,
  banner: null,
};

const fondateur: SelfProfile = {
  node_id: 'n-f',
  pubkey: 'fondateur',
  friend_code: 'accord-fondateur',
  name: null,
  bio: null,
  avatar: null,
  banner: null,
};

function makeState(over: Partial<GroupStateJson> = {}): GroupStateJson {
  return {
    group_id: 'g1',
    name: 'Guilde',
    icon: null,
    founder: 'fondateur',
    members: [
      { pubkey: 'fondateur', roles: [] },
      { pubkey: 'moi', roles: ['modo'] },
      { pubkey: 'autre', roles: [] },
    ],
    bans: ['banni'],
    channels: [
      {
        channel_id: 'ch1',
        name: 'général',
        kind: 'text',
        category: null,
        position: 0,
        topic: 'Accueil du serveur',
      },
      {
        channel_id: 'ch2',
        name: 'blabla',
        kind: 'text',
        category: 'cat1',
        position: 0,
        topic: '',
      },
    ],
    categories: [{ category_id: 'cat1', name: 'Papotage', position: 0 }],
    roles: [
      { role_id: 'modo', name: 'Modo', color: 0xff0000, position: 5, permissions: 0x8 },
      { role_id: 'haut', name: 'Haut', color: 0, position: 9, permissions: 0 },
      { role_id: 'bas', name: 'Bas', color: 0, position: 1, permissions: 0 },
    ],
    invites: [],
    my_permissions: 0x1ff,
    ...over,
  };
}

function seed(state: GroupStateJson, self: SelfProfile = moi): void {
  useUi.setState({
    lang: 'fr',
    modal: { kind: 'serverSettings', groupId: 'g1' },
    toasts: [],
  });
  useSession.setState({ self, phase: 'ready' });
  useFriends.setState({ contacts: [], loaded: true });
  useGroups.setState({
    ids: ['g1'],
    states: { g1: state },
    messages: {},
    hasMore: {},
    loadingOlder: {},
    pins: {},
  });
}

function openTab(label: string): void {
  fireEvent.click(screen.getByRole('button', { name: label }));
}

beforeEach(() => {
  for (const mock of [
    stateMock,
    renameMock,
    kickMock,
    unbanMock,
    leaveMock,
    channelEditMock,
    channelPermsMock,
    categoryEditMock,
    categoryDelMock,
    roleEditMock,
    auditMock,
  ]) {
    mock.mockReset();
  }
  stateMock.mockResolvedValue(makeState());
  auditMock.mockResolvedValue({ entries: [] });
});

describe('ServerSettingsModal — structure', () => {
  it('présente les onglets et ouvre le profil par défaut', () => {
    seed(makeState());
    render(<ServerSettingsModal groupId="g1" />);

    expect(
      screen.getByRole('dialog', { name: 'Paramètres du serveur' }),
    ).toBeInTheDocument();
    for (const label of ['Profil', 'Salons', 'Rôles', 'Membres', 'Bannis']) {
      expect(screen.getByRole('button', { name: label })).toBeInTheDocument();
    }
    expect(screen.getByRole('textbox', { name: 'Nom du serveur' })).toHaveValue('Guilde');
  });

  it('se ferme avec Échap', () => {
    seed(makeState());
    render(<ServerSettingsModal groupId="g1" />);

    fireEvent.keyDown(window, { key: 'Escape' });

    expect(useUi.getState().modal).toBeNull();
  });
});

describe('ServerSettingsModal — profil', () => {
  it('renomme le serveur (MANAGE_CHANNELS)', async () => {
    seed(makeState());
    renameMock.mockResolvedValueOnce({ ok: true });
    render(<ServerSettingsModal groupId="g1" />);

    const input = screen.getByRole('textbox', { name: 'Nom du serveur' });
    fireEvent.change(input, { target: { value: 'Nouvelle guilde' } });
    fireEvent.click(screen.getByRole('button', { name: 'Renommer' }));

    await waitFor(() => expect(renameMock).toHaveBeenCalledWith('g1', 'Nouvelle guilde'));
  });

  it('grise le nom et masque l’icône pour un simple membre', () => {
    seed(makeState({ my_permissions: 0x3 }));
    render(<ServerSettingsModal groupId="g1" />);

    expect(screen.getByRole('textbox', { name: 'Nom du serveur' })).toBeDisabled();
    expect(screen.queryByRole('button', { name: 'Renommer' })).not.toBeInTheDocument();
    expect(
      screen.queryByRole('button', { name: 'Choisir une image' }),
    ).not.toBeInTheDocument();
  });
});

describe('ServerSettingsModal — salons', () => {
  it('liste les salons par catégorie, sans-catégorie d’abord', () => {
    seed(makeState());
    render(<ServerSettingsModal groupId="g1" />);
    openTab('Salons');

    const sections = screen.getAllByRole('region');
    const labels = sections.map((s) => s.getAttribute('aria-label'));
    expect(labels.indexOf('Sans catégorie')).toBeLessThan(labels.indexOf('Papotage'));
    // Création possible pour l'ADMIN : salon (type + catégorie) et catégorie.
    expect(screen.getByText('Nouveau salon')).toBeInTheDocument();
    expect(screen.getByRole('combobox', { name: 'Type' })).toBeInTheDocument();
    expect(screen.getByText('Nouvelle catégorie')).toBeInTheDocument();
    // Le sujet du salon texte est éditable.
    expect(screen.getAllByRole('textbox', { name: 'Sujet' })[0]).toHaveValue(
      'Accueil du serveur',
    );
  });

  it('reste en lecture seule sans MANAGE_CHANNELS', () => {
    seed(makeState({ my_permissions: 0x3 }));
    render(<ServerSettingsModal groupId="g1" />);
    openTab('Salons');

    expect(screen.queryByText('Nouveau salon')).not.toBeInTheDocument();
    expect(screen.getByText('général')).toBeInTheDocument();
    expect(
      screen.queryByRole('textbox', { name: 'Nom du salon' }),
    ).not.toBeInTheDocument();
  });
});

describe('ServerSettingsModal — rôles', () => {
  it('expose création, couleur, permissions et attribution pour l’ADMIN', () => {
    seed(makeState());
    render(<ServerSettingsModal groupId="g1" />);
    openTab('Rôles');

    expect(screen.getByText('Nouveau rôle')).toBeInTheDocument();
    // 3 rôles éditables × 10 cases de permission.
    expect(screen.getAllByRole('checkbox')).toHaveLength(30);
    expect(
      screen.getAllByText('Administrateur (toutes les permissions)').length,
    ).toBeGreaterThan(0);
    // « moi » porte déjà Modo : le bouton bascule sur Retirer.
    expect(screen.getAllByRole('button', { name: 'Retirer' })).toHaveLength(1);
    expect(screen.getAllByRole('button', { name: 'Attribuer' }).length).toBeGreaterThan(
      0,
    );
  });

  it('verrouille les rôles de position supérieure ou égale à la sienne', () => {
    // « moi » (rôle Modo, position 5) n'a que MANAGE_ROLES : Haut (9) et
    // Modo (5) sont verrouillés, Bas (1) reste éditable.
    seed(makeState({ my_permissions: 0x80 }));
    render(<ServerSettingsModal groupId="g1" />);
    openTab('Rôles');

    expect(
      screen.getAllByText(
        'Rôle de position supérieure ou égale à la vôtre — lecture seule.',
      ),
    ).toHaveLength(2);
    expect(screen.getAllByRole('checkbox')).toHaveLength(10);
  });
});

describe('ServerSettingsModal — membres', () => {
  it('expulse un membre après confirmation (jamais soi-même ni le fondateur)', async () => {
    seed(makeState());
    kickMock.mockResolvedValueOnce({ ok: true });
    render(<ServerSettingsModal groupId="g1" />);
    openTab('Membres');

    // Seul « autre » est expulsable : ni soi-même, ni le fondateur.
    const kickButtons = screen.getAllByRole('button', { name: 'Expulser' });
    expect(kickButtons).toHaveLength(1);
    fireEvent.click(kickButtons[0]!);
    expect(kickMock).not.toHaveBeenCalled();

    const dialog = screen.getByRole('alertdialog');
    fireEvent.click(within(dialog).getByRole('button', { name: 'Confirmer' }));

    await waitFor(() => expect(kickMock).toHaveBeenCalledWith('g1', 'autre'));
  });

  it('colore le nom selon le rôle le plus haut et affiche les rôles', () => {
    seed(makeState());
    render(<ServerSettingsModal groupId="g1" />);
    openTab('Membres');

    expect(screen.getByText('accord-moi (vous)')).toHaveStyle({ color: '#ff0000' });
    expect(screen.getByText('Modo')).toBeInTheDocument();
    expect(screen.getByText('fondateur')).toBeInTheDocument();
  });

  it('masque expulsion et bannissement sans les permissions', () => {
    seed(makeState({ my_permissions: 0x3 }));
    render(<ServerSettingsModal groupId="g1" />);
    openTab('Membres');

    expect(screen.queryByRole('button', { name: 'Expulser' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Bannir' })).not.toBeInTheDocument();
  });
});

describe('ServerSettingsModal — bannis', () => {
  it('débannit avec la permission BAN', async () => {
    seed(makeState());
    unbanMock.mockResolvedValueOnce({ ok: true });
    render(<ServerSettingsModal groupId="g1" />);
    openTab('Bannis');

    fireEvent.click(screen.getByRole('button', { name: 'Débannir' }));

    await waitFor(() => expect(unbanMock).toHaveBeenCalledWith('g1', 'banni'));
  });

  it('cache le bouton sans la permission BAN', () => {
    seed(makeState({ my_permissions: 0x3 }));
    render(<ServerSettingsModal groupId="g1" />);
    openTab('Bannis');

    expect(screen.queryByRole('button', { name: 'Débannir' })).not.toBeInTheDocument();
  });
});

describe('ServerSettingsModal — quitter le serveur', () => {
  it('quitte après confirmation et revient à l’accueil', async () => {
    seed(makeState({ my_permissions: 0x3 }));
    leaveMock.mockResolvedValueOnce({ ok: true });
    render(<ServerSettingsModal groupId="g1" />);

    fireEvent.click(screen.getByRole('button', { name: 'Quitter le serveur' }));
    const dialog = screen.getByRole('alertdialog');
    fireEvent.click(within(dialog).getByRole('button', { name: 'Confirmer' }));

    await waitFor(() => expect(leaveMock).toHaveBeenCalledWith('g1'));
    await waitFor(() => expect(useUi.getState().modal).toBeNull());
    expect(useUi.getState().view).toEqual({ kind: 'friends' });
  });

  it('désactive le départ du fondateur accompagné, avec explication', () => {
    seed(makeState(), fondateur);
    render(<ServerSettingsModal groupId="g1" />);

    expect(screen.getByRole('button', { name: 'Quitter le serveur' })).toBeDisabled();
    expect(
      screen.getByText(
        'Le fondateur ne peut pas quitter le serveur tant qu’il reste d’autres membres.',
      ),
    ).toBeInTheDocument();
    expect(leaveMock).not.toHaveBeenCalled();
  });

  it('laisse partir un fondateur resté seul', () => {
    seed(makeState({ members: [{ pubkey: 'fondateur', roles: [] }] }), fondateur);
    render(<ServerSettingsModal groupId="g1" />);

    expect(screen.getByRole('button', { name: 'Quitter le serveur' })).toBeEnabled();
  });
});

describe('ServerSettingsModal — catégories', () => {
  it('renomme puis supprime une catégorie (salons conservés)', async () => {
    seed(makeState());
    categoryEditMock.mockResolvedValue({ ok: true });
    categoryDelMock.mockResolvedValue({ ok: true });
    render(<ServerSettingsModal groupId="g1" />);
    openTab('Salons');

    const input = screen.getByRole('textbox', {
      name: 'Renommer la catégorie Papotage',
    });
    expect(input).toHaveDisplayValue('Papotage');
    fireEvent.change(input, { target: { value: 'Discussions' } });
    fireEvent.click(screen.getByRole('button', { name: 'Renommer' }));
    await waitFor(() =>
      expect(categoryEditMock).toHaveBeenCalledWith('g1', 'cat1', {
        name: 'Discussions',
      }),
    );

    fireEvent.click(screen.getByRole('button', { name: 'Supprimer la catégorie' }));
    const dialog = screen.getByRole('alertdialog');
    fireEvent.click(within(dialog).getByRole('button', { name: 'Confirmer' }));
    await waitFor(() => expect(categoryDelMock).toHaveBeenCalledWith('g1', 'cat1'));
  });

  it('masque les contrôles de catégorie sans MANAGE_CHANNELS', () => {
    seed(makeState({ my_permissions: 0x3 }));
    render(<ServerSettingsModal groupId="g1" />);
    openTab('Salons');

    expect(screen.queryByDisplayValue('Papotage')).not.toBeInTheDocument();
    expect(
      screen.queryByRole('button', { name: 'Supprimer la catégorie' }),
    ).not.toBeInTheDocument();
  });

  it('déplace un salon dans une catégorie via le sélecteur', async () => {
    seed(makeState());
    channelEditMock.mockResolvedValue({ ok: true });
    render(<ServerSettingsModal groupId="g1" />);
    openTab('Salons');

    // Comboboxes « Catégorie » : formulaire de création, puis un par salon
    // (général sans catégorie d'abord, puis blabla dans Papotage).
    const selects = screen.getAllByRole('combobox', { name: 'Catégorie' });
    expect(selects.length).toBe(3);
    fireEvent.change(selects[1]!, { target: { value: 'cat1' } });
    const save = screen
      .getAllByRole('button', { name: 'Enregistrer' })
      .find((b) => !(b as HTMLButtonElement).disabled);
    fireEvent.click(save!);

    await waitFor(() =>
      expect(channelEditMock).toHaveBeenCalledWith('g1', 'ch1', { category: 'cat1' }),
    );
  });
});

describe('ServerSettingsModal — overrides de permissions', () => {
  it('applique un refus tri-état par rôle sur un salon', async () => {
    seed(makeState());
    channelPermsMock.mockResolvedValue({ ok: true });
    render(<ServerSettingsModal groupId="g1" />);
    openTab('Salons');

    fireEvent.click(screen.getAllByRole('button', { name: 'Permissions' })[0]!);
    const select = screen.getByRole('combobox', {
      name: 'Modo — Envoyer des messages',
    });
    fireEvent.change(select, { target: { value: 'deny' } });

    await waitFor(() =>
      expect(channelPermsMock).toHaveBeenCalledWith('g1', 'ch1', 'modo', 0, 0x2),
    );
  });

  it('reflète l’override existant et sait revenir à l’héritage', async () => {
    seed(
      makeState({
        overrides: [{ channel_id: 'ch1', role_id: 'modo', allow: 0, deny: 0x2 }],
      }),
    );
    channelPermsMock.mockResolvedValue({ ok: true });
    render(<ServerSettingsModal groupId="g1" />);
    openTab('Salons');

    fireEvent.click(screen.getAllByRole('button', { name: 'Permissions' })[0]!);
    const select = screen.getByRole('combobox', {
      name: 'Modo — Envoyer des messages',
    });
    expect(select).toHaveValue('deny');
    fireEvent.change(select, { target: { value: 'inherit' } });

    await waitFor(() =>
      expect(channelPermsMock).toHaveBeenCalledWith('g1', 'ch1', 'modo', 0, 0),
    );
  });
});

describe('ServerSettingsModal — réordonnancement des rôles', () => {
  it('échange les positions avec le voisin via les flèches', async () => {
    seed(makeState());
    roleEditMock.mockResolvedValue({ ok: true });
    render(<ServerSettingsModal groupId="g1" />);
    openTab('Rôles');

    // Ordre affiché : Haut (9), Modo (5), Bas (1) — le premier ne monte pas.
    expect(screen.getByRole('button', { name: 'Monter Haut' })).toBeDisabled();
    fireEvent.click(screen.getByRole('button', { name: 'Descendre Haut' }));

    await waitFor(() =>
      expect(roleEditMock).toHaveBeenCalledWith('g1', 'haut', { position: 5 }),
    );
    expect(roleEditMock).toHaveBeenCalledWith('g1', 'modo', { position: 9 });
  });

  it('bloque tout déplacement croisant un rôle non gérable', () => {
    // « moi » (Modo, position 5) n'a que MANAGE_ROLES : seul Bas est
    // éditable et son voisin du dessus (Modo) ne l'est pas.
    seed(makeState({ my_permissions: 0x80 }));
    render(<ServerSettingsModal groupId="g1" />);
    openTab('Rôles');

    expect(screen.getByRole('button', { name: 'Monter Bas' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Descendre Bas' })).toBeDisabled();
    expect(screen.queryByRole('button', { name: 'Monter Haut' })).not.toBeInTheDocument();
  });
});

describe('ServerSettingsModal — journal d’audit', () => {
  it('cache l’onglet sans ADMIN', () => {
    seed(makeState({ my_permissions: 0x3 }));
    render(<ServerSettingsModal groupId="g1" />);

    expect(
      screen.queryByRole('button', { name: 'Journal d’audit' }),
    ).not.toBeInTheDocument();
  });

  it('liste les entrées décodées avec acteur et description', async () => {
    seed(makeState());
    auditMock.mockResolvedValue({
      entries: [
        {
          op_id: 'op2',
          lamport: 2,
          wall_ms: 1_000,
          author: 'fondateur',
          kind: 'kick',
          params: { member: 'autre' },
        },
        {
          op_id: 'op1',
          lamport: 1,
          wall_ms: 500,
          author: 'fondateur',
          kind: 'add_channel',
          params: { channel_id: 'ch1', name: 'général', kind: 'text' },
        },
      ],
    });
    render(<ServerSettingsModal groupId="g1" />);
    openTab('Journal d’audit');

    expect(await screen.findByText(/a expulsé autre/)).toBeInTheDocument();
    expect(screen.getByText(/a créé le salon général/)).toBeInTheDocument();
    expect(auditMock).toHaveBeenCalledWith('g1', undefined, 50);
  });

  it('pagine avec « Charger la suite » (curseur op_id)', async () => {
    seed(makeState());
    const page = Array.from({ length: 50 }, (_, i) => ({
      op_id: `op${i}`,
      lamport: 100 - i,
      wall_ms: 1_000,
      author: 'fondateur',
      kind: 'leave',
      params: {},
    }));
    auditMock
      .mockResolvedValueOnce({ entries: page })
      .mockResolvedValueOnce({ entries: [] });
    render(<ServerSettingsModal groupId="g1" />);
    openTab('Journal d’audit');

    fireEvent.click(await screen.findByRole('button', { name: 'Charger la suite' }));

    await waitFor(() => expect(auditMock).toHaveBeenLastCalledWith('g1', 'op49', 50));
  });
});
