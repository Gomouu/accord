# PERFORMANCE — measured budgets

> What has actually been measured, on what, and with what result. A budget with
> no number next to it is a wish, not a budget. Every figure here comes from a
> benchmark that lives in the repository and can be re-run.

## 1. Large histories

**Bench**: `crates/accord-node/benches/history.rs`.

```bash
cargo bench -p accord-node --bench history
```

It populates an **encrypted database on disk** (SQLCipher, the same
`Db::open` the application uses) with 100 000 direct messages in one
conversation, each indexed for search exactly as the send/receive path indexes
it. It then measures the **JSON-RPC methods the interface really calls**, not
isolated SQL statements — including the JSON serialisation handed back to the
UI.

Opening a conversation, for instance, is what `DmView` does on mount:
`dm.history` (page of 50) followed by `dm.pins`.

### 1.1 Results

Apple M1 Pro, 16 GB, macOS 27.0, `--release`, corpus of 100 000 messages.
"Before" is the code as of 6.2; "after" is this branch.

| Path | Before | After | Budget |
|---|---|---|---|
| Open a conversation, 1 000 messages | 1.03 ms | 0.95 ms | 300 ms |
| Open a conversation, 10 000 messages | 1.05 ms | 0.94 ms | 300 ms |
| **Open a conversation, 100 000 messages** | 0.92 ms | **0.91 ms** | **300 ms** |
| Deep pagination (scrolling up mid-history) | 1.10 ms | 0.99 ms | — |
| `friends.list`, everything read | 0.06 ms | 0.07 ms | — |
| `friends.list`, 50 000 unread waiting | 46.4 ms | **1.57 ms** | — |
| Search, rare word (~100 matches) | 1.34 ms | **0.59 ms** | — |
| Search, word present in all 100 000 messages | 1.26 **s** | **2.89 ms** | — |
| Filtered search with no keyword (`has:`, `from:` alone) | 130 ms | **1.09 ms** | — |
| Database size | 152.7 MiB | 162.5 MiB | to watch |

### 1.2 What the numbers say

**The conversation-opening budget is met with roughly 330× margin, and it was
already met before this branch.** The cost follows the size of the *page*, not
the size of the history: the `(peer, lamport)` index answers the page directly,
and reactions, attachments and mentions load in one batch per page rather than
one query per message. Nothing there needed optimising — which is why nothing
there was optimised.

**What did not scale was search**, and the cost of a stale unread counter:

- A word present in every message cost **1.26 s**, with the database mutex held
  for the whole time — so it also suspended message reception. The blind index
  was never the problem; exploiting it was. Every match was re-read one message
  at a time (two SQL statements per match), then the entire result set was
  sorted in memory to keep the 200 most recent.
- A filtered search with no keyword read and sorted the whole history to find
  its 1 000 most recent candidates.
- `count_dm_unread` filters on author and tombstone, which `dm_by_peer` does not
  carry, so counting a backlog re-read one table row per unread message.

### 1.3 What was changed

1. **Bounded candidates, chosen and sorted by SQLite** (`Db::search_candidates`).
   One statement per message table instead of two per match, ordered by recency
   and truncated in SQL.
2. **Two query plans, chosen from a bounded probe of the index.** A rare word is
   looked up through the blind index; a frequent one is found by walking
   messages from the most recent and probing the index. Each plan is a disaster
   in the other's case, and both cells were measured by forcing each plan:

   | plan | rare word (~100 matches) | word in all 100 000 |
   |---|---|---|
   | through the blind index | **0.59 ms** | 317 ms |
   | walking messages by recency | 79 ms | **2.9 ms** |

   The probe that picks between them stops counting as soon as the threshold is
   crossed, so deciding costs the same whatever the word's real frequency.
3. **Three indexes** (schema migration 14, index creation only, no table
   rewrite): `dm_by_sent` and `gmsg_by_sent` for recency ordering, and
   `dm_unread` covering the unread predicate exactly.

Cost: **+9.8 MiB on a 162 MiB database (+6.4%)**. That is what the three
indexes weigh at this volume.

### 1.4 Assumed limits

- **Search only considers the 1 000 most recent matches**
  (`SEARCH_CANDIDATE_CAP`, five times what the UI can display). On a frequent
  word in a large history, a `has:image` filter can therefore miss an older
  attachment. This is a deliberate trade: unbounded, the query held the database
  mutex for over a second, which suspended message reception too. The previous
  code was complete but only for keyword queries — the no-keyword path was
  already capped at 1 000.
