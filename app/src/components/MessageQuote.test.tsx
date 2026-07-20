/**
 * Tests de l'aperçu de citation : nom + extrait, replis (supprimé /
 * introuvable) et saut au message d'origine quand `onJump` est fourni.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useUi } from '../stores/ui';
import type { DisplayMessage } from './messageModel';
import { MessageQuote } from './MessageQuote';

function msg(text: string, deleted = false): DisplayMessage {
  return {
    msg_id: 'm',
    author: 'alice',
    sent_ms: 0,
    edited: null,
    deleted,
    body: { type: 'text', text, reply_to: null, attachments: 0 },
  } as DisplayMessage;
}

const nameOf = (a: string) => (a === 'alice' ? 'Alice' : a);

beforeEach(() => {
  useUi.setState({ lang: 'fr' });
});

describe('MessageQuote', () => {
  it('affiche le nom de l’auteur et l’extrait cité', () => {
    render(<MessageQuote quoted={msg('salut à toi')} nameOf={nameOf} />);
    expect(screen.getByText('Alice')).toBeInTheDocument();
    expect(screen.getByText('salut à toi')).toBeInTheDocument();
  });

  it('replie sur « indisponible » quand le message cité est introuvable', () => {
    render(<MessageQuote quoted={undefined} nameOf={nameOf} />);
    expect(screen.getByText('Message d’origine indisponible')).toBeInTheDocument();
  });

  it('replie sur « supprimé » quand le message cité l’est', () => {
    render(<MessageQuote quoted={msg('x', true)} nameOf={nameOf} />);
    expect(screen.getByText('Message supprimé')).toBeInTheDocument();
  });

  it('est un bouton qui saute au message quand onJump est fourni', async () => {
    const onJump = vi.fn();
    render(<MessageQuote quoted={msg('salut')} nameOf={nameOf} onJump={onJump} />);
    await userEvent.click(screen.getByRole('button'));
    expect(onJump).toHaveBeenCalledOnce();
  });

  it('n’est pas cliquable sans onJump', () => {
    render(<MessageQuote quoted={msg('salut')} nameOf={nameOf} />);
    expect(screen.queryByRole('button')).toBeNull();
  });
});
