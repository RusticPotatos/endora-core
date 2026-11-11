#!/bin/sh
# Tests that the bundle test touches only what it made (ci/owned.sh, test-bundle.sh).
#
#   sh deploy/compose/ci/test-owned.sh
#
# Needs Docker able to run a privileged container (docker:dind), and alpine:3.21 (pulled if it is
# not on the host). It never runs a scenario on the daemon it is started against: it starts a
# disposable Docker daemon of its own (docker:dind, no network, its socket in a temporary
# folder), PROVES that daemon is a different, empty one before any scenario, and points every
# docker call at it. The host's daemon only runs that one container, which is removed at the
# end; the host's other containers, volumes and networks are compared before and after.
#
# On that daemon each scenario seeds a stopped decoy, as a real install's leftovers would be
# (project `endora`, a container with an anonymous volume holding a marker, a network), then:
#   control   the old teardown (`-p endora ... down -v --remove-orphans`) destroys the decoy, so
#             the scenarios below can fail;
#   bundle    the real test-bundle.sh, whose `up` fails (no image can be pulled): the decoy,
#             and everything else on the daemon, is exactly as it was; a foreign container with
#             one of the bundle's fixed names makes it refuse before it changes anything;
#   library   owned.sh against a small bundle of the same shape: a fresh success whose cleanup
#             removes everything it made, a failing `up` whose partial result is cleaned, a
#             refusal for a taken container, volume or network name, a refusal when the daemon
#             or the model cannot be read, and a teardown that leaves a foreign volume alone
#             even when the guard was bypassed.
set -u

here=$(cd "$(dirname "$0")" && pwd)
bundle_dir=${TEST_OWNED_BUNDLE_DIR:-$(cd "$here/.." && pwd)}   # the override is for running this against an older copy
FAILED=0
ok() { echo "  ok    $1"; }
bad() { echo "  FAIL  $1"; FAILED=1; }
expect() { if [ "$2" -eq 0 ]; then ok "$1"; else bad "$1"; fi; }

command -v docker >/dev/null || { echo "  FAIL  docker is needed"; exit 1; }
command -v python3 >/dev/null || { echo "  FAIL  python3 is needed"; exit 1; }

orig_host=${DOCKER_HOST-}
host_docker() { if [ -n "$orig_host" ]; then DOCKER_HOST=$orig_host docker "$@"; else env -u DOCKER_HOST docker "$@"; fi; }

run=$(od -An -N6 -tx1 /dev/urandom | tr -d ' \n')
work=$(mktemp -d /tmp/endora-owned.XXXXXX)     # short: a unix socket path is limited to 108 bytes
sock="$work/docker.sock"
dind="endora-owned-test-$run"
dind_id=""

# What the host's daemon has, apart from the one disposable container and its volume.
host_snapshot() {
  mine=$(host_docker inspect "$dind" --format '{{range .Mounts}}{{.Name}} {{end}}' 2>/dev/null || true)
  {
    host_docker ps -a --no-trunc --format '{{.ID}} {{.Names}} {{.State}}' | grep -vF " $dind " || true
    host_docker volume ls -q | grep -vxF "$(printf '%s' "$mine" | tr ' ' '\n' | grep . || echo __none__)" || true
    host_docker network ls --no-trunc --format '{{.ID}} {{.Name}}' || true
  } | sort
}

cleanup() {
  # Only the container this run started, found by the name and label it was given.
  if [ -n "$(host_docker ps -aq --filter "name=^$dind\$" --filter "label=endora-owned-test=$run" 2>/dev/null)" ]; then
    host_docker rm -fv "$dind" >/dev/null 2>&1
  fi
  rm -rf "$work"
}
trap cleanup EXIT
trap 'exit 130' INT TERM

host_before=$(host_snapshot)

echo "== a disposable Docker daemon of its own"
host_docker run -d --privileged --network none --name "$dind" --label "endora-owned-test=$run" \
  -e DOCKER_TLS_CERTDIR= -v "$work:/sock-dir" docker:29-dind \
  --host=unix:///sock-dir/docker.sock --group="$(id -g)" >/dev/null || { echo "  FAIL  could not start docker:dind"; exit 1; }
