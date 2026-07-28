/**
 * Tests de « Récupérer l'historique depuis cet appareil ».
 *
 * Deux promesses tenues ici, et rien d'autre :
 *
 * 1. l'écran n'attend PAS la réponse de `devices.transfer_history` — qui ne
 *    revient qu'à la fin, des minutes plus tard — pour montrer l'avancement ;
 * 2. un transfert qui se termine sans une seule page reçue **ne se raconte
 *    jamais comme un simple « terminé »** quand le carnet n'était pas vide :
 *    le nœud ne distingue pas « rien de plus ancien » d'« version d'en face
 *    trop ancienne pour répondre », et c'est ici que ça se dit.
 */

import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

type EventHandler = (method: string, params: unknown) => void;
const handlers = new Set<EventHandler>();
const devicesTransferHistory = vi.fn();

// `rpc` bouchonné aussi : l'avancement passe par lui, et un vrai client
// ouvrirait une WebSocket qu'aucun nœud n'écoute pendant les tests.
vi.mock('../../lib/client', () => ({
  rpc: {
    onEvent: (handler: EventHandler) => {
      handlers.add(handler);
      return () => handlers.delete(handler);
    },
  },
  api: {
    devicesTransferHistory: (device: string) => devicesTransferHistory(device),
  },
}));

import { HistoryTransferButton } from './HistoryTransferButton';
import { frSettings } from '../../i18n/fr.settings';
import { fr } from '../../i18n/fr';
import { useUi } from '../../stores/ui';

const L = frSettings.settings;
const PUBKEY = 'cd'.repeat(32);

/**
 * Simule un `event.history_transfer` du nœud. Sous `act` : l'événement arrive
 * hors de tout gestionnaire React, comme dans l'application.
 */
function pousser(done: number, total: number, messages: number, complete: boolean): void {
  act(() => {
    for (const handler of [...handlers]) {
      handler('event.history_transfer', { done, total, messages, complete });
    }
  });
}

/** Début d'un libellé à trou, sans son premier marqueur d'interpolation. */
function debut(libelle: string): string {
  return (libelle.split('{')[0] ?? '').trim();
}

function monter() {
  const onActif = vi.fn();
  render(<HistoryTransferButton pubkey={PUBKEY} bloque={false} onActif={onActif} />);
  return { onActif };
}

/**
 * Promesse que le test dénoue quand il veut : c'est ce qui permet de vérifier
 * l'affichage AVANT que `devices.transfer_history` ait rendu la main.
 */
function differer<T>(): { promise: Promise<T>; resolve: (v: T) => void } {
  let resolve!: (v: T) => void;
  const promise = new Promise<T>((r) => {
    resolve = r;
  });
  return { promise, resolve };
}

beforeEach(() => {
  handlers.clear();
  devicesTransferHistory.mockReset();
  useUi.setState({ lang: 'fr', toasts: [] });
});

