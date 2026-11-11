# Publishing Core's image

Core's image is `ghcr.io/rusticpotatos/endora-node`; `deploy/compose` pins it
([ADR 0147](adr/0147-full-system-bundle.md)). **Nothing publishes it automatically.** The
workflow `.github/workflows/image.yml` builds the image and checks that nothing local is in it
on every push to `develop` or `main` and every `v*` tag, and stops there: no login, no push, no
tag, `latest` untouched. Pushing to `main`, pushing a tag and merging a pull request never write
a package. This keeps the first public push from touching any image already in the registry.

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
