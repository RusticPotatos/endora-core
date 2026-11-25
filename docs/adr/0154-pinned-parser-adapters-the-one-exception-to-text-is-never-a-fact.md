# 0154 - Pinned parser adapters: the one exception to "text a command printed is never a fact"

## Status

Accepted, 2026-11-25, for a **practice run on a non-live copy only**. AI-assisted (Claude). The owner
decided on 2026-11-25 that the integration be built and rehearsed first; **enabling an adapter on a
live instance, and classifying any adapter, stay reserved to the owner** and are not done by this
record. Builds on [0150](0150-current-architecture-the-foundation.md) (the foundation) and on the
Docker and `bwrap` confinement backends of the git capability.

## Context

Until now a fact has come only from what Core's own code observed: a connection opened, a status
line it parsed, an exit code. `probes.rs` says it in one line: text a command printed "is never a
fact". That keeps Core's checks out of reach of anything a remote service or a log can say, and it
has a price: the useful readings (how many ports listen, how much memory is free, how many updates
are pending) are printed text, and a person reads them by hand.

A sealed experiment (adapt-eval v0.5, 25 Nov 2026) asked a model to write small parsers for such
text. Under a fixed gate (security scan, own tests, hidden correctness cases, 66 wrong-shape cases
that must be refused, structural contract) six of six families were accepted, 28 of 28 real outputs
were read correctly and 17 of 17 deliberately wrong inputs were refused. The result is one run of
one creator model, observe band only.

## Decision

**Text becomes a fact only through a parser the owner classified, pinned by hash, ran confined, and
whose words never enter the record.** This is a narrow exception, not a change of principle:
everything else printed by a command, a remote service or a model is still never a fact.

- **The owner's config line is the classification.** `adapter <family> <probe-name> <tree-sha256>
  observe fact=<name>:<type>...`. The facts the call may return are the ones on that line (`int`,
  `bool`, or `word:a|b` from a closed list), not the ones the adapter's own `adapter.json` claims;
  that file and `manifest.json` are only cross-checked, and a disagreement, a `network` or `writes`
  claim, a non-empty `egress`, `write_scope` or `secrets`, or a band other than `observe` refuses the
  line when the config is read. **The only band accepted is `observe`**: this path cannot classify
  anything above it, so it can never grow into authority over an action.
- **The text comes from an owner-configured command probe**, run exactly as `probe.run` runs it
  (scrubbed environment, no stdin, no network of its own), kept whole up to 256 KiB. A command that
  exits non-zero, is ended by a signal or prints more than the cap is **not parsed**: a table cut
  short can look whole.
- **Pinned.** The adapter is a directory `<data>/adapters/<sha256-of-tree>/` (the tree hash the gate
  computes: sorted `[path, sha256]` pairs as compact JSON; a symlink or an odd file name is refused),
  copied there by the owner, never fetched, never inside the repository. Core recomputes the hash at
  start and before every call. At start a mismatch leaves the adapter **not offered** (the owner's
  other commands, the stop switch among them, keep working) and says so on stderr; between calls a
  mismatch is a failed call and nothing runs. The call is named `adapter.<family>.<hash8>`, so a
  policy row granted for one version can never authorize another.
- **Confined, as a read-only filter.** `python3 -I -S -B adapter.py` reads the text as
  `{"text": ...}` on stdin and writes one JSON object, in a 10 s wall clock, with the adapter
  directory read-only, no network, no writable path but a small tmpfs, memory and cpu limits and an
  answer capped at 64 KiB. Under `bwrap` where it can run (address-space, cpu and file limits through
  `prlimit`; no process limit, because `RLIMIT_NPROC` counts the whole user), else in a Docker
  container from a local image (`adapter-image`, never pulled). With neither, **nothing runs** and the
  call fails.
- **What each answer becomes** (`decide`, `check_facts`, the evidence record, acceptance and the hash
  chain are used as they are; no line of them changes):

  | The adapter says | Core records |
  |---|---|
  | `ok` and facts | the facts, the command's raw text beside them for offline replay, profile `adapter:<hash8>:<backend>` |
  | `partial` and facts | a **failed** call that keeps the checked facts: a criterion is never met by a partial read |
  | `refused` | a failed call, no facts (a watch counts the failure and pauses at its cap) |
  | not the contract, over time, crashed, too long | a failed call, no facts |
  | a fact the line did not declare, or of another type | a contract violation: the result is refused and the goal stops until the owner resumes it |

- **No adapter text enters the record.** The adapter's `reason` is dropped; Core writes fixed words
  and counts (`adapter 09d771f6: partial (rows 2 of 5)`). The existing instruction scan caught only
  20 of 55 planted instructions in adapter text, so it is a floor and not the control; the control is
  that no such text is ever stored or shown to a model.
- **Authority is unchanged.** The caller still needs a policy row for that exact call name at band
  `observe`, the goal's scope must name it, the item must be running and the stop switch off. Cost
  is zero and unenforced, exactly as `probe.run` declares.
- **Revoke** by deleting the config line (the call becomes `UnknownCall`); immediate stop by the
  stop switch. **Roll back** by removing every `adapter` line or running the previous binary: there is
  no schema change, so the old binary reads the store unchanged, and facts already written stay.

## Consequences

- One new capability beside `probe.run`, in the composition root (`app/foundation/src/adapters.rs`),
  one read-only filter runner in the confinement module and one function in the observe crate that
  reads a command's text whole. `endora-foundation-cli` gains `sha2` (already in the lock file).
- A call that fails keeps no raw text (`CallFailure` has no evidence field, and the domain is not
  changed): the text of a refusal is not replayable. A silent misread the adapter still accepts is
  the dangerous case and is only mitigated: the raw text of every accepted fact is kept, `partial` is
  a failure, and a live start begins in a shadow period in which no goal's criterion reads the facts.
- The Docker backend needs the daemon's socket, and `bwrap` needs unprivileged user namespaces; a
  Core in a container has neither by default. Live use therefore needs a deliberate choice of where
  the parser runs; this record does not make it.

## Not built

- No adapter is enabled on any live instance. No model creates or classifies an adapter at run time.
- Families without real samples (`smart_health`, `nginx_log`, the canary) and `zpool_status` (an empty
  shape only) are not part of the first wave.

## Alternatives considered

- **Trust the adapter's own `adapter.json` for the facts.** Rejected: then the model that wrote the
  code classifies it. The owner's line decides.
- **Let the adapter's `reason` into the record, scanned.** Rejected for the recall above.
- **Parse in Core's own process.** Rejected: model-written code in the authority boundary's process.
- **A new capability crate.** Rejected for now: one module and one helper, no abstraction without a
  second user.
