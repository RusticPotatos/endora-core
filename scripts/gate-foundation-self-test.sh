#!/bin/sh
# Proves scripts/gate-foundation.sh fails when it should: the parser on fixed fake `cargo test` output, and the
# whole script with a fake cargo (GATE_CARGO) that fails one step. Deterministic, no build, no network:
#   sh scripts/gate-foundation-self-test.sh
set -u
cd "$(dirname "$0")/.." || exit 1
G=scripts/gate-foundation.sh
export GATE_EXPECTED_IGNORED=0   # the real default depends on the host (Linux 0, elsewhere 4)
T=$(mktemp -d "${TMPDIR:-/tmp}/gate-foundation-selftest.XXXXXX") || exit 1
trap 'rm -rf "$T"' EXIT INT TERM
FAILS=0
ok() { echo "self-test ok:   $1"; }
bad() { echo "self-test FAIL: $1" >&2; FAILS=$((FAILS + 1)); }

# expect_rc <name> <wanted-rc> <needle-in-output-or-empty> <command...>
expect_rc() {
    name=$1; want=$2; needle=$3; shift 3
    out=$("$@" 2>&1); got=$?
    [ "$got" -eq "$want" ] || { bad "$name: exit $got, wanted $want: $out"; return; }
    case $out in *"$needle"*) ok "$name" ;; *) bad "$name: output lacks \`$needle\`: $out" ;; esac
}

GOOD="$T/good.txt"
cat > "$GOOD" <<'X'
running 3 tests
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.20s
X
BADF="$T/failed.txt"
sed 's/9 passed; 0 failed/8 passed; 1 failed/' "$GOOD" > "$BADF"
IGN="$T/ignored.txt"
sed 's/3 passed; 0 failed; 0 ignored/2 passed; 0 failed; 1 ignored/' "$GOOD" > "$IGN"
NONE="$T/none.txt"
echo "error: could not compile" > "$NONE"

expect_rc "parser sums passed/failed/ignored" 0 "FOUNDATION: passed=12 failed=0 ignored=0" sh -c "sh $G --parse < $GOOD"
expect_rc "parser prints the legacy line" 0 "legacy workspace tests: NOT RUN by this script" sh -c "sh $G --parse < $GOOD"
expect_rc "a failed test fails the parser" 1 "FOUNDATION: passed=11 failed=1 ignored=0" sh -c "sh $G --parse < $BADF"
expect_rc "ignored is allowed without --strict" 0 "ignored=1" sh -c "sh $G --parse < $IGN"
expect_rc "--strict fails on an unexpected ignored test" 1 "ignored=1, expected 0" sh -c "sh $G --parse --strict < $IGN"
expect_rc "--strict accepts the count the host expects" 0 "ignored=1" sh -c "GATE_EXPECTED_IGNORED=1 sh $G --parse --strict < $IGN"
expect_rc "--strict passes a clean run" 0 "ignored=0" sh -c "sh $G --parse --strict < $GOOD"
expect_rc "no result lines is a failure" 1 "no 'test result:' line" sh -c "sh $G --parse < $NONE"

# A fake cargo: GATE_FAKE_FAIL names the subcommand that fails; it records every call.
cat > "$T/cargo" <<'X'
#!/bin/sh
echo "$1" >> "$GATE_FAKE_CALLS"
if [ "$1" = "${GATE_FAKE_FAIL:-}" ]; then
    echo "fake $1 failure" >&2
    [ "$1" = test ] && echo "test result: FAILED. 4 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s"
    exit 101
fi
[ "$1" = test ] && echo "test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s"
exit 0
X
chmod +x "$T/cargo"
run_fake() { # <fail-step> ; runs the whole script in a scratch target dir
    GATE_CARGO="$T/cargo" GATE_FAKE_FAIL=$1 GATE_FAKE_CALLS="$T/calls.$1" CARGO_TARGET_DIR="$T/target" sh $G ${2:+"$2"}
}
expect_rc "fmt failure stops the script" 1 "cargo fmt --check" run_fake fmt
[ "$(cat "$T/calls.fmt")" = "fmt" ] && ok "nothing ran after the failed fmt" || bad "steps ran after fmt failed"
expect_rc "clippy failure stops the script" 1 "cargo clippy" run_fake clippy
[ "$(tr '\n' ' ' < "$T/calls.clippy")" = "fmt clippy " ] && ok "tests did not run after clippy failed" || bad "steps ran after clippy failed"
expect_rc "a failing cargo test fails the gate and prints the count" 1 "FOUNDATION: passed=4 failed=1 ignored=0" run_fake test
expect_rc "the gate never claims a workspace count" 1 "legacy workspace tests: NOT RUN by this script" run_fake test

if [ "$FAILS" -eq 0 ]; then echo "self-test OK"; else echo "self-test: $FAILS failure(s)" >&2; exit 1; fi