i=0
until DOCKER_HOST=unix://$sock docker info >/dev/null 2>&1; do
  i=$((i + 1)); [ "$i" -lt 60 ] || { echo "  FAIL  the disposable daemon did not come up"; host_docker logs "$dind" 2>&1 | tail -5; exit 1; }
  sleep 1
done
d() { DOCKER_HOST=unix://$sock docker "$@"; }
host_id=$(host_docker info --format '{{.ID}}')
dind_id=$(d info --format '{{.ID}}')

# The proof, before any scenario: a different daemon, empty, reachable only through our socket,
# and an object made on it is not on the host's daemon.
prove_isolated() {
  [ -n "$host_id" ] && [ -n "$dind_id" ] && [ "$host_id" != "$dind_id" ] || return 1
  [ "$(d info --format '{{.ID}}')" = "$dind_id" ] || return 1
  [ "$(host_docker inspect "$dind" --format '{{.HostConfig.NetworkMode}}')" = none ] || return 1
  probe="endora-owned-probe-$run"
  d network create "$probe" >/dev/null || return 1
  if host_docker network ls --format '{{.Name}}' | grep -qxF "$probe"; then d network rm "$probe" >/dev/null; return 1; fi
  d network ls --format '{{.Name}}' | grep -qxF "$probe" || return 1
  d network rm "$probe" >/dev/null
}
prove_isolated
expect "the scenarios' daemon is a different one from the host's (ID, no network, an object made there is not here)" $?
[ "$FAILED" -eq 0 ] || { echo "no scenario was run: the daemon is not proven isolated"; exit 1; }
empty=$(d ps -aq; d volume ls -q; d network ls --format '{{.Name}}' | grep -vxE 'bridge|host|none')
[ -z "$empty" ]; expect "and it starts empty" $?
[ "$FAILED" -eq 0 ] || exit 1
export DOCKER_HOST=unix://$sock
# Every scenario starts by proving it again, so none can run on another daemon.
assert_isolated() { [ "$(docker info --format '{{.ID}}')" = "$dind_id" ] && [ "$dind_id" != "$host_id" ] || { echo "ABORT: not the disposable daemon"; exit 1; }; }

host_docker save alpine:3.21 >/dev/null 2>&1 || host_docker pull -q alpine:3.21 >/dev/null
host_docker save alpine:3.21 | docker load >/dev/null || { echo "  FAIL  could not load alpine into the disposable daemon"; exit 1; }

# ---- fixtures ----
fx="$work/fx"; mkdir -p "$fx"
token="marker-$run"
cat > "$fx/decoy.yaml" <<EOF
name: endora
services:
  keeper:
    image: alpine:3.21
    command: sleep 600
    volumes: ["/marker"]
    networks: [keepnet]
networks:
  keepnet: {}
EOF
# A bundle of the real one's shape: fixed container, volume and network names, an anonymous volume.
cat > "$fx/compose.yaml" <<EOF
name: endora
services:
  a:
    image: alpine:3.21
    container_name: endora-owned-fx-a
    command: sleep 600
    volumes: ["endora-owned-fx-data:/d", "/anon"]
    networks: [fixed]
networks:
  fixed:
    name: endora-owned-fx-net
volumes:
  endora-owned-fx-data:
    name: endora-owned-fx-data
EOF
# The same bundle with the volume declared external: a volume made by someone else, which
# `down -v` must leave alone (a volume Compose declares itself is its to remove).
cat > "$fx/compose.foreign.yaml" <<EOF
name: endora
services:
  a:
    image: alpine:3.21
    container_name: endora-owned-fx-a
    command: sleep 600
    volumes: ["endora-owned-fx-data:/d", "/anon"]
    networks: [fixed]
networks:
  fixed:
    name: endora-owned-fx-net
volumes:
  endora-owned-fx-data:
    name: endora-owned-fx-data
    external: true
EOF
cat > "$fx/compose.fail.yaml" <<EOF
services:
  b:
    image: alpine:3.21
    command: sh -c "exit 3"
EOF

snapshot() { { docker ps -a --no-trunc --format '{{.ID}} {{.Names}} {{.State}}'; docker volume ls -q; docker network ls --no-trunc --format '{{.ID}} {{.Name}}'; } | sort; }
reset() { assert_isolated; ids=$(docker ps -aq); [ -z "$ids" ] || docker rm -fv $ids >/dev/null; docker volume prune -af >/dev/null; docker network prune -f >/dev/null; }
seed_decoy() {
  docker compose -p endora -f "$fx/decoy.yaml" up -d >/dev/null 2>&1 || return 1
  docker compose -p endora -f "$fx/decoy.yaml" exec -T keeper sh -c "printf %s $token > /marker/m" || return 1
  docker inspect endora-keeper-1 --format '{{range .Mounts}}{{.Name}}{{end}}' > "$work/decoy.vol"
  docker compose -p endora -f "$fx/decoy.yaml" stop >/dev/null 2>&1
}
decoy_intact() {
  [ "$(docker inspect endora-keeper-1 --format '{{.State.Status}}' 2>/dev/null)" = exited ] || return 1
  docker network ls --format '{{.Name}}' | grep -qxF endora_keepnet || return 1
  [ "$(docker run --rm -v "$(cat "$work/decoy.vol")":/m alpine:3.21 cat /m/m 2>/dev/null)" = "$token" ]
}
# Nothing on the daemon carries a project label of the test's own (endora-ci-*).
no_owned_left() {
  { docker ps -a --format '{{.Label "com.docker.compose.project"}}'
    docker volume ls --format '{{.Label "com.docker.compose.project"}}'
    docker network ls --format '{{.Label "com.docker.compose.project"}}'; } | grep -q '^endora-ci-' && return 1
  return 0
}

# ---- control ----
echo "== control: the old teardown destroys a decoy (so these scenarios can fail)"
reset; seed_decoy; decoy_intact; expect "the decoy is seeded: stopped, with its marker volume and network" $?
# What the old test did on exit, with the project name a real install uses: the decoy's service is
# not in this bundle, so Compose reads it as an orphan of the project.
docker compose -p endora -f "$fx/compose.yaml" down -v --remove-orphans >/dev/null 2>&1
decoy_intact; r=$?; [ $r -ne 0 ]; expect "down -v --remove-orphans under project 'endora' removes it" $?

# ---- the real test-bundle.sh ----
echo "== test-bundle.sh: its up fails; a decoy and everything else on the daemon is preserved"
reset; seed_decoy
before=$(snapshot)
sh "$bundle_dir/test-bundle.sh" > "$work/bundle.out" 2>&1
r=$?; [ $r -ne 0 ]; expect "it fails (no image can be pulled here)" $?
grep -q '^== up' "$work/bundle.out" && ! grep -q 'guard: ' "$work/bundle.out"
expect "  and it got as far as up: the guard let a clean daemon through" $?
after=$(snapshot)
[ "$before" = "$after" ]; expect "  and every container, volume and network is exactly as before" $?
decoy_intact; expect "  and the decoy still has its container, network and marker volume" $?
no_owned_left; expect "  and nothing of the test's own project is left" $?

echo "== test-bundle.sh: a foreign container with one of its fixed names makes it refuse"
reset; seed_decoy
docker create --name endora-console --label com.docker.compose.project=elsewhere -v /marker alpine:3.21 true >/dev/null
before=$(snapshot)
sh "$bundle_dir/test-bundle.sh" > "$work/bundle.out" 2>&1
r=$?; [ $r -ne 0 ]; expect "it fails" $?
grep -q "guard: the container 'endora-console' already exists" "$work/bundle.out"; expect "  naming the container in the way" $?
! grep -q '^== Core' "$work/bundle.out"; expect "  before any check of the products ran" $?
[ "$before" = "$(snapshot)" ]; expect "  and the daemon is exactly as before: nothing created, nothing removed" $?
decoy_intact; expect "  and the decoy is intact" $?

# ---- owned.sh on a bundle of the same shape ----
echo "== owned.sh: fresh success, then cleanup of everything the run made"
reset; seed_decoy
before=$(snapshot)
(
  . "$here/owned.sh"
  project=$(unique_project)
  compose() { docker compose -p "$project" -f "$fx/compose.yaml" "$@"; }
  guard || exit 1
  compose up -d --wait >/dev/null 2>&1 || exit 2
  mine=$(docker ps -aq --filter "label=com.docker.compose.project=$project")
  [ -n "$mine" ] && docker volume ls -q --filter "label=com.docker.compose.project=$project" | grep -qxF endora-owned-fx-data \
    && docker network ls --format '{{.Name}}' | grep -qxF endora-owned-fx-net || exit 3
  docker volume ls -q | grep -c . > "$work/vols-during"
  teardown || exit 4
)
expect "the guard passes on a clean daemon, up succeeds, and teardown reports nothing left (exit $?)" $?
[ "$before" = "$(snapshot)" ]; expect "  every container, anonymous and named volume and network it made is gone; the rest is untouched" $?
decoy_intact; expect "  and the decoy is intact" $?
[ "$(cat "$work/vols-during")" -ge 3 ]; expect "  (the run did make volumes: the decoy's, its own named one and its anonymous one)" $?

echo "== owned.sh: an up that fails half-way"
reset; seed_decoy
before=$(snapshot)
(
  . "$here/owned.sh"
  project=$(unique_project)
  compose() { docker compose -p "$project" -f "$fx/compose.yaml" -f "$fx/compose.fail.yaml" "$@"; }
  guard || exit 1
  compose up -d --wait >/dev/null 2>&1 && exit 2
  # The first service was made before the second failed: there is something to clean.
  [ -n "$(docker ps -aq --filter "label=com.docker.compose.project=$project")" ] || exit 3
  teardown || exit 4
)
expect "up fails with a partial result, and teardown removes all of it (exit $?)" $?
[ "$before" = "$(snapshot)" ]; expect "  nothing else on the daemon changed" $?
decoy_intact; expect "  and the decoy is intact" $?

echo "== owned.sh: the guard refuses a name that is taken, and changes nothing"
for taken in container volume network; do
  reset; seed_decoy
  case $taken in
    container) docker create --name endora-owned-fx-a --label com.docker.compose.project=elsewhere alpine:3.21 true >/dev/null ;;
    volume) docker volume create endora-owned-fx-data >/dev/null ;;
    network) docker network create endora-owned-fx-net >/dev/null ;;
  esac
  before=$(snapshot)
  out=$(
    . "$here/owned.sh"
    project=$(unique_project)
    compose() { docker compose -p "$project" -f "$fx/compose.yaml" "$@"; }
    guard 2>&1
  )
  r=$?; [ $r -ne 0 ]; expect "a $taken with the bundle's fixed name makes the guard refuse" $?
  printf '%s' "$out" | grep -q "the $taken 'endora-owned-fx-"; expect "  naming it" $?
  [ "$before" = "$(snapshot)" ]; expect "  and the guard changed nothing" $?
