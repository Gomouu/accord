/**
 * Tests de la zone de saisie avec pièces jointes : aperçus retirables,
 * bornes UI (10 pièces, 8 Mio), publication via files.share_bytes à l'envoi
 * (texte vide admis), collage de fichiers et signalement des échecs.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Mock } from 'vitest';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { MAX_TAILLE_PIECE } from '../lib/attachments';
import type { Contact, DmMessage, GroupMessage, GroupStateJson } from '../lib/api';
import type {
  VoiceRecorderCallbacks,
  VoiceRecorderResult,
  VoiceRecorderError,
} from '../lib/voiceRecorder';
import { useContextMenu } from '../stores/contextMenu';
import { useDms } from '../stores/dms';
import { useFriends } from '../stores/friends';
import { channelKey, useGroups } from '../stores/groups';
import { useMessageEdit } from '../stores/messageEdit';
import { useSession } from '../stores/session';
import { useUi } from '../stores/ui';
import { MessageInput } from './MessageInput';

vi.mock('../lib/client', () => ({
  rpc: { onEvent: vi.fn(() => () => {}), onStatus: vi.fn(() => () => {}) },
  api: {
    filesShareBytes: vi.fn(),
    // Publication par chemin natif (bouton de pièce jointe sous Tauri) : non
    // exercée ici (jsdom n'est pas Tauri → repli sur `<input type=file>`), mais
    // présente pour que le chemin `choisirFichiers` ne casse pas au montage.
    filesShare: vi.fn(),
    // Émis en best effort par `useTypingEmitter` dès qu'un texte non vide
    // est saisi avec un `typingTarget` — non exercé par les tests existants
    // (aucun ne tapait dans un composeur avec cible), mais nécessaire dès
    // qu'un test ArrowUp saisit du texte avant d'appuyer sur Haut.
    dmTyping: vi.fn(() => Promise.resolve()),
    groupsTyping: vi.fn(() => Promise.resolve()),
  },
}));

/**
 * Enregistreur factice piloté depuis les tests : capture les rappels passés
 * par `MessageInput` pour simuler `onTick`/`onStop`/`onError` sans toucher au
 * micro réel (déjà couvert isolément par `lib/voiceRecorder.test.ts`). Défini
 * via `vi.hoisted` car `vi.mock` est hissé au-dessus des imports/déclarations.
 */
const { FakeVoiceRecorder } = vi.hoisted(() => {
  class FakeVoiceRecorder {
    static instances: FakeVoiceRecorder[] = [];
    started = false;
    stopped = false;
    canceled = false;
    callbacks: VoiceRecorderCallbacks;
    constructor(callbacks: VoiceRecorderCallbacks) {
      this.callbacks = callbacks;
      FakeVoiceRecorder.instances.push(this);
    }
    start(): Promise<void> {
      this.started = true;
      return Promise.resolve();
    }
    stop(): void {
      this.stopped = true;
    }
    cancel(): void {
      this.canceled = true;
    }
  }
  return { FakeVoiceRecorder };
});

vi.mock('../lib/voiceRecorder', () => ({
  VoiceRecorder: FakeVoiceRecorder,
  // Miroir de la convention réelle `voice-<durée>s.<ext>` (durée embarquée).
  voiceFileName: (mime: string, durationMs: number) =>
    `voice-${Math.round(durationMs / 100) / 10}s.${mime.includes('ogg') ? 'ogg' : 'webm'}`,
}));

import { api } from '../lib/client';

const shareMock = api.filesShareBytes as unknown as Mock;

/** Dernier enregistreur créé (un par clic sur le micro). */
function dernierEnregistreur(): InstanceType<typeof FakeVoiceRecorder> {
  const instance = FakeVoiceRecorder.instances.at(-1);
  if (instance === undefined) throw new Error('aucun VoiceRecorder créé');
  return instance;
}

beforeEach(() => {
  useUi.setState({ lang: 'fr', toasts: [], modal: null });
  useGroups.setState({ states: {}, messages: {} });
  useFriends.setState({ contacts: [] });
  useSession.setState({ self: null });
  useContextMenu.setState({ menu: null });
  useDms.setState({ conversations: {} });
  useMessageEdit.setState({ request: null });
  shareMock.mockReset();
  FakeVoiceRecorder.instances = [];
  window.localStorage.clear();
});

function renderInput(onSend = vi.fn(async () => {})) {
  render(<MessageInput placeholder="Écrire à @Alice" onSend={onSend} />);
  return onSend;
}

function addFiles(files: File[]): void {
  fireEvent.change(screen.getByLabelText('Joindre des fichiers', { selector: 'input' }), {
    target: { files },
  });
}

