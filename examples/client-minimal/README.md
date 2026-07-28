# Minimal third-party client

A runnable client for the local JSON-RPC API, in ~90 lines and **zero
dependencies**. It exists so the contract in [`docs/API_CONTRACT.md`](../../docs/API_CONTRACT.md)
has something that actually runs: a snippet in a document rots silently, a
program that is executed does not.

Requires **Node 22+** (for the built-in global `WebSocket`).

## Run it

Start a standalone daemon on a throwaway profile:

```bash
ACCORD_PASSPHRASE='a test passphrase' ACCORD_PROFILE=/tmp/accord-demo ACCORD_P2P_ADDR=0.0.0.0:0 cargo run -p accord-node --bin accord-noded
```

Then, in another terminal:

```bash
node examples/client-minimal/client.mjs /tmp/accord-demo
```

Expected output, against a fresh profile:

```
connected, API protocol 1
self: (no name) — ABSURD-BANNER-PLATE-SUFFER-ELBOW-DUTY-0625
0 contact(s)
listening for events; Ctrl-C to stop
```

## The four things it demonstrates

1. **Finding the socket.** The daemon writes `<profile>/session.json` at
   startup — `{"api": "<ip:port>", "token": "<hex64>"}`, `0600` on Unix. The
   desktop application has no such file; the token goes to the WebView over
   Tauri IPC.
2. **Authenticating first.** `auth` must be the very first frame. Any other
   method before it is rejected and the connection closes, and you have ten
   seconds. Check `protocole` — it is your only machine-readable handle on
   compatibility.
3. **Telling responses from events.** They share one socket. A frame **without
   an `id`** is an event. That is the only reliable discriminator.
4. **Ignoring what you do not know.** An unrecognised `event.*` name must be
   skipped, never treated as an error — the node is free to add events, and a
   client that throws on an unknown one breaks at every upgrade.

## 🔒 What the token is

A password. Anyone holding it has the same powers over the node as the desktop
application: read every conversation, send as you, export your data. It is not
a capability scoped to what your script needs — the API has no scopes today.
Do not log it, do not put it in a URL, do not commit it.

The API listens on **loopback only**. A third-party client sends no `Origin`
header, and the server allows that case explicitly (a native WebView is in the
same position); browser origins other than the app's own are refused, which is
what stops a web page from driving your node via DNS rebinding.

## What it deliberately does not do

No reconnection, no backoff, no `event.desynchronise` recovery beyond printing
a line. Those belong in a real client and would triple the length here without
teaching anything about the protocol. `docs/API_CONTRACT.md` §4 describes what
a production client owes.
