#!/bin/sh
# End-to-end smoke test of an endora-foundation binary: sh scripts/foundation-smoke.sh <binary>
# Temp database and config, one tcp probe at a local listener that this script starts and kills.
# Exits non-zero on the first failed step. No network beyond 127.0.0.1, no model, no secrets.
set -u
BIN=${1:?usage: foundation-smoke.sh <path-to-endora-foundation>}
[ -x "$BIN" ] || { echo "smoke: $BIN is not executable" >&2; exit 1; }
T=$(mktemp -d "${TMPDIR:-/tmp}/foundation-smoke.XXXXXX") || exit 1
LISTENER=""
cleanup() {
    [ -n "$LISTENER" ] && kill "$LISTENER" 2>/dev/null
    rm -rf "$T"
}
trap cleanup EXIT INT TERM

fail() { echo "smoke FAIL: $*" >&2; exit 1; }
step() { echo "smoke: $*"; }

# A local listener on a free port: accepts connections and closes them (python3, else perl, which
# a slim container image has).
if command -v python3 >/dev/null 2>&1; then
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
    i=$((i + 1)); [ "$i" -gt 50 ] && fail "the listener did not start"
    sleep 0.1
done
PORT=$(cat "$T/port")

DB="$T/live.db"
printf 'probe web tcp 127.0.0.1:%s 60\n' "$PORT" > "$T/foundation.conf"
TOKEN="$T/owner.token"
"$BIN" --db "$DB" --config "$T/foundation.conf" init --token-out "$TOKEN" >/dev/null 2>&1 || fail "init --token-out"
[ -s "$TOKEN" ] || fail "init made no token file"
run() { "$BIN" --db "$DB" --config "$T/foundation.conf" --token-file "$TOKEN" "$@"; }
# expect <exit-code> <text-to-find> <args...>
expect() {
    want=$1; needle=$2; shift 2
    out=$(run "$@" 2>&1); got=$?
    [ "$got" -eq "$want" ] || fail "$* exited $got, wanted $want: $out"
    case $out in *"$needle"*) ;; *) fail "$*: output lacks \`$needle\`: $out" ;; esac
    LAST=$out
}

step "capabilities"
expect 0 probe.run capabilities
step "goal add (auto-observe)"
expect 0 "goal 1" goal add --title "web answers" --probe web --acceptance auto-observe
step "routine add"
expect 0 "routine 1" routine add --goal 1 --every 1
step "run-due fires, verifies and auto-accepts"
expect 0 "accepted by policy:auto-observe" run-due
step "status"
expect 0 "#1 [done]" status
case $LAST in *"accepted_by policy:auto-observe"*) ;; *) fail "status does not name the policy actor" ;; esac
step "the next slot runs without the owner"
sleep 1.2
expect 0 "accepted by policy:auto-observe" run-due
expect 0 "#2 [done]" status
step "stop switch refuses run-due (exit 2)"
expect 0 "stop switch ON" stop
expect 2 "stop switch" run-due
expect 0 "off" resume
step "an acceptance policy is rejected when unknown"
expect 1 "unknown acceptance" goal add --title t --probe web --acceptance sometimes
step "backup and restore"
expect 0 "backup written" backup --to "$T/bk/copy.db"
live=$(run status 2>&1)
restored=$("$BIN" --db "$T/bk/copy.db" --config "$T/foundation.conf" --token-file "$TOKEN" status 2>&1)
[ "$live" = "$restored" ] || fail "the restored copy's status differs from the live one"
step "a closed port ends its item failed and the watch goes on"
kill "$LISTENER"; wait "$LISTENER" 2>/dev/null; LISTENER=""
sleep 1.2
expect 0 "failed (failure 1 in a row)" run-due
expect 0 "[failed]" status
echo "smoke OK: $BIN"