const IMAGE = new File(['ABC'], 'photo.png', { type: 'image/png' });
const PDF = new File(['%PDF'], 'doc.pdf', { type: 'application/pdf' });

describe('MessageInput — aperçus', () => {
  it('affiche nom et taille du fichier ajouté, vignette data: pour une image', async () => {
    renderInput();
    addFiles([IMAGE, PDF]);

    expect(screen.getByText('photo.png')).toBeInTheDocument();
    expect(screen.getByText('doc.pdf')).toBeInTheDocument();
    expect(screen.getByText('4 o')).toBeInTheDocument();
    // L'aperçu arrive après lecture du fichier (FileReader asynchrone).
    const vignette = await screen.findByAltText('photo.png');
    expect(vignette.getAttribute('src')).toMatch(/^data:image\/png;base64,/);
  });

  it('retire une pièce de la liste', () => {
    renderInput();
    addFiles([IMAGE]);

    fireEvent.click(screen.getByLabelText('Retirer photo.png'));

    expect(screen.queryByText('photo.png')).not.toBeInTheDocument();
  });

  it('ajoute les fichiers collés depuis le presse-papiers', () => {
    renderInput();

    fireEvent.paste(screen.getByRole('textbox'), {
      clipboardData: { files: [IMAGE] },
    });

    expect(screen.getByText('photo.png')).toBeInTheDocument();
  });
});

describe('MessageInput — bornes', () => {
  it('refuse un gros fichier déposé/collé et renvoie vers le bouton de pièce jointe', () => {
    renderInput();
    const gros = new File([new ArrayBuffer(MAX_TAILLE_PIECE + 1)], 'gros.bin');
    addFiles([gros]);

    expect(screen.getByRole('alert')).toHaveTextContent(
      '« gros.bin » est trop lourd pour le glisser-déposer — utilisez le bouton de pièce jointe (trombone) pour les gros fichiers',
    );
    expect(screen.queryByText('gros.bin')).not.toBeInTheDocument();
  });

  it('refuse au-delà de 10 pièces par message', () => {
    renderInput();
    const fichiers = Array.from(
      { length: 11 },
      (_, i) => new File(['x'], `f${i}.txt`, { type: 'text/plain' }),
    );
    addFiles(fichiers);

    expect(screen.getByRole('alert')).toHaveTextContent(
      '10 pièces jointes au maximum par message',
    );
    expect(screen.getByText('f9.txt')).toBeInTheDocument();
    expect(screen.queryByText('f10.txt')).not.toBeInTheDocument();
  });
});

describe('MessageInput — envoi', () => {
  it('publie chaque pièce puis envoie le message avec les références', async () => {
    const piece = {
      merkle_root: 'ab'.repeat(32),
      name: 'photo.png',
      size: 3,
      mime: 'image/png',
    };
    shareMock.mockResolvedValueOnce({ file: piece });
    const onSend = renderInput();

    addFiles([IMAGE]);
    fireEvent.change(screen.getByRole('textbox'), { target: { value: 'regarde !' } });
    fireEvent.click(screen.getByLabelText('Envoyer'));

    await waitFor(() => expect(onSend).toHaveBeenCalledWith('regarde !', [piece]));
    expect(shareMock).toHaveBeenCalledWith('photo.png', 'image/png', 'QUJD');
    // Les aperçus sont vidés après l'envoi.
    expect(screen.queryByText('photo.png')).not.toBeInTheDocument();
  });

  it('autorise l’envoi sans texte quand il y a des pièces jointes', async () => {
    shareMock.mockResolvedValueOnce({
      file: {
        merkle_root: 'cd'.repeat(32),
        name: 'doc.pdf',
        size: 4,
        mime: 'application/pdf',
      },
    });
    const onSend = renderInput();

    addFiles([PDF]);
    const envoyer = screen.getByLabelText('Envoyer');
    expect(envoyer).toBeEnabled();
    fireEvent.click(envoyer);

    await waitFor(() =>
      expect(onSend).toHaveBeenCalledWith('', [
        expect.objectContaining({ name: 'doc.pdf' }),
      ]),
    );
  });

  it('interdit l’envoi sans texte ni pièce jointe', () => {
    renderInput();

    expect(screen.getByLabelText('Envoyer')).toBeDisabled();
  });

  it('n’appelle pas files.share_bytes pour un envoi sans pièce', async () => {
    const onSend = renderInput();

    fireEvent.change(screen.getByRole('textbox'), { target: { value: 'bonjour' } });
    fireEvent.click(screen.getByLabelText('Envoyer'));

    await waitFor(() => expect(onSend).toHaveBeenCalledWith('bonjour', undefined));
    expect(shareMock).not.toHaveBeenCalled();
  });

  it('signale l’échec de publication et conserve la saisie', async () => {
    shareMock.mockRejectedValueOnce(new Error('trop volumineux'));
    const onSend = renderInput();

    addFiles([IMAGE]);
    fireEvent.change(screen.getByRole('textbox'), { target: { value: 'oups' } });
    fireEvent.click(screen.getByLabelText('Envoyer'));

    await waitFor(() =>
      expect(screen.getByRole('alert')).toHaveTextContent('Échec de l’envoi'),
    );
    expect(onSend).not.toHaveBeenCalled();
    expect(screen.getByRole('textbox')).toHaveValue('oups');
    expect(screen.getByText('photo.png')).toBeInTheDocument();
  });
});

