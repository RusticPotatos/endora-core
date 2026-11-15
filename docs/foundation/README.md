Status: CURRENT · interfaces are rated EXPERIMENTAL or FROZEN in STABILITY.md · reference page

# Endora foundation (Core)

New here? Start with [docs/start.md](../start.md) (a ten-minute first run). This page is the reference.

Core holds goals, decides what may run, runs it through one port, records what happened, asks the
owner before work counts as done, and picks up after a crash. It knows no model, no vendor and no
project. Everything it touches the world with is a **capability**, a separate crate behind one trait.

State is **one SQLite file**. Evidence in it is append-only **through the engine and on connections
that carry its triggers**; a hash chain makes any other change detectable. Read "Trust and what is
enforced" below before relying on either; interfaces are rated in [STABILITY.md](STABILITY.md).

## The six verbs

| Verb | What Core does |
|---|---|
| hold | keeps goals, routines and work items |
| decide | allows a call only if policy, scope, caps and the stop switch say so (default deny) |
| run | makes the call through `Capability::call`, nothing else |
| record | writes an intent before the call and a result after it |
| require | marks an item done only when its criteria are met and an authorised actor accepts |
| recover | settles items a dead process left running, from the file alone |

## Two levels

A **goal** says what and why, the calls it may make (scope), its data tier, its spend cap and its
done **criteria**, stored once. A **work item** is one unit of work under a goal. It answers to the
goal's criteria; it may carry a `subject` (`{subject}` in a check's input becomes it, so one
definition serves many jobs) or, rarely, criteria of its own. A **routine** is a goal's rhythm: each
firing makes one item. A missed slot is recorded; a slot is skipped while the last item is open.

## Authority