- **The database weighs about 1.6 KiB per message**, roughly two thirds of it
  the blind search index (9 tokens per message). A million messages would be
  about 1.6 GiB. **There is no purge and no archiving today.** This is the next
  thing to look at on large histories, and it is a product decision before it is
  a technical one.
- `friends.list` is still linear in the number of *unread* messages (not in
  history size): 1.57 ms for a 50 000-message backlog on one contact, and it is
  called on every conversation open and every incoming message. Fine at this
  volume; worth remembering if unread counters ever get slower.

### 1.5 Not measured here

- Cold start to usable interface (< 2 s budget), idle CPU, memory over 5 servers
  and 20 conversations: still uninstrumented.
- The React render of a page of 50 messages. This bench stops at the node's JSON
  reply, which is where the 0.91 ms is spent; what the UI then does with it is a
  separate measurement.

## 2. Reconnection stability

**Campaign**: `./reconnexion-30.sh`.

```bash
./reconnexion-30.sh
```

Not a latency measurement — a **flakiness** one. The roadmap's target for §9.3
is "30 consecutive runs without a failure", and this is what produces that
number.

One green run proves these tests *can* pass; it says nothing about how often
they fail. The defects already found on this path — a dial abandoned too early,
a lost WELCOME, a dead session not evicted — showed up once in ten or twenty
runs. A single run does not see them. So the campaign runs the whole series 30
times, and **any failure anywhere resets the counter to zero**: the claim is "30
in a row", not "30 of which most passed".

The series (7 test binaries, release profile, single-threaded):

| Crate | Test binaries |
|---|---|
| `accord-node` | `reconnexion_e2e`, `reconnexion_lifecycle_e2e`, `profil_perdu_e2e`, `profil_reboot_e2e`, `chaos_reseau_e2e` |
| `accord-transport` | `reconnexion_transport_e2e`, `multi_appareil_e2e`, `handshake_e2e` |

`chaos_reseau_e2e` runs the messaging path over the simulated UDP mesh with the
adverse conditions turned **on** — that mesh has carried per-datagram loss,
variable latency and a per-node kill switch for a long time, and every caller
was passing `NetConditions::default()`, meaning zero loss and zero latency. What
the project knew about its behaviour on a degraded network, it knew by
deduction.

| Condition | What it exercises |
|---|---|
| 33% datagram loss | the offline queue and its retries — a message only gets through by being re-sent |
| 5–120 ms jitter | the Lamport clock: arrival order must not decide displayed order |
| Hard cut (`set_down`) | datagrams vanish with no RST, no FIN, no error — a Wi-Fi dropping, not a clean shutdown |

### 2.1 Result

| Date | Machine | Series | Runs | Failures | Longest clean streak |
|---|---|---|---|---|---|
| 2026-07-26 | M1 Pro, release, idle | 7 binaries | 30 | **0** | **30** |
| 2026-07-26 | M1 Pro, release, idle | 8 binaries, chaos included | 30 | **0** | **30** |

Target met. The second row is the one that stands: adding `chaos_reseau_e2e` to
the campaign changed what the script runs, so the first number no longer
described it and the campaign was re-run in full rather than left to look
current.

### 2.2 How to read a failure

The script keeps the logs of any run that fails and prints the failing test
names. Re-running a single failing binary in a loop is usually the fastest way
in from there.

### 2.3 Assumed limits

- **The machine must be idle.** These tests wait on real network events with
  real deadlines. A compile or another test suite running alongside produces
  failures that say nothing about the code — that is a measurement artefact, not
  a flake, and it must not be recorded as one.
- **The reordering test does not prove reordering happened.** It sets up the
  conditions that allow it and checks the invariant holds; it does not count
  inversions. Proving it would need the simulator to report delivery order,
  which it does not.
- **No address change mid-session.** §9.3 asks for that campaign too and it does
  not exist: making a node change address while a session is live needs a rebind
  path the simulator has no handle for. Loss, reordering and hard cuts are
  covered; this one is not.
- **The adverse conditions are simulated, not real.** A simulated NAT and a
  simulated loss model are what the code was written against; a real carrier-
  grade NAT will find things neither covers.
- 30 runs bounds the failure rate loosely. It rules out a one-in-ten flake with
  high confidence; it says little about a one-in-five-hundred one.