/** Installe un groupe (deux membres, un rôle) et un contact nommé « Alice ». */
function setupGroup(): void {
  const state = {
    group_id: 'g1',
    name: 'G',
    icon: null,
    founder: null,
    members: [
      { pubkey: 'pk_alice', roles: [] },
      { pubkey: 'pk_bob', roles: [] },
    ],
    bans: [],
    channels: [],
    categories: [],
    roles: [{ role_id: 'r1', name: 'Mods', color: 0xff0000, position: 1, permissions: 0 }],
    invites: [],
    my_permissions: 0,
  } satisfies GroupStateJson;
  useGroups.setState({ states: { g1: state } });
  useFriends.setState({
    contacts: [{ pubkey: 'pk_alice', display_name: 'Alice' }] as unknown as Contact[],
  });
}

/** Édite la valeur du champ en plaçant le curseur en fin de texte. */
function typeInput(value: string): void {
  fireEvent.change(screen.getByRole('textbox'), {
    target: { value, selectionStart: value.length },
  });
}

describe('MessageInput — autocomplétion de mentions', () => {
  it('ouvre la liste avec membres, rôles et @everyone/@here en salon', () => {
    setupGroup();
    render(<MessageInput placeholder="p" onSend={vi.fn()} groupId="g1" />);

    typeInput('@');

    expect(screen.getByRole('listbox')).toBeInTheDocument();
    expect(screen.getByRole('option', { name: /@everyone/ })).toBeInTheDocument();
    expect(screen.getByRole('option', { name: /@here/ })).toBeInTheDocument();
    expect(screen.getByRole('option', { name: /Mods/ })).toBeInTheDocument();
    expect(screen.getByRole('option', { name: /Alice/ })).toBeInTheDocument();
  });

  it('filtre les suggestions au fil de la saisie', () => {
    setupGroup();
    render(<MessageInput placeholder="p" onSend={vi.fn()} groupId="g1" />);

    typeInput('@al');

    expect(screen.getByRole('option', { name: /Alice/ })).toBeInTheDocument();
    expect(screen.queryByRole('option', { name: /@everyone/ })).not.toBeInTheDocument();
  });

  it('insère la suggestion à Entrée sans envoyer le message', () => {
    setupGroup();
    const onSend = vi.fn();
    render(<MessageInput placeholder="p" onSend={onSend} groupId="g1" />);

    typeInput('@al');
    fireEvent.keyDown(screen.getByRole('textbox'), { key: 'Enter' });

    expect(screen.getByRole('textbox')).toHaveValue('@Alice ');
    expect(onSend).not.toHaveBeenCalled();
    expect(screen.queryByRole('listbox')).not.toBeInTheDocument();
  });

  it('navigue au clavier puis insère avec Tab', () => {
    setupGroup();
    render(<MessageInput placeholder="p" onSend={vi.fn()} groupId="g1" />);

    typeInput('@');
    const box = screen.getByRole('textbox');
    fireEvent.keyDown(box, { key: 'ArrowDown' }); // everyone -> here
    fireEvent.keyDown(box, { key: 'Tab' });

    expect(box).toHaveValue('@here ');
  });

  it('n’ouvre aucune liste en message privé (aucun candidat)', () => {
    render(<MessageInput placeholder="p" onSend={vi.fn()} />);

    typeInput('@');

    expect(screen.queryByRole('listbox')).not.toBeInTheDocument();
  });
});

/** Profil local minimal pour les scénarios de composeur en salon. */
const SELF = {
  node_id: 'n',
  pubkey: 'moi',
  friend_code: 'accord-moi',
  name: 'Moi',
  bio: null,
  avatar: null,
  banner: null,
  pronouns: null,
  accent_color: null,
  banner_color: null,
  avatar_decoration: null,
  profile_effect: null,
};

const GROUP_TARGET = { kind: 'group', groupId: 'g1', channelId: 'c1' } as const;

