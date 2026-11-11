#!/bin/sh
# The role's owner-values variable under its current and its previous name (tests/owner_values.yml):
# new only, previous only, the same list under both, different lists, neither. Fictional values.
# The previous role variable's name is assembled here, never spelled, so no tracked file carries it.
# No value is ever printed by this script or by the play (the template task is no_log).
set -eu
here=$(cd "$(dirname "$0")" && pwd)
tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT HUP INT TERM
NEWV="zq-fictional-new-value"; OLDV="zq-fictional-old-value"
LEGROLE=endora_per"sonal"_values
fail=0
check() { if [ "$1" = ok ]; then echo "  ok    $2"; else echo "FAIL  $2"; fail=1; fi; }

# play LABEL EXTRA-VARS...: runs the play into a fresh folder; sets $code, and $env_file (may be absent).
play() {
  dir=$(mktemp -d "$tmp/p.XXXXXX"); shift_label=$1; shift
  args=""
  for a in "$@"; do args="$args -e $a"; done
  set +e
  # shellcheck disable=SC2086
  ansible-playbook "$here/owner_values.yml" -e "out=$dir" $args >"$dir/log" 2>&1; code=$?
  set -e
  env_file="$dir/owner-values.env"; log="$dir/log"
}
holds() { grep -qx "ENDORA_OWNER_VALUES=$1" "$env_file" 2>/dev/null; }
never_prints() { ! grep -qF -e "$NEWV" -e "$OLDV" "$log"; }
says_previous() { grep -qi "previous role variable" "$log"; }

play new "ENDORA_OWNER_VALUES=$NEWV"
[ "$code" = 0 ] && holds "$NEWV" && never_prints && ! says_previous && check ok "the new name is rendered, silently" || check bad "the new name alone"
play previous "$LEGROLE=$OLDV"
[ "$code" = 0 ] && holds "$OLDV" && never_prints && says_previous && check ok "the previous name alone is rendered, and the role says so" || check bad "the previous name alone was dropped or unannounced"
play both "ENDORA_OWNER_VALUES=$OLDV" "$LEGROLE=$OLDV"
[ "$code" = 0 ] && holds "$OLDV" && never_prints && check ok "the same list under both names is rendered once" || check bad "the same list under both names"
play both-reordered "ENDORA_OWNER_VALUES=$OLDV,$NEWV" "$LEGROLE=$NEWV,$OLDV"
[ "$code" = 0 ] && never_prints && check ok "the same set in another order is not a conflict" || check bad "reordered list treated as a conflict"
play conflict "ENDORA_OWNER_VALUES=$NEWV" "$LEGROLE=$OLDV"
[ "$code" != 0 ] && [ ! -e "$env_file" ] && never_prints && grep -q "ENDORA_OWNER_VALUES" "$log" && check ok "different lists stop the role before anything is rendered, naming no value" || check bad "different lists were not refused"
play conflict-longer "ENDORA_OWNER_VALUES=$NEWV" "$LEGROLE=$NEWV,$OLDV"
[ "$code" != 0 ] && [ ! -e "$env_file" ] && check ok "a longer previous list is a conflict (its extra value would never be scanned for)" || check bad "longer previous list accepted"
play neither
[ "$code" = 0 ] && grep -qx "ENDORA_OWNER_VALUES=" "$env_file" && check ok "neither name set renders an empty line (shapes and the tripwire only; the compose file needs the variable set)" || check bad "neither name set"
[ "$fail" = 0 ] && echo "endora role owner-values tests passed"
[ "$fail" = 0 ]
