#!/bin/sh
# Documentation checks for the foundation docs. One local command, no network, no build:
#   sh scripts/check-docs.sh [ROOT]            ROOT defaults to the repository root
#   RUN_EXAMPLE=target/debug/endora-foundation sh scripts/check-docs.sh   also runs examples/a-observe/run.sh
# Exit 0 if every check passes; otherwise prints one `FAIL ...` line per problem and exits 1.
# Not wired into CI on purpose (a workflow change is a separate decision).
#
# What it checks
#   1. status    every non-ADR doc (README.md, AGENTS.md, CONTRIBUTING.md, docs/**, examples/**, deploy/**) starts with
#                "Status: <CURRENT|EXPERIMENTAL|LEGACY|UNSUPPORTED|PLANNED>"
#                (for the files HQ embeds in its binary, docs/standards.md and the top level of
#                docs/plans and docs/drafts, the line after the title: an app/hq test needs "# " first)
#   2. links     relative links and #anchors in those docs plus the root meta files, skipping fenced
#                code and inline code; docs/adr, docs/proposals and docs/plans are history kept as
#                written and are not link-checked
#   3. cited     backticked *.rs and *.sh paths in non-LEGACY docs exist in the tree
#   4. phrases   a short forbidden and required phrase list seeded from contradictions found in the
#                docs (see the table below); catches a regression of a KNOWN contradiction only
#   5. example   the output blocks of docs/start.md equal examples/a-observe/expected-output.txt
#   6. addresses private-range IPv4 addresses and home-directory paths in any *.md, except the
#                documented dummies listed in ALLOWED_* below
# What it cannot check: host names, machine names, people's names, or whether the prose is true.
# For those use scripts/check-nothing-personal.mjs with the owner's values, and review.
set -u
ROOT=${1:-$(cd "$(dirname "$0")/.." && pwd)}
cd "$ROOT" || { echo "check-docs: cannot enter $ROOT" >&2; exit 2; }
TMP=$(mktemp -d "${TMPDIR:-/tmp}/check-docs.XXXXXX") || exit 2
trap 'rm -rf "$TMP"' EXIT INT TERM
FAILS=0
fail() { printf 'FAIL %s\n' "$*"; FAILS=$((FAILS + 1)); }
LABELS='CURRENT|EXPERIMENTAL|LEGACY|UNSUPPORTED|PLANNED'

# Documented dummy values that may appear (everything else in a private range fails). They are built
# from parts so that no address literal sits in this file for a privacy scan to flag.
ip() { printf ' %s.%s.%s.%s ' "$1" "$2" "$3" "$4"; }
PLACEHOLDER=$(ip 172 31 99 10)   # the repository's documented placeholder address
ALLOWED_IPS="$PLACEHOLDER$(ip 172 31 99 0)$(ip 172 31 250 0)$(ip 172 31 240 0)$(ip 172 16 0 0)"
# the placeholder, two Docker bridge defaults and a network name from the LEGACY Ansible docs, a range name
ONLY_LEGACY_IPS=$PLACEHOLDER   # the placeholder may appear only in LEGACY docs and AGENTS.md/CLAUDE.md, never in a current doc
IGNORE_CITED=' test.sh agent.sh '   # file names that are examples inside a job, not files of this repository
ALLOWED_PATHS='^/(Users|home)/(x|user|you|me|name|username|USER|YOU|catalog)(/|$)'

