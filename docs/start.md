Status: CURRENT · first-run tutorial; its output blocks are checked against examples/a-observe/expected-output.txt

# Start here: run the foundation in ten minutes

You will create a goal ("a local web port must accept connections"), give it a rhythm, watch Core
check it and write evidence, accept the result as the owner, back the database up, and then kill the
process in the middle of a check and watch Core recover. About ten commands, one throwaway directory.

**Not production.** This uses a scratch database, no model, no network beyond `127.0.0.1` and no
account. Nothing here changes anything outside the directory you make. Interfaces are rated in
[STABILITY.md](foundation/STABILITY.md): the commands, flags and exit codes used below are FROZEN (the
output text is not part of that promise); the owner-decision machinery behind them is EXPERIMENTAL.

**You need** Rust 1.87 or newer, a Unix shell, and `python3` (or `perl`) for a throwaway listener.

**Want it all at once?** `sh examples/a-observe/run.sh` runs every step below in a temp directory
and compares the output with [expected-output.txt](../examples/a-observe/expected-output.txt); the
output blocks on this page are checked against that same file by `sh scripts/check-docs.sh`. In the
blocks, `PORT` stands for the listener's port, `TIME` for a timestamp in milliseconds and `HASH` for
a SHA-256 value, because those differ on every run; on your own run you will see real values (`PORT`
is `8080` below). Two lines in the blocks are notes the script adds, not output: `(no output: ...)`
in step 2 and the `$ kill -9 <run-due>` line in step 7 (by hand you type `sleep 1; kill -9 $!`).

## 0. Build and set up

```sh
cargo build -p endora-foundation-cli       # the binary is target/debug/endora-foundation
export PATH="$PWD/target/debug:$PATH"      # so the commands below work as written
mkdir -p "${TMPDIR:-/tmp}/endora-demo" && cd "${TMPDIR:-/tmp}/endora-demo"
```

Core reads three settings from flags or from the environment. Set them once; each holds a **path**
(never the token itself):

| Flag | Environment variable | Meaning |
|---|---|---|
| `--db FILE` | `ENDORA_FOUNDATION_DB` | the one SQLite file that holds everything |
| `--config FILE` | `ENDORA_FOUNDATION_CONFIG` | the config file: the probes you allow |
| `--token-file FILE` | `ENDORA_OWNER_TOKEN_FILE` | where the owner credential is; owner commands need it |

```sh
export ENDORA_FOUNDATION_DB="$PWD/foundation.db"
export ENDORA_FOUNDATION_CONFIG="$PWD/foundation.conf"
export ENDORA_OWNER_TOKEN_FILE="$PWD/owner.token"
```

To start over, delete that directory first (`rm -rf`) and repeat: `init` refuses to run twice on one database.

Start something for Core to look at, in a **second terminal** (it only needs to accept connections):

```sh
python3 -m http.server 8080 --bind 127.0.0.1
```

Write the config. A **probe** is a read-only check you allow, one per line: `probe NAME KIND TARGET
SECONDS` (kinds `tcp`, `http`, `command`; the number is how often a routine may fire it). The second
probe takes three seconds and is used in step 7.

```sh
cat > foundation.conf <<'CONF'
probe web tcp 127.0.0.1:8080 300
probe slow command sleep 3 300
CONF
```

(`examples/a-observe/config` is the same file with the port filled in by the script.)

## 1. The owner credential

```sh
endora-foundation init --token-out owner.token
```

<!-- example-a: 1 -->
```text
$ endora-foundation init --token-out owner.token
owner credential created; the token is in owner.token
```

`init` runs once per database. It makes the owner credential and writes the token to the file you
named, readable by you alone (mode 600). Only a salted hash of it is stored in the database. Commands
that decide things (`goal add`, `routine add`, `accept`, ...) are the owner's and need that file;
without it they exit 2 and say so.

## 2. A goal on its own does nothing

```sh
endora-foundation goal add --title "Web answers" \
  --why "the local web port must accept connections" --probe web --acceptance owner
endora-foundation run-due
```

<!-- example-a: 2 -->
```text
$ endora-foundation goal add --title "Web answers" --why "the local web port must accept connections" --probe web --acceptance owner
goal 1
$ endora-foundation run-due
(no output: the goal has no routine, so there is no item)
```

- `--title` and `--why` say what and why; `--probe web` adds the criterion "probe `web` is healthy"
  with its check (a tcp probe is healthy when `up` is `true`); `--acceptance owner` means a person
  accepts the result (the default; the alternative `auto-observe` is for standing watches).
- `goal add` also grants Core the right to make the one read-only call its check needs.
- `run-due` ("run what is due") printed **nothing**, exit 0. That is not a failure: `run-due` works on
  work items, and a goal creates none. Something has to create items, and that is a routine.

## 3. A routine gives the goal a rhythm

```sh
endora-foundation routine add --goal 1 --every 300 --title "check web"
endora-foundation run-due
```

