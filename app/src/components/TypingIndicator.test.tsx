/**
 * Tests de la ligne d'indicateur de frappe : rien sans écrivain, libellés
 * 1 / 2 / 3+ écrivains avec pseudos résolus depuis les contacts, repli sur
 * l'identifiant court pour un pair inconnu.
 */

import { beforeEach, describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/react';
import type { Contact } from '../lib/api';
import { useFriends } from '../stores/friends';
import { useTyping, dmTypingKey, TYPING_EXPIRY_MS } from '../stores/typing';
import { useUi } from '../stores/ui';
import { TypingIndicator } from './TypingIndicator';

const KEY = dmTypingKey('alice-pk');

function contact(pubkey: string, displayName: string): Contact {
  return {
    node_id: 'noeud',
    pubkey,
    friend_code: 'accord-lion-foret-12345',
    display_name: displayName,
    bio: null,
    avatar: null,
    banner: null,
    state: 'friend',
    last_seen_ms: 0,
  };
}

/** Pose directement `count` écrivains (échéance lointaine) sur KEY. */
function setWriters(pubkeys: string[]): void {
  const deadline = Date.now() + TYPING_EXPIRY_MS;
  useTyping.setState({
    writers: { [KEY]: Object.fromEntries(pubkeys.map((p) => [p, deadline])) },
  });
}

beforeEach(() => {
  useUi.setState({ lang: 'fr' });
  useTyping.setState({ writers: {} });
  useFriends.setState({
    contacts: [contact('alice-pk', 'Alice'), contact('bob-pk', 'Bob')],
  });
});

describe('TypingIndicator', () => {
  it("n'affiche rien sans écrivain", () => {
    // Arrange / Act
    const { container } = render(<TypingIndicator typingKey={KEY} />);

    // Assert
    expect(screen.queryByRole('status')).not.toBeInTheDocument();
    expect(container.firstElementChild).toHaveClass('h-5', 'shrink-0');
    expect(container.firstElementChild).not.toHaveClass('-mt-5');
  });

  it('nomme le seul écrivain', () => {
    // Arrange
    setWriters(['alice-pk']);

    // Act
    const { container } = render(<TypingIndicator typingKey={KEY} />);

    // Assert
    expect(screen.getByRole('status')).toHaveTextContent('Alice est en train d’écrire…');
    expect(container.querySelectorAll('.typing-dot')).toHaveLength(3);
  });

  it('utilise le nom fourni par le contexte du serveur', () => {
    setWriters(['alice-pk']);

    render(
      <TypingIndicator
        typingKey={KEY}
        nameOf={(pubkey) => (pubkey === 'alice-pk' ? 'Alicia' : pubkey)}
      />,
    );

    expect(screen.getByRole('status')).toHaveTextContent('Alicia est en train d’écrire…');
  });

  it('nomme les deux écrivains', () => {
    // Arrange
    setWriters(['alice-pk', 'bob-pk']);

    // Act
    render(<TypingIndicator typingKey={KEY} />);

    // Assert
    expect(screen.getByRole('status')).toHaveTextContent(
      'Alice et Bob sont en train d’écrire…',
    );
  });

  it('reste générique à partir de trois écrivains', () => {
    // Arrange
    setWriters(['alice-pk', 'bob-pk', 'carol-pk']);

    // Act
    render(<TypingIndicator typingKey={KEY} />);

    // Assert
    expect(screen.getByRole('status')).toHaveTextContent(
      'Plusieurs personnes sont en train d’écrire…',
    );
  });

  it("replie sur l'identifiant court pour un pair inconnu", () => {
    // Arrange
    setWriters(['zz-inconnu-pk']);

    // Act
    render(<TypingIndicator typingKey={KEY} />);

    // Assert
    expect(screen.getByRole('status')).toHaveTextContent('zz-inc est en train d’écrire…');
  });
});
