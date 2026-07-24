import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import type { GroupStateJson } from '../../lib/api';
import { PERMISSIONS, useGroups } from '../../stores/groups';
import { useMute } from '../../stores/mute';
import { useSession } from '../../stores/session';
import { useUi } from '../../stores/ui';
import { Modals } from '../Modals';
import { ServerHeaderMenu } from './ServerHeaderMenu';

const basePermissions = PERMISSIONS.VIEW | PERMISSIONS.SEND;
const originalLeave = useGroups.getState().leave;
const originalAddCategory = useGroups.getState().addCategory;

function groupState(myPermissions = basePermissions): GroupStateJson {
  return {
    group_id: 'g1',
    name: 'Guilde',
    icon: null,
    founder: null,
    members: [],
    bans: [],
    channels: [],
    categories: [],
    roles: [],
    invites: [],
    my_permissions: myPermissions,
  };
}

function seed(myPermissions = basePermissions): void {
  useUi.setState({
    lang: 'fr',
    modal: null,
    view: { kind: 'group', groupId: 'g1', channelId: null },
    hideMutedChannels: false,
  });
  useSession.setState({ self: null });
  useMute.setState({ serverLevels: {}, channelLevels: {} });
  useGroups.setState({
    ids: ['g1'],
    states: { g1: groupState(myPermissions) },
    unread: {},
    mentions: {},
    leave: originalLeave,
    addCategory: originalAddCategory,
  });
}

function renderMenu(withModals = false) {
  return render(
    <>
      <ServerHeaderMenu groupId="g1" onClose={vi.fn()} />
      {withModals && <Modals />}
    </>,
  );
}

beforeEach(() => seed());

afterEach(() => {
  vi.restoreAllMocks();
  useGroups.setState({
    leave: originalLeave,
    addCategory: originalAddCategory,
  });
});

describe('ServerHeaderMenu', () => {
  it('opens the dedicated channel modal instead of server settings', () => {
    seed(basePermissions | PERMISSIONS.MANAGE_CHANNELS);
    renderMenu();

    fireEvent.click(screen.getByRole('menuitem', { name: 'Créer un salon' }));

    expect(useUi.getState().modal).toEqual({
      kind: 'createChannel',
      groupId: 'g1',
    });
  });

  it('hides management actions from regular members', () => {
    renderMenu();

    expect(
      screen.queryByRole('menuitem', { name: 'Paramètres du serveur' }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole('menuitem', { name: 'Créer un salon' }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole('menuitem', { name: 'Créer une catégorie' }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole('menuitem', { name: 'Créer un événement' }),
    ).not.toBeInTheDocument();
  });

  it('uses a focused category dialog', async () => {
    const addCategory = vi.fn(() => Promise.resolve('category-1'));
    useGroups.setState({ addCategory });
    seed(basePermissions | PERMISSIONS.MANAGE_CHANNELS);
    useGroups.setState({ addCategory });
    renderMenu(true);

    fireEvent.click(screen.getByRole('menuitem', { name: 'Créer une catégorie' }));
    const dialog = screen.getByRole('dialog', { name: 'Nouvelle catégorie' });
    fireEvent.change(within(dialog).getByRole('textbox'), {
      target: { value: 'Design' },
    });
    fireEvent.click(within(dialog).getByRole('button', { name: 'Créer une catégorie' }));

    await waitFor(() => expect(addCategory).toHaveBeenCalledWith('g1', 'Design'));
    await waitFor(() => expect(useUi.getState().modal).toBeNull());
  });

  it('confirms leaving in app without window.confirm', async () => {
    const leave = vi.fn(() => Promise.resolve());
    const nativeConfirm = vi.spyOn(window, 'confirm');
    useGroups.setState({ leave });
    renderMenu(true);

    fireEvent.click(screen.getByRole('menuitem', { name: 'Quitter le serveur' }));

    expect(nativeConfirm).not.toHaveBeenCalled();
    expect(leave).not.toHaveBeenCalled();
    const dialog = screen.getByRole('alertdialog', { name: 'Quitter le serveur' });
    fireEvent.click(within(dialog).getByRole('button', { name: 'Confirmer' }));

    await waitFor(() => expect(leave).toHaveBeenCalledWith('g1'));
    await waitFor(() => expect(useUi.getState().modal).toBeNull());
    expect(useUi.getState().view).toEqual({ kind: 'friends' });
    expect(nativeConfirm).not.toHaveBeenCalled();
  });

  it('keeps the Discord-style section order and danger treatment', () => {
    seed(basePermissions | PERMISSIONS.INVITE | PERMISSIONS.MANAGE_CHANNELS);
    renderMenu();

    const menu = screen.getByRole('menu', { name: 'Menu du serveur' });
    const labels = Array.from(
      menu.querySelectorAll<HTMLElement>('[role="menuitem"],[role="menuitemcheckbox"]'),
    ).map((item) => item.textContent);

    expect(labels).toEqual([
      'Inviter des personnes',
      'Paramètres du serveur',
      'Créer un salon',
      'Créer une catégorie',
      'Créer un événement',
      'Notifications',
      'Masquer les salons muets',
      'Modifier mon profil de serveur',
      'Quitter le serveur',
      'Copier l’ID du serveur',
    ]);
    expect(within(menu).getAllByRole('separator')).toHaveLength(4);
    expect(
      within(menu).getByRole('menuitem', { name: 'Quitter le serveur' }),
    ).toHaveClass('server-menu-danger');
    expect(labels.at(-1)).toBe('Copier l’ID du serveur');
  });
});

describe('ServerHeaderMenu — marquer comme lu', () => {
  it('n’affiche pas l’entrée quand rien n’est non lu', () => {
    // Une action sans effet dans un menu use la confiance qu'on lui accorde.
    renderMenu();
    expect(
      screen.queryByRole('menuitem', { name: 'Marquer comme lu' }),
    ).not.toBeInTheDocument();
  });

  it('affiche l’entrée en tête dès qu’un salon a des non-lus', () => {
    // Régression réelle : l'entrée existait dans l'ancien menu de la barre
    // latérale et a disparu à la refonte du menu d'en-tête (4.5.0). Personne
    // ne l'a vu parce que l'e2e qui la couvrait n'était pas dans le gate.
    useGroups.setState({ unread: { g1: { general: 3 } } });
    renderMenu();
    const items = screen.getAllByRole('menuitem');
    expect(items[0]).toHaveTextContent('Marquer comme lu');
  });

  it('affiche l’entrée quand seule une mention est non lue', () => {
    useGroups.setState({ mentions: { g1: 1 } });
    renderMenu();
    expect(
      screen.getByRole('menuitem', { name: 'Marquer comme lu' }),
    ).toBeInTheDocument();
  });
});