<!-- example-a: 3 -->
```text
$ endora-foundation routine add --goal 1 --every 300 --title "check web"
routine 1
$ endora-foundation run-due
routine 1: item 1 missed 0 skipped_open false
item 1: 1/1 criteria met
```

- `--goal 1` is the goal from step 2; `--every 300` fires the routine every 300 seconds; each firing
  makes one work item.
- `run-due` fired the routine (`item 1`; `missed 0 skipped_open false` says no slot was missed and
  none was skipped), started the item, made the probe call, and checked the result against the
  criteria.
- **"1/1 criteria met"** means: the goal has one criterion, and its check passed (the probe reported
  `up=true`). It counts criteria, not calls. A goal made with `--criteria TEXT --check CALL INPUT FIELD
  EXPECT --probe NAME` has **two** criteria, because `--check` belongs to the `--criteria` before it and
  `--probe` adds a criterion of its own; its output reads "2/2 criteria met" and makes two calls. That is
  the code working as written, not a counting error.

## 4. The item waits for you

```sh
endora-foundation status
```

<!-- example-a: 4 -->
```text
$ endora-foundation status
STOP SWITCH: off

GOALS
  #1 Web answers (why: the local web port must accept connections) scope: probe.run

ITEMS
  #1 [running] goal #1 check web criteria 1/1 attempts 1 missed 0

EVIDENCE (tail)
  e1 t=TIME item=- core core.owner-enrolled observe allow control: an owner credential was created
  e2 t=TIME item=- owner core.goal observe allow control: goal 1 created (acceptance: owner)
  e3 t=TIME item=- owner core.grant observe allow control: core may call probe.run up to observe
  e4 t=TIME item=- owner core.routine observe allow control: routine 1 declared (pauses after 5 failures)
  e5 t=TIME item=1 runner core.start observe allow control: item started
  e6 t=TIME item=1 core probe.run observe allow intent:
  e7 t=TIME item=1 core probe.run observe allow result: tcp 127.0.0.1:PORT up=true

ROUTINES
  routine 1 goal #1 every 300 s: running, failures 0/5

OPEN DECISIONS
  item 1: every criterion met. accept 1 | decline 1
```

Item 1 is `[running]` although every criterion is met: **meeting the criteria is not the same as
being done.** Work counts as done only when an authorized actor accepts it ("require" in Core's six
verbs). `OPEN DECISIONS` says what is waiting on you. The `EVIDENCE` lines are the rows Core wrote:
`e5` to `e7` are "item started", "intent: about to call `probe.run`" and "result: the connection
opened". The intent was written **before** the call, the result after.

## 5. Accept, then read the evidence

```sh
endora-foundation accept 1
endora-foundation evidence --item 1
endora-foundation verify-evidence
```

<!-- example-a: 5 -->
```text
$ endora-foundation accept 1
item 1 accepted by owner
$ endora-foundation evidence --item 1
e5 t=TIME runner core.start observe allow control:
item started
e6 t=TIME core probe.run observe allow intent:

e7 t=TIME core probe.run observe allow result:
tcp 127.0.0.1:PORT up=true
[facts] up=true
e8 t=TIME owner core.accept observe allow control:
accepted by owner
$ endora-foundation verify-evidence
evidence chain intact: 8 rows, head HASH (keep the head elsewhere to catch removal of the last rows)
```

- `accept 1` is the owner's decision. Item 1 is now `done`. Run it without the token and it exits 2
  (refused by authority) and names the flag to use.
- `evidence --item 1` prints every record about the item in full: actor, call, band (`observe`:
  changes nothing), decision (`allow`), kind (`intent`, `result`, `control`) and, for a result, the
  **facts** Core's code observed (`up=true`). A criterion's check can only read facts, never text.
- `verify-evidence` recomputes the hash chain over the evidence rows. It detects an edited row; it
  cannot see the removal of the last rows, so keep the printed head somewhere else (see
  [trust and spend](trust-and-spend.md)).

## 6. Back up

```sh
endora-foundation backup --to backup.db
endora-foundation --db backup.db status | grep '\['
```

<!-- example-a: 6 -->
```text
$ endora-foundation backup --to backup.db
backup written to backup.db (restore: point --db at it)
$ endora-foundation --db backup.db status | grep '\['
  #1 [done] goal #1 check web criteria 1/1 attempts 1 missed 0 accepted_by owner
```

