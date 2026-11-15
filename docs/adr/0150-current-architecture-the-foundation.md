# 0150 - Current architecture: the foundation

## Status

Proposed, 2026-11-15. AI-assisted (Claude). Does not edit [0050](0050-the-shape-of-the-system.md),
which stays as written, as history. HQ-era decisions (0001 to 0149) remain history; nothing in them is
deleted.

## Context

The foundation (Core plus replaceable capabilities, driven from a command line) replaced the HQ
product as the current architecture. No record says so, and 0050 mixes principles that the
foundation keeps with decisions it no longer carries. This record separates them, so a reader can
tell which rules still apply, and lists where the code departs from them. Internally the project
called this discipline ROCA in 0050; it is established Domain-Driven Design modularity plus Clean
Architecture, not a framework of its own.

## Decision

**Retained principles** (each checked against the code in [the architecture page](../architecture.md)):

- *Responsibility-based modularity.* Code is organised by what it is responsible for (Core:
  hold, decide, run, record, require, recover; capabilities: one crate per integration), not by
  technical layer across the whole system.
- *Dependencies point inward.* Core's `domain` imports only `endora-kernel`. Capability crates depend
  on Core; Core never names a capability.
- *Domain isolation.* The foundation's rules run without a database or a model in tests
  (`Core::in_memory`).
- *Deliberate interface boundaries.* `Capability` is the one port Core calls the world through.

**Superseded decisions** (0050):

- The node as the single authority and trust boundary. The trust boundary is now process and
  filesystem isolation of untrusted executors (`docs/foundation/TRUST-BOUNDARY.md`).
- Versioned HTTP + JSON with SSE as the application protocol. The foundation has none: a command line
  over one file, and a config file.
- The embedded console, and the single container image that serves it.
- The list of five bounded contexts (platform, capabilities, team, sound, mail); the foundation is
  not one of them.

**Independent choices** (implementation choices, not requirements of the principles; each can change
without changing them): Rust; SQLite in one file; the command line and the config file as the
transport; macOS `sandbox-exec` and Docker per call as the confinement backends.

**Structure follows responsibility.** There is no mandatory three-layer template. `domain` /
`application` / `infrastructure` modules are justified for Core because it owns durable state, policy,
execution and recovery. A small capability can be one Rust module. A bounded context is not a crate.
Nothing about Endora's internal structure is imposed on an external integration: today the contract
is a Rust trait compiled in, with no wire protocol, schema, SDK, plugin loader or conformance suite.

## Known deviations

The code departs from the retained principles in three places. They are documented, not fixed
(decision of 15 Nov 2026: no code change and no new abstraction in this increment):

1. **No storage port.** `application` imports the concrete SQLite `Store` and a `ManualClock` from
   `infrastructure`. 0050 said storage is reached only through application-defined ports.
2. **Capability crates depend on Core's whole crate**, including its SQLite store.
3. **Domain purity is by convention.** The three layers are modules of one crate that depends on
   `rusqlite` and `sha2`; the compiler does not stop a domain file from importing them.

## Consequences

- The README and `docs/` split into current and legacy material; ADR 0050 is not rewritten.
- The architecture page states the placement rule (config line, one module, adapter, new crate, new
  context) and the evidence for the claim that capabilities are independent of Core, including what
  that evidence does not show.
- If a second store, a fake store or a non-SQLite engine is needed, extract a storage port then.
