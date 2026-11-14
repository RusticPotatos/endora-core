# Foundation slice 1: report

Branch `feat/foundation-slice1`, from Core e9c7923.
Machine: the Linux test host (8 cores). No NAS, no deploy, no push, no model, no network beyond localhost.
Ported and unported rules: [PORTED-RULES.md](PORTED-RULES.md).

## What exists

Two new crates and nothing else in the workspace changed except the manifest:

- `domains/foundation` (`endora-foundation`): ROCA layout. `domain/` (hold, decide, accept, evidence;
  imports only `endora-kernel`), `application/` (`Core`, the one `Capability` port), `infrastructure/`
  (SQLite through `endora-persistence`, two clocks). Binary `foundation-status`: plain text goals, items,
  evidence tail.
- `domains/foundation-observe` (`endora-foundation-observe`): capability A. `probe.http` (loopback only),
  `probe.command` (declared scripts in a sandbox dir), `service.restart` (reversible), `service.wipe`
  (declared irreversible, a dummy target for T2). Binary `foundation-run`: the runner a test kills.

## Acceptance tests (all pass, deterministic, no model)

| Test | Where | Shows |
|---|---|---|
| T1 | `slice1::t1_...` | Goal "keep service X healthy" + routine; two firings, two durable items (file reopened), each with its own criteria, verified by the probe; evidence kept (503 then 200). Firing 2 used one capped restart (second refused, cap 1); policy accepted the observe item, refused the reversible one (`NeedsOwner`), the owner accepted it. |
| T2 | `slice1::t2_...` | Scripted proposer: restart outside its row's band, irreversible wipe, call outside the goal's scope, unknown call, ungranted actor, then a call and a restart after the stop switch. All 7 refused; asserted on the dummy service (0 restarts, `data.db` intact); 7 `refusal` evidence rows. |
| T3 | `slice1::t3_...`, `t3b_...` | A real second process, SIGKILLed mid-item. Restart: item `running` with the dead process's evidence; `recover()` reopens it, evidence prefix unchanged, `fire_due()` fires nothing, item resumed (attempt 2) and accepted. T3b: killed inside a reversible call (intent, no result): the item fails explicitly ("unknown whether it took effect"). |
| T4 | `core_rules::t4_...` | The database's triggers refuse UPDATE and DELETE on `evidence`; a source scan finds no statement that could; the table's writer is crate-private. |
| T5 | `core_rules::t5_...` | File copied while idle to a new directory, opened, SHA-256 of the canonical dump of all 9 tables identical; `persistence::snapshot` (VACUUM INTO) copy also identical. Backup (file copy) 0.31 ms, restore (open + dump hash) 4.4 ms, on a small database. |
| T6 | `t6_independence.rs`, `core_rules::t6_...` | Domain manifest and source name no capability; engine tested with none registered. **Files under `domains/foundation` changed by adding capability A: 0** (`git diff --stat 73679b7 9ab5943 -- domains/foundation`). Caveat: T6 itself first failed because two *doc comments* in the domain gave `probe.http` as an example; fixed in `73679b7` (2 files, comment text only) before the capability commit. |

Counts: 36 tests in the two crates (23 domain unit, 7 engine, 4 slice, 2 independence).

## Size

