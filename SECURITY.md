Status: CURRENT · the security policy of the foundation (Core, its capabilities, the confinement, the `endora-foundation` command); the earlier HQ node's policy is in docs/legacy/SECURITY-HQ.md

# Security policy

This policy covers the **foundation**: Core, its capabilities and the `endora-foundation` command line.
What the foundation's security rests on, and where it is known to be weak, is written down in
[docs/foundation/TRUST-BOUNDARY.md](docs/foundation/TRUST-BOUNDARY.md); what spend limits really hold is
in [docs/trust-and-spend.md](docs/trust-and-spend.md). Read both before reporting: a gap listed there is
already known, though a way to make it worse than described is a finding.

## Supported versions

**There are no releases of the foundation yet.** The version number in the workspace manifest is a label
inherited from the earlier HQ product, not a release of the foundation. Only the current head of the
foundation branch receives security attention (today the branch `docs/developer-docs-v1`; `main` once the
foundation is merged into it). Fixes land on that head; nothing is backported, and there is no long-term
support window. Interfaces are rated FROZEN or EXPERIMENTAL in
[docs/foundation/STABILITY.md](docs/foundation/STABILITY.md); neither rating is a security guarantee.

## What is in scope

A flaw is in scope if it lets something do more than the owner's policy allows, or lets an untrusted
party read or change what only the owner or Core may. For example:

- **The confinement.** An untrusted executor (an agent, a script an agent wrote) that reads the owner
  token file, opens or writes the database, signals Core, writes outside its job directory, reaches the
  network from a profile that denies it, or escapes the macOS `sandbox-exec` profile or the Docker
  container Core builds for it.
- **Authority.** A call that runs without a policy row, an `irreversible` call that runs, an
  outward-reversible call that runs without the owner, work that counts as done without an authorized
  actor's acceptance, an owner command that succeeds without the owner token, or a stop switch that does
  not stop.
- **Spend.** A call that is charged less than it cost, a spend cap that is passed without the violation
  being recorded, or a declared maximum that is not checked before the call. (The dollar cap of the
  real-executor path is documented as unverified; showing it fail is welcome.)
- **Evidence.** A change to or removal of an evidence row through the engine that `verify-evidence`
  does not detect, or evidence that leaks a secret the redaction should have removed.
- **Secrets.** The owner token or a credential appearing in an argument, an environment value, a log,
  an evidence row, a job directory or a backup.
- **The command line and the container recipe** (`deploy/foundation/`): a way to make a flag or a config
  line do something its page says it cannot.
- **Memory safety.** `unsafe` code is forbidden in the workspace (a compiler lint); a memory-safety defect
  anywhere in the foundation crates is high priority.

## What is not in scope

- **The earlier HQ product** (the node, the console, the team, sound and mail code under `app/node`,
  `app/hq`, `domains/team`, `domains/platform`, `domains/capabilities` and their neighbours) is
  **LEGACY and unsupported**. It is kept in the tree as history. Reports about it will be read but not
  fixed; do not run it on anything you care about. Its old policy is in
  [docs/legacy/SECURITY-HQ.md](docs/legacy/SECURITY-HQ.md).
- An **unconfined process of the same user** reading the owner token file or editing the database. This is
  the stated boundary, not a bug: the foundation protects against untrusted executors that are confined,
  not against code you run unconfined as yourself.
- A **compromised host, physical access, or whoever may talk to the Docker daemon** (the daemon is root).
- Behaviour already listed as a **known gap** in the trust-boundary page, unless you show it is worse than
  described.
- Vulnerabilities in a third-party dependency with no effect on the foundation. Please still tell us.
- Missing hardening with no demonstrated effect.

## How to report

Please do **not** put vulnerability details in a public issue, and do not disclose them publicly before
the maintainer has had time to fix them.

GitHub's private vulnerability reporting is the intended route:

- Use the repository **Security** tab's **"Report a vulnerability"** if it is offered.
- If it is not offered, open a public issue titled **"security contact request"** that contains **no
  details at all** (no description, no component, no proof of concept) and ask for a private channel. The
  maintainer will answer there and move the conversation off the public page.

A useful report contains:

1. what you found, in a sentence;
2. the commit or branch and the platform (macOS version, Linux distribution, Docker version) you tested;
3. the exact steps or a minimal script that reproduces it, and what you expected and saw;
4. the effect: what an attacker gains, and what they already need (a local account, a confined agent,
   network access);
5. whether it touches the owner token, the database, the evidence chain or spend;
6. a suggested fix, if you have one (optional).

Do not include real secrets, tokens or personal data; a placeholder is enough.

## What the project does not promise

This is one maintainer's project. There is **no service-level agreement**: no guaranteed time to
acknowledge, assess or fix a report. There is **no bug bounty** and no payment of any kind. A report
will be acknowledged and answered as soon as the maintainer is able, a fix will be made where one is
warranted, and the reporter will be credited in the change if they wish, and coordinated with on
disclosure timing. Nothing here is legal advice or a warranty; see the [licence](LICENSE).
