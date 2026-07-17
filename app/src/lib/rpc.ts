/**
 * Client JSON-RPC 2.0 sur WebSocket, conforme à API.md :
 * première requête `auth` obligatoire, notifications `event.*`,
 * reconnexion automatique avec repli exponentiel.
 *
 * Le jeton n'est jamais journalisé.
 */

export interface RpcError {
  code: number;
  message: string;
}

export class RpcCallError extends Error {
  readonly code: number;
  constructor(err: RpcError) {
    super(err.message);
    this.code = err.code;
  }
}

/** Surface minimale d'un WebSocket (injectable pour les tests). */
export interface WsLike {
  send(data: string): void;
  close(): void;
  onopen: (() => void) | null;
  onmessage: ((ev: { data: unknown }) => void) | null;
  onclose: (() => void) | null;
  onerror: (() => void) | null;
}

export type WsFactory = (url: string) => WsLike;

export type RpcStatus = 'idle' | 'connecting' | 'ready' | 'reconnecting' | 'closed';

type EventHandler = (method: string, params: unknown) => void;
type StatusHandler = (status: RpcStatus) => void;

interface Pending {
  resolve: (value: unknown) => void;
  reject: (error: Error) => void;
}

const RETRY_MIN_MS = 500;
const RETRY_MAX_MS = 15_000;

const defaultFactory: WsFactory = (url) => new WebSocket(url) as unknown as WsLike;

export class RpcClient {
  private readonly factory: WsFactory;
  private ws: WsLike | null = null;
  private nextId = 1;
  private readonly pending = new Map<number, Pending>();
  private readonly eventHandlers = new Set<EventHandler>();
  private readonly statusHandlers = new Set<StatusHandler>();
  private url = '';
  private token = '';
  private closedByUser = false;
  private retryMs = RETRY_MIN_MS;
  private retryTimer: ReturnType<typeof setTimeout> | null = null;
  private statusValue: RpcStatus = 'idle';
  private generation = 0;

  constructor(factory: WsFactory = defaultFactory) {
    this.factory = factory;
  }

  get status(): RpcStatus {
    return this.statusValue;
  }

  /** Ouvre la connexion et s'authentifie. Résout une fois prêt. */
  connect(port: number, token: string): Promise<void> {
    const previous = this.ws;
    const generation = ++this.generation;
    this.ws = null;
    if (this.retryTimer !== null) {
      clearTimeout(this.retryTimer);
      this.retryTimer = null;
    }
    this.failPending(new RpcCallError({ code: -1, message: 'connexion remplacée' }));
    previous?.close();
    this.url = `ws://127.0.0.1:${port}/`;
    this.token = token;
    this.closedByUser = false;
    return this.open(generation);
  }

  /** Appelle une méthode et rend son résultat typé par l'appelant. */
  call<T>(method: string, params: Record<string, unknown> = {}): Promise<T> {
    const ws = this.ws;
    if (!ws || this.statusValue !== 'ready') {
      return Promise.reject(new RpcCallError({ code: -1, message: 'hors ligne' }));
    }
    const id = this.nextId++;
    return new Promise<T>((resolve, reject) => {
      this.pending.set(id, { resolve: (v) => resolve(v as T), reject });
      ws.send(JSON.stringify({ jsonrpc: '2.0', id, method, params }));
    });
  }

  /** Abonne un gestionnaire d'événements `event.*`. Rend le désabonnement. */
  onEvent(handler: EventHandler): () => void {
    this.eventHandlers.add(handler);
    return () => this.eventHandlers.delete(handler);
  }

  /** Abonne un gestionnaire de statut de connexion. */
  onStatus(handler: StatusHandler): () => void {
    this.statusHandlers.add(handler);
    return () => this.statusHandlers.delete(handler);
  }

  /**
   * Force une tentative de reconnexion immédiate, sans attendre la fin du
   * repli exponentiel en cours. Sans effet si la connexion n'est pas en
   * attente de reprise (fermée par l'utilisateur, déjà prête, ou tentative
   * déjà en vol) : seul le minuteur d'attente est court-circuité.
   */
  retryNow(): void {
    if (this.closedByUser || this.retryTimer === null) return;
    clearTimeout(this.retryTimer);
    this.retryTimer = null;
    this.retryMs = RETRY_MIN_MS;
    const generation = this.generation;
    void this.open(generation).catch(() => {
      // Échec géré par onclose → nouvelle tentative planifiée.
    });
  }

