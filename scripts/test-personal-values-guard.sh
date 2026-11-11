#!/bin/sh
# The actual-value check survives the renames. Fictional values only: nothing here is a real value.
#  1. a value given through ENDORA_OWNER_VALUES that sits in a tracked file is found (exit 1);
#  2. without the variable the check runs on shapes alone and passes a clean tree (exit 0);
#  3. the previous variable name alone is refused (exit 2), never silently ignored;
#  4. a personal make file under its previous name stops `make`, and stays git-ignored.
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
old=ENDORA_PERSONAL; old="${old}_VALUES"
oldfile=lo; oldfile="${oldfile}cal.mk"
ok() { echo "  ok    $1"; }
bad() { echo "FAIL  $1"; exit 1; }
repo="$tmp/r"; mkdir "$repo"; cd "$repo"
git init -q; git config user.email t@example.com; git config user.name t
echo "a harmless line" > a.txt; echo "the fictional-planted-value is here" > b.txt
git add a.txt b.txt
set +e
ENDORA_OWNER_VALUES=fictional-planted-value node "$root/scripts/check-nothing-personal.mjs" >"$tmp/o1" 2>&1; c1=$?
node "$root/scripts/check-nothing-personal.mjs" >"$tmp/o2" 2>&1; c2=$?
env "$old=fictional-planted-value" node "$root/scripts/check-nothing-personal.mjs" >"$tmp/o3" 2>&1; c3=$?
set -e
[ "$c1" = 1 ] && ok "a value given through ENDORA_OWNER_VALUES is found in a tracked file" || bad "planted value not found (exit $c1)"
! grep -qF fictional-planted-value "$tmp/o1" && ok "the report never repeats the value it found" || bad "the report echoed the planted value"
[ "$c2" = 0 ] && ok "without the variable a clean tree passes on shapes alone" || bad "shape-only run failed (exit $c2)"
[ "$c3" = 2 ] && ok "the previous variable name alone is refused" || bad "previous variable name not refused (exit $c3)"
mkdir "$tmp/m"; cd "$tmp/m"; : > "$oldfile"
set +e; make -f "$root/Makefile" -n help >"$tmp/o4" 2>&1; c4=$?; set -e
[ "$c4" != 0 ] && grep -q "old name" "$tmp/o4" && ok "a personal make file under its previous name stops make" || bad "old-named make file not refused"
: > owner.mk
set +e; make -f "$root/Makefile" -n help >"$tmp/o5" 2>&1; c5=$?; set -e
grep -q "old name" "$tmp/o5" && bad "refused although owner.mk exists" || ok "with owner.mk present the old name is not an error"
cd "$repo"; cp "$root/.gitignore" .gitignore; : > "$oldfile"; : > owner.mk
[ "$(git check-ignore "$oldfile" owner.mk | wc -l | tr -d ' ')" = 2 ] && ok "both names are git-ignored" || bad "a personal make file name is not ignored"
echo "personal-values guard tests passed"
