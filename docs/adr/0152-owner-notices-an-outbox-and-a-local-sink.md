# 0152 - Owner notices: an outbox and a local sink

## Status

Accepted, 2026-11-25. AI-assisted (Claude). Builds on [0148](0148-the-house-watch.md) (a notice on a
change, after a grace) and [0150](0150-current-architecture-the-foundation.md) (the foundation).
Delivery to an outside service is **not** part of this record: see "Not built".

## Context

The foundation watches with routines and has never told the owner anything: a watch that fails five
times in a row pauses itself and waits to be noticed in `status`. The house watch (0148) already has
the right behaviour for a person: speak only on a change, after a grace, one message per change,
retry what could not be sent. The foundation had no place to keep such a message until it was
delivered, and its decide path refuses outward calls (`decide.rs`), so it cannot simply send one.

## Decision

**Core writes a notice into an outbox table; a sink delivers it; the sink is local.**

- **A notice is a fact about state, not a capability call.** It holds the routine's name (the
  owner's own title), a state (`failing` or `recovered`) and a time, and never a probe's output,
  evidence or error text. It is not on the evidence chain and does not go through `decide`.
- **When (the rule is pure, `domain/notice.rs`).** A watch is *failing* once it has failed at least
  3 checks in a row and the run (first failure to now) has lasted at least 10 minutes: one `failing`
  notice, once per run. The next check that passes makes one `recovered` notice, but only for a run
  that was said; a blip that ended inside the grace is never said. Core also sweeps on every
  `run-due` and `serve` tick, so a run that stopped failing because the routine paused (the cap
  pauses it) is still said after the grace.
- **Idempotent.** A notice's key is the routine, the state and when the run began; the table makes it
  unique, so asking twice changes nothing.
- **Outbox and retry.** Notices wait in `notices` until a sink takes them. Core hands the
  undelivered ones to the sink oldest first, once per tick, and marks one taken only after the sink
  answers `Ok`; a refusal counts an attempt and leaves it for the next tick. A sink must answer by
  the key, so a crash between its write and Core's mark cannot duplicate.
- **The seam.** `application::NoticeSink` (`deliver(&Notice) -> Result<(), String>`) is the only
  way out. Two local sinks exist: `FileSink`, an append-only file named by the config line
  `notice-file PATH` (mode 600, one line per key), and the `notices` command's own listing of the
  outbox (`notices [--all]`; `notices ack ID`, which needs the owner token).
- **The stop switch does not silence notices.** A stop is when the owner most needs to be told.
- **Schema 11 is additive:** two new tables (`notices`, `watch_state`), no existing table or row
  touched, tested against real schema-9 and schema-10 files. Back up before upgrading, as for any schema step.

## Not built

- **No network sink.** Delivery to ntfy (a topic that is a credential, and an outward send) is the
  owner's to authorise, as 0148 made the house watch's topic. A future sink is a new implementation
  of `NoticeSink` in the composition root, behind that authorisation. The decide path keeps refusing
  outward calls: a notice sink is not a capability and does not widen what an agent can do.
- No notice for "routine paused", "decision waiting" or "repair taken" yet; each is another
  `NoticeState` with its own rule when it is wanted.

## Consequences

- The owner can see a failing watch from `notices` or a file without reading `status`.
- A routine that paused while failing stays *said* until the owner resumes it and a check passes;
  `recovered` is therefore never said for a watch that is still down.
- The outbox grows by at most two rows per outage; nothing prunes it yet (the owner's `ack` only
  hides a row), which is fine at this rate.
- The notice line carries the routine's title, which the owner wrote; control characters and quotes
  are replaced and the name is cut at 80 characters, so a title cannot forge a second line.