describe('HistoryTransferButton', () => {
  it('n’attend pas la réponse de l’appel pour afficher la progression', async () => {
    // 🔴 La promesse reste EN VOL pendant toute l'assertion : c'est le point.
    // `devices.transfer_history` ne rend la main qu'à la fin du transfert, et
    // une interface qui l'attendrait resterait muette pendant des minutes.
    const appel = differer<{ conversations: number; pages: number }>();
    devicesTransferHistory.mockReturnValue(appel.promise);
    monter();

    fireEvent.click(screen.getByRole('button', { name: L.historyTransferAction }));
    pousser(2, 5, 7, false);

    const barre = await screen.findByRole('progressbar');
    expect(barre).toHaveAttribute('aria-valuenow', '2');
    expect(barre).toHaveAttribute('aria-valuemax', '5');
    expect(screen.getByText(/7/)).toHaveTextContent(debut(L.historyTransferRunning));
    // Rien n'est conclu tant que l'appel n'a pas rendu la main.
    expect(screen.queryByText(L.historyTransferAmbiguous)).not.toBeInTheDocument();
  });

  it('nomme les DEUX causes quand zéro page arrive sur un carnet non vide', async () => {
    // 🔴 Le cas qui vaut cet écran. Vu du nœud, « l'autre appareil n'a rien de
    // plus ancien » et « l'autre appareil ignore la demande, version trop
    // ancienne » sont le même transfert à zéro page. Annoncer « terminé »
    // ferait passer un pair dépassé pour un historique déjà complet.
    devicesTransferHistory.mockResolvedValue({ conversations: 4, pages: 0 });
    monter();

    fireEvent.click(screen.getByRole('button', { name: L.historyTransferAction }));

    expect(await screen.findByText(L.historyTransferAmbiguous)).toBeInTheDocument();
    expect(screen.queryByText(L.historyTransferEmptyBook)).not.toBeInTheDocument();
    expect(
      screen.queryByText(debut(L.historyTransferDone), { exact: false }),
    ).not.toBeInTheDocument();
  });

  it('ne fabrique aucun doute quand le carnet est vide', async () => {
    // Zéro page sans une seule conversation à parcourir n'a qu'une lecture :
    // il n'y avait rien à demander. Servir l'avertissement ici l'userait.
    devicesTransferHistory.mockResolvedValue({ conversations: 0, pages: 0 });
    monter();

    fireEvent.click(screen.getByRole('button', { name: L.historyTransferAction }));

    expect(await screen.findByText(L.historyTransferEmptyBook)).toBeInTheDocument();
    expect(screen.queryByText(L.historyTransferAmbiguous)).not.toBeInTheDocument();
  });

  it('résume ce qui est arrivé quand des pages ont été reçues', async () => {
    devicesTransferHistory.mockResolvedValue({ conversations: 3, pages: 12 });
    monter();

    fireEvent.click(screen.getByRole('button', { name: L.historyTransferAction }));

    const resume = await screen.findByText(debut(L.historyTransferDone), {
      exact: false,
    });
    expect(resume).toHaveTextContent('12');
    expect(resume).toHaveTextContent('3');
    expect(screen.queryByText(L.historyTransferAmbiguous)).not.toBeInTheDocument();
  });

  it('signale l’échec sans inventer de conclusion', async () => {
    // Appareil hors de la liste signée du compte, par exemple : le nœud refuse.
    devicesTransferHistory.mockRejectedValue(new Error('appareil inconnu'));
    monter();

    fireEvent.click(screen.getByRole('button', { name: L.historyTransferAction }));

    await waitFor(() => expect(useUi.getState().toasts).toHaveLength(1));
    expect(useUi.getState().toasts[0]?.text).toBe(fr.errors.actionFailed);
    expect(screen.queryByText(L.historyTransferAmbiguous)).not.toBeInTheDocument();
    expect(screen.queryByText(L.historyTransferEmptyBook)).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: L.historyTransferAction })).toBeEnabled();
  });

  it('prévient la liste du début et de la fin, pour qu’un seul transfert tourne', async () => {
    // ⚠️ `event.history_transfer` ne nomme pas l'appareil source : deux
    // transferts simultanés additionneraient leurs avancements dans les deux
    // barres. C'est la liste qui l'empêche, sur la foi de ce signal.
    devicesTransferHistory.mockResolvedValue({ conversations: 1, pages: 1 });
    const { onActif } = monter();

    fireEvent.click(screen.getByRole('button', { name: L.historyTransferAction }));

    expect(onActif).toHaveBeenCalledWith(true);
    await waitFor(() => expect(onActif).toHaveBeenCalledWith(false));
  });

  it('refuse de démarrer quand un transfert tourne ailleurs', () => {
    render(<HistoryTransferButton pubkey={PUBKEY} bloque onActif={vi.fn()} />);

    const bouton = screen.getByRole('button', { name: L.historyTransferAction });
    expect(bouton).toBeDisabled();
    fireEvent.click(bouton);
    expect(devicesTransferHistory).not.toHaveBeenCalled();
  });

  it('coupe l’abonnement quand l’écran disparaît en cours de transfert', () => {
    // Refermer les réglages pendant un transfert de plusieurs minutes ne doit
    // pas laisser un abonnement branché sur un composant démonté.
    const appel = differer<{ conversations: number; pages: number }>();
    devicesTransferHistory.mockReturnValue(appel.promise);
    const { unmount } = render(
      <HistoryTransferButton pubkey={PUBKEY} bloque={false} onActif={vi.fn()} />,
    );

    fireEvent.click(screen.getByRole('button', { name: L.historyTransferAction }));
    expect(handlers.size).toBe(1);

    unmount();
    expect(handlers.size).toBe(0);
  });
});