/** Installe un groupe d'un seul salon `c1` avec le membre local `moi`. */
function seedComposer(over: Partial<GroupStateJson>, channelKind: 'text' | 'announcement'): void {
  useSession.setState({ self: SELF });
  const state = {
    group_id: 'g1',
    name: 'G',
    icon: null,
    founder: null,
    members: [{ pubkey: 'moi', roles: [] }],
    bans: [],
    channels: [
      { channel_id: 'c1', name: 'salon', kind: channelKind, category: null, position: 0, topic: '' },
    ],
    categories: [],
    roles: [],
    invites: [],
    my_permissions: 0x3,
    ...over,
  } satisfies GroupStateJson;
  useGroups.setState({ states: { g1: state } });
}

describe('MessageInput — composeur en lecture seule', () => {
  it('désactive le composeur quand l’utilisateur local est en sourdine', () => {
    seedComposer(
      { members: [{ pubkey: 'moi', roles: [], timeout_until_ms: Date.now() + 60_000 }] },
      'text',
    );
    render(
      <MessageInput
        placeholder="Écrire dans #salon"
        onSend={vi.fn()}
        groupId="g1"
        typingTarget={GROUP_TARGET}
      />,
    );

    expect(screen.getByRole('status').textContent).toMatch(/sourdine/);
    expect(screen.queryByRole('textbox')).not.toBeInTheDocument();
  });

  it('passe un salon d’annonces en lecture seule sans MANAGE_CHANNELS', () => {
    seedComposer({ my_permissions: 0x3 }, 'announcement');
    render(
      <MessageInput
        placeholder="Écrire dans #salon"
        onSend={vi.fn()}
        groupId="g1"
        typingTarget={GROUP_TARGET}
      />,
    );

    expect(screen.getByRole('status').textContent).toMatch(/gestionnaires/);
    expect(screen.queryByRole('textbox')).not.toBeInTheDocument();
  });

  it('laisse écrire dans un salon d’annonces avec MANAGE_CHANNELS', () => {
    seedComposer({ my_permissions: 0x3 | 0x8 }, 'announcement');
    render(
      <MessageInput
        placeholder="Écrire dans #salon"
        onSend={vi.fn()}
        groupId="g1"
        typingTarget={GROUP_TARGET}
      />,
    );

    expect(screen.getByRole('textbox')).toBeInTheDocument();
    expect(screen.queryByRole('status')).not.toBeInTheDocument();
  });
});

