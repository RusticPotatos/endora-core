Status: CURRENT · reference page

# Trust boundary (14 Nov 2026)

Honest statement of what Core's security rests on. Nothing here is frozen. "Finding N" below refers
to two independent reviews held in the project's private history; the facts are stated here.

## D1. The boundary: isolation of untrusted executors

Untrusted executors (an agent, a script an agent wrote) must not be able to read Core's state or
the owner's credentials. That is done by **process and filesystem isolation**, not by Core's code.
A capability's declaration (band, tier, maximum cost, facts) is **policy metadata, not the
boundary**: a capability that lies in it defeats the policy, never the isolation.
It is verified per target environment: on macOS with `agent_attacks.rs` (macOS only, it never ran on Linux), on Linux with the `docker.rs` attack table:

| Target | Status |
|---|---|
| macOS `sandbox-exec` profile | tested: token unreadable, database unopenable, no network (scripted profile; the real-agent profile allows outbound port 443). After the 15 Nov 2026 fix also tested: no LaunchServices open, no preference or pasteboard access (`macos_profile_hardening.rs`). **EXPERIMENTAL since 15 Nov 2026**: an escape was found and fixed, the fix awaits a new independent review (D2) |
| Linux / Docker backend (Linux test host) | tested with the `docker.rs` attack table (secrets, writes outside the job directory, network, no Docker socket, mount and ptrace, fork and memory limits, timeout, hard-kill reaping); the container runs with `--network none` (a hardened container on the default bridge can reach the host's own services and the LAN) |
| NAS (the maintainer's storage server) | Docker backend tested with the same `docker.rs` attack table, plus hand-run checks and a control container on the default bridge; the real backend runs with `--network none` (see STABILITY.md) |

**What the macOS profiles let a confined process read** (everything else is denied, reads of file
contents and listings; metadata such as `stat` stays open):

- Readable: `/usr`, `/bin`, `/sbin`, `/System`, `/Library`, `/private/var/db`, `/dev`; under
  `/opt/homebrew` only `bin`, `lib`, `opt`, `Cellar`, `share` and `etc/openssl@3` (Node needs its
  OpenSSL configuration); the job directory; the files and directories the config names; and, under the
  real-agent profile only, `/private/etc`.
- Denied by directory, even though a prefix above them is readable: `/Library/Keychains`,
  `/Library/Application Support`, `/usr/local/var`, `/usr/local/etc`, `/opt/homebrew/var`,
  `/opt/homebrew/etc` (except the OpenSSL directory above), the home folders (`/Users`), the temporary
  folders (`/private/tmp`, `/tmp`, `/private/var/folders`), `/private/var/root`, `/var/root`, `/Volumes`.
- Denied by name, in both profiles, wherever they lie: the database, its `-journal`, `-wal` and `-shm`
  files and the owner token file (the command line passes them whenever it runs a git capability). Before
  15 Nov 2026 only the real-agent profile did this, and only with a `claude-code` line; a database or
  token kept under a readable prefix such as `/usr/local/var` or `/opt/homebrew/share` was readable to a
  scripted agent (writes stayed denied). The command line prints a note when one of these paths lies
  under a prefix the profile can read.
- Still readable: everything else under the readable prefixes (for example `/usr/local/bin` and the rest
  of `/usr`, `/Library` outside the denied directories, `/opt/homebrew/share`). A backup or other copy
  of the database kept there is readable to a confined agent: keep copies under the home folder or a
  volume (denied) and never under a readable prefix.
- Paths are written into the profile escaped (quote and backslash); a path with a control character or
  that is not UTF-8 is refused before the call runs.

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
- The macOS sandbox profile starts from `(allow default)` and then denies, so every operation it
  allows is a potential route. Signals to other processes were open (finding 9); both macOS profiles
  now carry `(deny signal (target others))`, tested.
- **A sandbox escape was found on macOS and fixed (weekly independent review B, 15 Nov 2026; the fix
  awaits a new independent review).** Class: an operation the profile left allowed that makes a service
  outside the sandbox act for the caller. A confined process could ask LaunchServices to start an
  unconfined helper, which ran as the owner's user, so the token and the database were reachable.
  Impact: the isolation above did not hold on macOS against a hostile scripted agent. Fix: both
  profiles now deny those operations (`lsopen`, `appleevent-send`, the LaunchServices lookup), and
  also preference reads and writes (a persistent write outside the job directory, and reading another
  app's settings, through the preferences daemon) and the pasteboard lookup (the owner's clipboard).
  Each denial has a test. Why the row is EXPERIMENTAL: STABILITY.md.
- **What stays open on macOS:** exec; every mach lookup not named above (other system helpers stay
  reachable); under the real-agent profile outbound TCP port 443 to any host. The longer-term fix is
  to rebuild the profile from `(deny default)` with an allow-list (exec, the system read paths, the job
  directory); that is not done. The preference and pasteboard denials were not tried with a real
  Claude Code CLI.
- **macOS backend limits (found by the same review, not fixed):** a child that calls `setsid` leaves
  the process group and survives the group kill (it stays confined, but outlives the call); killing
  Core outright does not stop the agent; there is no limit on disk written or on process count. The
  Docker backend has pids, memory and cpu limits.
- Migrations are part of the trusted setup. They refuse a file whose evidence rows already carry
  hashes under an older `user_version` (finding 3), but whoever can run them can edit the file.

## D3. Evidence

The hash chain is **supplemental tamper detection for covered rows**, not proof of whole-database
integrity. Covered: every evidence row's content and its link to the row before. Not covered:
removal of the last rows, a chain re-forged by a writer who knows the format, and every other table
(`items`, `goals`, `policy`, `control`, `owner_credential`). Keep the head hash off the box.

## D4. Spend

- Three different things. *Declared enforcement* is a capability's own claim (`Enforced { units }`):
  Core checks the call's maximum is at or under it and passes the granted budget, and does **not**
  verify it. `git.change` declares `Enforced` only when the config has a `claude-code` line; otherwise
  it declares `Unenforced` and a zero maximum. *Measured limits*: turns (the CLI does the stopping; one real
  run overshot by one turn) and wall-clock (the capability's process-group kill, fake CLIs only; a macOS `setsid` child escapes it).
  *Unverified*: the dollar cap (`--max-budget-usd` is passed; no real run reached it).
- What holds for dollars today is Core's own: it **trusts the cost a capability reports**, clamped
  to the call's declared maximum; a larger report writes a `core.violation` row and stops the goal.
- A failed call that reports a cost of 0 is charged 0 (an unknown cost is charged the maximum).
- Check and intent are one `BEGIN IMMEDIATE` transaction: the reserved maximum counts against both
  caps until the actual cost replaces it, so two processes cannot both pass (`spend_race.rs`).
- `Enforced` means something only when an executor really limits its own spend; Core cannot tell.

## D5. Reading the rest

Interface ratings are in [STABILITY.md](STABILITY.md). Where this file and the code disagree, the
code and its tests win; fix this file.
