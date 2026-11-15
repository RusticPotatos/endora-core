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
  hold, decide, run, record, require, recover; capabilities: one per integration, as a module or a
  crate), not by technical layer across the whole system.
- *Dependencies point inward.* Core's `domain` imports only `endora-kernel`. Capability crates depend
  on Core; Core never names a capability.
- *Domain isolation.* The foundation's rules run without a database file or a model in tests
  (`Core::in_memory` opens an in-memory SQLite database); `domain/` itself imports no SQLite.
- *Deliberate interface boundaries.* `Capability` is the one port Core calls the world through.
- *Rust, `unsafe` forbidden* ("Rust, and no unsafe" in 0050), unchanged.

**Superseded decisions** (each is a section of 0050, checked against its text):

- *"HTTP + JSON, versioned, with SSE for liveness"* as the application protocol. The foundation has
  none: a command line over one file, and a config file.
- *"One container, serving its own console"*: the console embedded in the binary and the one image that
  ships it. The foundation has no console; its container recipe runs the command line.
- *The per-context layout*: "each context owns its own `domain` / `application` / `infrastructure`", and
  its list of contexts (`platform`, `capabilities`, `understanding`, `conversation`, `scheduling`). The
  foundation crates under `domains/` are not among them, and the three layers apply to Core only (below).
- *"SQLite, behind ports"* is kept as to SQLite and not carried out as to ports: see Known deviations.

0050's "MCP is not the application protocol" and "read projections rather than new tables" concern HQ's
clients and feeds; the foundation has neither, so they are neither retained nor contradicted.

**Not an 0050 decision, also superseded.** The earlier HQ architecture page ([architecture.md](../architecture.md),
its LEGACY section) frames the node as the single authority and trust boundary. In the foundation the
trust boundary is process and filesystem isolation of untrusted executors
(`docs/foundation/TRUST-BOUNDARY.md`).

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
