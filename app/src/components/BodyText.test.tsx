/**
 * Tests du corps de message : repli « supprimé », rendu du texte, marqueur
 * « modifié » et masquage AutoMod appliqué au rendu (modèle serverless).
 */

import { beforeEach, describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/react';
import { useUi } from '../stores/ui';
import type { DisplayMessage } from './messageModel';
import { BodyText } from './BodyText';

function texte(text: string): DisplayMessage['body'] {
  return { type: 'text', text, reply_to: null, attachments: 0 };
}

function msg(partial: Partial<DisplayMessage>): DisplayMessage {
  return {
    msg_id: 'm',
    author: 'a',
    sent_ms: 0,
    edited: null,
    deleted: false,
    body: texte('hello'),
    ...partial,
  } as DisplayMessage;
}

beforeEach(() => {
  useUi.setState({ lang: 'fr' });
});

describe('BodyText', () => {
  it('affiche un repli en italique pour un message supprimé', () => {
    render(<BodyText message={msg({ deleted: true })} />);
    expect(screen.getByText('Message supprimé')).toBeInTheDocument();
  });

  it('rend le texte d’un message et pas de marqueur « modifié »', () => {
    render(<BodyText message={msg({ body: texte('bonjour') })} />);
    expect(screen.getByText(/bonjour/)).toBeInTheDocument();
    expect(screen.queryByText('(modifié)')).toBeNull();
  });

  it('affiche le texte édité et le marqueur « modifié »', () => {
    render(<BodyText message={msg({ edited: 'corrigé', body: texte('original') })} />);
    expect(screen.getByText(/corrigé/)).toBeInTheDocument();
    expect(screen.getByText('(modifié)')).toBeInTheDocument();
  });

  it('masque un mot filtré par l’AutoMod au rendu', () => {
    render(
      <BodyText
        message={msg({ body: texte('coucou vilain') })}
        automodWords={['vilain']}
      />,
    );
    expect(screen.getByText(/coucou/)).toBeInTheDocument();
    expect(screen.queryByText(/vilain/)).toBeNull();
  });
});