Product lines (rustfmt'd, including doc comments and blanks): **2,478**: `foundation` 2,227
(domain 618, application 730, infrastructure 837, bin 24, lib 18), `foundation-observe` 251.
Test lines: **1,340** (383 in-file unit, 389 engine tests, 568 slice and independence tests).
Slice 1 is about a quarter of the roughly 10k product-line budget. The two big files are `core.rs`
(693) and `store.rs` (789); the store is mostly SQL strings and row mapping.

## Touched outside the new crates

Only `Cargo.toml` (two workspace members) and `Cargo.lock` (the two new packages; no third-party
crate added). Old crates, HQ, node, console untouched. Dependencies of the new crates: `endora-kernel`,
`endora-persistence`, `rusqlite` 0.40 (same version persistence bundles; needed to name `params!` and
`Connection`), `sha2` 0.10 (already in the lock file at 0.10.9; used in `application`/`infrastructure`
for the inputs hash and the dump hash, never in the domain).

## Build and run times

- Cold, both crates plus their dependency closure (bundled SQLite, chacha, sha2), `cargo test --no-run`
  after `cargo clean`: **11.4 s**.
- Warm: touching a domain file and rebuilding both crates' tests: **1.1 s** (3 runs). One first rebuild
  right after the cold build took 25 to 32 s with the machine's load average at 13; not reproduced, cause
  not isolated.
- Full workspace, cold, from the gate: clippy 90 s; `cargo test --workspace --all-features` 474 s
  (build and run).
- Test run for the two crates: engine 0.26 s, slice tests 0.76 s (3.5 s once when the machine was
  busy), whole `cargo test` for both crates about 4 s.

## Gate

Full handoff gate from CLAUDE.md on the worktree at `9ab5943`: `cargo fmt --all --check`,
`cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo test --workspace
--all-features` (2,411 tests passed, 0 failed), `check-console`, `check-nothing-personal`,
`check-collection-parity`, `check-required` (with the listed requirements), `git diff --check`: all exit 0.
After that run I made the Store's writers crate-private (the hardening commit) and re-ran fmt, clippy
`-D warnings` and the tests **for the two new crates only**; no other crate depends on them, so the
full workspace gate was not repeated. Not run: the handheld terminal checks (unchanged code), and the
node/Ansible/NAS paths (out of scope).

## What does not work or is stubbed

- `outward_reversible` is always refused (`NeedsOwner`); there is no owner-confirm path.
- Spend caps: the rule and its test exist in `decide`; Core sums declared call costs, but nothing
  records spend and every cap is zero. Untested end to end beyond the zero-cap denial.
- `recover()` assumes a single process: with no lease or pid, calling it while a runner is alive would
  take that runner's item. Run it once at start.
- No repository trait: `application` uses the concrete `Store`. A ROCA shortcut (about 200 lines saved);
  extract a port when a second store or a fake is needed.
- Evidence has no hash chain; the append-only guarantee is API + triggers. Someone with the file can drop
  the triggers. `Store::db()` exposes the raw connection for backup and tests.
- The port has no timeout or cancel; `probe.command` and the restart script carry their own 10 s limit.
- `foundation-run` is a test harness (its `*-hold` modes exist to be killed), not a product CLI. The
  status CLI is read-only; goals, grants, firing and acceptance are library calls only.
- No desk, no ask-the-owner verb, no send-back, no idempotent accept replay (see PORTED-RULES).
- `verify()` runs checks as actor `core`; with no grant for `core` a check is refused and the criterion
  just stays unmet (the refusals are in the evidence).

## Friction reusing kernel, persistence, protocol

- **Kernel, good:** `Reversibility` (with `default_decision`, ordering, deny-by-default `Default`) and `Tier`
  fit exactly; the decision table was nearly free. `Clock`, `Timestamp`, `AppError` reused.
- **Kernel, rough:** `Tier` has no name round trip or `from_name` (I cast to integer and search a list);
  `Clock` is not `Send + Sync`, so every holder spells `Arc<dyn Clock + Send + Sync>`; `DomainError` holds
  only `&'static str`, so a rule cannot say which value broke it; the id-newtype macro is private, so I
  defined three row ids; `IdSource` is unused because SQLite assigns ids (the "domain never generates ids"
  rule held anyway: ids are assigned in the store).
- **Persistence, good:** `Db::open` plus `snapshot` (VACUUM INTO, read-only source, integrity-checked copy)
  made T5 easy and safe. One file, one mutex, owner-only permissions.
- **Persistence, rough:** `lock()` hands out the raw `rusqlite::Connection`, so every consumer needs its own
  `rusqlite` dependency and writes its own transaction (`BEGIN IMMEDIATE` by hand); no helper. Rollback
  journal, not WAL, so a plain file copy is only safe while idle (T5 relies on that; `snapshot` is the
  running-safe path). `prove_restore` is built around the node's tables and was not used.
- **Protocol, platform:** not used in slice 1 (no HTTP, no platform concern); nothing to report.
- Cold-build cost of reuse is nil: no new third-party crate.

## Next step

Slice 2 (git change task) was not started, as ordered. Cheapest follow-ups that unblock it: a `Store` port,
a lease on running items, and an owner-confirm path for `outward_reversible`.
