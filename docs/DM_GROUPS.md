# DM groups — design

> Milestone 5 (10.0). Three to twenty people in one thread, without creating a
> server with channels and roles for it.

## 1. The problem

Direct messages are strictly two-party. Talking to three people means creating a
server — disproportionate, and it changes the nature of the relationship. This
is the most visible gap in messaging once multi-device is delivered.

## 2. The decision, and where it departs from the roadmap

The roadmap proposed two options and recommended **(b) a dedicated lightweight
group**: a new structure with a signed member list, no roles, no channels, one
thread. The alternative **(a)** was a real group created internally without
being presented as a server.

**This implementation takes (a).** The reasoning, since the roadmap recommends
otherwise:

- Option (b) re-derives what already exists. A DM group needs signed membership,
  replication, conflict resolution, catch-up, encryption, calls, notifications
  and moderation of its own history. Every one of those exists for groups, is
  tested, and has had its edge cases found the hard way. A parallel structure
  would be a second implementation of each — a second place for the same bug.
- The stated drawback of (a) is that "the full op-log is oversized for three
  people". Measured against what a DM group actually does, that is theoretical:
  a three-person group performs about five operations in its life (create, one
  channel, three member additions). The op-log's cost follows the number of
  operations, not the number of features the structure *could* express.
- Milestone 6 compacts the op-log by snapshots. A DM group built on the op-log
  inherits that work; a parallel structure would need its own.
- 🔒 New wire surface is new attack surface. Reusing a path that already refuses
  what it must refuse is worth more than a smaller path that has to learn.

**What (a) costs, stated plainly**: a DM group carries fields it never uses —
roles, categories, bans, emojis. They stay empty. The waste is a few empty
`BTreeMap`s per group, and the risk is that a future change to servers leaks
into DM groups; §4 below is the guard against that.

## 3. Wire

`GroupOpBody::Create` gains one **additive tail field**, the mechanism D-047
already established for `SetMeta::banner_color`:

```
0x01 CREATE { name: str, dm: opt<u8> }     // absent or 0 = server, 1 = DM group
```

A sender predating the field writes no bytes for it and `Reader::opt_tail`
decodes it as `None`. ⚠️ The consequence is worth stating rather than
discovering: **an older client shown a DM group sees an ordinary server.** It is
not broken — messages arrive, the members are right — it is merely presented in
the wrong place. That is the honest degradation for an additive field, and it is
why the flag lives in `CREATE` rather than in a later op: a group's nature must
be fixed at birth and signed, never something a later operation can flip.

## 4. What a DM group refuses

Enforced where ops are applied, so the rules hold against a hostile peer and not
merely against our own UI:

| Rule | Why |
|---|---|
| At most 20 members | Beyond that a server is the right shape. The roadmap's proposal, kept. |
| Any member may **invite** | Like a thread. No roles exist to consult. But inviting is not enrolling — see §4.1. |
| No roles, no categories, no extra channels | The ops are refused outright rather than hidden in the UI. A DM group has exactly one channel, created with it. |
| No bans, no timeouts, no moderation ops | There is no moderator. Leaving is the remedy. |
| The `dm` flag is set once, by `CREATE` | Nothing can promote a server to a DM group or the reverse. |

### 4.1 🔴 Consent, and the mistake that made this section necessary

The first version of this design said: *"a member adds directly, the way one
adds someone to a discussion thread"*, and the whitelist accepted a bare
`AddMember`. **That was the force-join D-045 had removed** — `AddMember`, the
whole op-log and the group key pushed to somebody who had asked for nothing and
agreed to nothing — reintroduced inside a whitelist written to be careful.

It was found by an agent probing whether the promised "any member may add
someone" was reachable from the API. It was not: every RPC path to membership
goes through an invitation, and the whitelist refused invitations. The gap that
looked like a missing feature was in fact the design being wrong in the safer
direction, and the API being right.

So a DM group now uses the same two-step consent as a server: any member may
create an invitation, and nobody becomes a member until they have accepted.
`AddMember` **without** an `invite_id` is refused outright.

⚠️ The consequence is deliberate and visible: a freshly created DM group holds
its founder alone, and fills as invitations are accepted. The interface shows
that rather than displaying members who agreed to nothing.

A DM group is *more* intimate than a server, not less. Being placed in one
without consent is worse there, not more acceptable.

## 5. Open questions from the roadmap, decided

- **Who can add someone?** Any member may *invite*; the invitee decides. A DM
  group has no hierarchy to consult, and inventing one would be the first step
  back towards a server — but no hierarchy is not the same as no consent.
- **Can you leave, and what do you see afterwards?** Yes, via the existing
  member-removal op applied to oneself. History already received stays local —
  the same rule as removing a friend, which keeps the DM history. Nothing is
  retroactively erased, because nothing can be: the messages are already on the
  machine.
- **What happens when the last member leaves?** The group stops being replicated
  by anyone and simply ceases to exist for the network. No tombstone is
  broadcast: there is nobody left to tell.
- **How many members at most?** 20.
- **What does someone added later see?** The thread from their arrival. Existing
  members do not re-send history — that is `GROUP_SYNC`'s existing behaviour and
  it is the privacy-preserving default: joining a conversation should not hand
  over everything said before you were there.

## 6. What the interface does with this

The rules above are enforced where ops are applied; the interface's job is to
stop offering what they refuse, and to file a DM group where it belongs.

- **Filed with the conversations.** `groups.state.is_dm` routes the group out of
  the server rail and into the conversation list, and keeps the home sidebar
  while its thread is open — so no channel list, no categories, no roles tab.
- **One thread, opened directly.** The group's single channel is the only one
  `AddChannel` ever accepted; the row opens it without a channel to choose.
- **No permissions, deliberately.** `base_permissions` does not consult `is_dm`,
  so the founder of a DM group receives the full mask (measured: `1023`).
  Presenting it would draw pinning, purging, banning and invitations — every one
  refused at replay. The interface therefore reads **zero** permissions in a DM
  group and renders the three permitted actions (rename, add, leave) as their
  own controls, available to every member.

### ⚠️ Adding a member is not reachable

§5 decided that **any member may add anyone**, and the state layer implements
exactly that: `AddMember` under twenty members is on the whitelist, and a member
with no role can author it. **No RPC reaches it.**

- `groups.invite` / `groups.invite_create` author `InviteCreate`, which the
  whitelist refuses. Measured against a running node: `groups.invite` on a DM
  group answers `refusé : opération sans objet dans un groupe de MP`, and
  `state.invites` stays empty.
- `groups.invite_accept` and `groups.invite_link_redeem` both go through
  `finalize_invite_accept`, which looks the invitation up in `state.invites`
  before authoring `AddMember` — so they cannot complete either.
- `groups.create_dm` is the only production path that authors a bare
  `AddMember`, and only at creation. `test_force_add_member` is `#[cfg(test)]`.

Until the node grows a method that authors `AddMember` for a DM group, a DM
group's membership is fixed at creation. Leaving works (`Kick` where the target
is the author), including for the founder.
