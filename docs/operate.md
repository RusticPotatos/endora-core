Status: EXPERIMENTAL · what an operator needs to run the foundation for longer than a tutorial; each fact below was checked against the code or by running it on 15 Nov 2026

# Operating the foundation

*Audience: someone who keeps `endora-foundation` running (a laptop, a server, a container) and has read
[start.md](start.md). The reference is [foundation/README.md](foundation/README.md); the container recipe is
[deploy/foundation/README.md](../deploy/foundation/README.md). Nothing here is a promise about future
versions: interfaces are rated in [foundation/STABILITY.md](foundation/STABILITY.md), and the printed text
is not part of them.*

## Run as a service

Two settings make a long-running process: where the state is and what it may probe.

```sh
export ENDORA_FOUNDATION_DB=/var/lib/endora/foundation.db     # one SQLite file; choose a path you back up
export ENDORA_FOUNDATION_CONFIG=/etc/endora/foundation.conf   # the probes you allow
endora-foundation serve --every 60                            # run-due every 60 seconds, until SIGTERM
```

- `serve` runs `run-due` in a loop. `SIGTERM` (or `SIGINT`) ends it after the current pass, prints
  `stopping on signal` and exits 0. Run it under whatever supervises processes on your system and have it
  send `SIGTERM` to stop.
