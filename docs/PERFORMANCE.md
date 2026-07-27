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

The series (8 test binaries, release profile, single-threaded):

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
| Address change mid-session (`rebind`) | mobility: a peer moves to another IP *and* port under a live session — nothing closed, nothing renegotiated, a laptop switching from Wi-Fi to 4G |

`rebind` is new: the mesh could kill a node and it could bind a fresh one, but it
had no way to *move* one, which is a different thing — a moved node keeps its
inbox, so it keeps its sessions, its keys and its state. The transport half of
that campaign lives in `accord-transport`
(`reconnexion_transport_e2e`), for the reason given in §2.3.

### 2.1 Result

| Date | Machine | Series | Runs | Failures | Longest clean streak |
|---|---|---|---|---|---|
| 2026-07-26 | M1 Pro, release, idle | 7 binaries | 30 | **0** | **30** |
| 2026-07-26 | M1 Pro, release, idle | 8 binaries, chaos included | 30 | **0** | **30** |
| 2026-07-27 | M1 Pro, release, idle | 8 binaries, address change included | 30 | **0** | **30** |

Target met. The last row is the one that stands, and for the same reason the
second one replaced the first: adding a test to `chaos_reseau_e2e` (and one to
`reconnexion_transport_e2e`) changed what the script runs, so the earlier
numbers no longer described it. Re-running in full costs about ten minutes;
leaving a number to look current costs more than that the first time someone
trusts it. The mobility tests add roughly 12 s to a run.

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
- **Reaching a moved peer again does not prove its session followed it**, and
  the end-to-end campaign cannot tell the difference. Several paths lead to the
  same place: the transport re-targets the live session from the source address
  of the next datagram (`Endpoint::on_data`); the node's address book learns
  that same address from the incoming `Message` event and dials it; and even
  inside the transport, replying to a packet from an unknown address opens a
  handshake towards it. The last two negotiate a **fresh** session whose
  `install_session` then evicts the stale one — same table, same addresses, at
  the price of a full handshake instead of one field update. Disabling the
  mobility path therefore leaves `chaos_reseau_e2e` **green** (measured, not
  assumed). What separates the two is that no new session was ever negotiated,
  which is what `une_session_directe_suit_son_pair_qui_change_d_adresse`
  (`accord-transport/tests/reconnexion_transport_e2e.rs`) asserts, on the
  session events. The end-to-end test keeps its place — it covers the whole
  chain, database to socket — but it is not what proves mobility works.
- **Mobility is only covered for a peer that speaks after moving.** The learning
  is passive: a node that moves and then stays silent is written to at its dead
  address until the 25 s keep-alive reopens the other side's eyes, and that
  recovery is not exercised (waiting for it would dominate the campaign).
  Neither is mobility through a relay: only direct sessions are re-targeted, a
  tunnelled one keeping the relay's address. And the simulated move is
  instantaneous and lossless — a real Wi-Fi/4G handover drops packets for
  seconds.
- **The adverse conditions are simulated, not real.** A simulated NAT and a
  simulated loss model are what the code was written against; a real carrier-
  grade NAT will find things neither covers.
- 30 runs bounds the failure rate loosely. It rules out a one-in-ten flake with
  high confidence; it says little about a one-in-five-hundred one.

## 3. Large servers

**Bench**: `crates/accord-node/benches/large_servers.rs`.

```bash
cargo bench -p accord-node --bench large_servers
```

Milestone 6 (ROADMAP §18.3) opens with an instruction rather than a design:
start with measurements, because "optimising without measuring is guessing".
This section is that measurement, and **nothing here optimises anything** — the
work added a bench and this section, and touched no production file.

The bench builds a server at 50, 200 and 500 members: 12 channels across 4
categories, 6 roles each with a per-channel override, one invite per ten
members, and per member one `ADD_MEMBER` plus one `ASSIGN_ROLE` — 147, 462 and
1 092 signed operations. Every op goes through the production write path
(`group::author_op`, which replays it against the current state before signing
and persisting it), so the log is one the engine would really accept. The
database is encrypted on disk, as in §1.