describe('MessageInput — message vocal', () => {
  it('remplace le composeur par la rangée d’enregistrement au clic sur le micro', () => {
    renderInput();

    fireEvent.click(screen.getByLabelText('Enregistrer un message vocal'));

    expect(dernierEnregistreur().started).toBe(true);
    // Le composeur texte disparaît : rangée dédiée [annuler | compteur | envoyer].
    expect(screen.queryByRole('textbox')).not.toBeInTheDocument();
    expect(screen.getByRole('status')).toHaveTextContent('0:00');
    expect(screen.getByLabelText('Annuler l’enregistrement')).toBeInTheDocument();
    expect(screen.getByLabelText('Envoyer le message vocal')).toBeInTheDocument();
    expect(screen.queryByLabelText('Enregistrer un message vocal')).not.toBeInTheDocument();
    expect(screen.queryByLabelText('Joindre des fichiers', { selector: 'button' })).not.toBeInTheDocument();
  });

  it('la pastille ne pulse qu’au vrai démarrage de la capture (onStart)', () => {
    const { container } = render(
      <MessageInput placeholder="p" onSend={vi.fn(async () => {})} />,
    );
    fireEvent.click(screen.getByLabelText('Enregistrer un message vocal'));

    // Micro pas encore obtenu (getUserMedia en cours) : pas de pulsation.
    expect(container.querySelector('.animate-ping')).toBeNull();

    act(() => dernierEnregistreur().callbacks.onStart?.());

    expect(container.querySelector('.animate-ping')).not.toBeNull();
  });

  it('affiche le temps écoulé au fil des rappels onTick', () => {
    renderInput();
    fireEvent.click(screen.getByLabelText('Enregistrer un message vocal'));

    act(() => dernierEnregistreur().callbacks.onTick(4200));

    expect(screen.getByRole('status')).toHaveTextContent('0:04');
  });

  it('annule au clic sur la corbeille : micro relâché, composeur normal restauré', () => {
    renderInput();
    fireEvent.click(screen.getByLabelText('Enregistrer un message vocal'));
    const enregistreur = dernierEnregistreur();

    fireEvent.click(screen.getByLabelText('Annuler l’enregistrement'));

    expect(enregistreur.canceled).toBe(true);
    expect(screen.getByLabelText('Enregistrer un message vocal')).toBeInTheDocument();
    expect(screen.getByRole('textbox')).toBeEnabled();
    expect(shareMock).not.toHaveBeenCalled();
  });

  it('le clic sur la coche appelle stop() (le vrai envoi part de onStop)', () => {
    renderInput();
    fireEvent.click(screen.getByLabelText('Enregistrer un message vocal'));
    const enregistreur = dernierEnregistreur();

    fireEvent.click(screen.getByLabelText('Envoyer le message vocal'));

    expect(enregistreur.stopped).toBe(true);
  });

  it('publie le blob fini (durée dans le nom) puis envoie un message avec la pièce audio', async () => {
    const piece = {
      merkle_root: 'ab'.repeat(32),
      name: 'voice-3s.webm',
      size: 42,
      mime: 'audio/webm;codecs=opus',
    };
    shareMock.mockResolvedValueOnce({ file: piece });
    const onSend = renderInput();
    fireEvent.click(screen.getByLabelText('Enregistrer un message vocal'));
    const enregistreur = dernierEnregistreur();
    const result: VoiceRecorderResult = {
      blob: new Blob(['son'], { type: 'audio/webm;codecs=opus' }),
      mime: 'audio/webm;codecs=opus',
      durationMs: 3000,
      reason: 'manual',
    };

    await act(async () => enregistreur.callbacks.onStop(result));

    await waitFor(() => expect(onSend).toHaveBeenCalledWith('', [piece]));
    // La durée réelle (3 s) est embarquée dans le nom de la pièce.
    expect(shareMock).toHaveBeenCalledWith(
      'voice-3s.webm',
      'audio/webm;codecs=opus',
      expect.any(String),
    );
    // Le composeur redevient normal, sans repasser par du texte.
    expect(screen.getByLabelText('Enregistrer un message vocal')).toBeInTheDocument();
  });

  it('signale une limite atteinte (info) quand l’arrêt n’est pas volontaire', async () => {
    shareMock.mockResolvedValueOnce({
      file: {
        merkle_root: 'cd'.repeat(32),
        name: 'voice-message.webm',
        size: 10,
        mime: 'audio/webm',
      },
    });
    renderInput();
    fireEvent.click(screen.getByLabelText('Enregistrer un message vocal'));
    const enregistreur = dernierEnregistreur();

    await act(async () =>
      enregistreur.callbacks.onStop({
        blob: new Blob(['son']),
        mime: 'audio/webm',
        durationMs: 120_000,
        reason: 'max_duration',
      }),
    );

    await waitFor(() =>
      expect(
        useUi.getState().toasts.some((toast) => toast.kind === 'info'),
      ).toBe(true),
    );
  });

  it('permission refusée : toast d’erreur, composeur revient à l’état normal sans planter', () => {
    renderInput();
    fireEvent.click(screen.getByLabelText('Enregistrer un message vocal'));
    const enregistreur = dernierEnregistreur();

    const error: VoiceRecorderError = 'permission_denied';
    act(() => enregistreur.callbacks.onError(error));

    expect(
      useUi
        .getState()
        .toasts.some((toast) => toast.kind === 'error' && /Micro/.test(toast.text)),
    ).toBe(true);
    expect(screen.getByLabelText('Enregistrer un message vocal')).toBeInTheDocument();
    expect(screen.getByRole('textbox')).toBeEnabled();
  });

  it('relâche tout enregistreur en cours au démontage (pas de micro fantôme)', () => {
    const { unmount } = render(
      <MessageInput placeholder="Écrire à @Alice" onSend={vi.fn(async () => {})} />,
    );
    fireEvent.click(screen.getByLabelText('Enregistrer un message vocal'));
    const enregistreur = dernierEnregistreur();

    unmount();

    expect(enregistreur.canceled).toBe(true);
  });
});

