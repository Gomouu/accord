/**
 * Tests du client JSON-RPC : auth en première requête, corrélation des
 * identifiants, événements, gestion d'erreurs et reconnexion automatique.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { RpcClient, RpcCallError, type WsFactory, type WsLike } from './rpc';

/** WebSocket factice pilotable depuis les tests (côté « serveur »). */
class FakeWs implements WsLike {
  sent: string[] = [];
  isClosed = false;
  onopen: (() => void) | null = null;
  onmessage: ((ev: { data: unknown }) => void) | null = null;
  onclose: (() => void) | null = null;
  onerror: (() => void) | null = null;

  send(data: string): void {
    this.sent.push(data);
  }

  close(): void {
    this.isClosed = true;
    this.onclose?.();
  }

  /** Simule l'ouverture du lien. */
  serverOpen(): void {
    this.onopen?.();
  }

  /** Simule un message serveur (sérialisé en JSON). */
  serverSend(message: unknown): void {
    this.onmessage?.({ data: JSON.stringify(message) });
  }

  /** Simule une coupure réseau (sans action de l'utilisateur). */
  serverDrop(): void {
    this.onclose?.();
  }

  /** Requête envoyée à l'index donné, désérialisée. */
  request(index: number): {
    id: number;
    method: string;
    params: Record<string, unknown>;
  } {
    const raw = this.sent[index];
    if (raw === undefined) throw new Error(`aucune requête à l'index ${index}`);
    return JSON.parse(raw) as {
      id: number;
      method: string;
      params: Record<string, unknown>;
    };
  }

  /** Répond avec succès à la requête d'index donné. */
  respond(index: number, result: unknown): void {
    this.serverSend({ jsonrpc: '2.0', id: this.request(index).id, result });
  }

  /** Répond avec une erreur à la requête d'index donné. */
  respondError(index: number, code: number, message: string): void {
    this.serverSend({
      jsonrpc: '2.0',
      id: this.request(index).id,
      error: { code, message },
    });
  }
}

function makeClient(): { client: RpcClient; sockets: FakeWs[] } {
  const sockets: FakeWs[] = [];
  const factory: WsFactory = () => {
    const ws = new FakeWs();
    sockets.push(ws);
    return ws;
  };
  return { client: new RpcClient(factory), sockets };
}

/** Connecte et authentifie le client sur la première socket factice. */
async function connectReady(client: RpcClient, sockets: FakeWs[]): Promise<FakeWs> {
  const connecting = client.connect(4242, 'jeton-secret');
  const ws = sockets[0];
  if (!ws) throw new Error('socket non créée');
  ws.serverOpen();
  ws.respond(0, { protocole: 1 });
  await connecting;
  return ws;
}

describe('RpcClient — authentification', () => {
  it('envoie `auth` comme toute première requête, avec le jeton', async () => {
    const { client, sockets } = makeClient();
    const connecting = client.connect(4242, 'jeton-secret');
    const ws = sockets[0];
    expect(ws).toBeDefined();
    ws?.serverOpen();

    const first = ws?.request(0);
    expect(first?.method).toBe('auth');
    expect(first?.params).toEqual({ token: 'jeton-secret' });

    ws?.respond(0, { protocole: 1 });
    await connecting;
    expect(client.status).toBe('ready');
  });

  it('rejette la connexion si le jeton est refusé, sans réessayer', async () => {
    const { client, sockets } = makeClient();
    const connecting = client.connect(4242, 'mauvais-jeton');
    const ws = sockets[0];
    ws?.serverOpen();
    ws?.respondError(0, -32001, 'jeton invalide');

    await expect(connecting).rejects.toMatchObject({ code: -32001 });

    // Le serveur ferme ensuite : aucune reconnexion ne doit être planifiée.
    ws?.serverDrop();
    expect(client.status).toBe('closed');
    expect(sockets).toHaveLength(1);
  });

  it('refuse les appels tant que la connexion n’est pas prête', async () => {
    const { client } = makeClient();
    await expect(client.call('identity.self')).rejects.toBeInstanceOf(RpcCallError);
  });
});

describe('RpcClient — corrélation des requêtes', () => {
  it('résout chaque appel avec le résultat portant son id, même dans le désordre', async () => {
    const { client, sockets } = makeClient();
    const ws = await connectReady(client, sockets);

    const a = client.call<{ v: string }>('methode.a');
    const b = client.call<{ v: string }>('methode.b');

    // Réponses dans l'ordre inverse des requêtes (index 1 = a, 2 = b).
    ws.respond(2, { v: 'reponse-b' });
    ws.respond(1, { v: 'reponse-a' });

    await expect(a).resolves.toEqual({ v: 'reponse-a' });
    await expect(b).resolves.toEqual({ v: 'reponse-b' });
  });

  it('rejette un appel avec RpcCallError quand le serveur rend une erreur', async () => {
    const { client, sockets } = makeClient();
    const ws = await connectReady(client, sockets);

    const call = client.call('methode.inconnue');
    ws.respondError(1, -32601, 'méthode inconnue');

    await expect(call).rejects.toMatchObject({
      code: -32601,
      message: 'méthode inconnue',
    });
  });

  it('ignore les messages illisibles ou non textuels sans casser le flux', async () => {
    const { client, sockets } = makeClient();
    const ws = await connectReady(client, sockets);

    ws.onmessage?.({ data: 'pas du JSON {' });
    ws.onmessage?.({ data: new ArrayBuffer(4) });

    const call = client.call<{ ok: boolean }>('methode.a');
    ws.respond(1, { ok: true });
    await expect(call).resolves.toEqual({ ok: true });
  });

  it('rejette les appels en attente quand la connexion tombe', async () => {
    const { client, sockets } = makeClient();
    vi.useFakeTimers();
    try {
      const ws = await connectReady(client, sockets);
      const call = client.call('methode.a');
      ws.serverDrop();
      await expect(call).rejects.toMatchObject({ message: 'connexion fermée' });
    } finally {
      vi.useRealTimers();
    }
  });
});