Two of the figures are network paths, measured at the node's edge — everything
before the socket, nothing after it:

- **joining** replays the op-log the way the inviting peer really pushes it: one
  `CoreMsg::GroupOpMsg` at a time through `Node::ingest_core`, the entry point
  the router calls, paying per op the signature check, the `op_id` check and the
  insert;
- **fanning out** is the exact expansion of `Runtime::dispatch_outbound` for an
  `Outbound::GroupCast`: resolve the group state once, then per member resolve
  the delivery targets (one query) and enqueue the message (one row). A
  `GroupMsg` is *durable*, so `deliver_core_to_device` persists it in the outbox
  for every recipient whether or not that recipient is reachable — that write
  belongs to every message, not to a slow path.

`groups.state` is timed twice, because the two cases have nothing to do with one
another. The folded state is cached per `Db` instance, so a second call in a row
is nearly free — but **every op received invalidates it**, and so does
restarting the app. The cold figure is the one that describes opening a server.

### 3.1 Results

Apple M1 Pro (10 cores), 16 GB, macOS 27.0, `--release`, 2026-07-27. The bench
is single-threaded; the machine was 82% idle at the median over the run (§3.4).

| Path | 50 members | 200 members | 500 members |
|---|---|---|---|
| **Join: replay the op-log to a usable state** | 34.4 ms | **188 ms** | 827 ms |
| Ops replayed | 147 | 462 | 1 092 |
| `groups.state`, cold (opening a server, or after any op) | 0.51 ms | 1.35 ms | 3.28 ms |
| `groups.state`, warm (cache hit) | 0.25 ms | 0.64 ms | 1.57 ms |
| Fan out one message to every member | 4.3 ms | 15.4 ms | 41.0 ms |
| Materialised group state, in memory | 21.6 KiB | 64.6 KiB | 154.6 KiB |
| Op-log on the wire | 27.2 KiB | 85.8 KiB | 202.9 KiB |
| Op-log on disk, encrypted (empty schema subtracted) | 32 KiB | 124 KiB | 280 KiB |
| `groups.state` JSON handed to the UI | 15.9 KiB | 49.2 KiB | 115.8 KiB |
| Outbox bytes written by one message | 7.0 KiB | 28.1 KiB | 70.3 KiB |

**The 10-second target for a 200-member join is met, with about 50× to spare.**

Two reference points, so the join figure can be read rather than guessed at:

| | 50 | 200 | 500 |
|---|---|---|---|
| Fold the whole op-log **once** | 62 µs | 198 µs | 498 µs |
| Verify **all** the signatures in the log | 5.9 ms | 18.5 ms | 44.8 ms |

### 3.2 What the numbers say

**Nothing here is painful yet.** The worst single figure on a 500-member server
is an 827 ms join; at 200 members everything is under 200 ms. The three
structural limits the roadmap expected are real as *shapes*, but at the sizes it
names they cost milliseconds, not seconds.

**The join is where the curve is steepest** — ×2.5 members costs ×4.4 time — and
it is not steep for the reason one would assume. Replaying 1 092 ops is 0.5 ms
of folding; the join takes 827 ms. The gap is that the state is re-derived
**after every single op**: `Db::insert_group_op` invalidates the cached state,
and the `group_state` call that follows re-reads *the whole log out of SQLite*
and folds it again. A 1 092-op join therefore performs about 596 000 row reads
and as many op applications. Splitting the 827 ms:

| Part of a 500-member join | Time | Share |
|---|---|---|
| Ed25519 signature checks (1 092) | 44.8 ms | 5% |
| Folding, accumulated over every op (1 092 × ~0.25 ms) | ~272 ms | ~33% |
| Everything else, by subtraction — chiefly re-reading the log per op | ~510 ms | ~62% |

The last row is a subtraction, not a direct measurement, and it is the
interesting one: the dominant cost of joining is not the replay, and not the
cryptography. It is doing the replay's *input work* once per op.

