/**
 * Tests de l'overlay d'appel entrant : n'apparaît qu'en sonnerie entrante,
 * Accepter/Refuser appellent les actions du store, et la sonnerie
 * (`lib/ringtone`, mockée ici) suit strictement la phase.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import type { Contact } from '../lib/api';
import { useCalls } from '../stores/calls';
import { useFriends } from '../stores/friends';
import { useUi } from '../stores/ui';
import { IncomingCall } from './IncomingCall';

vi.mock('../lib/ringtone', () => ({
  startRingtone: vi.fn(),
  stopRingtone: vi.fn(),
}));

import { startRingtone, stopRingtone } from '../lib/ringtone';

const startRingtoneMock = startRingtone as unknown as ReturnType<typeof vi.fn>;
const stopRingtoneMock = stopRingtone as unknown as ReturnType<typeof vi.fn>;

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

beforeEach(() => {
  useUi.setState({ lang: 'fr', toasts: [] });
  useFriends.setState({ contacts: [alice] });
  useCalls.setState({ phase: 'idle', peer: null, callId: null, sincePhaseMs: null });
  startRingtoneMock.mockReset();
  stopRingtoneMock.mockReset();
});

describe('IncomingCall — overlay', () => {
  it('reste absent hors sonnerie entrante', () => {
    render(<IncomingCall />);

    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    expect(startRingtoneMock).not.toHaveBeenCalled();
  });

  it('event.call_incoming (phase incoming_ringing) affiche l’overlay et sonne', () => {
    useCalls.setState({
      phase: 'incoming_ringing',
      peer: 'alice',
      callId: 'c1',
      sincePhaseMs: Date.now(),
    });

    render(<IncomingCall />);

    expect(screen.getByRole('dialog')).toBeInTheDocument();
    expect(screen.getByText('Alice')).toBeInTheDocument();
    expect(startRingtoneMock).toHaveBeenCalledTimes(1);
    expect(stopRingtoneMock).not.toHaveBeenCalled();
  });

  it('accepter appelle calls.accept', () => {
    const accept = vi.fn(async () => {});
    useCalls.setState({
      phase: 'incoming_ringing',
      peer: 'alice',
      callId: 'c1',
      sincePhaseMs: Date.now(),
      accept,
    });

    render(<IncomingCall />);
    fireEvent.click(screen.getByRole('button', { name: 'Accepter' }));

    expect(accept).toHaveBeenCalledTimes(1);
  });

  it('refuser appelle calls.decline', () => {
    const decline = vi.fn(async () => {});
    useCalls.setState({
      phase: 'incoming_ringing',
      peer: 'alice',
      callId: 'c1',
      sincePhaseMs: Date.now(),
      decline,
    });

    render(<IncomingCall />);
    fireEvent.click(screen.getByRole('button', { name: 'Refuser' }));

    expect(decline).toHaveBeenCalledTimes(1);
  });

  it('la sonnerie s’arrête dès que la phase quitte incoming_ringing', () => {
    useCalls.setState({
      phase: 'incoming_ringing',
      peer: 'alice',
      callId: 'c1',
      sincePhaseMs: Date.now(),
    });
    const { rerender } = render(<IncomingCall />);
    expect(startRingtoneMock).toHaveBeenCalledTimes(1);

    useCalls.setState({ phase: 'active', callId: 'c1', peer: 'alice' });
    rerender(<IncomingCall />);

    expect(stopRingtoneMock).toHaveBeenCalled();
  });
});
