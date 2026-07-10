/**
 * Tests du fil de messages : rendu des corps, regroupement par auteur,
 * séparateurs de jour, pagination vers le haut, barre d'actions (garde
 * auteur-seul), édition en place, citations et pastilles de réaction.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, within } from '@testing-library/react';
import { MessageList, type DisplayMessage, type MessageListActions } from './MessageList';
import { useDms } from '../stores/dms';
import { useSession } from '../stores/session';
import { useUi } from '../stores/ui';

const DAY_MS = 24 * 60 * 60 * 1000;
const BASE_MS = new Date('2026-07-08T10:00:00').getTime();

const SELF = {
  node_id: 'noeud',
  pubkey: 'moi-pubkey',
  friend_code: 'accord-moi',
  name: null,
  bio: null,
  avatar: null,
  banner: null,
};

function textMsg(
  id: string,
  sentMs: number,
  text: string,
  extra: Partial<DisplayMessage> = {},
): DisplayMessage {
  return {
    msg_id: id,
    author: 'aabbccddee',
    sent_ms: sentMs,
    deleted: false,
    body: { type: 'text', text, reply_to: null, attachments: 0 },
    edited: null,
    ...extra,
  };
}

function makeActions(): MessageListActions {
  return {
    onReact: vi.fn(),
    onReply: vi.fn(),
    onEdit: vi.fn(),
    onDelete: vi.fn(),
  };
}

beforeEach(() => {
  useUi.setState({ lang: 'fr' });
  useSession.setState({ self: null });
});

describe('MessageList — rendu', () => {
  it('affiche le texte des messages et le nom court de l’auteur inconnu', () => {
    render(<MessageList messages={[textMsg('m1', BASE_MS, 'bonjour à tous')]} />);

    expect(screen.getByText('bonjour à tous')).toBeInTheDocument();
    expect(screen.getByText('aabbcc')).toBeInTheDocument();
  });

  it('regroupe les messages consécutifs du même auteur (nom affiché une fois)', () => {
    const messages = [
      textMsg('m1', BASE_MS, 'premier'),
      textMsg('m2', BASE_MS + 60_000, 'second'),
    ];
    render(<MessageList messages={messages} />);

    expect(screen.getAllByText('aabbcc')).toHaveLength(1);
    expect(screen.getByText('second')).toBeInTheDocument();
  });

  it('insère un séparateur par journée', () => {
    const messages = [
      textMsg('m1', BASE_MS - DAY_MS, 'hier'),
      textMsg('m2', BASE_MS, 'aujourd’hui'),
    ];
    render(<MessageList messages={messages} />);

    expect(screen.getAllByRole('separator')).toHaveLength(2);
  });

  it('signale les messages supprimés et les éditions', () => {
    const messages = [
      textMsg('m1', BASE_MS, 'disparu', { deleted: true }),
      textMsg('m2', BASE_MS + 1000, 'brouillon', { edited: 'version finale' }),
    ];
    render(<MessageList messages={messages} />);

    expect(screen.getByText('Message supprimé')).toBeInTheDocument();
    expect(screen.getByText(/version finale/)).toBeInTheDocument();
    expect(screen.getByText('(modifié)')).toBeInTheDocument();
  });
});

describe('MessageList — barre d’actions', () => {
  it('n’affiche aucune action sans le paramètre `actions` (salons de groupe)', () => {
    useSession.setState({ self: SELF });
    render(<MessageList messages={[textMsg('m1', BASE_MS, 'bonjour')]} />);

    expect(screen.queryByLabelText('Répondre')).not.toBeInTheDocument();
    expect(screen.queryByLabelText('Ajouter une réaction')).not.toBeInTheDocument();
  });

  it('réserve modifier/supprimer à l’auteur du message (garde auteur-seul)', () => {
    useSession.setState({ self: SELF });
    const messages = [
      textMsg('mien', BASE_MS, 'mon message', { author: SELF.pubkey }),
      textMsg('autre', BASE_MS + 1000, 'son message'),
    ];
    render(<MessageList messages={messages} actions={makeActions()} />);

    // Réagir et répondre sur les deux messages ; modifier/supprimer sur le sien.
    expect(screen.getAllByLabelText('Ajouter une réaction')).toHaveLength(2);
    expect(screen.getAllByLabelText('Répondre')).toHaveLength(2);
    expect(screen.getAllByLabelText('Modifier')).toHaveLength(1);
    expect(screen.getAllByLabelText('Supprimer')).toHaveLength(1);
  });

  it('n’affiche pas d’actions sur un message supprimé', () => {
    useSession.setState({ self: SELF });
    const messages = [
      textMsg('m1', BASE_MS, 'disparu', { author: SELF.pubkey, deleted: true }),
    ];
    render(<MessageList messages={messages} actions={makeActions()} />);

    expect(screen.queryByLabelText('Répondre')).not.toBeInTheDocument();
  });

  it('demande une confirmation légère avant de supprimer', () => {
    useSession.setState({ self: SELF });
    const actions = makeActions();
    const message = textMsg('m1', BASE_MS, 'à supprimer', { author: SELF.pubkey });
    render(<MessageList messages={[message]} actions={actions} />);

    fireEvent.click(screen.getByLabelText('Supprimer'));
    expect(actions.onDelete).not.toHaveBeenCalled();

    const dialog = screen.getByRole('alertdialog', { name: 'Supprimer ce message ?' });
    fireEvent.click(within(dialog).getByRole('button', { name: 'Supprimer' }));
    expect(actions.onDelete).toHaveBeenCalledWith(message);
  });

  it('réagit via le petit choix d’emojis courants', () => {
    useSession.setState({ self: SELF });
    const actions = makeActions();
    const message = textMsg('m1', BASE_MS, 'bonjour');
    render(<MessageList messages={[message]} actions={actions} />);

    fireEvent.click(screen.getByLabelText('Ajouter une réaction'));
    fireEvent.click(screen.getByRole('menuitem', { name: 'Réagir avec 👍' }));

    expect(actions.onReact).toHaveBeenCalledWith(message, '👍');
  });

  it('transmet le message visé par « répondre »', () => {
    useSession.setState({ self: SELF });
    const actions = makeActions();
    const message = textMsg('m1', BASE_MS, 'bonjour');
    render(<MessageList messages={[message]} actions={actions} />);

    fireEvent.click(screen.getByLabelText('Répondre'));

    expect(actions.onReply).toHaveBeenCalledWith(message);
  });
});

describe('MessageList — édition en place', () => {
  function renderEditing(actions: MessageListActions) {
    useSession.setState({ self: SELF });
    render(
      <MessageList
        messages={[textMsg('m1', BASE_MS, 'brouillon', { author: SELF.pubkey })]}
        actions={actions}
      />,
    );
    fireEvent.click(screen.getByLabelText('Modifier'));
    return screen.getByRole('textbox', { name: 'Modifier' }) as HTMLTextAreaElement;
  }

  it('remplace le corps par une zone pré-remplie, Entrée enregistre', () => {
    const actions = makeActions();
    const textarea = renderEditing(actions);

    expect(textarea.value).toBe('brouillon');
    fireEvent.change(textarea, { target: { value: 'version finale' } });
    fireEvent.keyDown(textarea, { key: 'Enter' });

    expect(actions.onEdit).toHaveBeenCalledWith(
      expect.objectContaining({ msg_id: 'm1' }),
      'version finale',
    );
    expect(screen.queryByRole('textbox', { name: 'Modifier' })).not.toBeInTheDocument();
    expect(screen.getByLabelText('Répondre')).toBeInTheDocument();
  });

  it('Échap annule sans enregistrer', () => {
    const actions = makeActions();
    const textarea = renderEditing(actions);

    fireEvent.change(textarea, { target: { value: 'abandonné' } });
    fireEvent.keyDown(textarea, { key: 'Escape' });

    expect(actions.onEdit).not.toHaveBeenCalled();
    expect(screen.getByText('brouillon')).toBeInTheDocument();
  });

  it('n’appelle pas l’API quand le texte est inchangé', () => {
    const actions = makeActions();
    const textarea = renderEditing(actions);

    fireEvent.keyDown(textarea, { key: 'Enter' });

    expect(actions.onEdit).not.toHaveBeenCalled();
  });
});

describe('MessageList — citations et réactions', () => {
  it('affiche la citation au-dessus d’une réponse', () => {
    const messages = [
      textMsg('orig', BASE_MS, 'message original'),
      textMsg('rep', BASE_MS + 1000, 'la réponse', {
        body: { type: 'text', text: 'la réponse', reply_to: 'orig', attachments: 0 },
      }),
    ];
    render(<MessageList messages={messages} />);

    // Le texte original apparaît dans son message et dans la citation.
    expect(screen.getAllByText('message original')).toHaveLength(2);
  });

  it('signale une citation dont l’original n’est pas chargé', () => {
    const messages = [
      textMsg('rep', BASE_MS, 'la réponse', {
        body: { type: 'text', text: 'la réponse', reply_to: 'inconnu', attachments: 0 },
      }),
    ];
    render(<MessageList messages={messages} />);

    expect(screen.getByText('Message d’origine indisponible')).toBeInTheDocument();
  });

  it('affiche les réactions agrégées sous le message', () => {
    useSession.setState({ self: SELF });
    const messages = [
      textMsg('m1', BASE_MS, 'bonjour', {
        reactions: [
          { emoji: '👍', author: 'aabbccddee' },
          { emoji: '👍', author: SELF.pubkey },
        ],
      }),
    ];
    render(<MessageList messages={messages} />);

    const pill = screen.getByRole('button', { name: 'Réagir avec 👍' });
    expect(pill).toHaveTextContent('2');
    expect(pill).toHaveAttribute('aria-pressed', 'true');
  });

  it('bascule sa réaction au clic sur une pastille', () => {
    useSession.setState({ self: SELF });
    const actions = makeActions();
    const message = textMsg('m1', BASE_MS, 'bonjour', {
      reactions: [{ emoji: '🎉', author: 'aabbccddee' }],
    });
    render(<MessageList messages={[message]} actions={actions} />);

    fireEvent.click(screen.getByRole('button', { name: 'Réagir avec 🎉' }));

    expect(actions.onReact).toHaveBeenCalledWith(message, '🎉');
  });
});

describe('MessageList — mode salon (groupes)', () => {
  it('masque « Répondre » quand onReply est absent (API des salons)', () => {
    useSession.setState({ self: SELF });
    const base = makeActions();
    const groupActions: MessageListActions = {
      onReact: base.onReact,
      onEdit: base.onEdit,
      onDelete: base.onDelete,
    };
    render(
      <MessageList
        messages={[textMsg('m1', BASE_MS, 'bonjour')]}
        actions={groupActions}
      />,
    );

    expect(screen.queryByLabelText('Répondre')).not.toBeInTheDocument();
    expect(screen.getByLabelText('Ajouter une réaction')).toBeInTheDocument();
  });

  it('permet la suppression du message d’autrui avec canModerate', () => {
    useSession.setState({ self: SELF });
    render(
      <MessageList
        messages={[textMsg('autre', BASE_MS, 'son message')]}
        actions={{ ...makeActions(), canModerate: true }}
      />,
    );

    // Suppression proposée (modération), édition toujours réservée à l'auteur.
    expect(screen.getByLabelText('Supprimer')).toBeInTheDocument();
    expect(screen.queryByLabelText('Modifier')).not.toBeInTheDocument();
  });

  it('épingle via la barre d’actions et bascule le libellé selon l’état', () => {
    useSession.setState({ self: SELF });
    const onTogglePin = vi.fn();
    const pinnedMsg = textMsg('epingle', BASE_MS, 'déjà épinglé');
    const freeMsg = textMsg('libre', BASE_MS + 1000, 'pas encore');
    render(
      <MessageList
        messages={[pinnedMsg, freeMsg]}
        actions={{ ...makeActions(), onTogglePin }}
        pinnedIds={new Set(['epingle'])}
      />,
    );

    expect(screen.getByLabelText('Désépingler')).toBeInTheDocument();
    fireEvent.click(screen.getByLabelText('Épingler'));

    expect(onTogglePin).toHaveBeenCalledWith(freeMsg, false);
  });

  it('colore le nom de l’auteur via colorOf (rôle le plus haut)', () => {
    render(
      <MessageList
        messages={[textMsg('m1', BASE_MS, 'bonjour')]}
        colorOf={() => '#ff0000'}
      />,
    );

    expect(screen.getByText('aabbcc')).toHaveStyle({ color: '#ff0000' });
  });
});

describe('MessageList — pagination vers le haut', () => {
  it('charge l’historique plus ancien en approchant du haut du fil', () => {
    const onLoadOlder = vi.fn();
    render(
      <MessageList
        messages={[textMsg('m1', BASE_MS, 'bonjour')]}
        hasMore
        onLoadOlder={onLoadOlder}
      />,
    );

    // jsdom : scrollTop vaut 0, donc sous le seuil de déclenchement.
    fireEvent.scroll(screen.getByRole('log'));
    expect(onLoadOlder).toHaveBeenCalledTimes(1);
  });

  it('ne déclenche rien quand tout l’historique est chargé', () => {
    const onLoadOlder = vi.fn();
    render(
      <MessageList
        messages={[textMsg('m1', BASE_MS, 'bonjour')]}
        hasMore={false}
        onLoadOlder={onLoadOlder}
      />,
    );

    fireEvent.scroll(screen.getByRole('log'));
    expect(onLoadOlder).not.toHaveBeenCalled();
  });
});

describe('MessageList — indicateur « Vu » (accusés de lecture)', () => {
  /** Enveloppe du store des MP (lamport connu) pour un message affiché. */
  function dmEntry(id: string, author: string, lamport: number) {
    return {
      msg_id: id,
      author,
      lamport,
      sent_ms: BASE_MS + lamport,
      acked: true,
      deleted: false,
      body: { type: 'text' as const, text: id, reply_to: null, attachments: 0 },
      edited: null,
    };
  }

  function arrangeDm(peerRead: Record<string, number>) {
    useSession.setState({ self: SELF });
    useUi.setState({ lang: 'fr', view: { kind: 'dm', peer: 'aabbccddee' } });
    useDms.setState({
      conversations: {
        aabbccddee: [
          dmEntry('mien-1', SELF.pubkey, 1),
          dmEntry('pair-1', 'aabbccddee', 2),
          dmEntry('mien-2', SELF.pubkey, 3),
          dmEntry('mien-3', SELF.pubkey, 4),
        ],
      },
      peerRead,
    });
    return [
      textMsg('mien-1', BASE_MS, 'un', { author: SELF.pubkey }),
      textMsg('pair-1', BASE_MS + 1, 'deux'),
      textMsg('mien-2', BASE_MS + 2, 'trois', { author: SELF.pubkey }),
      textMsg('mien-3', BASE_MS + 3, 'quatre', { author: SELF.pubkey }),
    ];
  }

  it('marque « Vu » le dernier de ses messages couvert par l’accusé', () => {
    const messages = arrangeDm({ aabbccddee: 3 });
    render(<MessageList messages={messages} />);

    const seen = screen.getByText('Vu');
    expect(seen).toBeInTheDocument();
    // Un seul indicateur, sous « trois » (lamport 3), pas sous « quatre » (4).
    expect(screen.getAllByText('Vu')).toHaveLength(1);
    expect(seen.parentElement?.textContent).toContain('trois');
  });

  it('reste absent tant que le pair n’a rien lu', () => {
    const messages = arrangeDm({});
    render(<MessageList messages={messages} />);

    expect(screen.queryByText('Vu')).not.toBeInTheDocument();
  });

  it('reste absent dans les salons de groupe', () => {
    const messages = arrangeDm({ aabbccddee: 4 });
    render(<MessageList messages={messages} groupId="g1" />);

    expect(screen.queryByText('Vu')).not.toBeInTheDocument();
  });
});
