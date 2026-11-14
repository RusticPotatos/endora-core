# Trust boundary (slice 7, 14 Nov 2026)

Honest statement of what Core's security rests on. Nothing here is frozen.

## D1. The boundary: isolation of untrusted executors

Untrusted executors (an agent, a script an agent wrote) must not be able to read Core's state or
the owner's credentials. That is done by **process and filesystem isolation**, not by Core's code.
It is verified per target environment: on macOS with `agent_attacks.rs` (macOS only, it never ran on Linux), on Linux with the `docker.rs` attack table:

| Target | Status |
|---|---|
| macOS `sandbox-exec` profile | tested: token unreadable, database unopenable, no network |
| Linux / Docker backend (Linux test host) | tested with the `docker.rs` attack table (secrets, writes outside the job directory, network, no Docker socket, mount and ptrace, fork and memory limits, timeout, hard-kill reaping); the container runs with `--network none` (a hardened container on the default bridge can reach the host's own services and the LAN) |
| NAS | Docker backend tested with the same `docker.rs` attack table, plus hand-run checks and a control container on the default bridge; the real backend runs with `--network none` (see STABILITY.md) |

A same-user process that is **not** confined can read the owner token file and edit the database
file. That is documented, not fixed; no application-level compensation is planned.

## D2. What the application-level checks are

They are **defence in depth against mistakes by trusted code**, not a boundary. Known limits:

- `OwnerProof` is not bound to a database or a process (finding 2): a proof minted by another
  in-process Core was accepted. Any code in the Core process is trusted code.
- `Core::fail(&Actor, item)` accepts any actor (finding 10): a library caller can fail an item that
  awaits the owner. The command line does not expose it to agents.
- The engine does not detect dropped append-only triggers when it opens a file (finding 4).
- A credential-less file can be enrolled by whoever runs `init` first (finding 7, setup context).
  `init` now exits 2 when a credential exists. Any command creates the database file before `init`.
- The macOS sandbox profile starts from `allow default`. Signals to other processes were open
  (finding 9); both macOS profiles now carry `(deny signal (target others))`, tested. Exec and mach
  lookups stay open.
- Migrations are part of the trusted setup. They refuse a file whose evidence rows already carry
  hashes under an older `user_version` (finding 3), but whoever can run them can edit the file.

## D3. Evidence

The hash chain is **supplemental tamper detection for covered rows**, not proof of whole-database
integrity. Covered: every evidence row's content and its link to the row before. Not covered:
removal of the last rows, a chain re-forged by a writer who knows the format, and every other table
(`items`, `goals`, `policy`, `control`, `owner_credential`). Keep the head hash off the box.

## D4. Spend

- No shipped capability declares `Enforced`. Core **trusts the cost a capability reports**, clamped
  to the call's declared maximum; a larger report writes a `core.violation` row and stops the goal.
- A failed call that reports a cost of 0 is charged 0 (an unknown cost is charged the maximum).
- Check and intent are one `BEGIN IMMEDIATE` transaction: the reserved maximum counts against both
  caps until the actual cost replaces it, so two processes cannot both pass (`spend_race.rs`).
- `Enforced` means something only when an executor really limits its own spend; a real-executor
  test is under way.

## D5. Reading the rest

Interface ratings are in [STABILITY.md](STABILITY.md). Where this file and the code disagree, the
code and its tests win; fix this file.
