#!/bin/sh
# Real-run lab for the Claude Code executor (see docs/foundation/README.md, "Real executors").
# Needs LAB (scratch dir), ENDORA_FOUNDATION_BIN, CRED (access-token file), CLAUDE_BIN.
# usage: lab.sh NAME MODEL MICRO_USD TURNS SECONDS INSTRUCTION [extra config lines file]
set -u
NAME=$1; MODEL=$2; MICROS=$3; TURNS=$4; SECS=$5; INSTR=$6; EXTRA=${7:-/dev/null}
R=${LAB:?set LAB}/$NAME; BIN=${ENDORA_FOUNDATION_BIN:?set ENDORA_FOUNDATION_BIN}
rm -rf "$R"; mkdir -p "$R/tmpl" "$R/scratch"
printf 'add() { echo $(( $1 - $2 )); }\n' > "$R/tmpl/lib.sh"
printf '. ./lib.sh\n[ "$(add 2 3)" = 5 ] || { echo "FAIL: add 2 3 gave $(add 2 3)"; exit 1; }\necho PASS\n' > "$R/tmpl/test.sh"
cat > "$R/config" <<CFG
git $R/scratch $R/tmpl -- claude-code
claude-code ${CLAUDE_BIN:?set CLAUDE_BIN} $MODEL $MICROS $TURNS $SECS
claude-credential ${CRED:?set CRED}
CFG
cat "$EXTRA" >> "$R/config"
E="$BIN --db $R/db --config $R/config --token-file $R/token"
$BIN --db "$R/db" --config "$R/config" init --token-out "$R/token" > "$R/init.out" 2>&1
$E overall-cap 100 > "$R/cap.out" 2>&1
( cd "$R" && $E git-task --cap 100 --repo "$R/tmpl" --test-cmd "sh test.sh" --instruction "$INSTR" > "$R/task.out" 2> "$R/task.err"; echo "exit=$?" >> "$R/task.out" )
$E evidence --item 1 > "$R/evidence.txt" 2>&1
$E status > "$R/status.txt" 2>&1
echo "== task.out"; cat "$R/task.out"; echo "== task.err"; cat "$R/task.err"