**Fan-out is linear and entirely local**: across the three sizes the cost fits
about 0.2 ms of fixed work plus **~82 µs per recipient**, and none of it is
network. Per recipient the node runs one device-list query and writes one outbox
row holding a full copy of the encoded message (144 bytes here) — 70 KiB written
to disk to send one message to 500 people. The body itself is encrypted once,
with the group epoch key; it is the *copies* that multiply. And they multiply
again per device: `delivery_targets` returns one target per switched-over
device, up to `MAX_DEVICES = 8`.

**`groups.state` is small but called often.** 3.3 ms cold at 500 members is
nothing on its own. What deserves a second look is how often the cold path runs:
the node emits `event.group_state` on every op it ingests, and the front end
reloads the whole state on that event (`app/src/stores/groups.ts`,
`handleGroupState`) with no coalescing. Joining a 500-member server would
therefore ask for ~1 092 cold reloads. That is arithmetic on two measured
quantities, not a measurement — see §3.5.

### 3.3 The three limits the roadmap expected

1. **"The op-log is replicated whole; a new member replays everything."**
   Confirmed, and it is the steepest of the three — but at 500 members it costs
   827 ms, not seconds. What the numbers add is *why* it is steep: not the
   replay, which is sub-millisecond, but the state being re-derived from the
   database after every op. Compaction attacks that too, since the cost grows
   with the square of the log length — but it attacks it by shortening the log,
   and the per-op re-derivation would grow back with it.

2. **"`MAX_LIST = 4096` bounds wire lists, so member lists too."** **This one is
   not true.** No member list ever crosses the wire: members are replicated one
   `ADD_MEMBER` op at a time, and the bounded `list<T>` encoding is used only for
   attachments, poll options, AutoMod words, device lists, DHT node lists, NAT
   candidates and file leaf hashes (`accord-proto`, every `put_list`/
   `Reader::list` call site). `MAX_LIST` cannot bound a member count, and no
   `MAX_MEMBERS` exists for servers — only `MAX_DM_MEMBERS = 20` for DM groups.
   What *is* unpaginated is the local `groups.state` reply: 115.8 KiB of JSON at
   500 members, built and serialised in full on every call. If member pagination
   is wanted, it is a JSON-RPC and UI question, not a wire one.

3. **"Delivery is a star from the sender: writing to 200 members is 200 sends."**
   Confirmed, with a detail the roadmap does not mention: the 200 sends are
   preceded by 200 *database writes*, paid whether or not the recipient is
   reachable, and multiplied by the number of devices per account.

### 3.4 What was then fixed, and what it bought

§3.2 found the cost was not where the roadmap expected: not the replay, not the
cryptography, but the state being **re-derived from the database after every
single op**. `insert_group_op` invalidated the cache, and the next `group_state`
re-read the whole log out of SQLite and refolded it — about 596 000 row reads for
a 1 092-op join.

The cache now remembers a **watermark**: the highest canonical key
`(lamport, node_id(author), op_id)` already folded. An op sorting strictly above
it would have been applied last in a full fold, so applying it on top of the
cached state is demonstrably the same thing. An op sorting below has to be
inserted in the middle, and there only a full fold is right — the cache is
dropped. A `CREATE` always drops it, because `fold` hoists the committed root out
of canonical order.

| Join | Before | After | |
|---|---|---|---|
| 50 members | 34.4 ms | **22.1 ms** | 1.6× |
| 200 members | 188 ms | **70.0 ms** | 2.7× |
| 500 members | 827 ms | **172.6 ms** | **4.8×** |

The gain grows with size, which is the signature of the quadratic re-reading
being gone rather than a constant being shaved.

⚠️ **What this does not do.** It does not compact the op-log, which the roadmap
proposed and which remains unbuilt — a new member still replays everything, it
is simply no longer re-read from disk once per op. And nothing here touches the
star delivery or the unpaginated `groups.state` JSON, both still as measured in
§3.2 and §3.3.