describe('MessageInput — bouton « + » (pièces jointes et sondage)', () => {
  it('en message privé, le « + » ouvre le sélecteur de fichiers (aucun menu)', () => {
    const clickSpy = vi.spyOn(HTMLInputElement.prototype, 'click');
    renderInput();

    fireEvent.click(screen.getByLabelText('Joindre des fichiers', { selector: 'button' }));

    expect(clickSpy).toHaveBeenCalled();
    expect(useContextMenu.getState().menu).toBeNull();
    clickSpy.mockRestore();
  });

  it('aucune option de sondage en message privé (D-048)', () => {
    renderInput();

    fireEvent.click(screen.getByLabelText('Joindre des fichiers', { selector: 'button' }));

    // MP : aucune modale de sondage possible, le menu n'est jamais déplié.
    expect(useContextMenu.getState().menu).toBeNull();
    expect(screen.queryByLabelText('Créer un sondage')).not.toBeInTheDocument();
  });

  it('en salon de groupe, le « + » déplie un menu (joindre puis sonder)', () => {
    seedComposer({}, 'text');
    render(
      <MessageInput
        placeholder="Écrire dans #salon"
        onSend={vi.fn()}
        groupId="g1"
        typingTarget={GROUP_TARGET}
      />,
    );

    fireEvent.click(screen.getByLabelText('Joindre des fichiers', { selector: 'button' }));

    const menu = useContextMenu.getState().menu;
    expect(menu).not.toBeNull();
    expect(menu?.items.map((item) => item.label)).toEqual([
      'Joindre des fichiers',
      'Créer un sondage',
    ]);
  });

  it('l’entrée « joindre » du menu « + » ouvre le sélecteur de fichiers', () => {
    const clickSpy = vi.spyOn(HTMLInputElement.prototype, 'click');
    seedComposer({}, 'text');
    render(
      <MessageInput
        placeholder="Écrire dans #salon"
        onSend={vi.fn()}
        groupId="g1"
        typingTarget={GROUP_TARGET}
      />,
    );

    fireEvent.click(screen.getByLabelText('Joindre des fichiers', { selector: 'button' }));
    const joindre = useContextMenu
      .getState()
      .menu?.items.find((item) => item.label === 'Joindre des fichiers');
    joindre?.onClick();

    expect(clickSpy).toHaveBeenCalled();
    clickSpy.mockRestore();
  });

  it('l’entrée « sondage » du menu « + » ouvre la modale de création du salon', () => {
    seedComposer({}, 'text');
    render(
      <MessageInput
        placeholder="Écrire dans #salon"
        onSend={vi.fn()}
        groupId="g1"
        typingTarget={GROUP_TARGET}
      />,
    );

    fireEvent.click(screen.getByLabelText('Joindre des fichiers', { selector: 'button' }));
    const sondage = useContextMenu
      .getState()
      .menu?.items.find((item) => item.label === 'Créer un sondage');
    sondage?.onClick();

    expect(useUi.getState().modal).toEqual({
      kind: 'createPoll',
      groupId: 'g1',
      channelId: 'c1',
    });
  });
});

/** Message DM texte minimal, pour peupler `useDms` dans les tests ArrowUp. */
function dmTextMessage(over: Partial<DmMessage>): DmMessage {
  return {
    msg_id: 'm',
    author: 'pk_alice',
    lamport: 1,
    sent_ms: 1000,
    acked: true,
    deleted: false,
    body: { type: 'text', text: 'x', reply_to: null, attachments: 0 },
    edited: null,
    ...over,
  };
}

const DM_TARGET = { kind: 'dm', peer: 'pk_alice' } as const;

describe('MessageInput — ArrowUp édite le dernier message propre (composeur vide)', () => {
  it('demande l’édition du dernier message texte encore éditable (ignore le supprimé)', () => {
    useSession.setState({ self: SELF });
    useDms.setState({
      conversations: {
        pk_alice: [
          dmTextMessage({ msg_id: 'm1', author: 'pk_alice', lamport: 1 }),
          dmTextMessage({ msg_id: 'm2', author: 'moi', lamport: 2, body: { type: 'text', text: 'coucou', reply_to: null, attachments: 0 } }),
          dmTextMessage({ msg_id: 'm3', author: 'moi', lamport: 3, deleted: true }),
        ],
      },
    });
    render(<MessageInput placeholder="p" onSend={vi.fn()} typingTarget={DM_TARGET} />);

    fireEvent.keyDown(screen.getByRole('textbox'), { key: 'ArrowUp' });

    expect(useMessageEdit.getState().request?.msgId).toBe('m2');
  });

  it('trouve le dernier message propre dans un salon de groupe', () => {
    useSession.setState({ self: SELF });
    seedComposer({}, 'text');
    const messages: GroupMessage[] = [
      {
        msg_id: 'g1',
        channel_id: 'c1',
        author: 'moi',
        lamport: 1,
        sent_ms: 1000,
        deleted: false,
        body: { type: 'text', text: 'salut le salon', reply_to: null, attachments: 0 },
        edited: null,
      },
    ];
    useGroups.setState((s) => ({
      messages: { ...s.messages, [channelKey('g1', 'c1')]: messages },
    }));
    render(
      <MessageInput
        placeholder="p"
        onSend={vi.fn()}
        groupId="g1"
        typingTarget={GROUP_TARGET}
      />,
    );

    fireEvent.keyDown(screen.getByRole('textbox'), { key: 'ArrowUp' });

    expect(useMessageEdit.getState().request?.msgId).toBe('g1');
  });

  it('ne déclenche rien quand le composeur contient déjà du texte', () => {
    useSession.setState({ self: SELF });
    useDms.setState({
      conversations: { pk_alice: [dmTextMessage({ msg_id: 'm1', author: 'moi' })] },
    });
    render(<MessageInput placeholder="p" onSend={vi.fn()} typingTarget={DM_TARGET} />);
    typeInput('en cours de saisie');

    fireEvent.keyDown(screen.getByRole('textbox'), { key: 'ArrowUp' });

    expect(useMessageEdit.getState().request).toBeNull();
  });

  it('no-op silencieux quand aucun message propre n’est éditable', () => {
    useSession.setState({ self: SELF });
    useDms.setState({
      conversations: { pk_alice: [dmTextMessage({ msg_id: 'm1', author: 'pk_alice' })] },
    });
    render(<MessageInput placeholder="p" onSend={vi.fn()} typingTarget={DM_TARGET} />);

    fireEvent.keyDown(screen.getByRole('textbox'), { key: 'ArrowUp' });

    expect(useMessageEdit.getState().request).toBeNull();
  });

  it('laisse l’autocomplétion de mentions capter Haut plutôt que de déclencher une édition', () => {
    setupGroup();
    useSession.setState({ self: SELF });
    render(<MessageInput placeholder="p" onSend={vi.fn()} groupId="g1" />);
    typeInput('@');
    expect(screen.getByRole('listbox')).toBeInTheDocument();

    fireEvent.keyDown(screen.getByRole('textbox'), { key: 'ArrowUp' });

    expect(useMessageEdit.getState().request).toBeNull();
  });
});