Every call declares a **band**: `observe` (reads), `reversible` (can be undone),
`outward_reversible` (leaves the house, can be undone) or `irreversible`. Unclassified counts as
irreversible, and registration rejects it. Policy rows (owner-granted) cover a band and a **tier**
(`public`, `internal`, `confidential`, `restricted`). Observe and reversible calls run on a row;
outward-reversible needs the owner; irreversible never runs. A declaration (band, tier, maximum cost,
facts) is **policy metadata, not the boundary**: the boundary is isolation (see "Trust and what is
enforced"). A capability that lies in its declaration defeats the policy, not the isolation, so review
the declaration when you add a capability. The owner's stop switch refuses every call.

Spend: every amount is in **micro-USD** (a `u64`; 1 USD = 1,000,000; the cap flags, `overall-cap`,
the `claude-code` config line and all output use it). A call declares a **maximum** cost, checked
against the goal's and the overall cap before it runs. The call returns the **actual** cost. The
cost recorded and charged is **clamped to the declared maximum**; the raw report stays in the
evidence text (`[cost] reported N micro-USD, charged the declared maximum M`). A report above the
maximum is also a violation: a row says so, the item fails and the goal stops until `resume-goal`.
A zero cap refuses every call whose maximum is above zero. A spec also declares the most **seconds**
(default 600) and **turns** (default none) it can accept; Core grants those to the capability with
the cost. See "Spend: what is enforced" below.

A criterion's check names a **fact** of the call's result and its exact value (`up true`,
`exit 0`, `tests pass`). Facts are numbers, yes/no values or fixed words chosen by the capability's
code; text an agent or a remote wrote can never be one.

## The command line

`endora-foundation [--db FILE] [--config FILE] [--token-file FILE] <command>` (or
`ENDORA_FOUNDATION_DB`, `ENDORA_FOUNDATION_CONFIG`, `ENDORA_OWNER_TOKEN_FILE`: a PATH, never the
token). Exit 0 ok, 1 error, 2 refused by authority (the reason is printed). Owner commands
(`goal add`, `routine add|resume`, `git-task`, `accept`, `decline`, `confirm-criterion`, `resume`,
`resume-goal`) need the token file and exit 2 without it or with a wrong one.

```
goal add --title T [--why W] --criteria TEXT [--check CALL INPUT FIELD EXPECT]
         [--criteria ...] [--probe NAME] [--scope a,b] [--tier internal] [--cap MICRO_USD]
         [--acceptance owner|auto-observe]
                            a `--check` belongs to the `--criteria` before it; `--probe NAME`
                            adds a criterion of its own (so `--criteria` + `--check` + `--probe` = 2)
routine add --goal ID (--every SECONDS | --probe NAME) [--title T] [--auto-accept]
status                      goals, items, evidence tail, open decisions
run-due                     fire due routines, start new items, run their checks
serve --every SECONDS       run-due in a loop (a container's command); SIGTERM ends it cleanly
git-task [--goal ID] [--repo PATH] --instruction T --test-cmd CMD [--cap MICRO_USD]
                            prepare, change (the configured agent), verify; waits for the owner
                            (--cap: the new goal's spend cap; a real executor needs a non-zero one)
overall-cap MICRO_USD       the owner sets the most all goals may spend together (starts at 0)
                            amounts are micro-USD: 1 USD = 1,000,000
publish ITEM                always refused (exit 2): nothing here publishes
init --token-out FILE       once: make the owner credential; the token goes to FILE (mode 600)
accept ITEM | decline ITEM  the owner's decision (needs the token)
confirm-criterion ITEM N    the owner confirms criterion N (one with no check); `confirm` is a
                            deprecated alias that warns. It never approves a call.
stop | resume               stop: anyone;  resume: the owner;  resume-goal GOAL lifts a cost stop
verify-evidence             recompute the evidence hash chain; exit 1 naming the first broken row
evidence --item ID          every record about an item, in full
backup --to FILE            consistent copy; restore by pointing --db at it
capabilities                every registered call with its band, tier and maximum cost
```

There is no `--help`. A command Core does not know prints the one-line usage and exits 1; with `--db`
that also creates the database file. A goal on its own does nothing: `run-due` only works on work
items, and a routine is what makes them.

`goal add` grants Core the observe-band calls its checks need (the owner wrote them). A routine
item stays running until you `accept` it (or the routine allows `--auto-accept` for observe work), so
the routine waits for you. A standing watch can set the goal's **acceptance** to `auto-observe`: an
item whose calls were all observe-band and whose criteria were all met by structured checks is
accepted as `policy:auto-observe` and the next slot runs; anything else waits for you. Only
`goal add` sets it, and the file refuses a later change. A git task is reversible work, so it is
never auto-accepted. The config file is not kept in the repository:

```
probe web tcp 127.0.0.1:8080 60                    # name kind target interval-seconds
probe api http 127.0.0.1:8080/health 60
probe slow command sleep 3 300                     # kind `command`: the rest of the line before the interval
git /var/lib/endora/jobs /srv/template -- sh agent.sh   # scratch root, template, agent command line
```

Real-executor lines (see "Real executors"):

```
claude-code /path/to/claude claude-haiku-5-5 300000 8 180   # binary model max-micro-usd max-turns [max-seconds]
claude-credential /path/to/access-token-file              # read at call time; recommended: an owner-only file
claude-arg --effort                                       # (repeatable) extra CLI words
```

A probe gives the call `probe.run <name>` with facts `up` (tcp, http), `status` (http) and `exit`
(command). Probe commands and the git agent line are the owner's, written in the config.

## Trust and what is enforced

The boundary is **process and filesystem isolation**: an untrusted executor (an agent, a script it
writes) runs where it cannot read the owner token or open the database. Everything inside Core's
own process is trusted code. Details and the list of known gaps: [TRUST-BOUNDARY.md](TRUST-BOUNDARY.md).

- **Isolation is verified per target.** macOS `sandbox-exec` profile: tested (`agent_attacks.rs`:
  token unreadable, database unopenable). Linux Docker backend: tested on the Linux test host (see below). NAS:
  Docker backend tested with the `docker.rs` attack table (secrets, writes outside the job directory, network, no Docker socket, mount and ptrace, fork and memory limits, timeout, hard-kill reaping), plus hand-run checks and a bridged control (see STABILITY.md); `agent_attacks.rs` is macOS only. A same-user, unconfined process can read the
  token and edit the database: documented, not fixed.
- **Application-level checks are defence in depth against mistakes by trusted code, not a boundary.**
  Actor types, `OwnerProof`, the owner credential and the triggers catch a mistaken call; they do not
  stop code that runs in the Core process or a file-level writer. First-time enrolment (`init`) is a
  step in the **trusted setup context**: it exits 2 if a credential exists, and it cannot tell who ran
  it first on a credential-less file. Migrations are also part of the trusted setup.
- **Evidence.** Append-only through the engine API and on connections that carry the triggers. The
  hash chain (`prev_hash`, `hash`; `verify-evidence`) is **supplemental tamper detection for the rows
  it covers**, not proof of whole-database integrity: it cannot see the removal of the last rows, a
  re-forged chain, or any other table. Keep the printed head hash elsewhere.
- **The owner token** is stored only as a salted SHA-256 (256 bits of OS randomness, so a slow KDF
  adds a dependency and no safety); it is never printed, logged or put in an argument or environment
  value. The observe capability's child processes and the git capability's agent get a scrubbed
  environment (minimal `PATH` and `HOME`, plus an allow-list: `probe-env` in the config).
- **Spend** has three parts that must not be read as one.
  1. *Declared enforcement* is a capability's own claim (`BudgetMode::Enforced { units }`). Core
     checks that the call's maximum is at or under the claim and passes the granted budget; **Core
     does not verify the claim**. `git.change` declares `Enforced` only when the config has a
     `claude-code` line; with the default scripted executor it declares `Unenforced` and a zero
     maximum, and `probe.run` has a zero maximum.
  2. *Measured limits.* Turns: the Claude Code CLI enforces `--max-turns` with a one-step overshoot
     (a real run: `--max-turns 1` ended at 2 turns). Wall-clock: the capability's process-group kill,
     tested with fake CLIs and scripted agents only.
  3. *Unverified.* The dollar cap: `--max-budget-usd` is passed, but no real run reached it.

  What holds for dollars today is Core's own: it **trusts the cost a capability reports**, clamped to
  the call's declared maximum (a report above it is a violation row, the item fails, the goal
  stops). A failed call that reports 0 is charged 0. Before a call Core reserves the maximum against
  the goal and overall caps in the same transaction that writes the intent, so two processes on one
  file cannot both pass the check. See "Spend: what is enforced".
