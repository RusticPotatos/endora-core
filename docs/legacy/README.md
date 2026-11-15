Status: LEGACY · index of the HQ-era material; true for its time, not the foundation

# Legacy: Endora HQ and the node

Before the foundation, this repository held **Endora HQ**: a team of AI agents run by a model-backed
**node** with a web console, deployed with Docker Compose or Ansible. That product is not the
foundation. Its code is still in the tree (`app/node`, `app/hq`, `domains/team`, `domains/platform`,
`domains/capabilities`, and others) and its documents are kept, labelled `Status: LEGACY` in their first
line. Nothing here is a description of how the foundation behaves.

If you came for the foundation: [README](../../README.md), then [docs/README.md](../README.md).

| Material | Where | Note |
|---|---|---|
| The former front page (HQ, `make run-node`) | [README-HQ.md](README-HQ.md) | moved here from the repository root |
| Installing and running HQ | [install](../install.md), [hosting](../hosting.md), [homelab](../homelab.md), [model-hosting](../model-hosting.md), [recovery](../recovery.md), [releasing-core-image](../releasing-core-image.md), [terminal-console](../terminal-console.md) | node and console deployments |
| Console contract | [api/console-v1](../api/console-v1.md) | HTTP + JSON + SSE, node era |
| Deployment recipes | `deploy/compose/`, `deploy/ansible/` | compose and Ansible for the node; each README carries the label |
| HQ-era design documents | [domain-map](../domain-map.md), [where-things-go](../where-things-go.md) (a console map), [patterns](../patterns.md), [constitution](../constitution.md), [roadmap](../roadmap.md), [direction-reset](../direction-reset.md), [core-origin](../core-origin.md), [standards](../standards.md), [testing](../testing.md) | some principles carry over (see [architecture](../architecture.md)); check there first |
| Slice 1 report | [foundation-slice1/](../foundation-slice1/SLICE1-REPORT.md) | an early foundation report; several statements are superseded |
| Proposals and plans | `docs/proposals/`, `docs/plans/` | working papers, kept as written |
| Architecture decision records | [adr/](../adr/README.md) | numbers 0001 to 0149 are history; the current architecture is [ADR 0150](../adr/0150-current-architecture-the-foundation.md) (Proposed) |

The HQ-era architecture text is kept at the end of [architecture.md](../architecture.md), labelled LEGACY.