describe('MessageInput — brouillons de composeur persistés', () => {
  it('conserve le texte par cible et le restaure au retour', () => {
    const { rerender } = render(
      <MessageInput placeholder="p" onSend={vi.fn(async () => {})} typingTarget={DM_TARGET} />,
    );
    typeInput('brouillon pour Alice');
    expect(screen.getByRole('textbox')).toHaveValue('brouillon pour Alice');

    // Changement de conversation : le composeur se vide pour la nouvelle cible.
    rerender(
      <MessageInput
        placeholder="p"
        onSend={vi.fn(async () => {})}
        groupId="g1"
        typingTarget={GROUP_TARGET}
      />,
    );
    expect(screen.getByRole('textbox')).toHaveValue('');

    // Retour au MP : le brouillon d’Alice est restauré tel quel.
    rerender(
      <MessageInput placeholder="p" onSend={vi.fn(async () => {})} typingTarget={DM_TARGET} />,
    );
    expect(screen.getByRole('textbox')).toHaveValue('brouillon pour Alice');
  });

  it('restaure un brouillon déjà stocké au montage (après redémarrage)', () => {
    window.localStorage.setItem('draft:dm:pk_alice', 'texte survivant');
    render(
      <MessageInput placeholder="p" onSend={vi.fn(async () => {})} typingTarget={DM_TARGET} />,
    );
    expect(screen.getByRole('textbox')).toHaveValue('texte survivant');
  });

  it('efface le brouillon après un envoi réussi', async () => {
    const onSend = vi.fn(async () => {});
    render(<MessageInput placeholder="p" onSend={onSend} typingTarget={DM_TARGET} />);
    typeInput('à envoyer');
    expect(window.localStorage.getItem('draft:dm:pk_alice')).toBe('à envoyer');

    fireEvent.click(screen.getByLabelText('Envoyer'));

    await waitFor(() => expect(onSend).toHaveBeenCalledWith('à envoyer', undefined));
    await waitFor(() =>
      expect(window.localStorage.getItem('draft:dm:pk_alice')).toBeNull(),
    );
  });
});

describe('MessageInput — commandes slash à l’envoi', () => {
  it('transforme /shrug juste avant l’envoi (Entrée)', async () => {
    const onSend = renderInput();

    fireEvent.change(screen.getByRole('textbox'), { target: { value: '/shrug osef' } });
    fireEvent.keyDown(screen.getByRole('textbox'), { key: 'Enter' });

    await waitFor(() =>
      expect(onSend).toHaveBeenCalledWith('osef ¯\\_(ツ)_/¯', undefined),
    );
  });

  it('transforme /me au clic sur Envoyer', async () => {
    const onSend = renderInput();

    fireEvent.change(screen.getByRole('textbox'), { target: { value: '/me observe' } });
    fireEvent.click(screen.getByLabelText('Envoyer'));

    await waitFor(() => expect(onSend).toHaveBeenCalledWith('*observe*', undefined));
  });

  it('n’altère pas un message normal ni une commande inconnue', async () => {
    const onSend = renderInput();

    fireEvent.change(screen.getByRole('textbox'), { target: { value: '/foo bar' } });
    fireEvent.click(screen.getByLabelText('Envoyer'));

    await waitFor(() => expect(onSend).toHaveBeenCalledWith('/foo bar', undefined));
  });
});

