# What the bundle test may touch: only what it made itself (test-bundle.sh sources this;
# ci/test-owned.sh tests it against a disposable Docker daemon).
#
# The bundle fixes names no project name can change: Console's and Audio's compose files set
# container_name, and Audio's services run on the host's network. Run under the project name
# `endora` (the one a real install uses), the test's `down -v --remove-orphans` once removed
# what a real install had left on the daemon. So the test:
#
#   1. runs under a project name of its own, random per run (`unique_project`);
#   2. refuses to start, before it changes anything, when a container, volume or network it
#      would create already exists on the daemon, whoever made it (`guard`). Unique names do
#      not make a fixed container_name safe on their own, and a refusal is the only safe
#      answer to a name that is already taken;
#   3. takes down only what carries its own project's label (`teardown`), and only once
#      `owned` is set, which is just before the first thing it creates. A run that was
#      refused removes nothing.
#
# The caller defines `compose` (always `docker compose -p "$project" ...`) and `project`.

# unique_project: a Compose project name no install, and no other run, has.
unique_project() {
  printf 'endora-ci-%s' "$(od -An -N6 -tx1 /dev/urandom | tr -d ' \n')"
}

# claimed: "container NAME", "volume NAME" or "network NAME" for each name the bundle would
# create under this project, as Compose resolves them (external ones are not ours to make).
claimed() {
  compose config --format json | python3 -c '
import json, sys
c = json.load(sys.stdin)
for s in c.get("services", {}).values():
    if s.get("container_name"):
        print("container", s["container_name"])
for kind in ("volume", "network"):
    for r in c.get(kind + "s", {}).values():
        if not r.get("external") and r.get("name"):
            print(kind, r["name"])
'
}

# guard: 0 when nothing the test would create exists yet. Otherwise 1, saying what is in the way.
# Fails closed: a daemon or a model it cannot read is a refusal, not a pass.
guard() {
  g=$(mktemp -d) || return 1
  refused=0
  docker ps -a --format '{{.Names}}' > "$g/container" 2>/dev/null \
    && docker volume ls -q > "$g/volume" 2>/dev/null \
    && docker network ls --format '{{.Name}}' > "$g/network" 2>/dev/null \
    || { echo "guard: cannot list the Docker daemon's containers, volumes and networks; nothing was started" >&2; rm -rf "$g"; return 1; }
  claimed > "$g/claims" 2>/dev/null \
    || { echo "guard: cannot read the bundle's names from 'compose config'; nothing was started" >&2; rm -rf "$g"; return 1; }
  if [ ! -s "$g/claims" ]; then
    echo "guard: the bundle claims no names, so there is nothing to check them against; nothing was started" >&2
    rm -rf "$g"; return 1
  fi
  while read -r kind name; do
    if grep -qxF -- "$name" "$g/$kind"; then
      echo "guard: the $kind '$name' already exists on this Docker daemon and is not this run's." >&2
      refused=1
    fi
  done < "$g/claims"
  # Nothing may carry this run's own label yet: the name is random, so this is a second proof.
  label="label=com.docker.compose.project=$project"
  for found in "ps -aq" "volume ls -q" "network ls -q"; do
    # shellcheck disable=SC2086
    if [ -n "$(docker $found --filter "$label" 2>/dev/null)" ]; then
      echo "guard: something on this daemon already carries the project label $project" >&2
      refused=1
    fi
  done
  rm -rf "$g"
  if [ "$refused" -ne 0 ]; then
    echo "guard: refusing to start. Run the bundle test on a Docker daemon with no Endora on it (CI's plain runner, or a disposable one such as docker:dind); nothing was changed." >&2
    return 1
  fi
  return 0
}

# teardown: removes what this run made and nothing else. Compose's `down -v` takes this
# project's containers, networks and anonymous volumes (and its own named volumes, which carry
# the project label); a second pass by label, with the project name unique to this run, takes
# anything a failed `up` left half-made. Returns 1 if anything of ours is still there.
teardown() {
  compose down -v --remove-orphans >/dev/null 2>&1 || true
  label="label=com.docker.compose.project=$project"
  ids=$(docker ps -aq --filter "$label" 2>/dev/null || true)
  # shellcheck disable=SC2086
  [ -z "$ids" ] || docker rm -fv $ids >/dev/null 2>&1 || true
  ids=$(docker volume ls -q --filter "$label" 2>/dev/null || true)
  # shellcheck disable=SC2086
  [ -z "$ids" ] || docker volume rm -f $ids >/dev/null 2>&1 || true
  ids=$(docker network ls -q --filter "$label" 2>/dev/null || true)
  # shellcheck disable=SC2086
  [ -z "$ids" ] || docker network rm $ids >/dev/null 2>&1 || true
  left=""
  for found in "ps -aq" "volume ls -q" "network ls -q"; do
    # shellcheck disable=SC2086
    left="$left$(docker $found --filter "$label" 2>/dev/null || true)"
  done
  if [ -n "$left" ]; then
    echo "teardown: something of project $project is still on the Docker daemon" >&2
    return 1
  fi
  return 0
}
