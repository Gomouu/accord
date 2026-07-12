/**
 * Tests de la vue FORUM : la liste des posts (fils) rendue depuis un état de
 * groupe, séparée actifs / archivés, et le flux « Nouveau post » qui crée un
 * fil sous le forum puis publie le premier message DANS le fil (jamais dans la
 * racine du forum, refusée par le nœud). `root_msg` = 16 octets zéro.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';

vi.mock('../lib/client', () => ({
  rpc: { call: vi.fn(), onEvent: vi.fn(() => () => {}), onStatus: vi.fn(() => () => {}) },
  api: {},
}));

import type { GroupChannel, GroupThread } from '../lib/api';
import { useGroups } from '../stores/groups';
import { useUi } from '../stores/ui';
import { ForumView } from './ForumView';

const GROUP = 'g1';
const FORUM = 'forum-chan';

function forumChannel(over: Partial<GroupChannel> = {}): GroupChannel {
  return {
    channel_id: FORUM,
    name: 'entraide',
    kind: 'forum',
    category: null,
    position: 0,
    topic: 'Posez vos questions',
    ...over,
  };
}

function post(threadId: string, name: string, archived = false): GroupThread {
  return { thread_id: threadId, parent_channel: FORUM, root_msg: '0'.repeat(32), name, archived };
}

function renderForum(posts: GroupThread[], canPost = true) {
  return render(
    <ForumView
      groupId={GROUP}
      channel={forumChannel()}
      posts={posts}
      canManage={false}
      canModerate={false}
      canPost={canPost}
      colorOf={() => null}
      emojiMap={new Map()}
      knownMentions={new Set()}
      automodWords={[]}
      nameOf={(a) => a}
    />,
  );
}

beforeEach(() => {
  useUi.setState({ lang: 'fr', toasts: [] });
});

describe('ForumView — liste des posts', () => {
  it('affiche les posts actifs et archivés dans leurs sections', () => {
    renderForum([
      post('t1', 'Bug au démarrage'),
      post('t2', 'Idée de fonctionnalité'),
      post('t3', 'Ancien sujet', true),
    ]);

    expect(screen.getByText('Posts actifs')).toBeInTheDocument();
    expect(screen.getByText('Posts archivés')).toBeInTheDocument();
    expect(screen.getByText('Bug au démarrage')).toBeInTheDocument();
    expect(screen.getByText('Idée de fonctionnalité')).toBeInTheDocument();
    expect(screen.getByText('Ancien sujet')).toBeInTheDocument();
  });

  it('montre l’état vide quand le forum n’a aucun post', () => {
    renderForum([]);
    expect(
      screen.getByText('Aucun post pour l’instant — créez-en un pour lancer la discussion.'),
    ).toBeInTheDocument();
    expect(screen.queryByText('Posts actifs')).not.toBeInTheDocument();
  });

  it('masque le bouton « Nouveau post » sans droit d’écriture', () => {
    renderForum([], false);
    expect(screen.queryByRole('button', { name: 'Nouveau post' })).not.toBeInTheDocument();
  });
});

describe('ForumView — création d’un post', () => {
  it('crée le fil (root_msg = zéro) puis publie le 1er message dans le fil', async () => {
    const createThread = vi.fn().mockResolvedValue('new-thread');
    const send = vi.fn().mockResolvedValue(undefined);
    useGroups.setState({ createThread, send });

    renderForum([]);

    fireEvent.click(screen.getByRole('button', { name: 'Nouveau post' }));
    fireEvent.change(screen.getByLabelText('Titre du post'), {
      target: { value: 'Mon premier post' },
    });
    fireEvent.change(screen.getByLabelText('Votre message'), {
      target: { value: 'Bonjour à tous' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Publier' }));

    await waitFor(() => expect(createThread).toHaveBeenCalledTimes(1));
    // Fil créé sous le FORUM avec un root_msg de 16 octets zéro (32 hex).
    expect(createThread).toHaveBeenCalledWith(GROUP, FORUM, '0'.repeat(32), 'Mon premier post');
    // 1er message publié DANS le fil (thread_id), jamais dans la racine forum.
    expect(send).toHaveBeenCalledWith(GROUP, 'new-thread', 'Bonjour à tous');
  });
});

describe('ForumView — accessibilité du formulaire « Nouveau post »', () => {
  it('déplace le focus sur le titre à l’ouverture, Échap referme et rend le focus', () => {
    renderForum([]);
    const bouton = screen.getByRole('button', { name: 'Nouveau post' });

    fireEvent.click(bouton);
    const titre = screen.getByLabelText('Titre du post');
    expect(titre).toHaveFocus();
    expect(bouton).toHaveAttribute('aria-expanded', 'true');

    fireEvent.keyDown(titre, { key: 'Escape' });
    expect(screen.queryByLabelText('Titre du post')).not.toBeInTheDocument();
    expect(bouton).toHaveFocus();
  });
});
