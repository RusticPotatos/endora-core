#!/bin/sh
# Tests for scripts/check-core-origin.sh, which holds this repository to what it was made from:
# the origin record names a full source commit, every workspace member and every file a workflow
# runs is present, and nothing private (`.local/`, `owner.mk`, an `.env`, a key) is in the tree.
# A split that quietly dropped a gate, or carried a private file along, must fail here.
#
#   sh scripts/test-check-core-origin.sh
set -u

HERE=$(cd "$(dirname "$0")" && pwd)
SUT="$HERE/check-core-origin.sh"
WORK=$(mktemp -d "${TMPDIR:-/tmp}/check-core-origin-test.XXXXXX")
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
  echo "  FAIL  scripts/check-core-origin.sh does not exist"
  exit 1
fi

# Built in pieces: the nothing-personal check rightly flags a 40-character hex run in a file.
SHA=$(printf '%s%s%s%s' 0123456789 abcdef0123 456789abcd ef01234567)

# A fixture tree that satisfies every rule.
fixture() {
  dir=$WORK/$1
  mkdir -p "$dir/docs" "$dir/scripts" "$dir/crates/one" "$dir/.github/workflows" "$dir/deploy/compose"
  printf '[workspace]\nmembers = [\n    "crates/one",\n]\n' > "$dir/Cargo.toml"
  printf '[package]\nname = "one"\n' > "$dir/crates/one/Cargo.toml"
  printf 'echo gate\n' > "$dir/scripts/gate-step.sh"
  printf 'echo up\n' > "$dir/deploy/compose/up.sh"
  printf 'Source repository: RusticPotatos/Endora\nSource commit: %s\n' "$SHA" > "$dir/docs/core-origin.md"
  cat > "$dir/.github/workflows/ci.yml" <<'EOF'
jobs:
  a:
    steps:
      - run: sh scripts/gate-step.sh
      - run: sh deploy/compose/up.sh --self-test
      - run: sh "$GITHUB_WORKSPACE/scripts/${{ matrix.skipped }}.sh"
EOF
}

run() { sh "$SUT" --root "$WORK/$1" >"$WORK/$1.out" 2>&1; }

fixture good
run good
expect "a complete tree passes" $?

fixture no-record
rm "$WORK/no-record/docs/core-origin.md"
run no-record
r=$?; [ $r -ne 0 ]; expect "a missing origin record fails" $?

fixture short-sha
printf 'Source repository: RusticPotatos/Endora\nSource commit: 0123abc\n' > "$WORK/short-sha/docs/core-origin.md"
run short-sha
r=$?; [ $r -ne 0 ]; expect "an abbreviated source commit fails" $?

fixture missing-script
rm "$WORK/missing-script/scripts/gate-step.sh"
run missing-script
r=$?; [ $r -ne 0 ]; expect "a workflow step whose script is gone fails" $?
grep -q 'scripts/gate-step.sh' "$WORK/missing-script.out"
expect "  and it names the missing script" $?

fixture missing-member
rm "$WORK/missing-member/crates/one/Cargo.toml"
run missing-member
r=$?; [ $r -ne 0 ]; expect "a workspace member without a Cargo.toml fails" $?

fixture dot-local
mkdir -p "$WORK/dot-local/.local/gate" && : > "$WORK/dot-local/.local/gate/x.log"
run dot-local
r=$?; [ $r -ne 0 ]; expect "a .local folder in the tree fails" $?

fixture local-mk
: > "$WORK/local-mk/owner.mk"
run local-mk
r=$?; [ $r -ne 0 ]; expect "a owner.mk in the tree fails" $?

fixture env-file
: > "$WORK/env-file/deploy/compose/.env"
run env-file
r=$?; [ $r -ne 0 ]; expect "an .env file in the tree fails" $?

fixture key-file
: > "$WORK/key-file/deploy/server.pem"
run key-file
r=$?; [ $r -ne 0 ]; expect "a key file in the tree fails" $?

# In a git checkout only what git would carry counts: the gate writes .local/gate into its tree,
# and a git-ignored file is not carried.
fixture ignored
printf '.local/\nowner.mk\n' > "$WORK/ignored/.gitignore"
mkdir -p "$WORK/ignored/.local/gate" && : > "$WORK/ignored/.local/gate/x.log" && : > "$WORK/ignored/owner.mk"
git -C "$WORK/ignored" init -q
run ignored
expect "a git-ignored .local and owner.mk are not carried, so pass" $?
rm "$WORK/ignored/.gitignore"
run ignored
r=$?; [ $r -ne 0 ]; expect "and fail once git would carry them" $?

# macOS reaches its temporary folder through a symlink (/var -> /private/var), and git reports the
# physical path of a checkout's top level: the same git-ignored files must still not count when the
# tree is named through a symlink.
ln -s "$WORK" "$WORK-link"
trap 'rm -rf "$WORK" "$WORK-link"' EXIT
printf '.local/\nowner.mk\n' > "$WORK/ignored/.gitignore"
sh "$SUT" --root "$WORK-link/ignored" >"$WORK/ignored-link.out" 2>&1
expect "a git-ignored .local is not carried when the tree is reached through a symlink" $?
rm "$WORK/ignored/.gitignore"
sh "$SUT" --root "$WORK-link/ignored" >"$WORK/ignored-link.out" 2>&1
r=$?; [ $r -ne 0 ]; expect "and fails once git would carry it, through the symlink too" $?

fixture workdir
mkdir -p "$WORK/workdir/crates/one/scripts"
: > "$WORK/workdir/crates/one/scripts/own.py"
cat >> "$WORK/workdir/.github/workflows/ci.yml" <<'EOF'
  b:
    steps:
      - working-directory: crates/one
        run: python3 scripts/own.py
EOF
run workdir
expect "a script named relative to a working-directory passes" $?
rm "$WORK/workdir/crates/one/scripts/own.py"
run workdir
r=$?; [ $r -ne 0 ]; expect "and fails once that script is gone" $?

fixture example-ok
: > "$WORK/example-ok/deploy/compose/bundle.env.example"
run example-ok
expect "an .env.example file is allowed" $?

if [ "$FAILED" -eq 0 ]; then
  echo "check-core-origin tests passed"
  exit 0
fi
exit 1