`backup --to FILE` writes a consistent copy, safe while the database is in use. To restore, point
`--db` at the copy. (Do not copy the live file with `cp`.) A backup is never overwritten (a second
`backup --to` with the same name fails), is created mode 600, and holds the database only: **not the owner
token file**, which cannot be made again, so keep a second copy of it (see
[Known limits](foundation/README.md#known-limits)).

**Back up before you upgrade the binary. A newer binary upgrades any database file it opens, including a
backup you only inspect; keep one copy that the newer binary has never opened.** The `status` command in
the block above opens the backup like any other file: with a newer binary it would migrate that copy too.
More on running it for longer: [operate.md](operate.md).

## 7. Kill it in the middle of a check

A second goal and routine use the three-second probe. Start `run-due` in the background, kill it hard
while the check is in flight, then look:

```sh
endora-foundation goal add --title "Slow check" --why "a probe that takes three seconds" --probe slow
endora-foundation routine add --goal 2 --every 300 --title "slow check"
endora-foundation run-due &          # starts item 2 and begins the three-second probe
sleep 1; kill -9 $!                  # SIGKILL: no clean-up code runs
endora-foundation status | grep '\['
endora-foundation run-due
endora-foundation status | grep '\['
endora-foundation evidence --item 2
endora-foundation verify-evidence
```

<!-- example-a: 7 -->
```text
$ endora-foundation goal add --title "Slow check" --why "a probe that takes three seconds" --probe slow
goal 2
$ endora-foundation routine add --goal 2 --every 300 --title "slow check"
routine 2
$ kill -9 <run-due>        # the process dies while item 2 is checking
$ endora-foundation status | grep '\['
  #1 [done] goal #1 check web criteria 1/1 attempts 1 missed 0 accepted_by owner
  #2 [running] goal #2 slow check criteria 0/1 attempts 1 missed 0
$ endora-foundation run-due
recovered item 2 -> open: interrupted on attempt 1: resumed from its recorded evidence
item 2: 1/1 criteria met
$ endora-foundation status | grep '\['
  #1 [done] goal #1 check web criteria 1/1 attempts 1 missed 0 accepted_by owner
  #2 [running] goal #2 slow check criteria 1/1 attempts 2 missed 0
$ endora-foundation evidence --item 2
e12 t=TIME runner core.start observe allow control:
item started
e13 t=TIME core probe.run observe allow intent:

e14 t=TIME core core.recover observe allow control:
interrupted on attempt 1: resumed from its recorded evidence
e15 t=TIME runner core.start observe allow control:
item started
e16 t=TIME core probe.run observe allow intent:

e17 t=TIME core probe.run observe allow result:
command sleep
[facts] exit=0
$ endora-foundation verify-evidence
evidence chain intact: 17 rows, head HASH (keep the head elsewhere to catch removal of the last rows)
```

- The background `run-due` may print `routine 2: item 2 missed 0 skipped_open false` before it dies: the
  routine firing. The script hides it.
- After the kill, item 2 is `[running]` with `criteria 0/1`: the process died with the item started
  and the intent written but no result. Nothing else knows that, except the file.
- The next `run-due` begins with **recovery**: `recovered item 2 -> open: interrupted on attempt 1:
  resumed from its recorded evidence`. Core read the file, saw an item left running by a dead process,
  recorded `core.recover`, put the item back to `open`, and ran it again (`attempts 2`). Recovery
  works from the database alone; it needs no log and no memory of the dead process. (Had that first
  `run-due` still been alive, the second would have left item 2 alone: each running item names the process
  running it, and recovery takes over only a dead process's items.)
- The second `status | grep '\['` shows `criteria 1/1` and `attempts 2`. `evidence --item 2` shows both
  attempts: the first has an intent and no result, then `core.recover`, then the second attempt with an
  intent and a result (`[facts] exit=0`). `verify-evidence` says the chain is still intact (17 rows
  here).

## What you learned, and where next

You ran the six verbs: **hold** (goal, routine, items), **decide** (the grant, the owner-only
commands), **run** (one call, `probe.run`), **record** (intent and result rows), **require** (the
accept) and **recover** (the restart). Next:

- the commands and the config grammar: [docs/foundation/README.md](foundation/README.md);
- running it for longer (service, backup and restore, upgrade, growth) and what it cannot do yet:
  [operate.md](operate.md) and [Known limits](foundation/README.md#known-limits);
- what Core trusts and what it does not: [trust-and-spend.md](trust-and-spend.md);
- the words: [terminology.md](terminology.md);
- writing code on top: [architecture.md](architecture.md#where-does-my-code-go).

## If something goes wrong

- **`no database: pass --db <path> or set ENDORA_FOUNDATION_DB`**: the settings in step 0 are missing.
  There is no `--help` and no `--version`: an unknown command prints a one-line usage and exits 1, and with
  `--db` it also creates a database file (the schema, no rows). An unknown flag is ignored, not rejected.
- **`this command needs the owner token`** (exit 2): pass `--token-file`, or set
  `ENDORA_OWNER_TOKEN_FILE` to the path `init` wrote. If the variable is set but empty or names a missing
  file, the message is instead `cannot read the owner token file <path>: entity not found` (exit 2; an
  empty variable leaves the path blank).
- **`run-due` prints nothing**: no routine exists, or none is due. The routine's slot is 300 seconds.
- **`accept 9` prints `storage backend error: Query returned no rows`**: there is no item 9. It is the
  engine's raw error for an unknown id, not corruption.
- **`not a probe in the config`**: the `--probe` name is not a line in `foundation.conf`, or Core was
  started without `--config`.
- Stop the listener (Ctrl-C in the second terminal) and delete the demo directory when you are done.
