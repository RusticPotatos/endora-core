# 0151 - A repair policy for a failed watch

## Status

Proposed. AI-assisted (Claude). Builds on [0150](0150-current-architecture-the-foundation.md); changes none
of its retained principles.

## Context

A watch (a routine whose criterion is a probe) that fails ends its item as failed, counts a failure and, at
the cap, pauses the routine. Nothing else happens: the Foundation has taken no action on its own since it
started. For an everyday computer the first useful step is the smallest one: when a probe fails, try the one
fix the owner already knows, once, and look again. The rule that a model never authorizes stays: the action
is a call like any other and goes through `decide`.

## Decision

A **repair** is a new, small concept: *the one reversible call a goal's failed watch may make*.

- **Declared by the owner.** `Core::add_repair` (owner proof required; command: `repair add --goal ID
  --action NAME`) stores one repair per goal (`repairs` table, schema 10) and, in the same step, the owner's
  policy row: the actor `repair` may make that call at most once per item. Nothing else creates the row.
- **Refused at registration.** The call must be registered, in the goal's scope, taking the given input, and
  declared in the **reversible** band. An observe call (it changes nothing, and it would let policy accept the
  item), an outward-reversible call, an irreversible or unclassified call, a second repair for the goal and an
  unknown call are all refused, and nothing is stored. The command line's `action <name> reversible <command>`
  config line refuses any other declaration when the config loads, so an irreversible action never registers.
- **Decided, never authorized by a model.** When a check fails, `check_watch` makes the repair as an ordinary
  call, `Core::call`, as the actor `repair`. The existing `decide` answers: the stop switch, the goal's scope,
  the policy row, its band and tier, its cap and the spend caps all apply, and an outward call is still
  refused. No row, the stop switch on, or a used cap: nothing runs and the refusal is recorded.
- **One use per failure episode.** An episode is the routine's run of failed items, ended by a healthy check.
  The uses of the repair on this item and on the earlier failed items of the run count against the row's cap
  (`Store::episode_uses`), so a repair that did not help is not tried again at the next slot.
- **Re-check, then the owner.** If the call ran, the failed check runs again. Met: the item stays running as
  `WatchEnd::Repaired`. Policy accepts only observe-band work, so the item waits for the owner, and the
  routine's next slot is held behind it even for an `auto-observe` goal (`routine_has_unfinished_item`),
  because otherwise the next failure would open a new episode and repair again before the owner has looked.
  Not met: the item fails as before.
- **Recorded.** The call's intent and result (or its refusal) are evidence rows on the hash chain, by the actor
  `repair`; the re-check is another probe call; one `core.repair` control row says whether the check then
  passed; `repair add` writes a control row signed by the owner.

The pure rule (what may be a repair) is `domain::repair`; the rest is `Core` and the store.

## Consequences

- Behaviour changes only for a goal that has a repair. The binary can ship with none: no live repair exists
  until the owner writes an `action` line, a goal scope that names `action.run`, and runs `repair add`.
- The schema moves to 10 (a new table, no old row changes). An older build refuses a version-10 file, so a
  rollback restores the backup taken before the upgrade.
- A paused or retired routine (the owner's word, routine pause/retire) never triggers a repair: an item it
  opened before the pause fails as it always did. A routine whose item is held after a repair still respects
  pause: it fires nothing until the owner resumes it.
- An action that needs host privileges or the Docker socket is a security change and is the owner's decision,
  outside this record. The action runs like a command probe, with a scrubbed environment and no input from
  the failing probe.
- Not decided here: a repair with more than one step, a repair that reaches outward (it needs per-call owner
  approval, which does not exist), and a repair chosen from a probe's output (a model never chooses).
