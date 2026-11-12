# 0149 - The owner accepts work that nobody reviews

## Status

Accepted, 2026-11-12, by the owner's ruling on an independent design study of `main`
(34cb2cf). AI-assisted (Claude). Amends [0124](0124-guards-against-how-agent-teams-fail.md) and
[0142](0142-one-team-of-loops.md); builds on [0132](0132-outcomes-and-a-desk-that-interrupts-only-for-decisions.md)
(the desk) and [0145](0145-core-console-contract.md) (the Console contract).

## Context

A task is `done` because its criteria were satisfied **and** an authorized review or acceptance
decision occurred. Two paths closed work with no such decision:

- A loop worked by one member alone (`solo`) had no reviewer, so `review_asked` called
  `done_alone`: HQ closed the item itself, as `met_criteria` when a gate had passed, or
  `completed_unverified` with no branch (ADR 0124, amended 2026-11-03). Nobody accepted anything.
- `reviewer_among` never picks an equal-rank member, so an all-local team has no reviewer. On a
  non-solo project the engine posted a `ready_to_read` card with only `read` and `not_now`: the
  owner could not decide the work, and the stranded-work scan was the only thing that moved it.

`rejected` existed only as a reviewer's send-back; `accepted` did not exist.

Three candidates were weighed:

- **A. Owner acceptance on the existing card** (chosen). Two choices, `accept` and `send_back`,
  on the existing `ready_to_read` card, through the existing `POST /v1/decisions/{id}`; a new
  closure reason; a per-project policy that says whether HQ may close a solo item itself.
- **B. Let an equal-rank member review** (rejected). Changing `reviewer_among` so a same-rank
  member reviews would make a self-review a "review": a reviewer from the same model class, with
  the same blind spots and often the same sandbox, is not a legitimate review, and it would
  manufacture the approval the rule forbids.
- **C. A new route or approval subsystem** (rejected). The desk, its card, the decision route and
  the send-back path already model "a decision on work in review". A second mechanism would be a
  second place for the gate, the audit and the Console to get wrong.

## Decision

**A**, the smallest.

1. **Two choices on the `ready_to_read` card of work in review**, on the existing route (body
   unchanged; the existing optional note, `words` on `/v1/decisions/{id}`, `answer` on HQ's desk
   route, is posted to the thread):
   - `accept` closes the task `done` with the new closure reason `accepted_by_owner`, recording
     `by = "owner"` and when, as other closures do. It refuses, with no state change, when the
     task is no longer `in_review`, and when a gate applies and has not passed, by the same rule
     a reviewer's approval is held to (`work::gate_passed_for_done`, shared with `work::is_done`).
   - `send_back` moves the task from `in_review` to `doing`, posts the note, emits the feed's
     `task.sent_back` (`via: "owner"`) and wakes the holder with `WakeReason::SentBack`, as a
     reviewer's send-back does. It refuses a stale card the same way.
   - A card decided once is decided once: a second `accept` is `409` (`this task is over`).
2. **A per-project acceptance policy**, `acceptance`, set on project creation next to `solo`:
   - `owner` (the default, and what every loop made before this reads as): a solo item set ready
     stays `in_review`, and the engine posts the same card. HQ never closes it.
   - `unverified`: the owner's explicit, documented opt-out. `done_alone` behaves exactly as
     before and records `completed_unverified`, which is **never shown or counted as accepted**.
   One card at a time: a card already waiting for a task is not put up again (the stranded
   scan's nudge asks a review again).
3. **Stranded-work cleanup never closes a task.** An `in_review` item the scan finds is asked
   again at most once, and a waiting card is not duplicated; the scan sets no `done` and no
   `dropped` (scenario "Cleaning up stranded work never closes it").
4. **Storage.** Like `completed_unverified`, an acceptance is kept in its own sibling table,
   `owner_acceptances (task, by, at_ms)`, not in `closures`, so a build from before this reads
   every closure and finds none for that item rather than a reason it cannot read. The policy is
   a setting beside `loop_solo` (`loop_acceptance:<project>`), absent for existing loops, which
   read as `owner`: there is no column to migrate, and closed tasks that are
   `completed_unverified` stay valid history.

Four outcomes stay apart in tests and in what the owner reads: **completed** (reached
`in_review`; not done by itself under `owner`), **accepted** (`accepted_by_owner`), **rejected**
(sent back to `doing`) and **abandoned** (`dropped`, with an existing reason).

## Consequences

- **The `owner` default changes solo behaviour.** A loop worked alone no longer closes itself:
  its items wait on the owner's desk. A solo loop that should keep closing itself, such as the
  round-4 experiment arm, is made with `"acceptance": "unverified"`. Its briefing says which.
- **The unverified opt-out is explicit and honest.** It records unverified completion and is
  never displayed or counted as accepted (Live Work says "Done, not verified"; an acceptance is
  "Done, accepted by the owner").
- **No new subsystem, no new route, no new desk kind.** The Console contract gains choices, a
  closure reason and two project fields, additively within v1.
- **History is unchanged.** Existing closures read as they did; tasks closed `completed_unverified`
  stay so. A rollback to an older build reads the data, and finds no closing for an accepted item.
- **The self-review is rejected on purpose.** `reviewer_among` is unchanged.
- **Gates, privacy and release gates are untouched.** Accepting gated work still needs its gate
  to have passed; the push to the fork is still offered, and still the owner's to approve.
- The standalone Console (separate repository) must offer the two choices and show the reason;
  see the finding in the change's report. It is not edited here.
