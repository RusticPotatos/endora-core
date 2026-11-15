#!/bin/sh
# Example A, observation: goal, routine, run, accept, evidence, backup and crash recovery.
# Usage: sh examples/a-observe/run.sh [path-to-endora-foundation]   (default: target/debug/endora-foundation)
#        UPDATE_EXPECTED=1 sh examples/a-observe/run.sh              rewrites expected-output.txt
# Non-interactive. Everything happens in a temp directory (database, token, config, backup) and
# against a throwaway listener on 127.0.0.1. It reads and writes nothing else. Needs sh, python3 or
# perl, and about 15 seconds. Exit 0 only if the transcript equals expected-output.txt.
set -u
HERE=$(cd "$(dirname "$0")" && pwd)
BIN=${1:-${CARGO_TARGET_DIR:-$HERE/../../target}/debug/endora-foundation}
[ -x "$BIN" ] || { echo "run.sh: $BIN is not executable (cargo build -p endora-foundation-cli)" >&2; exit 1; }
case $BIN in /*) ;; *) BIN=$PWD/$BIN ;; esac   # absolute: the script changes directory
T=$(mktemp -d "${TMPDIR:-/tmp}/endora-example-a.XXXXXX" | sed "s#//*#/#g") || exit 1
LISTENER=""; RUNNER=""
cleanup() {
    [ -n "$RUNNER" ] && kill -9 "$RUNNER" 2>/dev/null
    [ -n "$LISTENER" ] && kill "$LISTENER" 2>/dev/null
    rm -rf "$T"
}
trap cleanup EXIT INT TERM
die() { echo "run.sh FAIL: $*" >&2; exit 1; }

# A listener on a free loopback port that accepts connections and closes them.
if command -v python3 >/dev/null 2>&1 && python3 -c '' 2>/dev/null; then
python3 - "$T/port" <<'PY' &
import socket, sys
s = socket.socket()
s.bind(("127.0.0.1", 0))
s.listen(16)
open(sys.argv[1], "w").write(str(s.getsockname()[1]))
while True:
    c, _ = s.accept()
    c.close()
PY
else
perl -MIO::Socket::INET -e '
$s = IO::Socket::INET->new(Listen => 16, LocalAddr => "127.0.0.1", LocalPort => 0, Proto => "tcp", ReuseAddr => 1) or die;
open(F, ">", $ARGV[0]); print F $s->sockport; close(F);
while ($c = $s->accept) { close($c); }' "$T/port" &
fi
LISTENER=$!
i=0
while [ ! -s "$T/port" ]; do
    i=$((i + 1)); [ "$i" -gt 50 ] && die "the listener did not start"
    sleep 0.1
done
PORT=$(cat "$T/port")
sed "s/PORT/$PORT/" "$HERE/config" > "$T/foundation.conf"

# The transcript: each command is echoed as the tutorial shows it, then its output. Volatile
# values (port, temp directory, timestamps, hashes) are replaced so the file can be compared.
OUT="$T/transcript"; : > "$OUT"
norm() {
    sed -e "s#$T#WORK#g" -e "s/127\.0\.0\.1:$PORT/127.0.0.1:PORT/g" \
        -e 's/ t=[0-9]*/ t=TIME/g' -e 's/head [0-9a-f]\{64\}/head HASH/' \
        -e 's/[0-9a-f]\{64\}/HASH/g' -e 's/[[:space:]]*$//'
}
say() { printf '%s\n' "$*" >> "$OUT"; }
# ep ARGS...: run the binary the way the tutorial writes it (db, config and token as flags).
show() { # the command as a person would type it: arguments with spaces are quoted
    out=""
    for a in "$@"; do
        case $a in *[\ \"]*) out="$out \"$a\"" ;; *) out="$out $a" ;; esac
    done
    printf '$ endora-foundation%s%s\n' "$out" "${FILTER:+ | grep '$FILTER'}" | norm >> "$OUT"
}
ep() {
    show "$@"
    "$BIN" --db "$T/foundation.db" --config "$T/foundation.conf" --token-file "$T/owner.token" "$@" > "$T/raw" 2>&1
    LASTEXIT=$?
    if [ -n "${FILTER:-}" ]; then grep "$FILTER" "$T/raw" | norm >> "$OUT"; else norm < "$T/raw" >> "$OUT"; fi
}
code() { # code N ARGS...: run, then record and check the exit code
    n=$1; shift
    ep "$@"
    [ "$LASTEXIT" -eq "$n" ] || die "$*: exit code $LASTEXIT, wanted $n"
}

say "# 1. the owner credential"
cd "$T" || exit 1   # relative names below, exactly as the tutorial types them
say "\$ endora-foundation init --token-out owner.token"
"$BIN" --db "$T/foundation.db" --config "$T/foundation.conf" init --token-out owner.token 2>&1 | norm >> "$OUT"
[ -s "$T/owner.token" ] || die "init made no token"

say "# 2. a goal alone does nothing"
ep goal add --title "Web answers" --why "the local web port must accept connections" --probe web --acceptance owner
ep run-due
say "(no output: the goal has no routine, so there is no item)"

say "# 3. a routine gives the goal a rhythm"
ep routine add --goal 1 --every 300 --title "check web"
ep run-due

say "# 4. the item waits for the owner"
ep status

say "# 5. accept, then read the evidence"
code 0 accept 1
ep evidence --item 1
ep verify-evidence

say "# 6. backup"
ep backup --to backup.db
say "\$ endora-foundation --db backup.db status | grep '\\['"
"$BIN" --db backup.db status 2>&1 | grep '\[' | norm >> "$OUT"

say "# 7. crash recovery: kill -9 in the middle of a check"
ep goal add --title "Slow check" --why "a probe that takes three seconds" --probe slow
ep routine add --goal 2 --every 300 --title "slow check"
"$BIN" --db "$T/foundation.db" --config "$T/foundation.conf" run-due >/dev/null 2>&1 &
RUNNER=$!
i=0
while ! "$BIN" --db "$T/foundation.db" status 2>&1 | grep -q '\[running\]'; do
    i=$((i + 1)); [ "$i" -gt 60 ] && die "item 2 never reached running"
    sleep 0.1
done
kill -9 "$RUNNER"; wait "$RUNNER" 2>/dev/null; RUNNER=""
say "\$ kill -9 <run-due>        # the process dies while item 2 is checking"
FILTER='\['; ep status; FILTER=
ep run-due
FILTER='\['; ep status; FILTER=
ep evidence --item 2
ep verify-evidence

if [ "${UPDATE_EXPECTED:-}" = 1 ]; then
    cp "$OUT" "$HERE/expected-output.txt"; echo "run.sh: expected-output.txt rewritten"; exit 0
fi
if diff -u "$HERE/expected-output.txt" "$OUT"; then echo "run.sh OK: transcript matches expected-output.txt"; else die "transcript differs from expected-output.txt"; fi
