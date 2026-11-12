#!/bin/sh
# Tests for scripts/check-image-privacy.sh that need no Docker: a stand-in `docker` runs the very
# commands the script would run in the image or the build context, on this host, with a stand-in
# `find` whose behaviour the test sets. What they prove: a `find` that fails (cannot traverse, or is
# missing) fails the check instead of reading as "nothing found"; a clean scan passes; a planted
# local file fails. The script's own `--self-test` does the same with real images (needs Docker).
#
#   sh scripts/test-image-privacy.sh
set -eu

here=$(cd "$(dirname "$0")" && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
shim="$work/shim"; mkdir -p "$shim" "$work/state"
fail=0

# The stand-in find. FAKE_FIND: clean (prints nothing, exit 0), plant (prints one local file per
# root the script scans, exit 0), fail (prints an error on stderr, exit 1, as find does when it
# cannot enter a directory), missing (exit 127, as a shell does for a command that is not there).
cat > "$shim/find" <<'SH'
#!/bin/sh
case "$FAKE_FIND" in
  clean) exit 0 ;;
  plant) echo /ctx/owner.mk; echo /root/owner.mk; exit 0 ;;
  fail) echo "find: '/root': Permission denied" >&2; exit 1 ;;
  missing) echo "find: not found" >&2; exit 127 ;;
esac
exit 99
SH

# The stand-in docker: `build -q -f - DIR` runs the Dockerfile's RUN line (its /found, /all and /done
# land in the test's own folder) and fails as a build does when that line fails; `run ... sh -c CMD`
# runs CMD; `run IMAGE` prints what the context image's CMD would print; `rmi` does nothing.
cat > "$shim/docker" <<'SH'
#!/bin/sh
st=$STATE
case "$1" in
  build)
    run=$(sed -n 's/^RUN //p')
    run=$(printf '%s' "$run" | sed "s#/found#$st/found#g; s#/all#$st/all#g; s#/done#$st/done#g")
    sh -c "$run" || exit 1
    echo sha256:stand-in ;;
  run)
    shift
    while [ $# -gt 0 ]; do
      case "$1" in
        --rm) shift ;;
        --entrypoint) shift 2 ;;
        sha256:stand-in) cat "$st/done" "$st/found"; exit $? ;;
        *) break ;;
      esac
    done
    shift   # the image name
    [ "${1:-}" = "-c" ] || exit 2
    exec sh -c "$2" ;;
  rmi) exit 0 ;;
  *) exit 2 ;;
esac
SH
chmod +x "$shim/find" "$shim/docker"

# check NAME EXPECT FIND MODE ARG...: runs the script with the stand-in find; EXPECT is pass or fail.
check() {
  name=$1; expect=$2; mode=$3; shift 3
  rm -f "$work/state/found" "$work/state/done" "$work/state/all"
  rc=0
  PATH="$shim:$PATH" STATE="$work/state" FAKE_FIND=$mode sh "$here/check-image-privacy.sh" "$@" > "$work/out" 2>&1 || rc=$?
  if { [ "$expect" = pass ] && [ "$rc" -eq 0 ]; } || { [ "$expect" = fail ] && [ "$rc" -ne 0 ]; }; then
    echo "ok   $name"
  else
    echo "FAIL $name (exit $rc, wanted $expect)"; sed 's/^/     /' "$work/out"; fail=1
  fi
}

check "image: a clean scan passes" pass clean image stand-in:image
check "image: a planted local file fails" fail plant image stand-in:image
check "image: a find that cannot traverse fails" fail fail image stand-in:image
check "image: a missing find fails" fail missing image stand-in:image
check "context: a clean scan passes" pass clean context "$work"
check "context: a planted local file fails" fail plant context "$work"
check "context: a find that cannot traverse fails" fail fail context "$work"
check "context: a missing find fails" fail missing context "$work"

# A failure must say it could not list, never claim the image is clean.
PATH="$shim:$PATH" STATE="$work/state" FAKE_FIND=fail sh "$here/check-image-privacy.sh" image stand-in:image > "$work/out" 2>&1 || true
if grep -q '^ok ' "$work/out" || ! grep -q 'could not list' "$work/out"; then
  echo "FAIL image: a failed scan must report 'could not list' and no ok line"; sed 's/^/     /' "$work/out"; fail=1
else
  echo "ok   image: a failed scan reports 'could not list' and no ok line"
fi

if [ "$fail" -ne 0 ]; then echo "image-privacy tests failed"; exit 1; fi
echo "image-privacy tests passed"
