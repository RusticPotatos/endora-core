#!/bin/sh
# Tests for how the renamed personal-values interfaces treat the previous names.
#
# Core renamed the personal-values variable (and, in Ansible, its role variable and the vault key
# behind it) and the git-ignored personal make file. A deployed machine still has the previous
# names in its shell, its inventory or its make file, so each interface must do one of two
# things, never a third (a silent downgrade to "no personal values"):
#   * a TOOL THAT CHECKS (the repository check, the history check, the image check, the
#     terminal's check) refuses, with the name to use, when only the previous name is set;
#   * what CONSUMES the setting at run time (the Makefile that deploys, the Ansible role) accepts
#     the previous name, says so, and uses its value.
# Both refuse when the two names hold different values: it is not knowable which one is meant.
# The node's own rule is tested in Rust (domains/capabilities, `outgoing::tests`).
#
# Cases per interface: new name only, previous name only, both the same, both different, neither.
# Fictional values only; every case also proves no value is ever echoed.
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
NEWV="zq-fictional-new-value"
OLDV="zq-fictional-old-value"
# The previous names are assembled, never spelled, so no tracked file carries them.
LEG=ENDORA_PER"SONAL"_VALUES
LEGFILE=lo"cal".mk
fail=0; n=0
ok() { n=$((n + 1)); echo "  ok    $1"; }
bad() { n=$((n + 1)); fail=$((fail + 1)); echo "FAIL  $1"; }

# expect CODE LABEL FILE: the exit code ($code) matches and the output never holds a value.
expect() {
  want=$1; label=$2; file=$3
  [ "$code" = "$want" ] || { bad "$label (exit $code, wanted $want)"; return; }
  if grep -qF -e "$NEWV" -e "$OLDV" "$file"; then bad "$label: output echoed a value"; return; fi
  ok "$label"
}
# mentions NAME LABEL FILE: the refusal names the variable to use.
mentions() { grep -q -- "$1" "$3" && ok "$2" || bad "$2 (no $1 in the output)"; }

# A throwaway repository: one tracked file that carries the OLD fictional value, one clean one.
repo="$tmp/repo"; mkdir "$repo"
git -C "$repo" init -q -b main; git -C "$repo" config user.name tester; git -C "$repo" config user.email t@example.com
git -C "$repo" config commit.gpgsign false
echo "an ordinary line" > "$repo/clean.txt"; echo "a line holding $OLDV here" > "$repo/planted.txt"
git -C "$repo" add -A; git -C "$repo" commit -q -m one

# runenv OUT NEW LEGACY CMD...: run CMD in the repository with the given variables ("-" = unset).
runenv() {
  out=$1; nv=$2; lv=$3; shift 3
  set +e
  ( cd "$repo"
    [ "$nv" = "-" ] && unset ENDORA_OWNER_VALUES || export ENDORA_OWNER_VALUES="$nv"
    [ "$lv" = "-" ] && unset "$LEG" || export "$LEG=$lv"
    unset ENDORA_REVIEWED_BINARIES_SHA256
    "$@" >"$out" 2>&1 ); code=$?
  set -e
}

for tool in "node $root/scripts/check-nothing-personal.mjs" "node $root/scripts/check-owner-values.mjs --release"; do
  short=$(echo "$tool" | sed 's|.*/||; s| .*||')
  # shellcheck disable=SC2086
  runenv "$tmp/o" "$NEWV" - $tool;       expect 0 "$short: the new name alone is read (its value is not in the tree)" "$tmp/o"
  runenv "$tmp/o" "$OLDV" - $tool;       expect 1 "$short: the new name alone finds a planted value" "$tmp/o"
  runenv "$tmp/o" - "$OLDV" $tool;       expect 2 "$short: the previous name alone is refused, not ignored" "$tmp/o"
  mentions ENDORA_OWNER_VALUES "$short: the refusal names the variable to use" "$tmp/o"
  runenv "$tmp/o" "$OLDV" "$OLDV" $tool; expect 1 "$short: both names with the same value read that value" "$tmp/o"
  runenv "$tmp/o" "$OLDV, $NEWV" "$NEWV,$OLDV" $tool; expect 1 "$short: the same set in another order and spacing is the same value" "$tmp/o"
  # The previous name holds the value that is planted, the new one holds another: ignoring the
  # previous one would pass a tree that holds a personal value.
  runenv "$tmp/o" "$NEWV" "$OLDV" $tool; expect 2 "$short: two different values are refused, not silently resolved" "$tmp/o"
  mentions ENDORA_OWNER_VALUES "$short: the conflict names the variables" "$tmp/o"
  case $tool in
    *--release*) runenv "$tmp/o" - - $tool; expect 2 "$short: neither name set is refused in release mode" "$tmp/o" ;;
    *)           runenv "$tmp/o" - - $tool; expect 0 "$short: neither name set runs on shapes alone, out loud or clean" "$tmp/o" ;;
  esac
