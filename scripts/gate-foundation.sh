#!/bin/sh
# The foundation-only gate: checks the foundation crates and nothing else.
#   sh scripts/gate-foundation.sh [--strict]
#   sh scripts/gate-foundation.sh --parse [--strict] < cargo-test-output   (the parser alone, used by the self-test)
#
# It is NOT the legacy workspace gate (scripts/gate.sh and .github/workflows/ci.yml run the whole workspace:
# HQ, node, sound, the terminal client and the rest). The numbers printed here are summed over the foundation
# packages below only and must never be read as a workspace count.
#
# Steps, in order; the first failure stops the script with a non-zero exit:
#   1. cargo fmt --check             foundation packages only
#   2. cargo clippy -D warnings      foundation packages, all targets and features
#   3. cargo test                    foundation packages, all features; output parsed into the final block
#   4. scripts/foundation-smoke.sh   against a freshly built endora-foundation binary
#   5. scripts/check-docs.sh         with RUN_EXAMPLE set, so examples/a-observe/run.sh runs too
#
# --strict: a skipped confirmation test is a failure. Precedent in this repo: ENDORA_STRICT_TESTS=1 (set by
# scripts/gate.sh on macOS) makes the tests that skip when Docker, the macOS sandbox or the executor image is
# missing fail instead (app/foundation/tests/cli.rs, domains/foundation-git/tests/docker.rs and
# macos_profile_hardening.rs). --strict sets it for step 3, and ALSO fails when the `ignored` count is not
# EXPECTED_IGNORED below. The foundation crates have no #[ignore] tests, but the four Docker-backend tests in
# domains/foundation-git/tests/docker.rs are compiled out as `ignored` on every host except Linux ("tested on
# Linux hosts only"), so the expected count is 0 on Linux and 4 elsewhere (macOS). A strict run on a Mac
# therefore does NOT exercise the Docker backend; only a strict Linux run does. Change the number in the same
# commit that adds or removes an ignored test, never to make a red run pass. GATE_EXPECTED_IGNORED overrides it.
#
# Needs no network beyond what cargo itself needs (crates.io for the first build). It writes only inside the
# repository: logs under target/gate-foundation/ (or $CARGO_TARGET_DIR); foundation-smoke.sh keeps its own
# temporary directory and removes it.
set -eu

# Cargo package names (the `name` of each Cargo.toml: shared/kernel, shared/persistence, domains/foundation,
# domains/foundation-observe, domains/foundation-git, app/foundation).
PKGS="endora-kernel endora-persistence endora-foundation endora-foundation-observe endora-foundation-git endora-foundation-cli"
if [ "$(uname -s)" = Linux ]; then EXPECTED_IGNORED=0; else EXPECTED_IGNORED=4; fi
EXPECTED_IGNORED=${GATE_EXPECTED_IGNORED:-$EXPECTED_IGNORED}
CARGO=${GATE_CARGO:-cargo}   # overridable so the self-test can stand in a fake cargo

STRICT=0
PARSE_ONLY=0
for a in "$@"; do
    case $a in
        --strict) STRICT=1 ;;
        --parse) PARSE_ONLY=1 ;;
        -h|--help) sed -n '2,5p' "$0"; exit 0 ;;
        *) echo "gate-foundation: unknown argument $a" >&2; exit 2 ;;
    esac
done

print_block() {
    echo "FOUNDATION: passed=$PASSED failed=$FAILED ignored=$IGNORED"
    echo "legacy workspace tests: NOT RUN by this script"
}

# summarise <file-with-cargo-test-output>
# Sums the `test result:` lines (unit, integration and doc tests of every foundation package), prints the final
# block and returns 0 only if there were results, none failed and (strict) the ignored count is the expected one.
summarise() {
    awk '
        /^test result: / {
            n++
            for (i = 1; i <= NF; i++) {
                if ($i == "passed;") p += $(i - 1)
                else if ($i == "failed;") f += $(i - 1)
                else if ($i == "ignored;") g += $(i - 1)
            }
        }
        END { printf "%d %d %d %d\n", n, p, f, g }
    ' "$1" > "$1.sum"
    read -r RESULTS PASSED FAILED IGNORED < "$1.sum"
    rm -f "$1.sum"
    print_block
    [ "$RESULTS" -gt 0 ] || { echo "gate-foundation FAIL: no 'test result:' line in the cargo test output" >&2; return 1; }
    [ "$FAILED" -eq 0 ] || { echo "gate-foundation FAIL: $FAILED test(s) failed" >&2; return 1; }
    if [ "$STRICT" -eq 1 ] && [ "$IGNORED" -ne "$EXPECTED_IGNORED" ]; then
        echo "gate-foundation FAIL (--strict): ignored=$IGNORED, expected $EXPECTED_IGNORED" >&2
        return 1
    fi
    return 0
}

if [ "$PARSE_ONLY" -eq 1 ]; then
    T=$(mktemp "${TMPDIR:-/tmp}/gate-foundation-parse.XXXXXX")
    trap 'rm -f "$T" "$T.sum"' EXIT INT TERM
    cat > "$T"
    summarise "$T"
    exit $?
fi

cd "$(dirname "$0")/.."
TD=${CARGO_TARGET_DIR:-target}
LOGDIR=$TD/gate-foundation
mkdir -p "$LOGDIR"

# -p <name> for every foundation package
set --
for p in $PKGS; do set -- "$@" -p "$p"; done

step() { echo "gate-foundation: $*"; }
die() { echo "gate-foundation FAIL: $*" >&2; exit 1; }

step "1/5 fmt (foundation packages)"
"$CARGO" fmt "$@" -- --check || die "cargo fmt --check"

step "2/5 clippy -D warnings (foundation packages)"
"$CARGO" clippy "$@" --all-targets --all-features -- -D warnings || die "cargo clippy"

step "3/5 test (foundation packages$([ "$STRICT" -eq 1 ] && echo ', strict'))"
[ "$STRICT" -eq 1 ] && export ENDORA_STRICT_TESTS=1
LOG=$LOGDIR/test.log
RC=0
"$CARGO" test "$@" --all-features -- --test-threads=4 > "$LOG" 2>&1 || RC=$?
if [ "$RC" -ne 0 ]; then tail -60 "$LOG"; else grep '^test result:' "$LOG" || true; fi
SUMMARY_RC=0
summarise "$LOG" || SUMMARY_RC=$?
[ "$RC" -eq 0 ] || die "cargo test exited $RC (log: $LOG)"
[ "$SUMMARY_RC" -eq 0 ] || die "the test summary failed (log: $LOG)"

step "4/5 smoke (endora-foundation binary)"
"$CARGO" build "$@" || die "cargo build"
BIN=$(pwd)/$TD/debug/endora-foundation
case $TD in /*) BIN=$TD/debug/endora-foundation ;; esac
sh scripts/foundation-smoke.sh "$BIN" || die "scripts/foundation-smoke.sh"

step "5/5 docs and the checked example"
RUN_EXAMPLE=$BIN sh scripts/check-docs.sh || die "scripts/check-docs.sh"

echo "gate-foundation: OK (foundation only)"
print_block