describe('MessageInput — focus demandé par le parent (focusKey)', () => {
  it('donne le focus au champ quand focusKey passe à un msg_id (clic « Répondre »)', async () => {
    const { rerender } = render(
      <MessageInput placeholder="p" onSend={vi.fn()} focusKey={null} />,
    );
    const champ = screen.getByPlaceholderText('p');
    expect(champ).not.toHaveFocus();

    rerender(<MessageInput placeholder="p" onSend={vi.fn()} focusKey="m1" />);
    await waitFor(() => expect(champ).toHaveFocus());
  });

  it('refait le focus quand on répond à un autre message (la clé change)', async () => {
    const { rerender } = render(
      <MessageInput placeholder="p" onSend={vi.fn()} focusKey="m1" />,
    );
    const champ = screen.getByPlaceholderText('p');
    await waitFor(() => expect(champ).toHaveFocus());

    (champ as HTMLTextAreaElement).blur();
    expect(champ).not.toHaveFocus();

    rerender(<MessageInput placeholder="p" onSend={vi.fn()} focusKey="m2" />);
    await waitFor(() => expect(champ).toHaveFocus());
  });
});

describe('MessageInput — avertissement AutoMod émetteur', () => {
  it('affiche l’avertissement quand le texte contient un mot filtré, sans bloquer l’envoi', async () => {
    const onSend = vi.fn(async () => {});
    render(<MessageInput placeholder="p" onSend={onSend} automodWords={['zut']} />);

    const champ = screen.getByRole('textbox');
    fireEvent.change(champ, { target: { value: 'oh zut alors' } });

    expect(screen.getByRole('status').textContent).toMatch(/mot filtré/);

    fireEvent.keyDown(champ, { key: 'Enter' });
    await waitFor(() => expect(onSend).toHaveBeenCalledWith('oh zut alors', undefined));
  });

  it('aucun avertissement pour un mot filtré au milieu d’un autre mot ou sans liste', () => {
    render(<MessageInput placeholder="p" onSend={vi.fn()} automodWords={['chat']} />);

    fireEvent.change(screen.getByRole('textbox'), { target: { value: 'le chaton dort' } });
    expect(screen.queryByRole('status')).not.toBeInTheDocument();

    fireEvent.change(screen.getByRole('textbox'), { target: { value: 'sans souci' } });
    expect(screen.queryByRole('status')).not.toBeInTheDocument();
  });
});

describe('MessageInput — compte à rebours du mode lent', () => {
  it('affiche le décompte et désactive l’envoi tant que l’échéance court', () => {
    vi.useFakeTimers();
    try {
      const onSend = vi.fn(async () => {});
      render(
        <MessageInput
          placeholder="p"
          onSend={onSend}
          slowmodeUntilMs={Date.now() + 3000}
        />,
      );

      expect(screen.getByRole('status').textContent).toBe('3s');

      const champ = screen.getByRole('textbox');
      fireEvent.change(champ, { target: { value: 'bonjour' } });
      expect(screen.getByRole('button', { name: 'Envoyer' })).toBeDisabled();

      // Entrée n'envoie pas non plus pendant le mode lent.
      fireEvent.keyDown(champ, { key: 'Enter' });
      expect(onSend).not.toHaveBeenCalled();
    } finally {
      vi.useRealTimers();
    }
  });

  it('décompte chaque seconde puis réactive l’envoi à expiration', () => {
    vi.useFakeTimers();
    try {
      render(
        <MessageInput
          placeholder="p"
          onSend={vi.fn()}
          slowmodeUntilMs={Date.now() + 2000}
        />,
      );

      expect(screen.getByRole('status').textContent).toBe('2s');

      act(() => {
        vi.advanceTimersByTime(1000);
      });
      expect(screen.getByRole('status').textContent).toBe('1s');

      act(() => {
        vi.advanceTimersByTime(1000);
      });
      expect(screen.queryByRole('status')).not.toBeInTheDocument();

      fireEvent.change(screen.getByRole('textbox'), { target: { value: 'bonjour' } });
      expect(screen.getByRole('button', { name: 'Envoyer' })).toBeEnabled();
    } finally {
      vi.useRealTimers();
    }
  });

  it('sans échéance (null), aucun indicateur et envoi permis', () => {
    render(<MessageInput placeholder="p" onSend={vi.fn()} slowmodeUntilMs={null} />);

    expect(screen.queryByRole('status')).not.toBeInTheDocument();
    fireEvent.change(screen.getByRole('textbox'), { target: { value: 'bonjour' } });
    expect(screen.getByRole('button', { name: 'Envoyer' })).toBeEnabled();
  });
});