- Run **one** `serve` (or one `run-due`) per database: one runner is still the design. Crash recovery
  settles items "a dead process left running" and leaves an item a live process is running alone (each
  running item names the lock file of its process), so a second `run-due`, a `serve` tick or a `git-task`
  beside it does not take that item over; nothing more is claimed. Overlapping runs can still make one process
  exit 1 with a misleading message: see [Known issues](#known-issues). What is and is not claimed:
  [TRUST-BOUNDARY.md D4b](foundation/TRUST-BOUNDARY.md#d4b-several-processes-on-one-database).
- Make the owner credential once with `init --token-out FILE` (the file is written mode 600). Owner
  commands then need `--token-file FILE` or `ENDORA_OWNER_TOKEN_FILE`, and refuse a token file that others
  can read (exit 2, `run chmod 600`). `serve`, `run-due`, `status`, `stop`, `evidence`, `verify-evidence` and
  `backup` do not.
- A first start creates the database file if it is missing, in every command, including a mistyped
  `--db` path. Check the path before you read an empty `status` as "nothing happened".

## Watch it

There is **no machine-readable status**: `status` prints plain text, and `status --json` is not an
option (unknown flags are ignored, so it prints the same text and exits 0). Not implemented. What you can
rely on is the exit code (0 ok, 1 error, 2 refused by authority) and, in the text, these things:

- `ITEMS`: a `[failed]` line with its reason; `[running]` with every criterion met waits for you
  (`OPEN DECISIONS` names it: `accept N | decline N`).
- `ROUTINES`: `PAUSED after N failures in a row (routine resume ID)`. A routine pauses itself after five
  failed items in a row (the default) and stays paused until the owner runs `routine resume ID`.
- `STOP SWITCH: ON` at the top.

An alert on any of these means parsing text that may change. The container healthcheck only proves that
the database file opens; it does not notice a paused routine or a failed item. `verify-evidence` is the
check for tampering: it exits 1 naming the first broken row.

## Back up and restore

- `backup --to FILE` is safe while `serve` runs: it takes one consistent view of what has been committed
  and checks the copy. The file is created mode 600 and parent directories are created if missing. It
  **never overwrites** an existing file (exit 1: `already exists`), so use a dated name.
- Copy the backup **off the machine or volume** that holds the database. A copy next to the original is
  lost with it.
- The backup contains the database only. It does **not** contain the owner token file (the database holds
  only a salted hash of the token) or the config file. Keep a copy of the token file with the backup, with the same care:
  see "Known limits" in [foundation/README.md](foundation/README.md#known-limits).
- Write down the `head` hash that `verify-evidence` prints when you back up; it is how you notice that the
  last rows of a database were removed.
- **Restore.** Stop `serve`, copy the backup over the database file (or point `--db` at it), start
  `serve`, run `verify-evidence` and compare the head hash. The work done after the backup is gone, and
  goal and item numbers are issued again from where the backup stood (after restoring a database
  with one goal, the next `goal add` prints `goal 2` again). Do not restore over a running `serve`.
- A container does the same with `docker compose` commands:
  [deploy/foundation/README.md](../deploy/foundation/README.md#restore).

## Upgrade the binary

**Back up before you upgrade the binary. A newer binary upgrades any database file it opens, including a
backup you only inspect; keep one copy that the newer binary has never opened.**

Every command opens the database read-write first, and opening runs every pending schema migration (the
current schema is version 8), so `status`, `evidence`, `verify-evidence` and `capabilities` upgrade an older file too.
There is no downgrade: a binary that finds a file newer than it understands refuses it (`this database
is schema version N, newer than this program understands ... nothing was changed`), and
[STABILITY.md](foundation/STABILITY.md) records no downgrade path.

Stop every running endora-foundation process (serve, git-task, cron runs) before the first run of the new binary: a newer binary upgrades the database in place and a live process of the older build would have its item recovered as dead. (Why: since schema 8 a running item names a lock file its process holds; a process of an older build holds none, so a newer binary cannot tell it is alive. See [TRUST-BOUNDARY.md D4b](foundation/TRUST-BOUNDARY.md#d4b-several-processes-on-one-database).) So:

1. stop `serve`; make a backup; copy it somewhere the new binary will not touch;
2. install the new binary and start it on the live file (it migrates once, in one transaction);
3. to roll back, restore that untouched copy with the old binary.

## Known issues

Found by an independent review of 15 Nov 2026 and checked here; none is fixed in this branch.

- **KNOWN ISSUE (recovery; fix planned: REC-2).** An item that was interrupted once, and whose re-run then meets its criteria, can be recovered again by later `run-due` passes (under `serve` there is one pass per tick: with `--every 2` it failed within three ticks, about 6 seconds). Each pass writes a false `core.recover` row ("interrupted on attempt N"), and the third marks the item `failed` ("interrupted again after 3 attempts") although its criteria were met and it was waiting for the owner. No item is run twice (a met criterion is not checked again) and the evidence chain stays intact, but the work is lost to the owner's decision and the evidence carries the false rows. It is present on `22fe97ac` (the public `docs/developer-docs-v1`) as well; it was found by an independent review of this branch. Workaround, checked on one database: right after the `run-due` that prints `item N: 1/1 criteria met` for a recovered item, run `accept N`; an accepted item is not recovered again. No workaround was tested for a running `serve`: its ticks may reach the item first.
- **Overlapping `run-due` processes.** A second `run-due` started while another runs (a second later is enough) takes the next open item. When both reach the same open item, the first process may exit 1 with `corrupt stored data: cannot transition from running to running` (a `serve` logs the same and carries on). **That message does not mean the database is damaged**: `verify-evidence` stays intact, no item ran twice and no data harm was observed (checked with three `run-due` started a second apart over six goals: two exited 1, the chain was intact). The message and the exit code are to be improved in a later increment (REC-2). Run one `serve` or one `run-due` at a time.
- **`--db x.db/` (a trailing slash) creates `x.db` with mode 644**, not 600 as for `--db x.db`. Write the path without the slash.
- **A 254-character database file name** exits 1 (`unable to open database file`) and leaves a 0-byte file of that name behind.
- **`scripts/gate.sh` starts by running `scripts/prune-builds.sh 15 "$HOME/repos"`**, which deletes the least recently used `target/` build folders of the worktrees under that folder (it keeps 15). Read it before running the gate on a machine with other work there.
- **`scripts/gate.sh` needs PyYAML** for its `image-workflow` check (`python3 -c 'import yaml'`; not installed on the Mac this was checked on, where the gate stops at that step).
- **`/usr/bin/git` inside the macOS profile.** The independent review found that `/usr/bin/git` cannot run inside the confined profile when Xcode in `/Applications` is the active developer directory (a Homebrew `git` works). Reported by the review on one Mac; not reproduced here.

## Growth

The file only grows: nothing prunes or archives evidence, and there is no command to do it (not
implemented). An empty database is about 86 KB. Every routine firing adds a few evidence rows (about four
for a probe item that ends in one pass), and every wrong-token attempt adds one (checked: one wrong token,
one new row). A routine that fires every minute therefore adds on the order of
thousands of rows a day. Watch the file's size, keep disk free for `backup` (it writes a second copy), and
start a new database if you ever need to shed history: back up first, keep the old file.
