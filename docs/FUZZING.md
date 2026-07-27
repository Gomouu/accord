# Fuzzing — what runs, for how long, and what it found

> Milestone 8 (ROADMAP §18.5) asks for a long campaign across all targets. This
> is what exists, what was actually run, and what the numbers do not say.

## 1. Two campaigns, two purposes

| | `fuzz.yml` | `fuzz-campagne.yml` |
|---|---|---|
| When | every night, 02:00 UTC | weekly (Sunday 01:00 UTC) + manual |
| Per target | 4 minutes | 45 minutes (configurable) |
| Shape | sequential | 9 parallel jobs |
| Purpose | **regression guard** | **search** |

The nightly catches what a corpus seed already reaches. It is a guard rail, not
an investigation. The weekly campaign is roughly eleven times deeper per target,
and keeps the enriched corpus as an artefact — that corpus is the real product,
because it makes the next campaign start where the last one stopped.

⚠️ **Neither is "several days", which is what the roadmap asks for.** A GitHub
job is cut at six hours. A genuine multi-day campaign needs a machine of one's
own; the corpus kept by the weekly run is what would seed it. Saying this
plainly is better than presenting 45 minutes as the equivalent.

## 2. The nine targets

`fuzz/fuzz_targets/`: `proto_decode`, `core_msg`, `group_op_body`, `group_state`,
`handshake_decode`, `dht_record`, `device_list`, `file_manifest`,
`backup_archive`.

They cover every decoding surface that takes bytes from a stranger. The rule
from ROADMAP §10.3 stands: **a new decoding surface gets a new target.**

## 3. Campaign of 2026-07-27

Apple M1 Pro, `cargo +nightly fuzz`, corpus seeds from `fuzz/seeds/`.

| Target | Duration | Executions | Crashes |
|---|---|---|---|
| `device_list` | 301 s | 70 391 994 | **0** |
| `handshake_decode` | 241 s | 47 127 339 | **0** |
| `group_state` | 241 s | 4 944 821 | **0** |
| `core_msg` | interrupted | — | **0** so far |

`device_list` was run first and longest on purpose: it is the structure at the
heart of multi-device identity and revocation, and it is where the 7.1 security
review found a real defect (`SECURITY.md` 16 and 17). Those defects were logic,
not decoding — which is exactly why a clean fuzz result here proves less than it
looks like it does.

### What these numbers do not say

- **Executions are not coverage.** `group_state` does fourteen times fewer runs
  per second than `device_list` because each run replays an op-log; that makes
  its 4.9 million runs *more* work, not less. Comparing the columns across rows
  is meaningless.
- **Fuzzing finds crashes, not wrong answers.** Every defect found by the 7.1
  adversarial review — a refused write reported as success, an unbounded
  timestamp — is a decoder that returns cleanly and a caller that draws the
  wrong conclusion. No fuzzer would have found either.
- **`core_msg` was cut short** by the harness that ran it, not by a finding. It
  is recorded as incomplete rather than dropped, because a table that quietly
  omits an interrupted run reads as a table where everything passed.
- Four of nine targets were run. The other five have only the nightly's four
  minutes behind them.

## 4. Running one yourself

```bash
CMAKE_POLICY_VERSION_MINIMUM=3.5 cargo +nightly fuzz run device_list fuzz/seeds/device_list -- -max_total_time=300
```

The `CMAKE_POLICY_VERSION_MINIMUM` is the vendored-Opus workaround documented in
`docs/DEV.md`; it is needed with CMake ≥ 4. And `+nightly` is not optional —
`cargo fuzz` needs the nightly toolchain for the sanitizer flags.