  /** Ferme définitivement (pas de reconnexion). */
  close(): void {
    const ws = this.ws;
    this.generation += 1;
    this.ws = null;
    this.closedByUser = true;
    if (this.retryTimer !== null) {
      clearTimeout(this.retryTimer);
      this.retryTimer = null;
    }
    this.failPending(new RpcCallError({ code: -1, message: 'connexion fermée' }));
    ws?.close();
    this.setStatus('closed');
  }

  private setStatus(status: RpcStatus): void {
    if (this.statusValue === status) return;
    this.statusValue = status;
    for (const handler of this.statusHandlers) handler(status);
  }

  private open(generation = this.generation): Promise<void> {
    this.setStatus(this.statusValue === 'idle' ? 'connecting' : 'reconnecting');
    return new Promise<void>((resolve, reject) => {
      let settled = false;
      const ws = this.factory(this.url);
      this.ws = ws;

      ws.onopen = () => {
        if (generation !== this.generation || this.ws !== ws) {
          if (!settled) {
            settled = true;
            reject(new RpcCallError({ code: -1, message: 'connexion remplacée' }));
          }
          ws.close();
          return;
        }
        // Auth obligatoire en première requête (API.md §Authentification).
        const id = this.nextId++;
        this.pending.set(id, {
          resolve: () => {
            this.retryMs = RETRY_MIN_MS;
            this.setStatus('ready');
            settled = true;
            resolve();
          },
          reject: (e) => {
            settled = true;
            this.closedByUser = true; // jeton refusé : inutile d'insister
            reject(e);
          },
        });
        ws.send(
          JSON.stringify({
            jsonrpc: '2.0',
            id,
            method: 'auth',
            params: { token: this.token },
          }),
        );
      };

      ws.onmessage = (ev) => {
        if (generation !== this.generation || this.ws !== ws) return;
        if (typeof ev.data !== 'string') return;
        this.handleMessage(ev.data);
      };

      ws.onclose = () => {
        if (generation !== this.generation || this.ws !== ws) {
          if (!settled) {
            settled = true;
            reject(new RpcCallError({ code: -1, message: 'connexion remplacée' }));
          }
          return;
        }
        this.ws = null;
        this.failPending(new RpcCallError({ code: -1, message: 'connexion fermée' }));
        if (!settled) {
          settled = true;
          reject(new RpcCallError({ code: -1, message: 'connexion impossible' }));
        }
        this.scheduleReconnect(generation);
      };

      ws.onerror = () => {
        // onclose suit toujours ; rien à faire ici.
      };
    });
  }

  private handleMessage(raw: string): void {
    let msg: unknown;
    try {
      msg = JSON.parse(raw);
    } catch {
      return;
    }
    if (typeof msg !== 'object' || msg === null) return;
    const m = msg as {
      id?: number | string | null;
      method?: string;
      params?: unknown;
      result?: unknown;
      error?: RpcError;
    };

    // Notification serveur → événement.
    if (m.method !== undefined && (m.id === undefined || m.id === null)) {
      for (const handler of this.eventHandlers) handler(m.method, m.params);
      return;
    }

    // Réponse corrélée.
    if (typeof m.id === 'number') {
      const pending = this.pending.get(m.id);
      if (!pending) return;
      this.pending.delete(m.id);
      if (m.error) pending.reject(new RpcCallError(m.error));
      else pending.resolve(m.result);
    }
  }

  private failPending(error: Error): void {
    for (const [, pending] of this.pending) pending.reject(error);
    this.pending.clear();
  }

  private scheduleReconnect(generation: number): void {
    if (generation !== this.generation) return;
    if (this.closedByUser) {
      this.setStatus('closed');
      return;
    }
    this.setStatus('reconnecting');
    this.retryTimer = setTimeout(() => {
      this.retryTimer = null;
      if (generation !== this.generation) return;
      void this.open(generation).catch(() => {
        // Échec géré par onclose → nouvelle tentative planifiée.
      });
    }, this.retryMs);
    this.retryMs = Math.min(this.retryMs * 2, RETRY_MAX_MS);
  }
}
