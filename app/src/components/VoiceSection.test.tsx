/**
 * Tests de la section « Salons vocaux » : entrée du salon par défaut, jonction
 * au clic (convention channel_id == group_id), rendu des participants (anneau
 * vert en parole, badges micro/son coupé) et curseur de volume par
 * participant distant.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';

vi.mock('../lib/client', () => {
  const handlers = new Set<(method: string, params: unknown) => void>();
  return {
    api: {},
    rpc: {
      // Le store de session s'abonne au statut de connexion à l'import.
      onStatus: () => () => {},
      onEvent: (handler: (method: string, params: unknown) => void) => {
        handlers.add(handler);
        return () => handlers.delete(handler);
      },
      /** Simule une notification poussée par le nœud (tests uniquement). */
      emitEvent: (method: string, params: unknown) => {
        for (const handler of handlers) handler(method, params);
      },
    },
  };
});

import { rpc } from '../lib/client';
import type { Contact, SelfProfile } from '../lib/api';
import { useFriends } from '../stores/friends';
import { useSession } from '../stores/session';
import { useUi } from '../stores/ui';
import { useVoice, type ParticipantState } from '../stores/voice';
import { VoiceSection } from './VoiceSection';

const fakeRpc = rpc as unknown as {
  emitEvent: (method: string, params: unknown) => void;
};