🔒 The correctness of the fast path is pinned by
`le_repli_incremental_rend_le_meme_etat_quel_que_soit_lordre_darrivee`, which
compares an incrementally built state against a full fold in both arrival
orders. That test took three attempts to become real — it first used commutative
ops, then never populated the cache between inserts, then let a trailing
`CREATE` reset the cache and hide the corruption. Each version passed with the
ordering guard deleted. Worth knowing before trusting it.

### 3.5 Five servers at once — the budget nobody had measured

ROADMAP §10.2 sets a 400 MB ceiling for five servers and twenty conversations,
and milestone 6 makes it a completion criterion. It had never been measured.
The instrument already existed — `large_servers.rs` carries a counting global
allocator — so this is one function, not a new harness.

**Five servers of 200 members, their states folded and held simultaneously:
334 828 bytes — 0.3 MB.** Against a 400 MB budget, that is three orders of
magnitude of room, and it lines up with the per-server figure in §3.1
(64.6 KiB at 200 members, times five).

⚠️ **What this number is not.** It counts bytes allocated and not returned
while the five states are folded, with the counter on: the dominant data
structure, not the process's RSS. A real RSS would include the Tokio runtime,
transport buffers, the SQLite page cache and the webview — none of which are
here. Read as "the application's memory", it would be wrong. It bounds the
group-state cost from below, which is what the criterion was actually asking
about, and the remaining three orders of magnitude mean the answer does not
change even if everything else is a hundred times heavier.

The five states are folded inside a single measured closure and dropped after,
rather than one at a time: folding them in sequence and letting each go would
measure the largest of the five, not their sum.

### 3.6 Assumed limits

- **The machine was quiet, but not dedicated.** §2.3 applies here too, and this
  is a laptop that other work shares. The run behind the table was started only
  once the CPU was 92% idle and was sampled throughout: median 82% idle, tenth
  percentile 58%, with brief dips. Earlier runs taken while a parallel test
  suite was going reported the same figures 10–35% slower, which is what
  contention does to them — it inflates, it never flatters. **Read these as two
  significant digits, not three.**
- **The memory figure is what the folded state holds**, measured with a counting
  global allocator around `GroupState::fold` (the bench arms the counter only
  for that window, so the timings are not paying for it). It is the state, not
  the process: caches, the database's page cache and everything the UI holds are
  outside it.
- **The op-log is synthetic and regular**: one admission and one role assignment
  per member, a handful of channels and roles, no churn. A real server that has
  lived for years also carries kicks, bans, renames, pins and moderation
  tombstones — more ops for the same member count, and `apply_moderation`
  re-applies every tombstone on every ingest, which this corpus does not
  exercise.
- **`groups.list` is not in this table**, and it is what the app calls first at
  startup. It materialises every joined group (so it pays one cold fold per
  server, the third row above) and then counts unread messages and mentions per
  channel. Its shape is therefore "servers × channels" on top of "servers ×
  members"; measuring it needs a fixture with several servers, which this bench
  does not build.

### 3.7 Not measured here

- **Everything past the socket.** Session sealing, the UDP write, retransmission
  and the anti-entropy round trip that precedes a join are all outside the
  fan-out and join figures. The join number is the CPU and disk cost of
  ingesting the log, which a real join pays *in addition to* the network.
- **The shape of the burst.** `route_core` answers a `GroupSyncPull` by sending
  every op back to back with no pacing: a 500-member server is 1 092 datagrams
  in a tight loop. Whether that survives a real link — or causes the loss the
  anti-entropy then has to repair — is a network measurement this bench cannot
  make.
- **The front end's amplification.** The ~1 092 cold `groups.state` reloads
  implied by §3.2 are multiplication, not measurement. Confirming it needs an
  instrumented UI, and it is the first thing to check before concluding anything
  about what joining a large server *feels* like.
- **Memory across five servers.** The roadmap's second target ("memory stays
  under budget with 5 servers of 200 members") is not measured: this bench holds
  one server at a time.
- **Voice at scale.** Conference rooms (roadmap item D) are untouched here; the
  full-mesh limit is `VOICE_MAX_PARTICIPANTS = 10` and has nothing to do with
  these numbers.