- **Known gap:** a per-call owner approval (`approve-call`) does not exist; an outward-reversible
  call is refused, not asked about. It would be an additive command.

### Where the isolation is verified

"Linux test host" and "NAS" name the maintainer's own machines (Ubuntu 26.04 and 24.04). They record
what was tested; you do not need either.

| Environment | Status |
|---|---|
| macOS `sandbox-exec` (Apple silicon) | Verified by `agent_attacks.rs`: confined agent cannot read the token file, open or write the database, or accept; and (a finding of an independent review, fixed) cannot signal the engine or pid 1 while it can still signal its own children (`slice5b.rs`, `slice6.rs`). The same script unconfined can read the token and edit the database (the stated boundary). |
| Linux Docker backend (Linux test host, Ubuntu 26.04, kernel 7.0, Docker 29, rootful daemon) | Verified by `domains/foundation-git/tests/docker.rs` (attack script: secrets, writes, network, pids, memory, token and database absent, no Docker socket) and by the 27 confinement tests running for real through it. See "The Docker backend" below. |
| Linux `bwrap` | NOT verified: absent on the Linux test host and the NAS, and user namespaces are AppArmor-restricted there. Argument vector unit-tested; the probe now runs the real flags. |
| Real executor (Claude Code CLI) on macOS | Three real runs (a fix accepted by the owner, a turn-limit stop, a confinement probe): the sandbox acted before the token layer; see "Real executors" for what was and was not exercised by the agent's own tools. Not run on Linux (the Docker container has no network, so a real agent needs its own design there). |

## The Docker backend (Linux hosts without a usable `bwrap`)

