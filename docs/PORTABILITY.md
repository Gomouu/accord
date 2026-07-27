# Data portability — the export format

> Milestone 7 (§19.4.2). A user who cannot leave with their data is captive. A
> project that argues for sovereignty has to be exemplary about this.

## 1. Two formats, and why both exist

| | `.accordbackup` | this export |
|---|---|---|
| Encrypted | yes | **no** |
| Readable without Accord | no | yes |
| Restores a machine identically | yes | no |
| Purpose | **coming back** | **leaving** |

They do not replace each other. The encrypted backup restores a machine; this
export is what you take with you. Only one of them is any use to a person who
has decided to stop using Accord — and only one of them is safe to leave on a
desktop.

🔒 **The export is not encrypted.** That is what it is for, and it makes it the
most dangerous file the application knows how to write: every conversation, in
clear text, in one place. The interface says so when you ask for it, and the
document repeats it in its own header (`warning`) so it stays explicit once
separated from the application that produced it.

## 2. Producing and reading one

```
portable.export  {}                     -> the document below
portable.import  { document: <doc> }    -> { inserted, skipped, rejected }
```

## 3. The format

```json
{
  "format": 1,
  "generator": "accord 7.1.0",
  "warning": "This file is NOT encrypted. It contains every conversation in clear text.",
  "account": "<hex 64: your own account public key>",
  "truncated": false,
  "contacts": [
    { "pubkey": "<hex 64>", "display_name": "Camille", "added_ms": 1785000000000 }
  ],
  "conversations": [
    {
      "peer": "<hex 64: the other account>",
      "peer_name": "Camille",
      "truncated": false,
      "messages": [
        {
          "msg_id": "<hex 32>",
          "author": "<hex 64: peer or you>",
          "lamport": 42,
          "sent_ms": 1785000000000,
          "deleted": false,
          "text": "the readable text, or null for a non-textual body",
          "kind": 0,
          "body_hex": "<the original encoded body>",
          "edited_hex": "<the edited body, or absent>"
        }
      ]
    }
  ]
}
```

**Why both `text` and `body_hex`.** `text` is for a human, and for any tool that
just wants to read the conversation. `body_hex` is the original envelope, and it
is what makes the document re-importable exactly as it was. Carrying only the
text would give a pretty export that cannot come back; carrying only the bytes
would give the opposite.

Messages are ordered **oldest first**, as on screen. An export you have to read
backwards is not readable.

## 4. What import refuses, and why

🔒 **A conversation whose peer is not already a contact is refused whole.** An
export file is unauthenticated input — it arrives from a disk, nobody signed it.
Accepting it as-is would let any file invent correspondents and write messages
under their names. The same rule already governs catch-up between one's own
devices (`ingest_self_sync_item`), for the same reason: an import fills a
history, it does not create a relationship.

🔒 **A message whose author is neither the peer nor you is refused.** Without
that check a forged file could file a message signed in Q's name inside P's
conversation, and the interface would display it as such.

**An unknown `format` is refused, not guessed.** A future version may change
what a field means without changing its name.

**Import is idempotent.** Insertion keys on `msg_id`, so re-importing the same
file twice inserts nothing the second time — that is what `skipped` counts.

## 5. Assumed limits

- **Attachments are referenced, not embedded.** A message carries its file
  references inside `body_hex`; the blobs themselves are not in the document. A
  full export including media is a separate piece of work, and pretending
  otherwise here would produce an export that silently loses images.
- **Group and server history is not exported.** Only direct conversations. The
  op-log is a replicated structure whose export raises questions this format
  does not answer — most obviously, what it means to hold a copy of a group's
  history after leaving it.
- **Bounded at 10 000 conversations and 100 000 messages each**, and the
  document **says so** via `truncated` when a bound bites. A silently truncated
  export is worse than a short one: the reader concludes there was nothing more.
- **Reactions, pins and read marks are not carried.** They are local
  bookkeeping, not content.