done

# ---- the terminal's check -------------------------------------------------------------------
pc="$root/clients/terminal/scripts/check-private.py"
runpc() { out=$1; nv=$2; lv=$3
  set +e
  ( cd "$root"
    [ "$nv" = "-" ] && unset ENDORA_OWNER_VALUES || export ENDORA_OWNER_VALUES="$nv"
    [ "$lv" = "-" ] && unset "$LEG" || export "$LEG=$lv"
    python3 -I "$pc" >"$out" 2>&1 ); code=$?
  set -e
}
runpc "$tmp/o" - -;            expect 0 "terminal check: neither name set runs on shapes alone" "$tmp/o"
runpc "$tmp/o" "$NEWV" -;      expect 0 "terminal check: the new name alone is read" "$tmp/o"
runpc "$tmp/o" - "$OLDV";      expect 2 "terminal check: the previous name alone is refused" "$tmp/o"
runpc "$tmp/o" "$NEWV" "$OLDV"; expect 2 "terminal check: two different values are refused" "$tmp/o"
runpc "$tmp/o" "$OLDV" "$OLDV"; expect 0 "terminal check: both names with the same value are read" "$tmp/o"

# ---- the image check (no Docker needed for the settings step) ---------------------------------
ip="$root/scripts/check-image-privacy.sh"
runip() { out=$1; nv=$2; lv=$3
  set +e
  ( cd "$root"
    [ "$nv" = "-" ] && unset ENDORA_OWNER_VALUES || export ENDORA_OWNER_VALUES="$nv"
    [ "$lv" = "-" ] && unset "$LEG" || export "$LEG=$lv"
    sh "$ip" values >"$out" 2>&1 ); code=$?
  set -e
}
runip "$tmp/o" - -;             expect 0 "image check: neither name set" "$tmp/o"
runip "$tmp/o" "$NEWV,$OLDV" -; expect 0 "image check: the new name alone is read" "$tmp/o"
grep -q "2 " "$tmp/o" && ok "image check: it reports a count, never the values" || bad "image check: no count reported"
runip "$tmp/o" - "$OLDV";       expect 2 "image check: the previous name alone is refused" "$tmp/o"
runip "$tmp/o" "$NEWV" "$OLDV"; expect 2 "image check: two different values are refused" "$tmp/o"
runip "$tmp/o" "$OLDV" "$OLDV"; expect 0 "image check: both names with the same value are read" "$tmp/o"

# ---- the Makefile: what `make deploy` passes to the node ------------------------------------------
mk="$tmp/mk"; mkdir "$mk"; cp "$root/Makefile" "$mk/Makefile"
# runmk OUT NEW LEGACY [OWNERMK-CONTENT] [OLDFILE-CONTENT]: parse the Makefile with the given settings.
runmk() {
  out=$1; nv=$2; lv=$3
  rm -f "$mk/owner.mk" "$mk/$LEGFILE"
  [ -z "${4:-}" ] || printf '%s\n' "$4" > "$mk/owner.mk"
  [ -z "${5:-}" ] || printf '%s\n' "$5" > "$mk/$LEGFILE"
  set +e
  ( cd "$mk"
    [ "$nv" = "-" ] && unset ENDORA_OWNER_VALUES || export ENDORA_OWNER_VALUES="$nv"
    [ "$lv" = "-" ] && unset "$LEG" || export "$LEG=$lv"
    make -s -pn >"$out" 2>"$out.err" ); code=$?
  set -e
  cat "$out.err" >> "$out" 2>/dev/null || true
}
resolved() { # resolved FILE VALUE: the Makefile settled on exactly this value for the node
  grep -qE "^ENDORA_OWNER_VALUES :?= $2\$" "$1"
}
quiet() { # the parse output must not echo a value (the -p database holds the setting, so check only the error stream)
  ! grep -qF -e "$NEWV" -e "$OLDV" "$1.err"
}
runmk "$tmp/m" - -;                      [ "$code" = 0 ] && ok "make: no values set parses" || bad "make: no values set failed ($code)"
runmk "$tmp/m" "$NEWV" -;                [ "$code" = 0 ] && resolved "$tmp/m" "$NEWV" && ok "make: the new name is used" || bad "make: the new name was not used"
runmk "$tmp/m" - "$OLDV";                [ "$code" = 0 ] && resolved "$tmp/m" "$OLDV" && ok "make: the previous name in the environment is used, not dropped" || bad "make: previous name in the environment was dropped"
grep -q ENDORA_OWNER_VALUES "$tmp/m.err" && quiet "$tmp/m" && ok "make: it says the name changed, without a value" || bad "make: no notice of the changed name (or it echoed a value)"
runmk "$tmp/m" - - "$LEG = $OLDV";       [ "$code" = 0 ] && resolved "$tmp/m" "$OLDV" && ok "make: the previous name in owner.mk is used, not dropped" || bad "make: previous name in owner.mk was dropped"
runmk "$tmp/m" "$OLDV" "$OLDV";          [ "$code" = 0 ] && resolved "$tmp/m" "$OLDV" && ok "make: both names with the same value parse" || bad "make: same value under both names failed"
runmk "$tmp/m" "$NEWV" "$OLDV";          [ "$code" != 0 ] && quiet "$tmp/m" && ok "make: two different values stop the parse, without a value" || bad "make: conflicting values were not refused"
runmk "$tmp/m" - - "ENDORA_OWNER_VALUES = $NEWV
$LEG = $OLDV"; [ "$code" != 0 ] && quiet "$tmp/m" && ok "make: owner.mk holding two different values stops the parse" || bad "make: conflict inside owner.mk not refused"
runmk "$tmp/m" - - "" "ENDORA_OWNER_VALUES = $OLDV"; [ "$code" != 0 ] && quiet "$tmp/m" && ok "make: a personal make file under its previous name alone is refused" || bad "make: previous file name alone was accepted"
runmk "$tmp/m" - - "ENDORA_OWNER_VALUES = $NEWV" "ENDORA_OWNER_VALUES = $OLDV"; [ "$code" = 0 ] && resolved "$tmp/m" "$NEWV" && grep -q "NOT read" "$tmp/m.err" && quiet "$tmp/m" && ok "make: owner.mk wins over a leftover file under the previous name, which is said to be unread" || bad "make: leftover previous-name file was neither ignored nor announced"

