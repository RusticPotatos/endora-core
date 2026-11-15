Status: CURRENT · glossary; each entry says whether the thing exists

# Terminology

| Term | Meaning | Exists? |
|---|---|---|
| **Core** | the crate `endora-foundation`: holds goals and work, decides, runs through one port, records, requires acceptance, recovers. Knows no model and no capability by name. | CURRENT |
| **Capability** | what Core calls the world through: a Rust type implementing the `Capability` trait, **compiled into the binary**. Two ship: observe (`probe.run`) and git (`git.prepare`, `git.change`, `git.verify`, `git.publish` always refused). | CURRENT; the trait is EXPERIMENTAL |
| **Call** | one named operation a capability offers, declared by a `CallSpec` (band, tier, maximum cost/seconds/turns, input and fact fields, budget mode). | CURRENT; EXPERIMENTAL |
| **Plugin** | **not a thing today.** There is no loader, no manifest, no way to add a capability without compiling it in. A capability is not automatically a plugin. | PLANNED at most; not committed |
| **Agent** | software that proposes or does work, for example the Claude Code CLI that `git.change` can run. An agent is not a capability: the capability confines and measures it. | EXPERIMENTAL (macOS only) |
| **Skill** | an HQ-era concept (an agent's instruction bundle). Not part of the foundation. | LEGACY |
| **Model** | Core knows none. One can sit behind an executor; no model integration is specified. | not part of Core |
| **Executor** | what a capability runs under confinement: a script, or the Claude Code CLI. | CURRENT (script), EXPERIMENTAL (real agent) |
| **Probe** | an observe call you define in the config file (`probe NAME tcp\|http\|command TARGET SECONDS`); runs as `probe.run NAME`. | CURRENT |
| **Band / tier** | band = reversibility class of a call (`observe`, `reversible`, `outward_reversible`, `irreversible`); tier = data sensitivity (`public`, `internal`, `confidential`, `restricted`). Policy metadata, not the boundary. | CURRENT |
| **Fact** | a typed value (number, yes/no, or a word from a closed list the call declares) that capability code observed. The only thing a criterion's check can read. | CURRENT |
| **Evidence** | records written before (intent) and after (result) each call and for decisions; append-only through the engine; the hash chain adds detection. | CURRENT; chain EXPERIMENTAL |
| **Goal / item / routine** | goal = what and why, scope, criteria, cap; item = one unit of work under a goal; routine = a goal's rhythm, each firing makes one item. | CURRENT; FROZEN |
| **Owner / actor** | the person who decides, and the named kinds of caller; `OwnerProof` is defence in depth. | CURRENT; EXPERIMENTAL |
| **Trust boundary** | process and filesystem isolation of untrusted executors. Nothing else (see [trust-and-spend](trust-and-spend.md)). | CURRENT |

## How they relate

A **capability** *may run* an **executor**; an executor *may be* an **agent**; an agent *may use* a
**model**. Each arrow, as it stands:

- capability → executor: **CURRENT** for the git capability (a script or the Claude Code CLI, confined);
  the observe capability runs only the commands you write in the config.
- executor → agent: **EXPERIMENTAL**. One real agent has been run (Claude Code, three runs on macOS).
  Real agents under Docker are **UNSUPPORTED**.
- agent → model: **not specified.** Core is not told which model, if any; the Claude Code CLI uses its
  own. Nothing in Core depends on it.
- capability → Core: the capability implements a trait Core calls (CURRENT). Core never names a
  capability.
- a plugin loader between your code and Core: **does not exist**; a capability reaches Core by being
  compiled into a binary that depends on `endora-foundation`.
