#!/bin/sh
# Tests for scripts/check-required.sh, the fail-loud part of the handoff gate: a required check
# that is missing from the manifest, whose script does not exist, that does not print its
# success marker (a stub that only exits 0) or that fails must make the gate exit nonzero. A gate
# that passes with a required check absent is how a green run once meant "fewer checks ran".
#
#   sh scripts/test-check-required.sh
set -u

HERE=$(cd "$(dirname "$0")" && pwd)
SUT="$HERE/check-required.sh"
WORK=$(mktemp -d "${TMPDIR:-/tmp}/check-required-test.XXXXXX")
trap 'rm -rf "$WORK"' EXIT

FAILED=0
expect() {
  if [ "$2" -eq 0 ]; then
    echo "  ok    $1"
  else
    echo "  FAIL  $1"
    FAILED=1
  fi
}

if [ ! -f "$SUT" ]; then
  echo "  FAIL  scripts/check-required.sh does not exist"
  exit 1
fi

# A fixture tree: one good check, one that fails, one stub that exits 0 and prints nothing.
ROOT="$WORK/root"
mkdir -p "$ROOT/checks"
printf '#!/bin/sh\necho "good check ran"\necho "good passed"\nexit 0\n' > "$ROOT/checks/good.sh"
printf '#!/bin/sh\necho "bad check ran"\necho "bad passed"\nexit 3\n' > "$ROOT/checks/fails.sh"
printf '#!/bin/sh\nexit 0\n' > "$ROOT/checks/stub.sh"
printf 'print("py check ran")\nprint("py passed")\n' > "$ROOT/checks/good.py"
printf 'import sys\nprint("py bad passed")\nsys.exit(4)\n' > "$ROOT/checks/fails.py"
printf 'pass\n' > "$ROOT/checks/stub.py"

run() { # run MANIFEST_BODY [extra args...]: the output is in $WORK/out, the status is returned
  body=$1; shift
  printf '%s\n' "$body" > "$WORK/manifest"
  sh "$SUT" --root "$ROOT" "$@" "$WORK/manifest" > "$WORK/out" 2>&1
}

echo "the checker's own behavior"

run 'good|good passed|sh checks/good.sh' --require good
expect "a required check that runs and prints its marker passes (exit 0)" $?
grep -q 'ran 1, passed 1, failed 0, missing 0' "$WORK/out"
expect "it reports exactly what ran: ran 1, passed 1, failed 0, missing 0" $?

run 'good|good passed|sh checks/good.sh' --require isolation
[ $? -ne 0 ]
expect "negative control: a required check absent from the manifest exits nonzero" $?
grep -q 'required check isolation is not in the manifest' "$WORK/out"
expect "  and it says which one is missing" $?

run 'iso|good passed|sh checks/not-there.sh' --require iso
[ $? -ne 0 ]
expect "negative control: a required check whose script does not exist exits nonzero" $?
grep -q 'MISSING iso' "$WORK/out"
expect "  and it is reported MISSING, not skipped" $?

run 'iso|never printed|sh checks/stub.sh' --require iso
[ $? -ne 0 ]
expect "negative control: a stub that exits 0 without its marker exits nonzero" $?
grep -q 'NO-MARKER iso' "$WORK/out"
expect "  and it is reported NO-MARKER" $?

run 'iso|bad passed|sh checks/fails.sh' --require iso
[ $? -ne 0 ]
expect "a check that fails exits nonzero even when its marker was printed" $?
grep -q 'FAIL iso (exit 3)' "$WORK/out"
expect "  and it reports the exit code" $?

run '' --require iso
[ $? -ne 0 ]
expect "an empty manifest exits nonzero" $?

sh "$SUT" --root "$ROOT" --require iso "$WORK/no-such-manifest" > "$WORK/out" 2>&1
[ $? -ne 0 ]
expect "a missing manifest exits nonzero" $?

run 'good|good passed|sh checks/good.sh
iso|never printed|sh checks/stub.sh' --require good --require iso
[ $? -ne 0 ]
expect "one failing check among passing ones fails the whole gate" $?
grep -q 'ran 2, passed 1, failed 1, missing 0' "$WORK/out"
expect "  and the summary counts it: ran 2, passed 1, failed 1, missing 0" $?

