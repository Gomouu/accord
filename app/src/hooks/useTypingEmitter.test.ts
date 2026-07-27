/**
 * Tests de l'émetteur d'indicateur de frappe : émission à la première
 * frappe, throttle client (une émission par fenêtre de 2 s), rien pour un
 * texte vide, routage MP/salon, échec ignoré et réinitialisation du
 * throttle au changement de conversation.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Mock } from 'vitest';
import { renderHook } from '@testing-library/react';

vi.mock('../lib/client', () => ({
  api: {
    dmTyping: vi.fn(),
    groupsTyping: vi.fn(),
    // L'indicateur de frappe est une préférence SYNCHRONISÉE : l'écrire passe
    // par le miroir vers le nœud. Rien à vérifier ici, mais le mock doit
    // exister, sans quoi ce fichier échouerait sur un sujet qui n'est pas le
    // sien.
    setPref: vi.fn(() => Promise.resolve(0)),
  },
}));

import { api } from '../lib/client';
import { useUi } from '../stores/ui';
import {
  useTypingEmitter,
  TYPING_EMIT_INTERVAL_MS,
  type TypingTarget,
} from './useTypingEmitter';

const dmTypingMock = api.dmTyping as unknown as Mock;
const groupsTypingMock = api.groupsTyping as unknown as Mock;

beforeEach(() => {
  vi.useFakeTimers();
  dmTypingMock.mockReset();
  groupsTypingMock.mockReset();
  dmTypingMock.mockResolvedValue({ ok: true });
  groupsTypingMock.mockResolvedValue({ ok: true });
  useUi.getState().setTypingIndicatorEnabled(true);
});

afterEach(() => {
  vi.useRealTimers();
  useUi.getState().setTypingIndicatorEnabled(true);
});

describe('useTypingEmitter', () => {
  it('émet dm.typing à la première frappe pour un MP', () => {
    // Arrange
    const { result } = renderHook(() =>
      useTypingEmitter({ kind: 'dm', peer: 'alice-pk' }),
    );

    // Act
    result.current('b');

    // Assert
    expect(dmTypingMock).toHaveBeenCalledWith('alice-pk');
  });

  it('borne à une émission par fenêtre de 2 s', () => {
    // Arrange
    const { result } = renderHook(() =>
      useTypingEmitter({ kind: 'dm', peer: 'alice-pk' }),
    );

    // Act : frappes rapprochées, puis une après la fenêtre.
    result.current('b');
    result.current('bo');
    vi.advanceTimersByTime(TYPING_EMIT_INTERVAL_MS - 1);
    result.current('bon');
    expect(dmTypingMock).toHaveBeenCalledTimes(1);
    vi.advanceTimersByTime(1);
    result.current('bonj');

    // Assert
    expect(dmTypingMock).toHaveBeenCalledTimes(2);
  });

  it("n'émet rien pour un texte vide ou blanc", () => {
    // Arrange
    const { result } = renderHook(() =>
      useTypingEmitter({ kind: 'dm', peer: 'alice-pk' }),
    );

    // Act
    result.current('');
    result.current('   ');

    // Assert
    expect(dmTypingMock).not.toHaveBeenCalled();
  });

  it('n’émet rien quand l’indicateur de frappe est désactivé (réglage)', () => {
    // Arrange
    useUi.getState().setTypingIndicatorEnabled(false);
    const { result } = renderHook(() =>
      useTypingEmitter({ kind: 'dm', peer: 'alice-pk' }),
    );

    // Act
    result.current('bonjour');

    // Assert
    expect(dmTypingMock).not.toHaveBeenCalled();
  });

  it('reprend l’émission une fois l’indicateur de frappe réactivé', () => {
    // Arrange
    useUi.getState().setTypingIndicatorEnabled(false);
    const { result } = renderHook(() =>
      useTypingEmitter({ kind: 'dm', peer: 'alice-pk' }),
    );
    result.current('b');
    expect(dmTypingMock).not.toHaveBeenCalled();

    // Act
    useUi.getState().setTypingIndicatorEnabled(true);
    result.current('bo');

    // Assert
    expect(dmTypingMock).toHaveBeenCalledWith('alice-pk');
  });

  it("n'émet rien sans cible", () => {
    // Arrange
    const { result } = renderHook(() => useTypingEmitter(undefined));

    // Act
    result.current('bonjour');

    // Assert
    expect(dmTypingMock).not.toHaveBeenCalled();
    expect(groupsTypingMock).not.toHaveBeenCalled();
  });

  it('émet groups.typing pour une cible de salon', () => {
    // Arrange
    const { result } = renderHook(() =>
      useTypingEmitter({ kind: 'group', groupId: 'g1', channelId: 'c1' }),
    );

    // Act
    result.current('b');

    // Assert
    expect(groupsTypingMock).toHaveBeenCalledWith('g1', 'c1');
    expect(dmTypingMock).not.toHaveBeenCalled();
  });

  it("ignore silencieusement l'échec de l'API (best effort)", async () => {
    // Arrange
    dmTypingMock.mockRejectedValueOnce(new Error('pair hors ligne'));
    const { result } = renderHook(() =>
      useTypingEmitter({ kind: 'dm', peer: 'alice-pk' }),
    );

    // Act : l'échec est absorbé sans rejet non géré.
    result.current('b');
    await vi.runAllTimersAsync();

    // Assert
    expect(dmTypingMock).toHaveBeenCalledTimes(1);
  });

  it('repart de zéro quand la conversation change', () => {
    // Arrange
    const { result, rerender } = renderHook(
      ({ target }: { target: TypingTarget }) => useTypingEmitter(target),
      { initialProps: { target: { kind: 'dm', peer: 'alice-pk' } as TypingTarget } },
    );
    result.current('b');

    // Act : changement de pair immédiat, sans attendre la fenêtre.
    rerender({ target: { kind: 'dm', peer: 'bob-pk' } });
    result.current('c');

    // Assert
    expect(dmTypingMock).toHaveBeenCalledTimes(2);
    expect(dmTypingMock).toHaveBeenLastCalledWith('bob-pk');
  });
});
