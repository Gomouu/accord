/**
 * Tests du fil de messages : rendu des corps, regroupement par auteur,
 * séparateurs de jour, pagination vers le haut, barre d'actions (garde
 * auteur-seul), édition en place, citations et pastilles de réaction.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Mock } from 'vitest';
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import {
  MessageList,
  messageLink,
  type DisplayMessage,
  type MessageListActions,
} from './MessageList';
import type { Contact, GroupPoll, GroupStateJson } from '../lib/api';
import { useDms } from '../stores/dms';
import { useFriends } from '../stores/friends';
import { useGroups } from '../stores/groups';
import { useSession } from '../stores/session';
import { useUi } from '../stores/ui';

vi.mock('../lib/files', () => ({ lireFichier: vi.fn() }));

vi.mock('../lib/client', () => ({
  rpc: { call: vi.fn(), onEvent: vi.fn(() => () => {}), onStatus: vi.fn(() => () => {}) },
  api: {
    groupsPollVote: vi.fn(() => Promise.resolve({ ok: true })),
    groupsPollClose: vi.fn(() => Promise.resolve({ ok: true })),
    groupsState: vi.fn(() => Promise.resolve(undefined)),
  },
}));

import { lireFichier } from '../lib/files';
import { api } from '../lib/client';

const lireMock = lireFichier as unknown as Mock;
const pollVoteMock = api.groupsPollVote as unknown as Mock;
const pollCloseMock = api.groupsPollClose as unknown as Mock;
const pollStateMock = api.groupsState as unknown as Mock;

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
  pronouns: null,
  accent_color: null,
  banner_color: null,
  avatar_decoration: null,
  profile_effect: null,
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
  useUi.setState({ lang: 'fr', view: { kind: 'friends' } });
  useSession.setState({ self: null });
  useFriends.setState({ contacts: [] });
  useGroups.setState({ ids: [], states: {} });
  lireMock.mockReset();
  pollVoteMock.mockReset().mockResolvedValue({ ok: true });
  pollCloseMock.mockReset().mockResolvedValue({ ok: true });
  pollStateMock.mockReset().mockResolvedValue(undefined);
});

describe('messageLink', () => {
  it('encode un lien de message privé', () => {
    expect(messageLink({ kind: 'dm', peer: 'pk' }, 'm1')).toBe('accord:msg/dm:pk/m1');
  });

  it('encode un lien de salon de groupe', () => {
    expect(messageLink({ kind: 'group', groupId: 'g', channelId: 'c' }, 'm1')).toBe(
      'accord:msg/group:g:c/m1',
    );
  });

  it('rend null pour la vue Amis (aucune conversation)', () => {
    expect(messageLink({ kind: 'friends' }, 'm1')).toBeNull();
  });
});

describe('MessageList — transfert et lien', () => {
  it('expose les actions Transférer et Copier le lien', () => {
    render(
      <MessageList
        messages={[textMsg('m1', BASE_MS, 'salut')]}
        actions={makeActions()}
      />,
    );

    expect(screen.getByLabelText('Transférer')).toBeInTheDocument();
    expect(screen.getByLabelText('Copier le lien')).toBeInTheDocument();
  });

  it('ouvre le sélecteur de transfert', () => {
    render(
      <MessageList
        messages={[textMsg('m1', BASE_MS, 'salut')]}
        actions={makeActions()}
      />,
    );

    fireEvent.click(screen.getByLabelText('Transférer'));

    expect(
      screen.getByRole('dialog', { name: 'Transférer le message' }),
    ).toBeInTheDocument();
  });

  it('copie le lien du message dans le presse-papiers', () => {
    const writeText = vi.fn(() => Promise.resolve());
    Object.defineProperty(navigator, 'clipboard', {
      value: { writeText },
      configurable: true,
    });
    useUi.setState({ view: { kind: 'dm', peer: 'pk' } });
    render(
      <MessageList
        messages={[textMsg('m1', BASE_MS, 'salut')]}
        actions={makeActions()}
        groupId={null}
      />,
    );

    fireEvent.click(screen.getByLabelText('Copier le lien'));

    expect(writeText).toHaveBeenCalledWith('accord:msg/dm:pk/m1');
  });
});

describe('MessageList — rendu', () => {
  it("rend la décoration d'avatar annoncée par un contact", () => {
    useFriends.setState({
      contacts: [
        {
          node_id: 'noeud-pair',
          pubkey: 'aabbccddee',
          friend_code: 'accord-pair',
          display_name: 'Alice',
          bio: null,
          avatar: null,
          banner: null,
          avatar_decoration: 'neon_ring',
          state: 'friend',
          last_seen_ms: 0,
        },
      ],
    });

    render(<MessageList messages={[textMsg('m1', BASE_MS, 'bonjour')]} />);

    expect(screen.getByTestId('avatar-decoration')).toBeInTheDocument();
  });

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

  it('préserve la position de lecture quand un message arrive loin du bas', () => {
    const premier = textMsg('m1', BASE_MS, 'premier');
    const { rerender } = render(<MessageList messages={[premier]} />);
    const log = screen.getByRole('log');
    Object.defineProperties(log, {
      clientHeight: { configurable: true, value: 300 },
      scrollHeight: { configurable: true, value: 1_000 },
      scrollTop: { configurable: true, writable: true, value: 120 },
    });
    fireEvent.scroll(log);

    rerender(
      <MessageList messages={[premier, textMsg('m2', BASE_MS + 1_000, 'nouveau')]} />,
    );

    expect(log.scrollTop).toBe(120);
  });

  it('continue de suivre les messages quand la lecture est proche du bas', () => {
    const premier = textMsg('m1', BASE_MS, 'premier');
    const { rerender } = render(<MessageList messages={[premier]} />);
    const log = screen.getByRole('log');
    Object.defineProperties(log, {
      clientHeight: { configurable: true, value: 300 },
      scrollHeight: { configurable: true, value: 1_000 },
      scrollTop: { configurable: true, writable: true, value: 680 },
    });
    fireEvent.scroll(log);
    Object.defineProperty(log, 'scrollHeight', { configurable: true, value: 1_200 });

    rerender(
      <MessageList messages={[premier, textMsg('m2', BASE_MS + 1_000, 'nouveau')]} />,
    );

    expect(log.scrollTop).toBe(1_200);
  });

  it('préserve aussi l’ancre si un message arrive pendant la pagination', () => {
    const loadOlder = vi.fn();
    const premier = textMsg('m1', BASE_MS, 'premier');
    const { rerender } = render(
      <MessageList messages={[premier]} hasMore onLoadOlder={loadOlder} />,
    );
    const log = screen.getByRole('log');
    Object.defineProperties(log, {
      clientHeight: { configurable: true, value: 300 },
      scrollHeight: { configurable: true, value: 1_000 },
      scrollTop: { configurable: true, writable: true, value: 40 },
    });
    fireEvent.scroll(log);
    expect(loadOlder).toHaveBeenCalledOnce();

    Object.defineProperty(log, 'scrollHeight', { configurable: true, value: 1_100 });
    const recent = textMsg('m2', BASE_MS + 1_000, 'récent');
    rerender(
      <MessageList messages={[premier, recent]} hasMore onLoadOlder={loadOlder} />,
    );
    expect(log.scrollTop).toBe(40);

    Object.defineProperty(log, 'scrollHeight', { configurable: true, value: 1_300 });
    rerender(
      <MessageList
        messages={[textMsg('m0', BASE_MS - 1_000, 'ancien'), premier, recent]}
        hasMore
        onLoadOlder={loadOlder}
      />,
    );
    expect(log.scrollTop).toBe(240);
  });

  it('ouvre une autre conversation sur ses messages récents', () => {
    useUi.setState({ view: { kind: 'dm', peer: 'alice' } });
    const { rerender } = render(
      <MessageList messages={[textMsg('m1', BASE_MS, 'conversation A')]} />,
    );
    const log = screen.getByRole('log');
    Object.defineProperties(log, {
      clientHeight: { configurable: true, value: 300 },
      scrollHeight: { configurable: true, value: 1_000 },
      scrollTop: { configurable: true, writable: true, value: 120 },
    });
    fireEvent.scroll(log);
    Object.defineProperty(log, 'scrollHeight', { configurable: true, value: 1_400 });

    act(() => useUi.setState({ view: { kind: 'dm', peer: 'bob' } }));
    rerender(<MessageList messages={[textMsg('m2', BASE_MS, 'conversation B')]} />);

    expect(log.scrollTop).toBe(1_400);
  });
});

describe('MessageList — sticker (MsgBody kind 4)', () => {
  const HASH = 'ab'.repeat(32);

  function stickerMsg(id: string, sentMs: number): DisplayMessage {
    return {
      msg_id: id,
      author: 'aabbccddee',
      sent_ms: sentMs,
      deleted: false,
      body: { type: 'sticker', name: 'wave', merkle_root: HASH },
      edited: null,
    };
  }

  it('affiche le jeton texte de repli pendant le chargement de l’image', () => {
    lireMock.mockReturnValueOnce(new Promise(() => {}));
    render(<MessageList messages={[stickerMsg('m1', BASE_MS)]} />);

    expect(screen.getByText(':wave:')).toBeInTheDocument();
  });

  it('affiche l’image du sticker une fois chargée, avec alt `:name:`', async () => {
    lireMock.mockResolvedValueOnce('data:image/webp;base64,QUJD');
    const { container } = render(<MessageList messages={[stickerMsg('m1', BASE_MS)]} />);

    await waitFor(() => {
      expect(container.querySelector('img[alt=":wave:"]')).toBeInTheDocument();
    });
    expect(lireMock).toHaveBeenCalledWith(HASH, 'aabbccddee');
    expect(screen.queryByText(':wave:')).not.toBeInTheDocument();
  });

  it('un message sticker supprimé retombe sur le texte générique', () => {
    const deleted: DisplayMessage = { ...stickerMsg('m1', BASE_MS), deleted: true };
    render(<MessageList messages={[deleted]} />);

    expect(screen.getByText('Message supprimé')).toBeInTheDocument();
    expect(lireMock).not.toHaveBeenCalled();
  });

  it('n’expose pas l’action « Modifier » pour un sticker (auteur soi-même)', () => {
    lireMock.mockReturnValueOnce(new Promise(() => {}));
    useSession.setState({ self: { ...SELF, pubkey: 'aabbccddee' } });
    render(
      <MessageList messages={[stickerMsg('m1', BASE_MS)]} actions={makeActions()} />,
    );

    expect(screen.queryByLabelText('Modifier')).not.toBeInTheDocument();
    expect(screen.getByLabelText('Supprimer')).toBeInTheDocument();
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

describe('MessageList — état de livraison', () => {
  it('affiche « Réessayer » sur un message échoué et rappelle onRetry', () => {
    useSession.setState({ self: SELF });
    const onRetry = vi.fn();
    const actions: MessageListActions = { ...makeActions(), onRetry };
    const message = textMsg('m1', BASE_MS, 'raté', {
      author: SELF.pubkey,
      delivery: 'failed',
    });
    render(<MessageList messages={[message]} actions={actions} />);

    const retry = screen.getByRole('button', { name: 'Réessayer' });
    fireEvent.click(retry);

    expect(screen.getByText('Échec de l’envoi')).toBeInTheDocument();
    expect(onRetry).toHaveBeenCalledTimes(1);
    expect(onRetry).toHaveBeenCalledWith(expect.objectContaining({ msg_id: 'm1' }));
  });

  it('affiche l’indicateur d’envoi en cours (pending) sur son message', () => {
    useSession.setState({ self: SELF });
    const message = textMsg('m1', BASE_MS, 'en vol', {
      author: SELF.pubkey,
      delivery: 'pending',
    });
    render(<MessageList messages={[message]} actions={makeActions()} />);

    expect(screen.getByText('envoi…')).toBeInTheDocument();
  });

  it('n’affiche aucune relance pour un message livré', () => {
    useSession.setState({ self: SELF });
    const message = textMsg('m1', BASE_MS, 'ok', {
      author: SELF.pubkey,
      delivery: 'sent',
    });
    render(
      <MessageList
        messages={[message]}
        actions={{ ...makeActions(), onRetry: vi.fn() }}
      />,
    );

    expect(screen.queryByRole('button', { name: 'Réessayer' })).not.toBeInTheDocument();
  });
});

describe('MessageList — saut au message', () => {
  it('met en surbrillance la cible d’un scrollTarget', () => {
    const messages = [
      textMsg('m1', BASE_MS, 'premier'),
      textMsg('m2', BASE_MS + 60_000, 'cible'),
    ];
    render(<MessageList messages={messages} scrollTarget={{ msgId: 'm2', nonce: 1 }} />);

    const row = screen.getByText('cible').closest('[data-msg-id]');
    expect(row).toHaveAttribute('data-msg-id', 'm2');
    expect(row).toHaveClass('msg-flash');
    // La cible seule est en surbrillance.
    expect(screen.getByText('premier').closest('[data-msg-id]')).not.toHaveClass(
      'msg-flash',
    );
  });

  it('un clic sur une citation demande le saut vers le message d’origine', () => {
    useUi.setState({ view: { kind: 'dm', peer: 'pair' }, jump: null });
    const messages = [
      textMsg('orig', BASE_MS, 'citation-cible'),
      textMsg('rep', BASE_MS + 1000, 'ma réponse', {
        body: { type: 'text', text: 'ma réponse', reply_to: 'orig', attachments: 0 },
      }),
    ];
    render(<MessageList messages={messages} actions={makeActions()} />);

    fireEvent.click(screen.getByRole('button', { name: /citation-cible/ }));

    expect(useUi.getState().jump).toMatchObject({ msgId: 'orig' });
  });
});

describe('MessageList — pseudos de serveur', () => {
  it('affiche le pseudo de serveur au lieu du pseudo global en salon', () => {
    useFriends.setState({
      contacts: [
        { pubkey: 'aabbccddee', display_name: 'GlobalAlice' } as unknown as Contact,
      ],
    });
    const state = {
      group_id: 'g1',
      name: 'G',
      icon: null,
      founder: null,
      members: [{ pubkey: 'aabbccddee', roles: [], nickname: 'ServerAlice' }],
      bans: [],
      channels: [],
      categories: [],
      roles: [],
      invites: [],
      my_permissions: 0,
    } satisfies GroupStateJson;
    useGroups.setState({ states: { g1: state } });

    render(<MessageList messages={[textMsg('m1', BASE_MS, 'coucou')]} groupId="g1" />);

    expect(screen.getByText('ServerAlice')).toBeInTheDocument();
    expect(screen.queryByText('GlobalAlice')).not.toBeInTheDocument();
  });

  it('retombe sur le pseudo global quand aucun pseudo de serveur n’est défini', () => {
    useFriends.setState({
      contacts: [
        { pubkey: 'aabbccddee', display_name: 'GlobalAlice' } as unknown as Contact,
      ],
    });
    const state = {
      group_id: 'g1',
      name: 'G',
      icon: null,
      founder: null,
      members: [{ pubkey: 'aabbccddee', roles: [] }],
      bans: [],
      channels: [],
      categories: [],
      roles: [],
      invites: [],
      my_permissions: 0,
    } satisfies GroupStateJson;
    useGroups.setState({ states: { g1: state } });

    render(<MessageList messages={[textMsg('m1', BASE_MS, 'coucou')]} groupId="g1" />);

    expect(screen.getByText('GlobalAlice')).toBeInTheDocument();
  });
});

describe('MessageList — sondage (MsgBody kind 7, D-048)', () => {
  function pollGroupState(over: Partial<GroupStateJson> = {}): GroupStateJson {
    return {
      group_id: 'g1',
      name: 'G',
      icon: null,
      founder: null,
      members: [{ pubkey: 'aabbccddee', roles: [] }],
      bans: [],
      channels: [],
      categories: [],
      roles: [],
      invites: [],
      my_permissions: 0,
      ...over,
    };
  }

  function pollTally(over: Partial<GroupPoll> = {}): GroupPoll {
    return {
      poll_id: 'p1',
      author: 'aabbccddee',
      closed: false,
      counts: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
      total_votes: 0,
      my_vote: null,
      ...over,
    };
  }

  function pollMsg(
    id: string,
    sentMs: number,
    extra: Partial<DisplayMessage> = {},
  ): DisplayMessage {
    return {
      msg_id: id,
      author: 'aabbccddee',
      sent_ms: sentMs,
      deleted: false,
      body: {
        type: 'poll',
        poll_id: 'p1',
        question: 'Pizza ou sushis ?',
        options: ['Pizza', 'Sushis'],
      },
      edited: null,
      ...extra,
    };
  }

  it('affiche la question et les options, votable à zéro sans entrée d’état encore convergée', () => {
    useGroups.setState({ states: { g1: pollGroupState({ polls: [] }) } });

    render(<MessageList messages={[pollMsg('m1', BASE_MS)]} groupId="g1" />);

    expect(screen.getByText('Pizza ou sushis ?')).toBeInTheDocument();
    expect(screen.getByLabelText('Voter pour Pizza')).toBeEnabled();
    expect(screen.getByText('0 vote(s)')).toBeInTheDocument();
  });

  it('désactive le vote et affiche « Résultats indisponibles » quand `polls` est absent de l’état', () => {
    useGroups.setState({ states: { g1: pollGroupState() } }); // pas de champ `polls`

    render(<MessageList messages={[pollMsg('m1', BASE_MS)]} groupId="g1" />);

    expect(screen.getByText('Résultats indisponibles')).toBeInTheDocument();
    expect(screen.getByLabelText('Voter pour Pizza')).toBeDisabled();
  });

  it('un clic sur une option appelle groups.polls.vote (RPC) et met à jour l’affichage (optimiste)', async () => {
    useGroups.setState({ states: { g1: pollGroupState({ polls: [pollTally()] }) } });

    render(<MessageList messages={[pollMsg('m1', BASE_MS)]} groupId="g1" />);
    fireEvent.click(screen.getByLabelText('Voter pour Sushis'));

    expect(pollVoteMock).toHaveBeenCalledWith('g1', 'p1', 1);
    await waitFor(() => {
      expect(screen.getByText('1 vote(s)')).toBeInTheDocument();
    });
    expect(screen.getByLabelText('Voter pour Sushis')).toHaveAttribute(
      'aria-pressed',
      'true',
    );
  });

  it('un sondage clos désactive le vote et affiche « Sondage fermé »', () => {
    useGroups.setState({
      states: {
        g1: pollGroupState({
          polls: [
            pollTally({ closed: true, counts: [1, 0], total_votes: 1, my_vote: 0 }),
          ],
        }),
      },
    });

    render(<MessageList messages={[pollMsg('m1', BASE_MS)]} groupId="g1" />);

    expect(screen.getByLabelText('Voter pour Pizza')).toBeDisabled();
    expect(screen.getByText('Sondage fermé')).toBeInTheDocument();
  });

  it('propose la fermeture à l’auteur du sondage', () => {
    useSession.setState({ self: { ...SELF, pubkey: 'aabbccddee' } });
    useGroups.setState({
      states: { g1: pollGroupState({ my_permissions: 0, polls: [pollTally()] }) },
    });

    render(<MessageList messages={[pollMsg('m1', BASE_MS)]} groupId="g1" />);

    expect(screen.getByText('Fermer le sondage')).toBeInTheDocument();
  });

  it('propose la fermeture à un porteur de MANAGE_CHANNELS même sans être l’auteur', () => {
    useSession.setState({ self: { ...SELF, pubkey: 'moderateur' } });
    useGroups.setState({
      states: { g1: pollGroupState({ my_permissions: 0x8, polls: [pollTally()] }) },
    });

    render(<MessageList messages={[pollMsg('m1', BASE_MS)]} groupId="g1" />);

    expect(screen.getByText('Fermer le sondage')).toBeInTheDocument();
  });

  it('masque la fermeture pour un membre ni auteur ni MANAGE_CHANNELS', () => {
    useSession.setState({ self: { ...SELF, pubkey: 'quelquun-dautre' } });
    useGroups.setState({
      states: { g1: pollGroupState({ my_permissions: 0, polls: [pollTally()] }) },
    });

    render(<MessageList messages={[pollMsg('m1', BASE_MS)]} groupId="g1" />);

    expect(screen.queryByText('Fermer le sondage')).not.toBeInTheDocument();
  });

  it('confirme puis appelle groups.polls.close au clic sur Fermer', async () => {
    useSession.setState({ self: { ...SELF, pubkey: 'aabbccddee' } });
    useGroups.setState({
      states: { g1: pollGroupState({ my_permissions: 0, polls: [pollTally()] }) },
    });

    render(<MessageList messages={[pollMsg('m1', BASE_MS)]} groupId="g1" />);
    fireEvent.click(screen.getByText('Fermer le sondage'));
    fireEvent.click(screen.getByText('Confirmer'));

    await waitFor(() => expect(pollCloseMock).toHaveBeenCalledWith('g1', 'p1'));
  });
});

describe('MessageList — séparateur nouveaux messages', () => {
  it('insère le séparateur avant le premier message d’autrui au-delà de la marque', () => {
    useSession.setState({ self: SELF });
    const messages = [
      textMsg('a', BASE_MS, 'lu', { lamport: 3 }),
      textMsg('b', BASE_MS + 60_000, 'nouveau', { lamport: 6 }),
    ];
    render(<MessageList messages={messages} dividerLamport={5} />);

    expect(
      screen.getByRole('separator', { name: 'Nouveaux messages' }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: /Sauter aux nouveaux/ }),
    ).toBeInTheDocument();
  });

  it('n’insère aucun séparateur quand tous les non-lus sont de soi', () => {
    useSession.setState({ self: SELF });
    const messages = [textMsg('a', BASE_MS, 'mien', { author: SELF.pubkey, lamport: 6 })];
    render(<MessageList messages={messages} dividerLamport={5} />);

    expect(
      screen.queryByRole('separator', { name: 'Nouveaux messages' }),
    ).not.toBeInTheDocument();
  });

  it('n’insère aucun séparateur pour une marque à zéro (jamais lu)', () => {
    const messages = [textMsg('a', BASE_MS, 'texte', { lamport: 6 })];
    render(<MessageList messages={messages} dividerLamport={0} />);

    expect(screen.queryByText('Nouveaux messages')).not.toBeInTheDocument();
  });
});

describe('MessageList — mode sélection (purge)', () => {
  it('affiche une case par message, reflète la sélection et masque les actions', () => {
    useSession.setState({ self: SELF });
    const onToggle = vi.fn();
    const messages = [
      textMsg('m1', BASE_MS, 'un'),
      textMsg('m2', BASE_MS + 1000, 'deux'),
    ];
    render(
      <MessageList
        messages={messages}
        actions={makeActions()}
        selection={{ active: true, selected: new Set(['m1']), onToggle }}
      />,
    );

    const boxes = screen.getAllByRole('checkbox', { name: 'Sélectionner le message' });
    expect(boxes).toHaveLength(2);
    expect(boxes[0]).toBeChecked();
    expect(boxes[1]).not.toBeChecked();

    fireEvent.click(boxes[1]!);
    expect(onToggle).toHaveBeenCalledWith('m2');

    // Les actions de survol sont masquées pendant la sélection.
    expect(screen.queryByLabelText('Ajouter une réaction')).not.toBeInTheDocument();
  });
});