# A python3 command: the interpreter is not a file under the root, the script after it is.
if command -v python3 > /dev/null 2>&1; then
  run 'py|py passed|python3 checks/good.py' --require py
  expect "a python3 check whose script exists runs and passes (exit 0)" $?
  grep -q 'ok   py' "$WORK/out" && grep -q 'ran 1, passed 1, failed 0, missing 0' "$WORK/out"
  expect "  and it is reported ok, ran 1, passed 1, not MISSING" $?

  run 'py|py passed|python3 checks/not-there.py' --require py
  [ $? -ne 0 ]
  expect "negative control: a python3 check whose script does not exist exits nonzero" $?
  grep -q 'MISSING py (checks/not-there.py does not exist' "$WORK/out"
  expect "  and it is reported MISSING for the script, not for python3" $?

  run 'py|py passed|python3' --require py
  [ $? -ne 0 ]
  expect "negative control: a bare python3 with no script exits nonzero" $?
  grep -q 'MISSING py' "$WORK/out"
  expect "  and it is reported MISSING" $?

  run 'py|py passed|python3 -c print("py passed")' --require py
  [ $? -ne 0 ]
  expect "negative control: python3 -c (no script file to validate) exits nonzero" $?
  grep -q 'MISSING py' "$WORK/out"
  expect "  and it is reported MISSING, never run" $?

  run 'py|never printed|python3 checks/stub.py' --require py
  [ $? -ne 0 ]
  expect "negative control: a python3 stub that exits 0 without its marker exits nonzero" $?
  grep -q 'NO-MARKER py' "$WORK/out"
  expect "  and it is reported NO-MARKER" $?

  run 'py|py bad passed|python3 checks/fails.py' --require py
  [ $? -ne 0 ]
  expect "a python3 check that fails exits nonzero even when its marker was printed" $?
  grep -q 'FAIL py (exit 4)' "$WORK/out"
  expect "  and it reports the exit code" $?

  # No python3 on PATH: fail closed. Give the runner only the tools it uses, minus python3.
  BIN="$WORK/bin"
  mkdir -p "$BIN"
  for tool in dirname mktemp rm grep tail sed cat; do
    found=$(command -v "$tool") && ln -s "$found" "$BIN/$tool"
  done
  printf '%s\n' 'py|py passed|python3 checks/good.py' > "$WORK/manifest"
  PATH="$BIN" "$(command -v sh)" "$SUT" --root "$ROOT" --require py "$WORK/manifest" > "$WORK/out" 2>&1
  [ $? -ne 0 ]
  expect "negative control: python3 not on PATH exits nonzero (fails closed, never skipped)" $?
  grep -q 'MISSING py (python3 is not on PATH' "$WORK/out"
  expect "  and it is reported MISSING for the interpreter" $?
else
  echo "  FAIL  python3 not on PATH here: the python3 command controls could not run"
  FAILED=1
fi

echo "the real manifest"

sh "$SUT" --list "$HERE/required-checks.txt" > "$WORK/out" 2>&1
expect "scripts/required-checks.txt is readable and parses" $?
grep -q '^isolation|' "$HERE/required-checks.txt"
expect "the manifest names the isolation check" $?
grep -q 'deploy/compose/check-isolation.sh' "$HERE/required-checks.txt"
expect "  and its command is deploy/compose/check-isolation.sh" $?
[ -f "$HERE/../deploy/compose/check-isolation.sh" ]
expect "that script exists in the tree" $?
grep -q '^image-workflow|.*|python3 scripts/test-image-workflow.py$' "$HERE/required-checks.txt"
expect "the manifest lists the image-workflow check with its python3 command unchanged" $?
[ -f "$HERE/test-image-workflow.py" ]
expect "  and scripts/test-image-workflow.py exists in the tree" $?
grep -q -- '--require isolation ' "$HERE/gate.sh"
expect "scripts/gate.sh requires the isolation check by name" $?
grep -q '^isolation-self-test|.*check-isolation.sh --self-test' "$HERE/required-checks.txt"
expect "the manifest names the isolation check's own test (BusyBox-style grep control)" $?
grep -q -- '--require isolation-self-test' "$HERE/gate.sh"
expect "scripts/gate.sh requires the isolation self-test by name" $?
sh "$HERE/../deploy/compose/check-isolation.sh" --self-test > "$WORK/out" 2>&1
expect "the isolation self-test passes on this host's grep (exit 0)" $?
grep -q 'negative control: with a grep that rejects --exclude' "$WORK/out"
expect "  and it ran the BusyBox-style negative control" $?

echo
if [ "$FAILED" -ne 0 ]; then
  echo "check-required tests failed"
  exit 1
fi
echo "check-required tests passed"
