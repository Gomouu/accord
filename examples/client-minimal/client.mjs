#!/usr/bin/env node
/**
 * Minimal third-party Accord client (milestone 7, §19.4.1).
 *
 * Zero dependencies: Node 22+ ships a global `WebSocket`. Run it against a
 * standalone daemon — see README.md next to this file.
 *
 * It does the four things every third-party client has to get right, and
 * nothing else: find the socket, authenticate, make one request, and tell
 * responses apart from events on the same socket.
 */

import { readFileSync } from 'node:fs';
import { join } from 'node:path';

const profil = process.argv[2] ?? process.env.ACCORD_PROFILE;
if (profil === undefined) {
  console.error('usage: node client.mjs <profile-directory>');
  process.exit(2);
}

// 1. Find the address and token. The daemon writes this at startup, 0600 on
//    Unix. Treat the token as a password: anyone holding it has the same
//    powers over the node as the desktop application.
const session = JSON.parse(readFileSync(join(profil, 'session.json'), 'utf8'));

const ws = new WebSocket(`ws://${session.api}`);
let prochainId = 0;
const enAttente = new Map();

/** Sends a request and resolves with its result, or rejects with its error. */
function appeler(methode, params = {}) {
  const id = prochainId++;
  return new Promise((resolve, reject) => {
    enAttente.set(id, { resolve, reject });
    ws.send(JSON.stringify({ jsonrpc: '2.0', id, method: methode, params }));
  });
}

ws.addEventListener('open', async () => {
  // 2. Authenticate. This MUST be the first frame — any other method before it
  //    is rejected and the connection closes. You have 10 seconds.
  const bonjour = await appeler('auth', { token: session.token });
  if (bonjour.protocole !== 1) {
    console.error(`unknown API protocol version: ${bonjour.protocole}`);
    process.exit(1);
  }
  console.log(`connected, API protocol ${bonjour.protocole}`);

  // 3. One request. `identity.self` is the cheapest useful call.
  const moi = await appeler('identity.self');
  console.log(`self: ${moi.name ?? '(no name)'} — ${moi.friend_code}`);

  const { contacts } = await appeler('friends.list');
  console.log(`${contacts.length} contact(s)`);
  console.log('listening for events; Ctrl-C to stop');
});

ws.addEventListener('message', (frame) => {
  const msg = JSON.parse(frame.data);

  // 4. Responses and events share the socket. A frame WITHOUT `id` is an
  //    event — that is the only reliable way to tell them apart.
  if (msg.id === undefined) {
    if (msg.method === 'event.desynchronise') {
      // The node is telling you it dropped events for you. Do not try to patch
      // up: re-read state through the *.list / *.history methods.
      console.log('fell behind — re-read state via *.list / *.history');
      return;
    }
    // Unknown event names must be IGNORED, never treated as errors: the node
    // is free to add events, and a client that throws on one it does not know
    // breaks on every upgrade.
    console.log(`event ${msg.method}`, JSON.stringify(msg.params));
    return;
  }

  const attente = enAttente.get(msg.id);
  if (attente === undefined) return;
  enAttente.delete(msg.id);
  if (msg.error !== undefined) attente.reject(new Error(msg.error.message));
  else attente.resolve(msg.result);
});

ws.addEventListener('error', () => {
  console.error(`cannot reach the node at ${session.api}`);
  process.exit(1);
});

ws.addEventListener('close', () => {
  console.error('connection closed by the node');
  process.exit(1);
});
