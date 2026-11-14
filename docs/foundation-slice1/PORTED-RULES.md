# Ported rules (foundation slice 1)

The old team crate's rules were read as **specs**; the code and tests here are fresh. Sources are
`domains/team/src/domain/<file>:<line>` at Core commit e9c7923, plus ADR 0149. Tests are in
`domains/foundation` (unit tests in the file named, integration tests in `tests/core_rules.rs`)
or `domains/foundation-observe/tests/slice1.rs`.

## Ported

| Rule | Source | Test |
|---|---|---|
| Work handed out says what done means: at least one criterion, at most 8, none blank | `evidence.rs:28` (`MOST_CRITERIA`), `evidence.rs:302` (`owner_may_hand_out`) | `hold::tests::a_goal_needs_what_why_and_one_to_eight_nonblank_criteria` |
| An item is done only when its criteria are met AND an authorised acceptance is recorded; nothing closes itself | ADR 0149 (decision, 1), `closure.rs:50` (`AcceptedByOwner`), `closure.rs:55` (`Acceptance::Owner` is the default) | `core_rules::nothing_closes_itself_criteria_met_waits_for_an_authorised_acceptance` |
| Whoever closes gated work, the gate (here: every criterion) must have passed | `work.rs:823` (`gate_passed_for_done`), ADR 0149 | `accept::tests::unmet_criteria_block_every_acceptance` |
| Saying "done" changes nothing: only an authorised actor accepts; anything that proposes work never does | `work.rs:802` (doc of `is_done`), ADR 0149 ("no member and no rule of HQ's can give" an acceptance) | `accept::tests::nobody_named_accepts_anything` |
| Accepting an item that is not in the accepting state is refused with no change; a done item is not done twice | ADR 0149 (refuses when no longer `in_review`; `done` to `done`), `work.rs:763` | `accept::tests::only_a_running_item_is_accepted_and_a_done_one_not_twice` |
| A routine has at most one open item: a firing while it is open is skipped, not stacked | `routines.rs:96-99` (`step`) | `hold::tests::a_routine_with_an_open_item_skips_its_firing`, `core_rules::firings_missed_while_down_are_recorded_and_run_once` (last part) |
| Each firing's work carries its own criteria | `routines.rs:109-125` (`criteria_for`) | `slice1::t1_a_standing_goal_with_a_routine_fires_twice_...` (each item has its own criteria rows, met separately) |
| Closing states are final; the transition table refuses the rest | `work.rs:763` (`may_set_status`), `work.rs:792` | `hold::tests::done_is_reached_from_running_only_and_final_states_stay_final` |
| Evidence is the node's record, never a model's report; a met criterion cites a record that is in it | `evidence.rs:4-6`, `evidence.rs:505` (`judge`) | `slice1::t1_...` (`met_evidence_id` set for every check); evidence is written only inside `Core` (see report: writers are crate-private) |
| A live criterion is shown by a fresh reading, not a remembered one | `evidence.rs:11-15` | `core_rules::nothing_closes_itself_...` (`verify` makes the check call at verify time) |
| Default deny on authority; irreversible and unclassified are blocked | kernel `reversibility.rs` (reused), ADR 0051 | `decide::tests::*`, `core_rules::default_deny_then_a_grant_allows_...` |

## Deliberately reversed or reshaped

| Old rule | Source | What the slice does instead, and why |
|---|---|---|
| Missed firings are skipped, never caught up | `routines.rs:131-134` (`next_due`), test at `routines.rs:296` | **Reversed by the brief:** missed slots are recorded (`firings.outcome = missed_down`) and the latest runs once. Tests: `hold::tests::firings_missed_while_down_...`, `core_rules::firings_missed_while_down_...` |
| `Acceptance::Unverified`: HQ may close a solo item itself as `completed_unverified` | `closure.rs:55-70`, ADR 0149 item 2 | **Not ported.** The brief says nothing closes itself. Replaced by a policy-authorised automatic accept that is *only* for observe-band routine items whose every criterion a deterministic check settled (`accept::tests::policy_accepts_...`, `policy_never_accepts_...`). It is recorded as `policy:auto_accept`, never as the owner. |
| A repeated accept answers the first decision (idempotent replay) | ADR 0149 item 1 (third bullet) | Not ported: there is no desk card or request id in slice 1. A second accept is a recorded refusal (`NotRunning(Done)`). Needs a surface to be meaningful. |
| `dropped` status with reasons (`obsolete`, `duplicate`, `wont_do`, `blocked_forever`), `superseded` needing a successor | `closure.rs:20` (`may_close`), `closure.rs:100-165` | Not ported: the brief fixes five states (open, running, done, blocked, failed). `dropped` folds into `failed` with a free-text reason. |
| Rhythm bounds: 15 minutes to 30 days | `routines.rs:138-146` (`may_keep`) | Not ported: slice 1 only requires a positive rhythm so tests can run in milliseconds. Re-add as policy when a real routine exists. |

## Not ported, and why

| Rule | Source | Why |
|---|---|---|
| Duplicate-ask suppression (`same_ask`, `fingerprint`, `placing`) | `guards.rs:66`, `guards.rs:95`, `desk.rs:48` | No desk and no ask-the-owner verb in slice 1. Expressible without "member" (it is about tasks and text); candidate for the slice that adds a question verb. |
| A request for the owner's yes must serve a numbered criterion (`serves`, `approval_serves`) | `guards.rs:223`, `guards.rs:259` | Same: no ask-the-owner verb yet. Expressible without "member"; port with the verb. |
| Desk digest vs interrupt kinds (`interrupts`, `DIGEST`) | `desk.rs:12` | No desk. |
| `criteria_hash`: a tester's pass counts only while the criteria still hash the same | `evidence.rs:265` | No tester. The property holds differently: an item's criteria rows have no update path. |
| Backoff of empty wakes (`back_off`) | `routines.rs:270` | No wakes or turns. |
| Sandbox-reported receipts that cannot satisfy a live criterion | `evidence.rs:6-10` (ADR 0117) | No sandbox reports in slice 1. |

## Needs D3 reversal? (cannot be said without "member"/"project"/"role")

| Rule | Source | Note |
|---|---|---|
| The reviewer is never the worker (`OwnWork`) | `work.rs:807-815` (`is_done`) | Stated over members. The slice's stricter form (only the owner or the standing policy ever accepts; a proposer never does) covers its intent, but a *peer review* rule cannot be expressed without a second actor kind. Needs D3 reversal if peer review is wanted back. |
| Stopped-work diagnosis: `Working`, `Parked` (member idle, still owed), `Held`, `Indeterminate` | `guards.rs:349` (`diagnose`), `guards.rs:412` (`nudge_key`) | Defined over "its member is idle". The slice's `recover()` handles a dead process only; it does not detect work that sits because nobody picks it up. Needs a runner concept first. |
| Owner send-back refused when the task's *project* is paused (`ProjectPaused`) | ADR 0149 item 1 (second bullet) | Needs "project" and a send-back out of an `in_review` state the brief removes. |
| Guards on a member woken again and again for nothing; claims copied from note to note | `guards.rs:1-12` | About members and notes; no equivalent surface here. |

## New in the slice (no old counterpart)

- Write-ahead evidence: an allowed call records an `intent` before it runs and a `result` after;
  recovery fails an item whose reversible call has an intent and no result, explicitly
  (`slice1::t3b_killed_inside_a_reversible_call_the_item_fails_explicitly_not_silently`).
- A unique key on (routine, slot) makes a double firing impossible even across a crash.
- Database triggers refuse any UPDATE or DELETE on `evidence` (`core_rules::t4_...`).
