Status: CURRENT · interface table; checked by domains/foundation/tests/stability_doc.rs

# Foundation interface stability

**Frozen by the maintainer on 14 Nov 2026 (the freeze commit is in the project's private history; the public history has one squashed commit) after: two independent authority reviews, a real
executor (3 runs, macOS), the Docker backend attacked on Linux (Linux test host), the integrated gate (2,588/0
workspace; 199 Linux).** The EXPERIMENTAL rows are unchanged and can still change. **Changed on 15 Nov 2026 (maintainer decision), after two weekly independent reviews:** (1) the macOS scripted-profile row, FROZEN until then, is **unfrozen** after review B found a sandbox escape (see the row, the "Known limits of the macOS backend" list and the reversal rule); (2) a new row, "Recovery of interrupted work", is rated EXPERIMENTAL: the freeze table had no recovery row (the README had rated crash recovery CURRENT), and a review found that recovery had no liveness check (a second `run-due`, a `serve` tick or an overlapping cron run recovered an item another live process was running). Both rows stay EXPERIMENTAL until the fixes have had a new independent review. **Also decided on 15 Nov 2026 (maintainer), for the FROZEN "CLI commands" row (no status change):** unknown options and extra words are now refused (exit 1) instead of silently ignored: a deliberate hardening decided by the maintainer on 15 Nov 2026 because a silently ignored typo (for example in `--token-file`) is a safety hazard; no documented use relied on ignored arguments; listed in the CHANGELOG. It is not additive, and the row records it here instead of pretending it is.

Statuses of an interface are only these two (a page carries a different set of labels: CURRENT,
EXPERIMENTAL, LEGACY, UNSUPPORTED, PLANNED, see [docs/README.md](../README.md); "review finding N" refers
to independent reviews held in the project's private history):

- **FROZEN**: additive changes only (new optional fields, new commands). Any breaking change needs a
  recorded decision and a schema or version bump. **Reversal rule:** a demonstrated gap from a NAS
  run, a real run with the new units, or a new independent review unfreezes the row.
- **EXPERIMENTAL**: new, covered only by its own tests, or exercised only by scripted stand-ins, or an
  open finding or a known limit says it may change.

A test (`domains/foundation/tests/stability_doc.rs`) fails if a row has any other status.

### Known limits at freeze
- Recovery of interrupted work knows whether a process is alive only through a lock file beside the database
  (`<db>.holders/`): it assumes every process that writes the database runs on **one host and one local file
  system** and that the directory is not deleted while a process lives. Verified with real processes on macOS
  (the two-process tests, the overlapping-run and kill storms) and, for the lease as first built, in one Linux
  container (aarch64, Docker Desktop's VM; four of six two-process tests passed, the two git-task tests were
  skipped there: no confinement backend; the later recovery-attempt and overlapping-run tests were not run there). Not run: the NAS, a Linux host with
  the Docker backend. The lease does not hold, and a live holder's items are recovered as if it had died, when: a host process and a container share one database file through a Docker Desktop bind mount (the lock does not cross the VM; reproduced by an independent review); a single holder file is deleted or replaced while its holder lives; or the same database is opened through a hard link or a file-only bind mount (each path gets its own `.holders` directory). See TRUST-BOUNDARY.md D4b.
- The NAS is verified for the Docker backend (see Verified environments below); the isolation relies on
  `--network none`: a hardened container on the default bridge can reach the host's own services and the
  LAN, so a real agent that needs network needs its own egress design.
- The dollar cap is unproven (no real run reached `--max-budget-usd`).
- The real executor has not been re-run with micro-USD units.
- Rootless Docker is untested.
- A real agent under the Docker backend is not supported: job variables, a credential among them, go
  through an env file in Core's scratch area (a per-call file under `<scratch>/.meta/.logs/`, not mounted into the
  container), removed only when the call ends normally: a Core killed outright leaves it, credential
  included, because nothing at start-up cleans it (README "Credentials"). The container has no
  network. It needs its own egress and credential design.
- A same-user, unconfined process can read the owner token and edit the database (the stated boundary).

### Known limits of the macOS backend (15 Nov 2026)
- A child that calls `setsid` leaves the agent's process group, so the group kill at the end of a call
  and at the deadline does not reach it. It stays confined to the job directory, but it outlives the call.
- Killing Core outright does not stop the agent (no die-with-parent on macOS).
- No limit on disk written in the job directory and none on the number of processes (the per-user
  process table is shared with the owner's session). The Docker backend has pids, memory and cpu limits.
- Preference and pasteboard access is denied for both profiles; a real Claude Code CLI under these
  stricter rules is not verified.
- The database, its sidecars and the token are denied by their canonical path, in both spellings of the Data volume, in both profiles, as the last
  rule (refused at start if one lies inside the job directory: a deny cannot hold against a rename there) (the scripted profile took no deny list before); `/usr/local/var`, `/usr/local/etc`, `/opt/homebrew/var`, `/opt/homebrew/etc`
  and `/Library/Application Support` are denied whole and `/opt/homebrew` is readable only for `bin`,
  `lib`, `opt`, `Cellar`, `share`. The full list is in TRUST-BOUNDARY.md D1.
- Open route: `launchctl print` shows other launchd jobs' environment variables (no deny closed it; TRUST-BOUNDARY.md D2).
- Open findings for a later fix: a symlink the agent makes to an absolute path travels in the patch
  (captured as a link, never followed).

### Verified environments
- macOS: the scripted sandbox profile passed the attack tests on the Mac, but an independent review (15 Nov 2026) found an escape; the row is EXPERIMENTAL again until the fix is independently reviewed. The real-agent profile
  stays EXPERIMENTAL: three real runs, but the agent's own tools never exercised the profile, and the stricter denials were not tried with a real CLI.
- Docker backend on the Linux test host (Ubuntu 26.04): the foundation crates gave 199 passed, 0 failed,
  0 skipped on four runs, and the `docker.rs` attack table held.
- Docker backend on a NAS (Ubuntu 24.04, rootful Docker, cgroup v2, AppArmor `docker-default` and
  seccomp enforcing): 200 passed, 0 failed, 0 skipped on three runs (the one extra test is
  `stability_doc`), and the `docker.rs` attack table held, with hand-run checks and a bridged control
  container (reached host services and the LAN: why `--network none` matters).

Interfaces are documented frozen or experimental here and nowhere else. "What would change it" is the
evidence that should move a row.

| Interface | Status | Evidence | What would change it |
|---|---|---|---|
| Work item and goal shapes (title, why, criteria, scope, tier, cap in micro-USD, subject) | FROZEN | `criteria.rs`, `core_rules.rs`, `contract_shapes.rs`, `micro_usd.rs`; both capabilities' tests; three real runs (an item run by the real executor, accepted by the owner) | `{subject}` is not validated at goal creation; a job needing per-item budgets or deadlines |
| Evidence record shape (item, call, actor, actor kind, band, decision, kind, intent, inputs hash, output, time, cost in micro-USD, profile) | FROZEN | `core_rules.rs` t4/t4b/t5, `typed_actor.rs`, `spend.rs`, `micro_usd.rs` (profile and unit); two independent reviews attacked the record | Structured (non-text) output stored beside the text; a second structured field |
| Evidence hash chain (supplemental tamper detection for covered rows, not whole-database integrity; `prev_hash`, `hash`, canonical row text, `verify-evidence`) | EXPERIMENTAL | `evidence_chain.rs`, `micro_usd.rs` (an old-unit chain keeps its head across the migration; a new row's profile is hashed), CLI `verify_evidence_...`; review findings 3 and 4 (re-chaining by migration fixed, dropped triggers not detected) | Anchoring the head off-box, chaining the other tables, noticing dropped triggers, a second verifier. Two canonical texts exist (rows with and without `cost_micros`); a change to either breaks chains |
| Capability call spec (name, band, tier, max cost in micro-USD, max seconds, max turns, typed inputs, declared facts, budget mode) and `Budget` (`max_cost_micros`, `max_seconds`, `max_turns`) | EXPERIMENTAL | `contract_shapes.rs`, `call_routing.rs`, `budget.rs`, `micro_usd.rs` (granted values reach the capability), `slice6.rs` (the real executor builds its flags from them); the unit and the seconds/turns fields came from the real runs | Core grants the spec's maximum: there is no per-call request and no shared pool of seconds or turns. A second real executor, or a request channel, may change it |
| Spend enforcement | EXPERIMENTAL | Cost: Core checks and reserves the maximum in one transaction (`spend_race.rs`), charges at most the maximum (`spend.rs`); **turns**: Core grants them and does not check them; the capability passes `--max-turns` to the CLI, which does the stopping; one real run: `--max-turns 1` stopped at 2, one turn over; **wall-clock**: the capability's process-group kill at the deadline, tested with fake CLIs and scripted agents only (on macOS a child that calls `setsid` leaves the group and survives it); **dollars**: `--max-budget-usd` is accepted by the CLI, but no real run reached the cap, so the dollar cap is **unproven** | A real run that reaches the dollar cap; a real hang killed at the deadline; any executor that enforces its own budget |
| Owner decision methods and commands (`accept`, `decline`, `confirm-criterion`, `resume`, `resume-goal`, `routine resume`, `grant`, `overall-cap`, goal and routine creation) | EXPERIMENTAL (defence in depth, not a boundary: `OwnerProof` is not bound to a database or process; see TRUST-BOUNDARY.md) | `owner_proof.rs`, `cli.rs` owner tests, `agent_attacks.rs`, `compile_fail` doctests on `Core::accept`; review finding 2; a real run where the owner accepted | `confirm` (deprecated alias) is removed next release; **`approve-call` does not exist** (additive: a per-call owner approval for outward-reversible calls) |
| Actor kinds, reserved names, `OwnerProof` and the owner credential | EXPERIMENTAL (defence in depth; `Core::fail` takes any actor; enrolment is a trusted setup step; `init` exits 2 when a credential exists) | `typed_actor.rs`, `owner_proof.rs`, `agent_attacks.rs`; unit tests in `domain/actor.rs`; review findings 2, 7, 10 | Token rotation (the engine refuses to replace a credential), a Keychain-backed token, a second owner |
| Routines and the missed-slot rule (1 s to 366 d, one summary row, pause at a failure cap, `routine resume`) | FROZEN | `watch_bounds.rs`, `core_rules.rs`, `cli.rs`, `spend_race.rs` (a slot never fires twice under 6 connections) | A real multi-process runner pair; a losing process may get an error rather than skip quietly |
| Acceptance policy (`owner`, `auto-observe`, routine `auto_accept`; policy never above observe) | FROZEN | `auto_observe.rs`, `core_rules.rs`, unit tests in `domain/accept.rs`; a git task is never auto-accepted (CLI test) | A capability whose result legitimately needs owner review at the observe band |
| Schema and forward migrations (`user_version`, eight ordered steps, newer file refused, a file with hashes under a version before the chain refused, the cents-to-micro-USD step, the nullable `items.holder` column of step 8) | FROZEN, with the note that migrations are part of the trusted setup context; step 8 is additive (one nullable column, old rows read as before) | `schema_version.rs` (real older schemas as fixtures), `micro_usd.rs` (a database holding cents), `evidence_chain.rs` (review finding 3 attacks), `spend.rs`, `domains/foundation/tests/recover_liveness.rs` (a file in the version 7 shape migrated: chain head and row count unchanged) | n/a for the forward path |
| Migration rollback and downgrade | EXPERIMENTAL | A failed step rolls back in code, untested; no downgrade path | A migration that can fail on real data |
| Concurrency (several processes on one file) | EXPERIMENTAL: one runner is still the design; a second `run-due`, a `serve` tick or a `git-task` beside it is safe for recovery (next row), and nothing more is claimed | `spend_race.rs` (spend caps, double firing: 6 connections, threads not processes), `watch_bounds.rs`, `stop` retry; review finding 1; two real processes in `app/foundation/tests/recover_liveness.rs` (macOS) | Races on the other `items` transitions and on the chain are untested with processes; a Linux, Docker or NAS run of the two-process tests |
| Recovery of interrupted work (`Core::recover`, `recover_interrupted`, `stuck_watch_items`, `settle_stranded_auto_observe`, the start-up of `run-due`, the holder lock files and the `items.holder` column) | EXPERIMENTAL: a new row on 15 Nov 2026 (the freeze table had none; the README had said CURRENT) after weekly review B found a liveness defect; re-freeze only after a new independent review of the fixes (recovery attempts and overlapping runs, 15 Nov 2026) | `domains/foundation/tests/recover_liveness.rs` (live holder left alone, dead holder recovered, pid reuse, missing or malformed holder name, a version 7 file), `app/foundation/tests/recover_liveness.rs` (two real processes: observe band, a git task, a `serve` tick, `kill -9`; an item recovered once that met its criteria stays waiting over further `run-due` passes and `serve` ticks; three kills still fail the item; six overlapping `run-due` over six open items; a storm with random `kill -9`), `domains/foundation/tests/recover_attempts.rs` (current-attempt rule, three attempts, schema 7 and 8 files with a stuck item, the lost start race), `recover_interrupted.rs`, `auto_observe.rs` (stranded item); the behaviour with no holder is the old one | A new independent review of the fix; a Linux, Docker and NAS run; a design for a database shared across a VM boundary (known to fail today); a process that never dies but stops working (a hung holder keeps its items) |
| CLI commands (`goal`, `routine`, `git-task`, `run-due`, `serve`, `status`, `init`, `accept`, `decline`, `confirm-criterion`, `stop`, `resume`, `resume-goal`, `overall-cap`, `evidence`, `verify-evidence`, `backup`, `capabilities`, `publish`) | FROZEN | `cli.rs` (process tests, including units and the printed profile), `agent_attacks.rs` | Output text is not a contract; only commands, flags, exit codes (0 ok, 1 error, 2 refused) and the unit (micro-USD) are |
| Git capability and its limits (`Config`, `Options`: protected paths, env allow-list, `trust_exit_code`, limits, `executor`, `claude_limits`, `credential_file`, `deny_paths`, `executor_image`) | EXPERIMENTAL | `slice5b.rs` (19 tests, scripted agents; 10 are named `attack_`), `slice6.rs` (fake CLI), three real runs; a second independent review. Limits: `git.publish` is always refused; the real executor needs outbound HTTPS and a credential (see README "Real executors"); `Options::read_dirs` is refused under Docker | A real agent on a larger task; a second real executor; the credential review |
| Confinement profile: macOS scripted agent (`sandbox-exec`) | EXPERIMENTAL (unfrozen 15 Nov 2026) | Tested on the Mac: `slice5b.rs`, `agent_attacks.rs` (token unreadable, database unopenable), the signal tests (finding 9 fixed: no signal to the engine or pid 1, own children and the group kill still work), and `macos_profile_hardening.rs` (which also checks that `<db>.holders/` is denied by name). **Why unfrozen:** weekly independent review B found a sandbox escape: a confined process could ask LaunchServices to start an unconfined helper, which ran as the owner outside the sandbox. The profile now denies those operations and preference and pasteboard access, with a test for each; **the fix is awaiting a new independent review**. The profile is `(allow default)` with denials, so exec, every mach lookup not named and any later allowed operation stay potential routes; an allow-list rebuild is the longer-term fix. Apple marks `sandbox-exec` deprecated | Re-freeze only after the fix and a new independent review; a deny-default rebuild; a macOS release that drops `sandbox-exec` |
| Confinement profile: macOS real-agent | EXPERIMENTAL | Three real runs; fake-CLI tests in `slice6.rs`. The agent's own tools were stopped by the CLI's permission layer before the profile was exercised; the CLI itself was shown confined (`EPERM` on a read of a canary). Outbound port 443 to any host stays open. The LaunchServices, Apple event, preference and pasteboard denials apply to this profile too (it derives from the scripted one) and are tested with scripted agents only; **not verified with a real CLI under the stricter rules** (no real run since the change; only `node -e` and `claude --version`, no network and no API call, were started under them) | A real agent that tries to leave the job; a host allow-list |
| Confinement profile: Linux `bwrap` | EXPERIMENTAL | NOT usable on the Linux test host or the NAS (no binary; AppArmor blocks user namespaces). Argument vector and the probe with the real flags unit-tested only | A host where it can run |
| Confinement backend: Docker container per call (Linux) | EXPERIMENTAL | Tested on the Linux test host (Docker 29, Ubuntu 26.04): `docker.rs` attack table (secrets, writes, network, pids, memory, daemon socket, timeout kill, SIGKILL clean-up) and the confinement tests run through it (an earlier text said 27; no grouping of the test files gives that number, so it is dropped: run `cargo test -p endora-foundation-git` on a Linux host for the count). Also tested on a NAS (Ubuntu 24.04, 200/0/0 on three runs, the same `docker.rs` attack table held). The daemon is root; a container has no network (no real agent); the four real `docker.rs` tests are `ignore`d off Linux, so a macOS run reports them as ignored, not passed (the fifth test is a helper that still runs); on Linux each prints `SKIP:` when Docker or the image is missing and fails under `ENDORA_STRICT_TESTS=1` | Rootless Docker; a hardened container on a bridge network; a network design for a real agent |
| Container recipe and image (`deploy/foundation`) | EXPERIMENTAL | Built on the Linux test host (118 MB, digest-pinned bases), healthcheck and smoke script pass in it; NOT deployed | A deployed run on the target host |

## Target environments

| Target | Status |
|---|---|
| macOS (the Mac) | Tested: scripted profile (escape found 15 Nov 2026, fixed, awaiting independent review), real-agent profile (three real runs, before the fix), the full gate. |
| Linux, test host | Docker backend tested (attack table, the confinement tests, smoke, image). `bwrap` not usable. Re-run on the integrated head (199 passed, 0 failed, 0 skipped on four runs); the NAS ran 200 on three. |
| Linux, NAS (Ubuntu 24.04, kernel 6.8, Docker 29) | Docker backend tested (rootful, cgroup v2, AppArmor `docker-default`, seccomp enforcing): 200 passed, 0 failed, 0 skipped on three runs; the `docker.rs` attack table held, plus hand-run checks (`--network none`). The image is not deployed as a service. |

## Freeze gate (maintainer, 14 Nov 2026)
Before any row is frozen: (1) the attack tests pass on the target (macOS: `agent_attacks.rs`; Linux:
the `docker.rs` attack table); (2) a real executor runs through the confinement; (3) only gaps those runs demonstrate are
fixed. No new security framework unless one of them shows a concrete need.