const self: SelfProfile = {
  node_id: 'n-moi',
  pubkey: 'moi',
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

const alice: Contact = {
  node_id: 'n-alice',
  pubkey: 'alice',
  friend_code: 'accord-alice',
  display_name: 'Alice',
  bio: null,
  avatar: null,
  banner: null,
  state: 'friend',
  last_seen_ms: 0,
};

/** Participant sans état particulier (volume neutre, rien de coupé). */
function idle(overrides: Partial<ParticipantState> = {}): ParticipantState {
  return {
    speaking: false,
    muted: false,
    deafened: false,
    volume: 100,
    serverMuted: false,
    serverDeafened: false,
    prioritySpeaker: false,
    ...overrides,
  };
}

/** Connecte le salon vocal du groupe donné avec ces participants. */
function seedVoice(
  groupId: string,
  participants: Array<[string, Partial<ParticipantState>]>,
): void {
  useVoice.setState({
    active: { groupId, channelId: groupId, muted: false, isCall: false },
    participants: new Map(participants.map(([pk, state]) => [pk, idle(state)])),
  });
}

beforeEach(() => {
  useUi.setState({ lang: 'fr', toasts: [] });
  useSession.setState({ self, phase: 'ready' });
  useFriends.setState({ contacts: [alice] });
  useVoice.setState({
    active: null,
    selfDeafened: false,
    masterVolume: 100,
    participants: new Map(),
    // La section resynchronise à la connexion : neutralisé dans ces tests
    // (l'état est semé à la main), testé séparément.
    sync: vi.fn(async () => {}),
  });
});

describe('VoiceSection', () => {
  it('affiche la section et l’entrée du salon vocal par défaut', () => {
    render(<VoiceSection groupId="g1" />);

    expect(screen.getByText('Salons vocaux')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Salon vocal' })).toBeInTheDocument();
  });

  it('rejoint au clic avec channel_id == group_id (convention UI)', () => {
    const join = vi.fn(async (_groupId: string, _channelId: string) => {});
    useVoice.setState({ join });
    render(<VoiceSection groupId="g1" />);

    fireEvent.click(screen.getByRole('button', { name: 'Salon vocal' }));

    expect(join).toHaveBeenCalledWith('g1', 'g1');
  });

  it('ne rejoint pas de nouveau quand on est déjà dans ce salon', () => {
    const join = vi.fn(async (_groupId: string, _channelId: string) => {});
    seedVoice('g1', [['moi', {}]]);
    useVoice.setState({ join });
    render(<VoiceSection groupId="g1" />);

    fireEvent.click(screen.getByRole('button', { name: 'Salon vocal' }));

    expect(join).not.toHaveBeenCalled();
  });

  it('signale l’échec de jonction par un toast d’erreur', async () => {
    useVoice.setState({
      join: vi.fn(async () => {
        throw new Error('salon plein');
      }),
    });
    render(<VoiceSection groupId="g1" />);

    fireEvent.click(screen.getByRole('button', { name: 'Salon vocal' }));

    await waitFor(() => {
      expect(useUi.getState().toasts).toHaveLength(1);
    });
    expect(useUi.getState().toasts[0]?.kind).toBe('error');
  });

  it('liste les participants connectés (pseudo du contact, code ami pour soi)', () => {
    seedVoice('g1', [
      ['moi', {}],
      ['alice', {}],
    ]);
    render(<VoiceSection groupId="g1" />);

    expect(screen.getByText('accord-moi')).toBeInTheDocument();
    expect(screen.getByText('Alice')).toBeInTheDocument();
  });

  it('entoure d’un anneau vert l’avatar de la personne qui parle', () => {
    seedVoice('g1', [
      ['moi', {}],
      ['alice', { speaking: true }],
    ]);
    render(<VoiceSection groupId="g1" />);

    const aliceRow = screen.getByText('Alice').closest('li');
    const selfRow = screen.getByText('accord-moi').closest('li');
    expect(aliceRow?.querySelector('.ring-green')).not.toBeNull();
    expect(selfRow?.querySelector('.ring-green')).toBeNull();
    // L'état de parole est aussi annoncé aux lecteurs d'écran.
    expect(screen.getByText('parle')).toBeInTheDocument();
  });

  it('n’affiche aucun participant quand on est connecté à un autre salon', () => {
    seedVoice('g2', [['moi', {}]]);
    render(<VoiceSection groupId="g1" />);

    expect(screen.queryByRole('list')).not.toBeInTheDocument();
    expect(screen.queryByText('accord-moi')).not.toBeInTheDocument();
  });

  it('resynchronise l’état vocal à la connexion au salon affiché', async () => {
    const sync = vi.fn(async () => {});
    seedVoice('g1', [['moi', {}]]);
    useVoice.setState({ sync });
    render(<VoiceSection groupId="g1" />);

    await waitFor(() => {
      expect(sync).toHaveBeenCalledTimes(1);
    });
  });

  it('affiche le badge micro coupé d’un participant muet', () => {
    seedVoice('g1', [
      ['moi', {}],
      ['alice', { muted: true }],
    ]);
    render(<VoiceSection groupId="g1" />);

    const badge = screen.getByRole('img', { name: 'Micro coupé' });
    expect(screen.getByText('Alice').closest('li')).toContainElement(badge);
    expect(screen.queryByRole('img', { name: 'Son coupé' })).not.toBeInTheDocument();
  });

  it('affiche les badges micro et son coupés d’un participant sourd', () => {
    seedVoice('g1', [['alice', { muted: true, deafened: true }]]);
    render(<VoiceSection groupId="g1" />);

    expect(screen.getByRole('img', { name: 'Micro coupé' })).toBeInTheDocument();
    expect(screen.getByRole('img', { name: 'Son coupé' })).toBeInTheDocument();
  });

  it('applique event.voice_mute au store (pair mis à jour en direct)', () => {
    seedVoice('g1', [['alice', {}]]);
    render(<VoiceSection groupId="g1" />);
    expect(screen.queryByRole('img', { name: 'Micro coupé' })).not.toBeInTheDocument();

    act(() => {
      fakeRpc.emitEvent('event.voice_mute', {
        pubkey: 'alice',
        muted: true,
        deafened: false,
      });
    });

    expect(screen.getByRole('img', { name: 'Micro coupé' })).toBeInTheDocument();

    // Charge utile malformée : ignorée sans erreur (frontière système).
    act(() => {
      fakeRpc.emitEvent('event.voice_mute', { pubkey: 42, muted: 'oui' });
    });
    expect(useVoice.getState().participants.get('alice')?.muted).toBe(true);
  });

  it('déplie un curseur de volume pour un participant distant', () => {
    const setVolume = vi.fn(async () => {});
    seedVoice('g1', [
      ['moi', {}],
      ['alice', { volume: 80 }],
    ]);
    useVoice.setState({ setVolume });
    render(<VoiceSection groupId="g1" />);

    // Replié par défaut ; le bouton porte l'état déplié.
    expect(screen.queryByRole('slider')).not.toBeInTheDocument();
    const toggle = screen.getByRole('button', { name: 'Régler le volume de Alice' });
    expect(toggle).toHaveAttribute('aria-expanded', 'false');

    fireEvent.click(toggle);

    const slider = screen.getByRole('slider', { name: 'Volume de Alice' });
    expect(slider).toHaveValue('80');
    expect(toggle).toHaveAttribute('aria-expanded', 'true');

    fireEvent.change(slider, { target: { value: '150' } });
    expect(setVolume).toHaveBeenCalledWith('alice', 150);
  });

  it('ne propose pas de curseur de volume pour soi-même', () => {
    seedVoice('g1', [['moi', {}]]);
    render(<VoiceSection groupId="g1" />);

    expect(
      screen.queryByRole('button', { name: /Régler le volume/ }),
    ).not.toBeInTheDocument();
  });

  it('signale l’échec du réglage de volume par un toast d’erreur', async () => {
    seedVoice('g1', [['alice', {}]]);
    useVoice.setState({
      setVolume: vi.fn(async () => {
        throw new Error('hors ligne');
      }),
    });
    render(<VoiceSection groupId="g1" />);

    fireEvent.click(screen.getByRole('button', { name: 'Régler le volume de Alice' }));
    fireEvent.change(screen.getByRole('slider', { name: 'Volume de Alice' }), {
      target: { value: '10' },
    });

    await waitFor(() => {
      expect(useUi.getState().toasts).toHaveLength(1);
    });
    expect(useUi.getState().toasts[0]?.kind).toBe('error');
  });
});