describe('RpcClient — événements', () => {
  it('diffuse les notifications sans id aux abonnés, puis respecte le désabonnement', async () => {
    const { client, sockets } = makeClient();
    const ws = await connectReady(client, sockets);

    const received: [string, unknown][] = [];
    const off = client.onEvent((method, params) => received.push([method, params]));

    ws.serverSend({
      jsonrpc: '2.0',
      method: 'event.dm',
      params: { peer: 'abc123', msg_id: 'def456' },
    });
    expect(received).toEqual([['event.dm', { peer: 'abc123', msg_id: 'def456' }]]);

    off();
    ws.serverSend({ jsonrpc: '2.0', method: 'event.group_key', params: {} });
    expect(received).toHaveLength(1);
  });
});

describe('RpcClient — reconnexion', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('se reconnecte et se ré-authentifie après une coupure', async () => {
    const { client, sockets } = makeClient();
    const ws = await connectReady(client, sockets);

    const statuses: string[] = [];
    client.onStatus((s) => statuses.push(s));

    ws.serverDrop();
    expect(client.status).toBe('reconnecting');

    // Premier repli : 500 ms.
    await vi.advanceTimersByTimeAsync(500);
    const ws2 = sockets[1];
    expect(ws2).toBeDefined();
    ws2?.serverOpen();
    expect(ws2?.request(0).method).toBe('auth');
    ws2?.respond(0, { protocole: 1 });

    expect(client.status).toBe('ready');
    expect(statuses).toEqual(['reconnecting', 'ready']);
  });

  it('double le délai de repli à chaque échec (borné)', async () => {
    const { client, sockets } = makeClient();
    const ws = await connectReady(client, sockets);

    ws.serverDrop();
    await vi.advanceTimersByTimeAsync(500);
    expect(sockets).toHaveLength(2);

    // La tentative échoue immédiatement : prochain essai à 1000 ms.
    sockets[1]?.serverDrop();
    await vi.advanceTimersByTimeAsync(999);
    expect(sockets).toHaveLength(2);
    await vi.advanceTimersByTimeAsync(1);
    expect(sockets).toHaveLength(3);
  });

  it('ne se reconnecte pas après une fermeture volontaire', async () => {
    const { client, sockets } = makeClient();
    await connectReady(client, sockets);

    client.close();
    expect(client.status).toBe('closed');

    await vi.advanceTimersByTimeAsync(60_000);
    expect(sockets).toHaveLength(1);
  });

  it('force une reconnexion immédiate sans attendre le repli (retryNow)', async () => {
    const { client, sockets } = makeClient();
    const ws = await connectReady(client, sockets);

    ws.serverDrop();
    expect(client.status).toBe('reconnecting');
    expect(sockets).toHaveLength(1);

    // Sans avancer les minuteurs : la reprise doit partir tout de suite.
    client.retryNow();
    expect(sockets).toHaveLength(2);
    const ws2 = sockets[1];
    ws2?.serverOpen();
    ws2?.respond(0, { protocole: 1 });
    expect(client.status).toBe('ready');
  });

  it('retryNow est sans effet hors attente de repli (prêt, puis fermé)', async () => {
    const { client, sockets } = makeClient();
    await connectReady(client, sockets);

    // Lien prêt : aucune tentative en attente, rien à court-circuiter.
    client.retryNow();
    expect(sockets).toHaveLength(1);

    // Fermé volontairement : ne relance jamais.
    client.close();
    client.retryNow();
    expect(sockets).toHaveLength(1);
    expect(client.status).toBe('closed');
  });

  it("ignore la fermeture tardive de l'ancienne socket après remplacement", async () => {
    const { client, sockets } = makeClient();
    const old = await connectReady(client, sockets);
    const lateClose = old.onclose;
    old.onclose = null;

    client.close();
    const connecting = client.connect(5252, 'nouveau-jeton');
    const current = sockets[1];
    current?.serverOpen();

    lateClose?.();
    current?.respond(0, { protocole: 1 });

    await expect(connecting).resolves.toBeUndefined();
    expect(client.status).toBe('ready');
    expect(sockets).toHaveLength(2);
  });
});
