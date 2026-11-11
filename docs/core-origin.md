# Where Endora Core came from

Source repository: RusticPotatos/Endora
Source commit: baf37a7d1d6fe9b63426b23e1dd51f27eb7fb02d

This repository is **Endora Core** (milestone g, decision RP1): the headless brain, HQ's engine,
the node, the Ansible collection, the full-system bundle and every gate that guards them. Its
history starts fresh, as one parentless commit. Its tree is the repaired state of later Core work
on that source: the source commit's tree, plus the changes made to Core since, plus the privacy
remediation (below). It is not the source tree verbatim. Measured against the source commit it
differs by 309 files (16 added, 72 deleted, 216 modified, 5 renamed). `RusticPotatos/Endora` is
public, keeps its name and its history, and that history stays public.

`sh scripts/check-core-origin.sh` holds the tree to this record: it names a full source commit,
every workspace member and every file a workflow runs is present, and nothing private (`.local/`,
`owner.mk`, an `.env`, a key) is carried. Its own test is `scripts/test-check-core-origin.sh`,
and `scripts/gate.sh` will not run without it.

## What travelled (all of it)

| Part | Where |
|---|---|
| The workspace: the node, the CLI, HQ, the worker, the runner, the sound hub, the handheld terminal and their crates | `app/`, `domains/`, `shared/`, `crates/`, `clients/`, `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml` |
| Decisions, plans, proposals, contracts and the rules for assistants | `docs/` (the ADRs, with their archive), `AGENTS.md`, `CLAUDE.md`, `CONTRIBUTING.md`, `GOVERNANCE.md` |
| The full-system bundle, its pins and its isolation and source checks | `deploy/compose/` |
| The `endora.homelab` Ansible collection and the lab recipes | `deploy/ansible/` |
| The image, the homelab compose file and the Makefile | `Dockerfile`, `docker-compose.yml`, `Makefile`, `.dockerignore` |
| Every workflow: CI, the full-system bundle test, the image | `.github/workflows/` |
| The handoff gate, the required-checks manifest and every check script | `scripts/` |
| Assistant guards and the round playbook | `.claude/` |

## What did not travel, and why

Left out on purpose: the cartoon Office's third-party artwork (65 image and font assets under
`docs/plans/endora-hq/prototype/assets` and `app/node/src/web/hq-art`) and the code that drew it,
so this tree holds no image or font file. Two further parts could be excluded only if a dependency
proof allowed it, and it does not:

- **The sound hub** (`domains/sound`, `app/sound`, `scripts/sound-*.sh`) duplicates Endora Audio.
  No Rust crate depends on it, but the image stages in `Dockerfile`, the `sound`, `senders` and
  `airplay2` services of `docker-compose.yml`, the Makefile's sound deploy, the `endora` Ansible
  role's `endora_sound*` variables and their render tests, the CI step
  `scripts/test-sound-airplay2.sh` and the collection-parity check all still use it. They are
  how a lab that has not yet moved to Endora Audio runs its sound, and whether it has moved
  cannot be read from this machine (milestone e). Removing it is its own change, once the live
  containers are checked.
- **The handheld terminal** (`clients/terminal`) is not retired: ADR 0099 makes it part of
  Endora, and CI builds and checks it. Nothing depends on it, so it can leave later by one
  deliberate change.

## Left pointing at the source repository, on purpose

A published Core is a cutover, not a copy, so these are changed at that step, by the person who
runs it, not here:

- The collection's install line (`deploy/ansible/collection/README.md`), which the NAS's
  `requirements.yml` pins to the source repository.
- Core's image name, `ghcr.io/rusticpotatos/endora-node`, kept by ADR 0147.
- The user-agent string the node sends when it fetches reference pages
  (`domains/capabilities/src/infrastructure/skills.rs`).
- `.claude/workflows/end-of-round.js`, which names an old sibling checkout.
- Links in older ADRs and proposals, which are history.

## Privacy remediation

Before its first publication this repository's history was rebuilt once, still unpublished, from the source
snapshot above. The rebuild replaces owner-specific identifiers (host and device names, addresses, account
and unit names, the name of the personal-values variable and of the git-ignored personal make file) with
generic ones, renames the capabilities crate to `endora-bridge`, and adds `MANIFEST.txt`. Apart from
that and the later Core changes described at the top, the tree is the source snapshot; the counts
are in the first paragraph.

### What the renames mean for a machine deployed before them

A rename in a tree changes nothing on a host that already runs the old names. What is known, and
what is not (the previous names are not spelled here, on purpose; find them by comparing with the
source commit):

| Renamed | Status |
|---|---|
| The personal-values environment variable (and, in Ansible, the role variable and the vault key behind it) | **Handled in source, not proven on a host.** The node and the `endora` role accept the previous name, say so, use its value, and refuse two different lists (`scripts/test-values-compat.sh`, `roles/endora/tests/owner_values.sh`, `outgoing::tests`). The checks refuse a lone previous name with the name to use. A vault key is read through the inventory's own mapping, so only the role variable matters. **The node's Rust is unproven**: it was formatted but not compiled or tested where this was written; its tests run only on the build host (XPS). Nothing proves the node reads the value on a real deployment. |
| The same variable, run through `docker compose` directly | **Fails closed, visibly.** Compose cannot compute a name, so it cannot read the previous one. `docker-compose.yml` now requires `ENDORA_OWNER_VALUES` to be set (empty is allowed): unset stops Compose with a message that names the variable and no value. `make deploy` and the role always set it (the role renders an empty line when there are none). This changes a fresh clone's bare `docker compose up`, which now asks for `ENDORA_OWNER_VALUES= docker compose up`. Tested by parsing only (`docker compose config`, no daemon). |
| The git-ignored personal make file | **Handled in the Makefile**: the previous file name alone stops `make` with the instruction; beside `owner.mk` it is announced as unread; a previous variable name inside `owner.mk` is used. |
| The capabilities crate (package `endora-bridge`; its folder is unchanged) | **No persisted name found**: the old name appears only in Cargo manifests and `use` paths, in no string, log filter, table or key. A build command that names the old package fails loudly. Not evidence about anything outside this tree. |
| The watch role's unit, timer, script folder, runtime folder, apt drop-in and keychain item | **Not handled, not proven compatible.** Nothing in the role removes the previous units, so a host that ran the old role would keep them beside the new ones until someone removes them. Needs a migration step with the old names supplied outside this repository. |
| The Mac brain's launchd label | **Not handled.** The installer boots out only the new label, so an agent loaded under the previous label keeps running beside it. Same remedy. |
| The worksite image name and deploy folder, and any default that names an image | **Not proven.** Image and default compatibility has no test at all. The runner's default image name changed; a host still holding only the previous image tag needs the matching image shipped with the new runner (`WORKER_IMAGE` overrides it). Not tested against a live host. |
| Host and device names, addresses | **Not checked.** Whether any default the node or a script reads still points at a renamed host or device was not searched for, and no deployed inventory was compared. |

No deployment compatibility is claimed: nothing here was run against the NAS, the Pi or the Mac.