status_line() { # status_line FILE: the line that must be the Status line
    case $1 in
        docs/standards.md|docs/plans/*.md|docs/drafts/*.md)
            case $1 in docs/plans/*/*|docs/drafts/*/*) head -n 1 "$1"; return ;; esac
            if head -n 1 "$1" | grep -q '^# '; then sed -n '2,$p' "$1" | grep -m 1 -v '^[[:space:]]*$'; return; fi ;;
    esac
    head -n 1 "$1"
}
find . -name '*.md' -not -path './target/*' -not -path './.git/*' -not -path './node_modules/*' |
    sed 's#^\./##' | LC_ALL=C sort > "$TMP/all"
# Scopes
grep -E '^(README\.md|AGENTS\.md|CONTRIBUTING\.md|docs/|examples/|deploy/)' "$TMP/all" | grep -v '^docs/adr/' > "$TMP/status_scope"
{ cat "$TMP/status_scope"; grep -E '^(AGENTS|CONTRIBUTING|SECURITY|GOVERNANCE|CODE_OF_CONDUCT|CHANGELOG|CLAUDE)\.md$' "$TMP/all"; } |
    grep -vE '^docs/(proposals|plans)/' | LC_ALL=C sort -u > "$TMP/link_scope"
: > "$TMP/current_scope"
while IFS= read -r f; do
    status_line "$f" | grep -Eq "^Status: (CURRENT|EXPERIMENTAL|UNSUPPORTED|PLANNED)( |$)" && echo "$f" >> "$TMP/current_scope"
done < "$TMP/status_scope"

# ---- 1. status lines
# The first line, except for the files HQ embeds in its binary (app/hq/build.rs: docs/standards.md and the
# top level of docs/plans and docs/drafts): a test in app/hq requires their text to start with "# ", so
# there the Status line is the first line after the title.
while IFS= read -r f; do
    status_line "$f" | grep -Eq "^Status: ($LABELS)( |$)" ||
        fail "status: $f: the Status line is missing or is not \"Status: <$LABELS>\" (first line; after the title for HQ-embedded files)"
done < "$TMP/status_scope"

# ---- 2. links and anchors
slugs() { # slugs FILE: GitHub-style heading anchors, one per line (fenced code skipped)
    awk '/^```/ {f = !f; next} f {next}
         /^#+[ \t]/ { h = $0; sub(/^#+[ \t]+/, "", h); h = tolower(h); gsub(/[`*]/, "", h);
                      gsub(/[^a-z0-9 _-]/, "", h); gsub(/ /, "-", h); print h }' "$1"
}
links() { # links FILE: "line<TAB>target" for each [text](target) outside code
    awk '/^```/ {f = !f; next} f {next}
         { l = $0; gsub(/`[^`]*`/, "", l)
           while (match(l, /\]\([^) \t]+/)) { t = substr(l, RSTART + 2, RLENGTH - 2); print NR "\t" t; l = substr(l, RSTART + RLENGTH) } }' "$1"
}
while IFS= read -r f; do
    dir=$(dirname "$f")
    links "$f" | while IFS="$(printf '\t')" read -r n t; do
        case $t in [A-Za-z][A-Za-z0-9+.-]*:*) continue ;; esac
        path=${t%%#*}; anchor=""
        case $t in *'#'*) anchor=${t#*#} ;; esac
        if [ -z "$path" ]; then target=$f; else target=$dir/$path; fi
        if [ ! -e "$target" ]; then fail "links: $f:$n: $t does not exist"; continue; fi
        if [ -n "$anchor" ]; then
            case $target in *.md) slugs "$target" | grep -qxF -- "$anchor" || fail "links: $f:$n: no heading anchor #$anchor in $target" ;; esac
        fi
    done
done < "$TMP/link_scope" > "$TMP/links.out"
# the pipeline's fail() ran in a subshell: relay its lines and count
if [ -s "$TMP/links.out" ]; then cat "$TMP/links.out"; FAILS=$((FAILS + $(wc -l < "$TMP/links.out"))); fi

# ---- 3. cited files (*.rs, *.sh in inline code of non-LEGACY docs)
find . \( -name '*.rs' -o -name '*.sh' \) -not -path './target/*' -not -path './.git/*' | sed 's#^\./##' > "$TMP/srcfiles"
sed 's#.*/##' "$TMP/srcfiles" | LC_ALL=C sort -u > "$TMP/srcnames"
while IFS= read -r f; do
    dir=$(dirname "$f")
    awk '/^```/ {fe = !fe; next} fe {next} {print}' "$f" | grep -o '`[^`]*`' | tr -d '`' | tr ' ' '\n' |
        sed -e 's/^[("]*//' -e 's/[.,;:)"]*$//' | grep -E '^[A-Za-z0-9_./-]+\.(rs|sh)$' | LC_ALL=C sort -u |
        while IFS= read -r w; do
            case $IGNORE_CITED in *" $w "*) continue ;; esac
            case $w in
                */*) [ -e "$w" ] || [ -e "$dir/$w" ] || grep -q "/$w\$" "$TMP/srcfiles" ||
                        fail "cited: $f: \`$w\` does not exist" ;;
                *) grep -qxF -- "$w" "$TMP/srcnames" || fail "cited: $f: \`$w\` does not exist anywhere in the tree" ;;
            esac
        done
done < "$TMP/current_scope" > "$TMP/cited.out"
if [ -s "$TMP/cited.out" ]; then cat "$TMP/cited.out"; FAILS=$((FAILS + $(wc -l < "$TMP/cited.out"))); fi

# ---- 4. forbidden and required phrases
# Forbidden phrases come from contradictions found by the DOCS-1 audit (15 Nov 2026):
#   spend: README said "no shipped capability declares Enforced" and, 90 lines later, that the Claude
#     Code executor does; trust: "the declaration is the trust boundary" vs "the boundary is isolation";
#     stale deploy line "written but not built"; "branch from develop" (the remote has no develop);
#     Fact::word takes &'static str (it takes &str); "one crate per integration" and "without a database"
#     (review round 1); dangling REVIEW2/REVIEW3 and "the Director".
{ cat "$TMP/current_scope"; echo README.md; echo AGENTS.md; echo CONTRIBUTING.md; echo docs/adr/0150-current-architecture-the-foundation.md; } | LC_ALL=C sort -u > "$TMP/phrase_scope"
forbid() { # forbid REGEX WHY: no file in phrase_scope may match (case-insensitive)
    while IFS= read -r f; do
        grep -inE -- "$1" "$f" | sed "s#^#FAIL phrases: $f:#; s#\$#   <- forbidden: $2#"
    done < "$TMP/phrase_scope" > "$TMP/forbid.out"
    if [ -s "$TMP/forbid.out" ]; then cat "$TMP/forbid.out"; FAILS=$((FAILS + $(wc -l < "$TMP/forbid.out"))); fi
}
forbid 'no shipped capability declares' 'contradicts the Claude Code executor declaring Enforced'
forbid 'declaration is the trust boundary' 'the boundary is isolation; declarations are policy metadata'
forbid 'written but not built' 'the container recipe was built on a Linux host'
forbid 'Pin both base images by digest first' 'the Dockerfile already pins both'
forbid 'branch from [*]*.?develop' 'the remote has no develop branch; branch from main'
forbid "Fact::word.{0,40}'static str" 'Fact::word takes &str; a word fact must be in the spec closed list'
forbid 'a separate crate behind one trait|one crate per integration' 'a capability can be a config line, one module, an adapter or a crate (placement rule)'
forbid 'without a database or a model' 'Core::in_memory opens an in-memory SQLite database; say "without a database file"'
forbid 'REVIEW[0-9]' 'dangling reference: no such document is in the tree'
forbid '(^|[^a-z])the Director([^a-z]|$)' 'dangling reference to a private role'
# Old HQ rules ("one crate per bounded context", the mandatory layering, "Endora follows ROCA") may stay in a
# section only if its first line is "LEGACY (HQ)" (optionally in bold); such a section, up to the next heading of
# the same or a higher level, is skipped; so is everything from a heading that starts "LEGACY:" to the end of the file. The text is joined across line breaks, so a wrapped phrase is found.
visible() { # visible FILE: the file's text without LEGACY (HQ)-marked sections, newlines as spaces
    awk '/^```/ {fence = !fence}
         !fence && /^#+ +LEGACY:/ { skip = 1; skiplvl = 0; print; next }
         !fence && /^#+ / { lvl = match($0, /[^#]/) - 1
                            if (skip && lvl <= skiplvl) skip = 0
                            if (!skip) { head = lvl; pending = 1 } ; print; next }
         pending && NF { pending = 0; if ($0 ~ /^[-*[:space:]]*LEGACY \(HQ\)/) { skip = 1; skiplvl = head; next } }
         !skip { print }' "$1" | tr '\n' ' '
}
forbid_flat() { # forbid_flat REGEX WHY
    while IFS= read -r f; do
        hit=$(visible "$f" | grep -oiE -- "$1" | head -n 1)
        [ -z "$hit" ] || printf 'FAIL phrases: %s: "%s"   <- forbidden outside a LEGACY (HQ) section: %s\n' "$f" "$hit" "$2"
    done < "$TMP/phrase_scope" > "$TMP/forbid.out"
    if [ -s "$TMP/forbid.out" ]; then cat "$TMP/forbid.out"; FAILS=$((FAILS + $(wc -l < "$TMP/forbid.out"))); fi
}
forbid_flat 'one crate +per +(bounded +)?context|a +crate +per +(bounded +)?context|Endora +follows +[*]*Responsibility-Oriented|Respect +the +layering +Domain' 'a bounded context is not a crate and there is no mandatory three-layer template (docs/architecture.md, ADR 0150)'
forbid_flat 'new +crate +under +`?domains/?`? +depending' 'place code by the placement rule: a module is the usual case'
required() { # required FILE FIXED-PHRASE
    grep -qF -- "$2" "$1" 2>/dev/null || fail "phrases: $1 must contain: $2"
}
required README.md 'local-first foundation for autonomous work'
required README.md 'Dependencies point inward'
required docs/architecture.md 'known shortcut: Store is concrete'
required docs/architecture.md '## Known deviations'
required docs/foundation/README.md 'policy metadata'
required docs/foundation/README.md '## Known deviations'
required docs/foundation/TRUST-BOUNDARY.md 'policy metadata'
roca=$(grep -c 'ROCA' README.md 2>/dev/null)
[ "${roca:-0}" -le 1 ] || fail "phrases: README.md mentions ROCA $roca times; at most one sentence is allowed"
# README must state the same counts as the STABILITY table
frozen=$(grep -E '^\| ' docs/foundation/STABILITY.md | awk -F'|' '{gsub(/^ +/, "", $3); if ($3 ~ /^FROZEN/) n++} END {print n + 0}')
exper=$(grep -E '^\| ' docs/foundation/STABILITY.md | awk -F'|' '{gsub(/^ +/, "", $3); if ($3 ~ /^EXPERIMENTAL/) n++} END {print n + 0}')
grep -qF "$frozen FROZEN, $exper EXPERIMENTAL" README.md ||
    fail "phrases: README.md must say \"$frozen FROZEN, $exper EXPERIMENTAL\" (counted from docs/foundation/STABILITY.md)"

# ---- 5. example A: the tutorial's output blocks are the expected file
EXP=examples/a-observe/expected-output.txt
if [ -f "$EXP" ] && [ -f docs/start.md ]; then
    trim() { awk 'NF {p = NR} {l[NR] = $0} END {for (i = 1; i <= p; i++) print l[i]}'; }
    n=1
    while grep -qF "# $n. " "$EXP"; do
        awk -v n="$n" '/^# [0-9]+\. / {split($2, a, "."); cur = a[1] + 0; next} cur == n {print}' "$EXP" | trim > "$TMP/exp.$n"
        awk -v m="<!-- example-a: $n -->" '$0 == m {f = 1; next} f == 1 && /^```/ {f = 2; next} f == 2 && /^```/ {exit} f == 2 {print}' docs/start.md | trim > "$TMP/doc.$n"
        [ -s "$TMP/doc.$n" ] || fail "example: docs/start.md has no <!-- example-a: $n --> block"
        cmp -s "$TMP/exp.$n" "$TMP/doc.$n" || fail "example: docs/start.md step $n differs from $EXP"
        n=$((n + 1))
    done
else
    fail "example: $EXP or docs/start.md is missing"
fi
if [ -n "${RUN_EXAMPLE:-}" ]; then
    sh examples/a-observe/run.sh "$RUN_EXAMPLE" > "$TMP/run.out" 2>&1 || { fail "example: run.sh failed"; tail -n 20 "$TMP/run.out"; }
fi

# ---- 6. private addresses and home-directory paths in any *.md
while IFS= read -r f; do
    grep -noE '(^|[^0-9.])(10\.[0-9]+\.[0-9]+\.[0-9]+|172\.(1[6-9]|2[0-9]|3[01])\.[0-9]+\.[0-9]+|192\.168\.[0-9]+\.[0-9]+)' "$f" |
    while IFS=: read -r n hit; do
        ip=$(printf '%s' "$hit" | sed 's/^[^0-9]*//')
        case $ALLOWED_IPS in *" $ip "*) ;; *) fail "addresses: $f:$n: private address $ip is not a listed dummy"; continue ;; esac
        case $ONLY_LEGACY_IPS in *" $ip "*)
            case $f in AGENTS.md|CLAUDE.md) ;; *)
                grep -qxF "$f" "$TMP/status_scope" || continue
                status_line "$f" | grep -q '^Status: LEGACY' || fail "addresses: $f:$n: $ip may appear only in LEGACY docs, use 127.0.0.1 or 192.0.2.x" ;;
            esac ;;
        esac
    done
    grep -noE '/(Users|home)/[A-Za-z0-9_.-]+' "$f" | while IFS=: read -r n hit; do
        printf '%s' "$hit" | grep -Eq "$ALLOWED_PATHS" && continue
        fail "addresses: $f:$n: home-directory path $hit"
    done
done < "$TMP/all" > "$TMP/addr.out"
if [ -s "$TMP/addr.out" ]; then cat "$TMP/addr.out"; FAILS=$((FAILS + $(wc -l < "$TMP/addr.out"))); fi

if [ "$FAILS" -eq 0 ]; then
    echo "check-docs OK: $(wc -l < "$TMP/status_scope" | tr -d ' ') status files, $(wc -l < "$TMP/link_scope" | tr -d ' ') link-checked files, $(wc -l < "$TMP/current_scope" | tr -d ' ') non-legacy files (host names, people's names and prose accuracy are not checked)"
    exit 0
fi
echo "check-docs: $FAILS problem(s)"
exit 1
