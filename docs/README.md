Status: CURRENT · entry points for the foundation; HQ-era material is in docs/legacy/

# Documentation

<!-- INTERIM-NOTICE:BEGIN delete this whole block, and every line marked INTERIM-NOTICE, once the foundation is merged into main -->
> **Interim notice (15 Nov 2026).** The foundation (Core, its capabilities, the `endora-foundation`
> command and these pages) currently lives on the branch `docs/developer-docs-v1`. The default branch
> `main` still holds the earlier HQ product until the foundation is merged, so a clone of `main` does
> not contain the code these pages describe. **Until then, check out `docs/developer-docs-v1` and
> branch from it, not from `main`.**
<!-- INTERIM-NOTICE:END -->

Every page starts with a `Status:` line using one of five labels: **CURRENT** (matches the code),
**EXPERIMENTAL** (works, may change), **LEGACY** (the earlier HQ product or design, true for its
time), **UNSUPPORTED** (known not to work or not to be relied on), **PLANNED** (not built). Interface
rows in [STABILITY.md](foundation/STABILITY.md) use FROZEN or EXPERIMENTAL instead. Run
`sh scripts/check-docs.sh` to check links, cited files and these lines.

Three entry points. Pick by what you are doing.

## Use it

*Audience: someone with Rust and a terminal who wants to run the foundation.*

| Page | Status | What it gives you |
|---|---|---|
| [start.md](start.md) | CURRENT | a ten-minute first run (example A), every flag explained |
| [foundation/README.md](foundation/README.md) | CURRENT | the reference: commands, config lines, authority, spend, confinement |
| [operate.md](operate.md) | EXPERIMENTAL | running it for longer: service, watching, backup and restore, upgrade, growth |
| [trust-and-spend.md](trust-and-spend.md) | CURRENT | what is a boundary, what is metadata, what spend limits really hold |
| [foundation/STABILITY.md](foundation/STABILITY.md) | CURRENT | which interfaces are FROZEN and which EXPERIMENTAL, with evidence |
| [foundation/TRUST-BOUNDARY.md](foundation/TRUST-BOUNDARY.md) | CURRENT | the security statement and its known gaps |
| [../deploy/foundation/README.md](../deploy/foundation/README.md) | EXPERIMENTAL | running it in a container |

## Build on it

*Audience: a Rust developer adding a capability or integrating a tool.*

| Page | Status | What it gives you |
|---|---|---|
| [terminology.md](terminology.md) | CURRENT | capability, call, plugin (not a thing today), agent, skill (legacy), executor, probe, fact, evidence, trust boundary |
| [architecture.md](architecture.md#what-an-integration-must-satisfy-today) | CURRENT | the capability contract exactly as it is today; what is not specified |
| [architecture.md](architecture.md#where-does-my-code-go) | CURRENT | where your code goes: config line, one module, adapter, new crate, new context |
| [foundation/README.md](foundation/README.md#adding-a-capability-one-page) | EXPERIMENTAL | "Adding a capability", seven steps; the contract is only as stable as the call-spec row in STABILITY |

There is no published crate, plugin loader, wire protocol, SDK or tutorial for writing a capability yet
(PLANNED pages: a worked capability example and an integration guide).

## Contribute

*Audience: someone changing Core, the capabilities, or these pages.*

| Page | Status | What it gives you |
|---|---|---|
| [../CONTRIBUTING.md](../CONTRIBUTING.md) | CURRENT | branch from `docs/developer-docs-v1` for now (see the notice above), tests first, ADRs for architecture changes |
| [architecture.md](architecture.md) | CURRENT | the six responsibilities, the crates as they are, the evidence for the architecture, known deviations |
| [adr/0150-current-architecture-the-foundation.md](adr/0150-current-architecture-the-foundation.md) | Proposed ADR | what of ADR 0050 is retained, superseded and independent |
| [adr/README.md](adr/README.md) | LEGACY index (0001 to 0149) | history; 0050 is unedited |

## Legacy

[legacy/](legacy/README.md) indexes the HQ product's documents (install, hosting, console contract,
design papers, proposals, plans). They are kept and labelled; they do not describe the foundation.
