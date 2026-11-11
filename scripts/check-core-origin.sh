#!/bin/sh
# Holds this repository to what it was made from (docs/core-origin.md): Endora Core started from
# a recorded commit of RusticPotatos/Endora, and a split must not lose a gate or carry a private
# file along. Four rules, each failing loudly:
#   1. docs/core-origin.md names its source repository and a full 40-character source commit;
#   2. every workspace member in Cargo.toml has its Cargo.toml;
#   3. every repository path a workflow names (scripts/, deploy/, app/, ...) exists, so a step
#      cannot be left running a script that did not travel (generated output is listed below);
#   4. nothing private is in the tree: no `.local/`, `owner.mk`, `.env` (an `.env.example` is
#      fine), `vault` + `.yml` or key file.
#
#   sh scripts/check-core-origin.sh                 check this checkout
#   sh scripts/check-core-origin.sh --root DIR      check another tree
#   sh scripts/check-core-origin.sh --self-test     run scripts/test-check-core-origin.sh
set -u

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/.." && pwd)
while [ $# -gt 0 ]; do
  case $1 in
    --root) root=$(cd "$2" && pwd) || exit 2; shift 2 ;;
    --self-test) exec sh "$here/test-check-core-origin.sh" ;;
    *) echo "usage: check-core-origin.sh [--root DIR | --self-test]" >&2; exit 2 ;;
  esac
done

# Paths a workflow names that a build makes: not in the tree on purpose.
generated="clients/terminal/dist"

status=0
fail() { echo "core-origin: $*" >&2; status=1; }

# 1. The origin record.
record="$root/docs/core-origin.md"
if [ ! -f "$record" ]; then
  fail "docs/core-origin.md is missing: it names the commit this repository was made from"
else
  grep -q '^Source repository: [A-Za-z0-9_.-]*/[A-Za-z0-9_.-]*$' "$record" \
    || fail "docs/core-origin.md has no 'Source repository: owner/name' line"
  grep -q '^Source commit: [0-9a-f]\{40\}$' "$record" \
    || fail "docs/core-origin.md has no 'Source commit:' line with a full 40-character commit"
fi

# 2. Workspace members.
members=$(awk '/^members *= *\[/ { on = 1; next } on && /\]/ { on = 0 } on { gsub(/[ ",]/, ""); if ($0 != "") print }' "$root/Cargo.toml" 2>/dev/null)
[ -n "$members" ] || fail "Cargo.toml lists no workspace members"
for m in $members; do
  [ -f "$root/$m/Cargo.toml" ] || fail "workspace member $m has no Cargo.toml"
done

# 3. Paths the workflows name.
if [ -d "$root/.github/workflows" ]; then
  paths=$(cat "$root"/.github/workflows/*.yml 2>/dev/null \
    | grep -o '\(scripts\|deploy\|app\|crates\|domains\|shared\|clients\|docs\)/[A-Za-z0-9_./-]*' \
    | sed 's/[.]*$//' | sort -u)
  # A step may name a path relative to its `working-directory:`, so a path counts as present
  # when it exists from the root or from any directory a workflow runs in.
  dirs=$(cat "$root"/.github/workflows/*.yml 2>/dev/null \
    | sed -n 's/^ *\(- \)\{0,1\}working-directory: *\([A-Za-z0-9_./-]*\) *$/\2/p' | sort -u)
  for p in $paths; do
    case " $generated " in *" ${p%/} "*) continue ;; esac
    case $p in clients/terminal/dist/*) continue ;; esac
    found=0
    [ -e "$root/$p" ] && found=1
    for d in $dirs; do
      [ -e "$root/$d/$p" ] && found=1
    done
    [ "$found" -eq 1 ] || fail "a workflow names $p, which is not in the tree"
  done
fi

# 4. Nothing private.
# In a git checkout only what git would carry counts (the gate writes .local/gate into its tree,
# which git ignores); elsewhere, every file.
# Git reports the physical path of its top level, so $root is compared by its physical path too:
# on macOS a temporary folder under /var is reached through the /var -> /private/var symlink.
if git -C "$root" rev-parse --show-toplevel >/dev/null 2>&1 && [ "$(git -C "$root" rev-parse --show-toplevel)" = "$(cd "$root" && pwd -P)" ]; then
  files=$(git -C "$root" ls-files --cached --others --exclude-standard)
else
  files=$(cd "$root" && find . \( -name .git -o -name target -o -name node_modules \) -prune -o -type f -print | sed 's|^\./||')
fi
private=$(printf '%s\n' "$files" | awk -F/ '
  { for (i = 1; i <= NF; i++) {
      n = $i
      if (n == ".local" || n == "owner.mk" || n == ("lo" "cal.mk") || n == ("vault" ".yml") || n == "id_rsa" || n == "id_ed25519" ||
          n ~ /\.pem$/ || n ~ /\.key$/ || (n ~ /^\.env/ && n != ".env.example")) { print $0; break }
  } }' | sort -u)
if [ -n "$private" ]; then
  for f in $private; do fail "private-looking file in the tree: $f"; done
fi

if [ "$status" -eq 0 ]; then
  echo "core-origin check passed"
fi
exit "$status"