# ---- direct `docker compose`: the one consumer that cannot read the previous name -------------------
# Compose cannot compute a variable name, so it cannot accept the previous one. What it must not do is
# start a node with no personal values and no word. The file therefore needs the variable SET (empty
# is allowed); unset stops it with a message that names no value. Parsed only: no daemon is contacted
# (DOCKER_HOST points nowhere), nothing is built or started.
cf="$root/docker-compose.yml"
grep -qF '${ENDORA_OWNER_VALUES?' "$cf" && ! grep -qF '${ENDORA_OWNER_VALUES:?' "$cf" && ! grep -qF '${ENDORA_OWNER_VALUES-' "$cf" && ! grep -qF '${ENDORA_OWNER_VALUES:-' "$cf" \
  && ok "compose: the file requires ENDORA_OWNER_VALUES to be set (empty allowed), and has no silent default" \
  || bad "compose: the file has a silent default for ENDORA_OWNER_VALUES, or refuses an empty one"
if docker compose version >/dev/null 2>&1; then
  # runcf OUT NEW LEGACY: compose config in a bare environment.
  runcf() {
    out=$1; nv=$2; lv=$3
    set +e
    ( cd "$root"
      set -- env -i PATH="$PATH" HOME="$tmp" DOCKER_HOST=unix:///nonexistent DOCKER_CONFIG="$tmp"
      [ "$nv" = "-" ] || set -- "$@" "ENDORA_OWNER_VALUES=$nv"
      [ "$lv" = "-" ] || set -- "$@" "$LEG=$lv"
      "$@" docker compose -f docker-compose.yml config >"$out" 2>&1 ); code=$?
    set -e
  }
  runcf "$tmp/c" - -;            [ "$code" != 0 ] && grep -q ENDORA_OWNER_VALUES "$tmp/c" && ok "compose: neither name set stops, naming the variable" || bad "compose: neither name set did not stop"
  runcf "$tmp/c" - "$OLDV";      [ "$code" != 0 ] && ! grep -qF -e "$OLDV" -e "$NEWV" "$tmp/c" && ok "compose: the previous name alone stops, and no value is printed" || bad "compose: the previous name alone was accepted or a value was printed"
  runcf "$tmp/c" "" -;          [ "$code" = 0 ] && ok "compose: the new name set but empty is accepted" || bad "compose: an empty ENDORA_OWNER_VALUES was refused"
  runcf "$tmp/c" "$NEWV" "$OLDV"; [ "$code" = 0 ] && ok "compose: the new name set is accepted (the checks and make refuse conflicts earlier)" || bad "compose: a set ENDORA_OWNER_VALUES was refused"
else
  echo "  skip  compose: no 'docker compose' here, so the parse cases are UNPROVEN on this machine (run on the XPS)"
fi

echo "$n checks, $fail failed"
[ "$fail" = 0 ] && echo "personal-values compatibility tests passed"
[ "$fail" = 0 ]