`Confinement::Auto` picks, in order: macOS `sandbox-exec`; Linux `bwrap` if a probe with the *real*
flag set passes; Docker if `docker version` answers and the executor image is **already present
locally** (nothing is ever pulled); else it fails closed with a message naming each backend tried
and why it failed. One plain container per executor call:
`docker run --rm --pull never --network none --read-only --cap-drop ALL --security-opt
no-new-privileges --pids-limit 128 --memory 512m --memory-swap 512m --cpus 1 --user <uid>:<gid>
--tmpfs /tmp:rw,noexec,nosuid,size=64m --init`, the job directory as the **only** mount, and an
`--env-file` holding only the allow-listed variables (the image's own `ENV` is removed first).

- **Image.** `Options::executor_image`, else `ENDORA_FOUNDATION_EXECUTOR_IMAGE`, else `alpine:3.23`
  (`sh` and busybox tools only). The git capability runs `git` itself in the container, so a job
  needs an image that has `git` (and whatever its test command needs). `Options::read_dirs`
  (a toolchain mounted from the host) is refused: put the toolchain in the image.
- **Paths.** The job directory is mounted at the same path it has on the host (not `/job`), so
  every path in the job's environment and arguments stays valid; the agent script named by
  absolute path is copied into `<job>/.exec/` for the call. The container user is the job
  directory's owner (never root), so the job directory can still be removed by Core.
- **Limits.** The wall-clock limit is enforced by `docker kill <name>`, and by an in-container
  `timeout` one second longer, so even a SIGKILLed Core leaves a container that ends itself at the limit.
  Each container name is recorded under `<scratch>/.meta/.containers/` *before* it starts; building
  a `GitChange` (a Core start) or calling `GitChange::reap_leftovers()` removes every recorded
  container (`docker rm -f`). Core's own `recover()` is in the domain crate and does not call it.
- **Trust.** The Docker daemon is root: a container escape is a host compromise, and whoever may
  talk to the daemon (the user running Core) is root-equivalent. Core must run as a user you
  already trust with that. A container with no network cannot run a real agent that needs a model API:
  that needs its own network design.

## Adding a capability (one page)

1. A new crate under `domains/` depending on `endora-foundation` and `endora-kernel`.
2. Implement `Capability`: `offers()` returns one `CallSpec` per call (name, **band**, input and
   output in words, tier, maximum cost); `call(name, input)` returns a `CallOutput` with the actual
   `cost_micros` (micro-USD), `facts` (what your code observed), `result`, `evidence` (text for
   people) and, if it ran confined, the `profile` it ran under (a structured evidence column).
3. Declare the band honestly: Core cannot see inside the call. Observe means it changes nothing.
4. Build facts only from what the code observed (an exit code, a parsed status), never from text
   an agent or a remote produced. A word-valued fact is accepted only if it is in the closed list the
   call's spec declares (`Field::word(name, &[...])`, each word at most 32 characters of `a-z 0-9 _ -`).
5. Register it: `core.register(Arc::new(MyCap::new(config)))?`. A duplicate call name, an empty
   name or a missing band is refused with the call's name.
6. Add it to the command line's `main.rs` behind a config line, add a policy row (`grant`) for each
   actor that may call it, and put the call in a goal's scope.
7. Test with a scripted stand-in: no model, no network, no paid API.

## The git capability's confinement (what it promises, what it does not)

`domains/foundation-git` runs an agent's code, so it is built as if the agent were hostile.

- **Acceptance is decided on a fresh copy.** `git.prepare` keeps the template, its tree hash and
  the protected-path rules in `<scratch>/.meta/<job>/`, which the sandbox hides from the agent.
  `git.change` copies the agent's files (never its `.git`, symlinks kept as symlinks) into a
  repository the capability made itself and records the diff from there, with no external diff,
  textconv, filter or attribute from the agent's tree. `git.verify` applies that patch to a clean
  export of the template and runs the tests there. A patch that touches a protected path (the test
  command's files, `tests/`, `scripts/`, the config's and per-job `protected` list) is refused as
  `tests=tampered` and recorded, not dropped.
- **A shell test must run to its end.** The harness adds a random marker after the sourced test
  script; `exit 0` inside the script or anything it sources is `tests=fail exit=0 incomplete`. A
  template test script that ends in its own `exit 0` needs `Options::trust_exit_code`. Code that
  runs in the test's own process and forges the harness could still look like a pass: the owner's
  `accept` remains the gate.
- **Confinement (macOS `sandbox-exec`; Linux `bwrap` written, not run; Linux Docker verified).** Writes only in the job
  directory; reads only there, the system paths and the files the config names; no network. The
  environment is explicit and minimal (`Options::env_allow` lets named variables through). The
  whole process group is killed when a call ends.
- **Evidence is capped and redacted.** 16 KB per output field with the full length in a marker;
  private-key blocks, `NAME=value` for secret-looking names and well-known token prefixes become
  `[redacted]` (the lists are `SECRET_NAMES` and `SECRET_PREFIXES` in `confine.rs`).
- `git.verify` stays **observe**: it runs agent code only in a confined, temporary copy (tested:
  no write outside it, no network, no read outside it).

## Real executors

`domains/foundation-git` can run the **Claude Code CLI** (`claude -p`) as the agent of `git.change`
(`Options::executor = AgentExecutor::ClaudeCode { binary, model, extra_args }`). It is the first
executor that is not a script, and the first to *declare* `Enforced` spend (a claim Core does not
verify, see "Spend: what is enforced").

**Declared budget.** `git.change` then declares a non-zero maximum (default 300,000 micro-USD =
0.30 USD, set by `ClaudeLimits`), the turns it accepts (default 8) and `Enforced { units: 500000 }`
(0.50 USD per call is the most this executor claims; a maximum above it is
`refused-unenforceable`). `Capability::call_budgeted` builds the command line from the budget Core
granted (cost, turns, seconds from the spec): the smaller of that and the executor's own ceilings.

| Limit | Who enforces it | How | Measured (CLI 2.1.322) |
|---|---|---|---|
| Dollars | the CLI | `--max-budget-usd <grant>` | cap never reached in any run (all runs cost 0.0002 to 0.0026 USD); overshoot behaviour **not verified** |
| Turns | the CLI | `--max-turns <n>` (works, but is missing from `--help`) | stops after the turn that exceeds it: `--max-turns 1` reported `num_turns=2`, ended `error_max_turns`, item failed |
| Seconds | the capability | kills the whole process group at the deadline | unit-tested with a fake CLI (a hang is killed; cost unknown, so the maximum is charged) |

The actual cost returned to Core is `total_cost_usd` from `--output-format json`, converted to
micro-USD and rounded **up** (a whole cheap run is 1,500 to 2,600 micro-USD; cents rounded it up to a
whole cent, a 4 to 6 times overcharge, which is why the unit changed). If it is above the granted
maximum, Core clamps the charge and records the violation. A report that cannot be read, or a kill,
is charged the maximum.

**The `real-agent` sandbox profile** (macOS; Linux refuses it). The result row's `profile` column says `real-agent` (`evidence --item` prints it).
It differs from the scripted profile only here: outbound TCP to port 443 and the DNS resolver
socket are allowed (no inbound, no bind, no loopback, no other port); `/private/etc` is readable
(certificates, resolver); the paths in `Options::deny_paths` (the CLI sets the token file and the
database) and the home folders that hold credentials or agent settings (SSH keys, cloud and GPG
credentials, the Library folder, general and Claude configuration) are denied by name on top of the
general `/Users` deny. Readable: the job folder, the
system paths, the `claude` binary file and nothing else of the home. Writable: the job folder only
(`HOME`, `CLAUDE_CONFIG_DIR` and `CLAUDE_CODE_TMPDIR` all point inside it).

**Credentials.** The real runs needed the Claude login's access token to be passed to the
sandboxed child (the profile hides the login's usual store). What was verified: the login is an
access token only (no refresh token, no keychain access); on macOS Core reads it from the file named
by `claude-credential <file>` and passes it only in the child's environment. A scan of the job
directories, databases, evidence rows, logs and the repository history after the macOS runs found
no credential. Core does not check the file's mode: keep it outside the repository and the job
directories, with mode 600 (a recommendation, not enforced behaviour). With the Docker backend the
job's variables, a credential among them, go through an env file in Core's scratch area,
a per-call file under `<scratch>/.meta/.logs/` (not the job directory that is mounted into the container; mode 600).
It is removed only when the call ends normally: a Core killed outright leaves it, credential
included, until it is removed by hand, because nothing at start-up cleans `.logs`. The macOS scan
does not cover this path. Running a real agent under the Docker backend
is therefore **not supported yet**: it needs its own design (no network by default, an egress and
credential design, for example a one-shot env file in a tmpfs deleted right after the container
starts). Scan for token shapes before any push or copy.
The CLI is started with `--restricted --safe-mode --strict-mcp-config
--disable-slash-commands --no-session-persistence`, only the tools `Read,Edit,Write,Glob,Grep,Bash`,
`--permission-prompts none`, and an allow list of `sh test.sh`, `cat`, `ls`, `git diff|status`:
no web tools, no MCP, no user, project or local settings.

**Residual risk (read this).** The agent has outbound HTTPS to any host and its parent process holds
the access token in its environment. An agent that finds a way to read that environment can send
the token out. What reduces it: only the access token is present (the refresh token and the Keychain
stay out of reach); the agent's own tool commands do not inherit it (env scrub); `ps` cannot run
in the profile (setuid) and the system `python3` shim fails there; the token file is not in the
job; the owner token file and the database are denied by name (a run showed `accept` failing with
`Operation not permitted`). What remains: no test proves the parent's environment unreadable by
every route (a script in an allowed interpreter with a sysctl call is untested); egress is not
limited to the API host (the profile filters by port, not by name); the token is valid until it
expires. Treat the machine's other secrets as safe only because they are not readable, and rotate
the access token after any run you did not trust. A Linux profile with a network allow-list is
future work.

## Spend: what is enforced

- **Units.** Micro-USD everywhere (`u64`, checked arithmetic). Files written in cents are migrated
  (schema 6): caps are multiplied by 10,000; evidence rows are never rewritten, an old row keeps its
  cents and its hash and is read as cents times 10,000.
- **`Enforced` can overshoot by one step.** It means the executor stops at, or one step above, the
  grant: a turn limit of 1 stopped at 2 (`num_turns=2`, `error_max_turns`). Core's clamp and violation
  row are the second line, and they only see what the capability reports.
- **Turns:** enforced by the executor (`--max-turns`), one-step overshoot, proven by a real run.
- **Wall-clock:** enforced by the capability's process-group kill at the deadline (and by the
  container's own timeout under Docker). Tested with a fake CLI and scripted agents; not with the
  real CLI hanging.
- **Dollars:** the CLI accepts `--max-budget-usd`, but no real run came near the cap, so the dollar
  cap is **accepted by the CLI and unproven**. Core's pre-call check against the caps and the
  clamp after the call are what hold today.

## Known deviations from the architecture

The project's design rules (responsibility-based modules, dependencies pointing inward, replaceable
infrastructure) are stated in [the architecture page](../architecture.md). The code deviates in three
known places. They are documented, not fixed:

- **Storage has no port.** `application` imports the concrete SQLite `Store` and a `ManualClock`
  from `infrastructure` (`domains/foundation/src/application/core.rs`, `owner.rs`). The only port is
  `Capability`. Replacing SQLite would touch the application layer.
- **Capabilities depend on Core's whole crate.** `foundation-observe` and `foundation-git` depend on
  `endora-foundation`, including its SQLite store, not on a narrow contract crate.
- **Domain purity is by convention.** `domain`, `application` and `infrastructure` are modules of one
  crate whose `Cargo.toml` has `rusqlite` and `sha2`; no compiler rule or test stops a domain file from
  importing them (today none does).
