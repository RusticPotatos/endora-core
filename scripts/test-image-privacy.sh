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
    echo "$*" >> "$st/runargs"
    while [ $# -gt 0 ]; do
      case "$1" in
        --rm) shift ;;
        --entrypoint|--user) shift 2 ;;
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

# The scan runs as root: a directory the image's own user cannot enter (/root, /var/cache/ldconfig:
# mode 0700 on a stock Debian) makes find exit 1, and with a non-root scan the one place an owner's
# file is likeliest to hide, /root, would be skipped or fail the scan. The owner-value search too.
rm -f "$work/state/runargs"
PATH="$shim:$PATH" STATE="$work/state" FAKE_FIND=clean ENDORA_OWNER_VALUES="Jane Doe" sh "$here/check-image-privacy.sh" image stand-in:image > "$work/out" 2>&1 || true
runs=$(grep -c . "$work/state/runargs" || true)
rootruns=$(grep -c -e '--user 0:0' "$work/state/runargs" || true)
if [ "$runs" -ge 2 ] && [ "$runs" -eq "$rootruns" ]; then
  echo "ok   image: the listing and the owner-value search both run as root ($rootruns runs)"
else
  echo "FAIL image: every docker run of the scan must use --user 0:0 ($rootruns of $runs did)"; fail=1
fi

# A failure must say it could not list, never claim the image is clean.
PATH="$shim:$PATH" STATE="$work/state" FAKE_FIND=fail sh "$here/check-image-privacy.sh" image stand-in:image > "$work/out" 2>&1 || true
if grep -q '^ok ' "$work/out" || ! grep -q 'could not list' "$work/out"; then
  echo "FAIL image: a failed scan must report 'could not list' and no ok line"; sed 's/^/     /' "$work/out"; fail=1
else
  echo "ok   image: a failed scan reports 'could not list' and no ok line"
fi

# The one narrow exception: pip's vendored copy of the public CA bundle (certifi), at its exact path.
# The REAL find expression of the script runs here, on a scratch tree standing in for the image's
# root, because a stand-in find cannot show what the expression itself lets through. Everything the
# exception must not cover has to be listed: a .pem or a key beside it, the same name elsewhere.
eval "$(sed -n '/^CERTIFI_CA=/p; /^FIND_LOCAL=/p' "$here/check-image-privacy.sh")"
expr=$FIND_LOCAL
ca=opt/venv/lib/python3.11/site-packages/pip/_vendor/certifi
tree="$work/tree"
scan() {
  rm -rf "$tree"; mkdir -p "$tree/$ca" "$tree/opt/venv/lib/python3.11/site-packages/other"
  "$@"
  # Run the script's own expression, with the scratch tree as the root and the image's paths under it.
  sh -c "$(printf '%s' "$expr" | sed "s#^find / #find $tree #; s#/opt/venv/#$tree/opt/venv/#g")" 2>&1
}
expect_scan() {
  name=$1; want=$2; shift 2
  got=$(scan "$@")
  if [ "$want" = none ] && [ -z "$got" ]; then echo "ok   $name"
  elif [ "$want" != none ] && printf '%s\n' "$got" | grep -q -- "$want"; then echo "ok   $name"
  else echo "FAIL $name (wanted $want, found: ${got:-nothing})"; fail=1; fi
}
plant() { for f in "$@"; do mkdir -p "$(dirname "$tree/$f")"; echo placeholder > "$tree/$f"; done; }
expect_scan "exception: pip's certifi cacert.pem at its exact path is not listed" none plant "$ca/cacert.pem"
expect_scan "exception: a key.pem beside it is listed" "certifi/key.pem" plant "$ca/cacert.pem" "$ca/key.pem"
expect_scan "exception: a .key beside it is listed" "certifi/server.key" plant "$ca/cacert.pem" "$ca/server.key"
expect_scan "exception: another .pem in the venv is listed" "other/ca.pem" plant "$ca/cacert.pem" opt/venv/lib/python3.11/site-packages/other/ca.pem
expect_scan "exception: a cacert.pem anywhere else is listed" "other/cacert.pem" plant opt/venv/lib/python3.11/site-packages/other/cacert.pem
expect_scan "exception: a directory at the exempt path is not pruned: an id_rsa inside it is listed" "cacert.pem/id_rsa" plant "$ca/cacert.pem/id_rsa"
expect_scan "exception: a cacert.pem under another python version is listed" "python3.12" plant opt/venv/lib/python3.12/site-packages/pip/_vendor/certifi/cacert.pem

if [ "$fail" -ne 0 ]; then echo "image-privacy tests failed"; exit 1; fi
echo "image-privacy tests passed"
