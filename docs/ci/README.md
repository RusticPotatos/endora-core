Status: PLANNED · a proposed workflow for the foundation checks; nothing here runs in CI until the owner applies it

# Foundation CI (proposed)

`scripts/gate-foundation.sh` checks the foundation crates only (`endora-kernel`, `endora-persistence`,
`endora-foundation`, `endora-foundation-observe`, `endora-foundation-git`, `endora-foundation-cli`): fmt,
clippy with `-D warnings`, tests, the foundation smoke script and the docs/example check. It prints
`FOUNDATION: passed=N failed=N ignored=N` and the line `legacy workspace tests: NOT RUN by this script`. With
`--strict` a skipped confirmation test (Docker, the macOS sandbox) fails, and so does any ignored test.
`scripts/gate-foundation-self-test.sh` shows the script fails when it should.

`foundation.proposed.yml` (named so editors treat it as YAML) is a GitHub Actions workflow that runs that script on ubuntu and macOS for pushes and
pull requests on `fix/**`, `feat/**`, `foundation-v1` and `docs/**`. It sits here, outside `.github`, because
pushing a workflow file needs a token with the `workflow` scope.

To apply it, the owner runs `gh auth refresh -s workflow`, then moves it to `.github/workflows/foundation.yml`
and pushes. It has not been run on GitHub Actions, and the script has not been run on Linux.

The legacy CI (`.github/workflows/ci.yml`) is unchanged: it runs the whole workspace (HQ, node, sound, the
handheld terminal) and its `--remote` step is red for a legacy reason. That red step is a legacy check, not a
foundation failure; the foundation workflow does not run it.
