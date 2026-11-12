# Publishing Core's image

Core's image is `ghcr.io/rusticpotatos/endora-node`; `deploy/compose` pins it
([ADR 0147](adr/0147-full-system-bundle.md)). **Nothing publishes it automatically.** The
workflow `.github/workflows/image.yml` builds the image and checks that nothing local is in it
on every push to `develop` or `main` and every `v*` tag, and stops there: no login, no push, no
tag, `latest` untouched. Pushing to `main`, pushing a tag and merging a pull request never write
a package. This keeps the first public push from touching any image already in the registry.

## Before the owner turns publication on

**Nothing publishes until then.** The repository variable `ENDORA_CORE_IMAGE_PUBLISH` is the owner's
switch; setting it, and starting the workflow by hand afterwards, are the only steps that can write
a package. Preparation never does either.

| # | Must be true | State |
|---|---|---|
| 1 | The owner sets `ENDORA_CORE_IMAGE_PUBLISH` to `enabled` (below, step 1) | **open: the owner's** |
| 2 | A person (the owner, or someone the owner names) starts *Core image* by hand with **Publish** ticked from `main` or a `vX.Y.Z` tag (below, step 2); no agent, push, tag or schedule does | **open: at the time** |
| 3 | Third-party notices cover what the image ships: [THIRD-PARTY-NOTICES.md](../THIRD-PARTY-NOTICES.md) | done; **regenerate it for the release**: build the image, run `sh scripts/gen-third-party-notices.sh <image> > THIRD-PARTY-NOTICES.md`, commit the result **before the release build** (the file is copied into the image, so the release build carries the one you committed). The output reads the same from an amd64 and an arm64 build; the Debian packages it lists are the ones the archive gave at that build |
| 3b | The image carries `LICENSE`, `NOTICE`, the notices and the licence texts of the Rust crates, in `/usr/share/doc/endora-core` (decided 12 Nov 2026, option B) | done: built into the image; its Dockerfile blob is excused by the existing value-exception manifest, unchanged |
| 4 | The source offer for the GPL and LGPL parts is stated (section 5 of the notices): which components, their licences, where the source is, a written offer through this repository's issues | done: the written offer stands as drafted (the owner accepted it, 12 Nov 2026); the owner answers requests made through the issues |
| 5 | The crates' licence texts accompany the binaries | done: the image carries them (`licenses/`, one folder per crate and an index), gathered at build time by `scripts/collect-crate-licences.sh` |
| 5b | The base images are pinned by digest (`rust:1.87-bookworm@sha256:…`, `debian:bookworm-slim@sha256:…`, the multi-platform index digests), with the update steps in a Dockerfile comment | done; an apt snapshot date, which would also freeze the packages the last stage installs, is **deferred** |
| 6 | The image privacy check fails when its own `find` fails, not only when it finds a file (`scripts/test-image-privacy.sh` proves it) | done |
| 7 | The bundle's pin check accepts `sha-<40 hex>`, `X.Y.Z` and either with `@sha256:<64 hex>` (`check-isolation.sh`); the examples keep the `sha-` form | done; set the digest form after the first publish if you want an immutable pin |
| 8 | What the registry holds is looked at first: a tag already there is overwritten by a push of the same tag (see below) | **open: at the time** |

## What a publish needs (all of it)

1. **Release publishing is turned on**: the repository variable `ENDORA_CORE_IMAGE_PUBLISH` is
   set to exactly `enabled` (GitHub: Settings → Secrets and variables → Actions → Variables).
   This is the owner's switch. Unset it, or set anything else, and no run can publish.
2. **A person starts the workflow by hand**: Actions → *Core image* → *Run workflow*, with
   **Publish** ticked. A push, a tag, a schedule or a pull request cannot publish.
3. **From `main` or a `vX.Y.Z` tag**, in `RusticPotatos/endora-core` (not a fork, not another
   repository).
4. **The build-and-check job passes first.** The publishing job needs it, so an image that carries
   a local file is never pushed.

## What it publishes

| Run from | Tags written |
|---|---|
| `main`, *Move latest* not ticked | `sha-<commit>` only |
| `main`, *Move latest* ticked | `sha-<commit>` and `latest` |
| a `vX.Y.Z` tag | `sha-<commit>` and `X.Y.Z` (never `latest`) |

`latest` moves only when someone ticks *Move latest* from `main`. A tag that already exists in the
registry is overwritten by a new push of the same tag, so check what is there before you publish
a version or move `latest`, and publish a release from a tag so the version is `X.Y.Z`.

## Turning it back off

Delete the variable (or set it to anything but `enabled`). The next run cannot publish.

## How the rules are held

`scripts/test-image-workflow.py` reads the real workflow and evaluates its conditions against
every event that matters (a push to `main` with the variable enabled, a tag, a pull request, a
dispatch without `publish`, a fork, a non-version tag, and so on), and checks that exactly one
job holds `packages: write`, that it is the only one that logs in or pushes, and which tags a
publish would give. CI runs it on every push (`ci.yml`), and `scripts/gate.sh` requires it.
