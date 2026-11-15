Status: CURRENT · consolidates the boundary and spend statements; details stay in docs/foundation/

# Trust and spend

Two questions people mix up: *what stops an untrusted program from doing harm?* and *what stops spend?*
Each has more than one answer, and the answers are of different kinds.

## What is the trust boundary

**Process and filesystem isolation of untrusted executors, and nothing else.** An agent, or a script an
agent wrote, runs where it cannot read the owner token or open the database: macOS `sandbox-exec`, or one
Docker container per call on Linux. Verified per target, see the table in
[TRUST-BOUNDARY.md](foundation/TRUST-BOUNDARY.md). A same-user process that is **not** confined can read
the token and edit the file: documented, not fixed.

Three things that sound like boundaries and are not:

| Thing | What it is |
|---|---|
| A capability's **declaration** (band, tier, maximum cost, facts, budget mode) | policy metadata. Core enforces policy from it; a capability that lies defeats the policy, not the isolation. Read it when you add a capability. |
| `OwnerProof`, actor kinds, the owner credential, the database triggers | defence in depth against mistakes by trusted code. Code in Core's own process is trusted. |
| In HQ-era documents, "the node is the trust boundary" | LEGACY: the node was HQ's authority. It is not the foundation's boundary. |

## Three different things called "enforced spend"

All amounts are micro-USD (1 USD = 1,000,000).

1. **Declared enforcement: a capability's claim.** A call's spec says `Enforced { units }` or
   `Unenforced`. Core checks that the call's maximum is at or under `units` and passes the granted
   budget; **Core does not verify the claim.** Today `git.change` declares `Enforced` (units 500,000)
   only when the config has a `claude-code` line; with the default scripted executor it declares
   `Unenforced` and a zero maximum. `probe.run` has a zero maximum.
2. **Measured limits.** *Turns:* the capability passes `--max-turns` to the Claude Code CLI, which does the stopping; Core
   grants turns and does not check them. In the one real run that tested it, `--max-turns 1` ended at
   `num_turns=2` (one turn over). *Wall-clock:* the capability kills the
   process group at the deadline; tested with fake CLIs and scripted agents only, not a real hang.
3. **Unverified.** *The dollar cap:* `--max-budget-usd` is passed to the CLI, but no real run reached
   it, so what happens at the cap is unknown.

What actually holds for dollars today is Core's own mechanism, which does not depend on the claim:

- **Before** a call, Core reserves the call's declared maximum against the goal's cap and the overall
  cap, in the same transaction that writes the intent (`BEGIN IMMEDIATE`), so two processes on one file
  cannot both pass.
- **After** a call, the cost the capability reports is clamped to the declared maximum; a larger report
  is a violation row, the item fails and the goal stops until `resume-goal`.
- Core **trusts the reported cost**. A failed call that reports 0 is charged 0; an unreadable report or a
  kill is charged the maximum. A zero cap refuses every call whose maximum is above zero.

So the cap is a hard limit on what Core *lets start* and *records*; it is not a limit on what an
unverified executor may actually spend. Detail and evidence: STABILITY.md row "Spend enforcement".

## Evidence is detection, not proof

Evidence rows are append-only through the engine API and on connections that carry the triggers. The
hash chain (`verify-evidence`) detects an edited covered row; it cannot see removal of the last rows,
a re-forged chain, or any other table. Keep the printed head hash elsewhere.