done

echo "== owned.sh: the guard fails closed"
reset
(
  . "$here/owned.sh"
  project=$(unique_project)
  compose() { docker compose -p "$project" -f "$fx/does-not-exist.yaml" "$@"; }
  guard >/dev/null 2>&1
); r=$?; [ $r -ne 0 ]; expect "a bundle whose names cannot be read is refused, not passed" $?
(
  . "$here/owned.sh"
  project=$(unique_project)
  compose() { docker compose -p "$project" -f "$fx/compose.yaml" "$@"; }
  DOCKER_HOST=unix:///nonexistent/docker.sock guard >/dev/null 2>&1
); r=$?; [ $r -ne 0 ]; expect "a daemon that cannot be read is refused, not passed" $?

echo "== owned.sh: teardown leaves a foreign volume alone even if the guard was bypassed"
reset
docker volume create endora-owned-fx-data >/dev/null
docker run --rm -v endora-owned-fx-data:/d alpine:3.21 sh -c "printf %s $token > /d/m"
(
  . "$here/owned.sh"
  project=$(unique_project)
  compose() { docker compose -p "$project" -f "$fx/compose.foreign.yaml" "$@"; }
  compose up -d --wait >/dev/null 2>&1 || exit 2     # Compose uses the volume of that name, made above, and does not own it
  teardown
)
expect "teardown of a run that reused another's named volume succeeds" $?
[ "$(docker run --rm -v endora-owned-fx-data:/d alpine:3.21 cat /d/m 2>/dev/null)" = "$token" ]
expect "  and the foreign volume, with its contents, is still there" $?

echo "== the host's daemon"
host_after=$(host_snapshot)
[ "$host_before" = "$host_after" ]; expect "the host daemon's other containers, volumes and networks are exactly as before" $?

if [ "$FAILED" -eq 0 ]; then
  echo "bundle ownership tests passed"
  exit 0
fi
exit 1
