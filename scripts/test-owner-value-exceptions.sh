#!/bin/sh
# Tests for the reviewed value-exception file of scripts/check-owner-values.mjs.
#
# What is under test: an owner value can sit inside an exact, functional upstream URL that must stay in
# a file (a Dockerfile, an ADR). The owner can excuse ONE exact URL token, at ONE exact path, a stated
# number of times, through a private file that is never in the repository
# (ENDORA_OWNER_VALUE_EXCEPTIONS_FILE). Anything else stays a finding: a bare value, the same URL in
# another path, a near-miss URL, a commit message, a file name, a binary. A malformed, forged or
# unknown input fails closed (exit 2).
# Fictional values and a fictional URL only: nothing here is a real value.
#
# Exit codes of the checker: 0 clean, 1 found something, 2 could not verify (fails closed).
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
chk="$root/scripts/check-owner-values.mjs"
tmp=$(mktemp -d); trap 'chmod -R u+rwx "$tmp" 2>/dev/null; rm -rf "$tmp"' EXIT
V1="zqval-one"; V2="zqval-two"; VALS="$V1,$V2"      # the sanctioned URL embeds position 2
U="https://example.invalid/$V2/upstream.git"
U2="https://example.invalid/$V2/second.git"
DF="Dockerfile"; ADR="docs/adr/9999-a-door.md"
fail=0; n=0
ok() { n=$((n + 1)); echo "  ok    $1"; }
bad() { n=$((n + 1)); fail=$((fail + 1)); echo "FAIL  $1"; }

newrepo() { d="$tmp/$1"; mkdir "$d"; git -C "$d" init -q -b main; git -C "$d" config user.name tester; git -C "$d" config user.email t@example.com; git -C "$d" config commit.gpgsign false; echo "$d"; }
commit() { git -C "$1" add -A >/dev/null; git -C "$1" commit -q --allow-empty -m "${2:-c}"; }
rootof() { git -C "$1" rev-list --max-parents=0 --all | head -1; }
sha() { printf '%s' "$1" | shasum -a 256 | cut -d' ' -f1; }
# line POSITION PATH TOKEN COUNT [ID]: one manifest entry, by hash of the token (never the token itself)
line() { printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$1" "$2" "$(sha "$3")" "${#3}" "$4" "${5:-reviewed-1}"; }
# exc FILE ROOT ENTRY...: a manifest file bound to a repository root
exc() { f=$1; rt=$2; shift 2; { printf '# test manifest\nroot\t%s\n' "$rt"; for e in "$@"; do printf '%s\n' "$e"; done; } > "$f"; }
# A fresh path per call. Called as $(nextexc), so a counter would not advance: mktemp gives a unique name.
nextexc() { mktemp "$tmp/exc.XXXXXX"; }
# run REPO EXCFILE|<none> [args]: sets $code and $out. Nothing is echoed.
# The out-of-band pin (ENDORA_OWNER_VALUE_EXCEPTIONS_SHA256) is supplied as the owner would: the file's
# own SHA-256, computed here. PINVAL=x overrides it for one call; NOPIN=1 supplies none for one call.
run() {
  r=$1; ef=$2; shift 2; out="$tmp/out.$n"
  set +e
  pin=""
  if [ "$ef" != "<none>" ] && [ -f "$ef" ]; then pin=$(shasum -a 256 "$ef" | cut -d' ' -f1); fi
  if [ -n "${PINVAL+x}" ]; then pin=$PINVAL; fi
  if [ -n "${NOPIN:-}" ]; then pin=""; fi
  if [ "$ef" = "<none>" ]; then
    (cd "$r" && env -u ENDORA_OWNER_VALUE_EXCEPTIONS_FILE -u ENDORA_REVIEWED_BINARIES_SHA256 -u ENDORA_OWNER_VALUE_EXCEPTIONS_SHA256 ENDORA_OWNER_VALUES="$VALS" node "$chk" "$@" >"$out" 2>&1); code=$?
  else
    (cd "$r" && env -u ENDORA_REVIEWED_BINARIES_SHA256 -u ENDORA_OWNER_VALUE_EXCEPTIONS_SHA256 ENDORA_OWNER_VALUES="$VALS" ENDORA_OWNER_VALUE_EXCEPTIONS_FILE="$ef" ${pin:+ENDORA_OWNER_VALUE_EXCEPTIONS_SHA256=$pin} node "$chk" "$@" >"$out" 2>&1); code=$?
  fi
  set -e
  unset PINVAL NOPIN
}
expect() {
  want=$1; label=$2; shift 2
  [ "$code" = "$want" ] || { bad "$label (exit $code, wanted $want)"; return; }
  for secret in "$@"; do
    if grep -qiF -- "$secret" "$out"; then bad "$label: output echoed a value or the URL"; return; fi
  done
  ok "$label"
}
kind() { grep -q -- "$1" "$out" && ok "$2" || bad "$2 (no $1 in report)"; }
# A repository whose two reviewed paths each carry the sanctioned URL (Dockerfile: twice, ADR: once).
base() {
  r=$(newrepo "$1")
  printf 'FROM scratch\nARG UPSTREAM=%s\nRUN git clone %s /src\n' "$U" "$U" > "$r/$DF"
  mkdir -p "$r/docs/adr"; printf '# A door\n\nIt builds from [upstream](%s).\n' "$U" > "$r/$ADR"
  commit "$r" one
}
GOOD() { exc "$1" "$(rootof "$2")" "$(line 2 "$DF" "$U" 2)" "$(line 2 "$ADR" "$U" 1)"; }

# --- positive ------------------------------------------------------------------------------------
base pos; E=$(nextexc); GOOD "$E" "$r"
run "$r" "<none>" --release;            expect 1 "control: without the file the same tree is a finding" "$V2" "$U"
run "$r" "$E" --release;                expect 0 "the exact URL at the two exact paths passes (release)" "$V2" "$U"
grep -q "reviewed value exceptions: 2 entries; excused 3 occurrence(s) in 2 object(s), 3 in 2 working-tree file(s)" "$out" && ok "the report says what was excused, by count" || bad "excused count not reported"
run "$r" "$E";                          expect 0 "the exact URL passes in default mode too" "$V2" "$U"
echo "second line" >> "$r/$ADR"; commit "$r" two
run "$r" "$E" --release;                expect 0 "history: every version carrying only the sanctioned URL passes" "$V2" "$U"
git -C "$r" mv "$ADR" docs/adr/9999-moved.md; commit "$r" moved
run "$r" "$E" --release;                expect 1 "a reviewed file moved to another path is a finding again" "$V2" "$U"

base twotok; printf 'ARG B=%s\n' "$U2" >> "$r/$DF"; commit "$r" more; E=$(nextexc)
exc "$E" "$(rootof "$r")" "$(line 2 "$DF" "$U" 2)" "$(line 2 "$DF" "$U2" 1)" "$(line 2 "$ADR" "$U" 1)"
run "$r" "$E" --release;                expect 0 "two reviewed tokens in one file, each with its own entry" "$V2" "$U" "$U2"
E=$(nextexc); exc "$E" "$(rootof "$r")" "$(line 2 "$DF" "$U" 2)" "$(line 2 "$ADR" "$U" 1)"
run "$r" "$E" --release;                expect 1 "a second, unreviewed URL in the same file is a finding" "$V2" "$U2"

# --- negative: what the exception must not excuse -----------------------------------------------
base bare; E=$(nextexc); GOOD "$E" "$r"; printf 'the word %s alone\n' "$V2" >> "$r/$DF"; commit "$r" bare
run "$r" "$E" --release;                expect 1 "a bare value beside the URL, in the same path, is a finding" "$V2" "$U"
base bare2; E=$(nextexc); GOOD "$E" "$r"; printf 'a note on %s here\n' "$V2" > "$r/NOTES.md"; commit "$r" note
run "$r" "$E" --release;                expect 1 "a bare value in another file is a finding" "$V2" "$U"
base embed; E=$(nextexc); GOOD "$E" "$r"; printf 'ARG X=%s\n' "https://example.invalid/$V2-other" >> "$r/$DF"; commit "$r" other
run "$r" "$E" --release;                expect 1 "the value inside a different URL in the same path is a finding" "$V2" "$U"

base elsewhere; E=$(nextexc); GOOD "$E" "$r"; printf 'ARG UPSTREAM=%s\n' "$U" > "$r/other.Dockerfile"; commit "$r" copy
run "$r" "$E" --release;                expect 1 "the same token in another path is a finding" "$V2" "$U"
base samebytes; E=$(nextexc); GOOD "$E" "$r"; cp "$r/$ADR" "$r/docs/copy.md"; commit "$r" copy
run "$r" "$E" --release;                expect 1 "identical bytes at a second path: the blob is not excused anywhere" "$V2" "$U"
base staged; E=$(nextexc); GOOD "$E" "$r"; cp "$r/$ADR" "$r/staged.md"; git -C "$r" add staged.md; rm "$r/staged.md"
run "$r" "$E" --release;                expect 1 "the same content staged at another path is a finding" "$V2" "$U"
base untracked; E=$(nextexc); GOOD "$E" "$r"; printf '%s\n' "$U" > "$r/new.txt"
run "$r" "$E" --release;                expect 1 "the token in an untracked file at another path is a finding" "$V2" "$U"
base dirty; E=$(nextexc); GOOD "$E" "$r"; printf 'and %s bare\n' "$V2" >> "$r/$DF"
run "$r" "$E" --release;                expect 1 "a working-tree edit adding a bare value at the reviewed path is a finding" "$V2" "$U"
base wtok; E=$(nextexc); GOOD "$E" "$r"; printf 'RUN echo\n' >> "$r/$DF"
run "$r" "$E" --release;                expect 0 "a working-tree edit that leaves only the sanctioned URL passes" "$V2" "$U"
base case_; E=$(nextexc); GOOD "$E" "$r"
blob=$(printf 'FROM scratch\nARG UPSTREAM=%s\n' "$U" | git -C "$r" hash-object -w --stdin)
git -C "$r" update-index --add --cacheinfo "100644,$blob,DOCKERFILE"
run "$r" "$E" --release;                expect 1 "a staged path differing only in case does not match" "$V2" "$U"

# near-miss URLs: each one replaces the reviewed text in the ADR and must stay a finding
nearmiss() { # LABEL TEXT
  base "nm$n"; E=$(nextexc); GOOD "$E" "$r"; printf '%s\n' "$2" > "$r/$ADR"; commit "$r" nm
  run "$r" "$E" --release; expect 1 "near-miss: $1" "$V2" "$U"
}
nearmiss "extra path after the .git" "See $U/extra now"
nearmiss "suffix glued on" "See ${U}x now"
nearmiss "prefix glued on" "See xhttps://example.invalid/$V2/upstream.git now"
nearmiss "dot suffix" "See $U.bak now"
nearmiss "other scheme" "See http://example.invalid/$V2/upstream.git now"
nearmiss "other host" "See https://example.invalid.test/$V2/upstream.git now"
nearmiss "upper case" "See $(printf '%s' "$U" | tr a-z A-Z) now"
nearmiss "query added" "See $U?ref=x now"
nearmiss "dash prefix" "See -$U now"
nearmiss "value only, no scheme" "See example.invalid/$V2/upstream.git now"
nearmiss "same length, one character changed" "See https://example.invalid/$V2/upstreaX.git now"
nearmiss "same length, other host" "See https://exampleXinvalid/$V2/upstream.git now"
nearmiss "token twice, once with a suffix" "See $U and ${U}z now"

base over; E=$(nextexc); GOOD "$E" "$r"; printf 'And again: %s\n' "$U" >> "$r/$ADR"; commit "$r" again
run "$r" "$E" --release;                expect 1 "one more occurrence than the reviewed count is a finding" "$V2" "$U"
base punct; printf '# A door\n\nIt builds from %s. Also %s, and (%s).\n' "$U" "$U" "$U" > "$r/$ADR"; git -C "$r" add -A; git -C "$r" commit -q --amend -m prose
E=$(nextexc); exc "$E" "$(rootof "$r")" "$(line 2 "$DF" "$U" 2)" "$(line 2 "$ADR" "$U" 3)"
run "$r" "$E" --release;                expect 0 "a URL followed by a full stop, a comma or a bracket is still the sanctioned token" "$V2" "$U"
base wlink; E=$(nextexc); GOOD "$E" "$r"; rm "$r/$ADR"; ln -s "$U" "$r/$ADR"
run "$r" "$E" --release;                expect 1 "a working-tree symlink at a reviewed path is a finding" "$V2" "$U"
base fewer; E=$(nextexc); exc "$E" "$(rootof "$r")" "$(line 2 "$DF" "$U" 5)" "$(line 2 "$ADR" "$U" 1)"
run "$r" "$E" --release;                expect 0 "fewer occurrences than the reviewed maximum is fine" "$V2" "$U"

# the token also holds a second value the entry does not name
T="https://example.invalid/$V1/$V2.git"
r=$(newrepo two); E=$(nextexc); printf 'ARG U=%s\n' "$T" > "$r/$DF"; mkdir -p "$r/docs/adr"; printf 'See [it](%s).\n' "$U" > "$r/$ADR"; commit "$r" one
run "$r" "$E" --release >/dev/null 2>&1 || true
exc "$E" "$(rootof "$r")" "$(line 1 "$DF" "$T" 1)" "$(line 2 "$DF" "$T" 1)" "$(line 2 "$ADR" "$U" 1)"
run "$r" "$E" --release;                expect 0 "control: naming both values for the token excuses it" "$V1" "$V2" "$T"
exc "$E" "$(rootof "$r")" "$(line 2 "$DF" "$T" 1)" "$(line 2 "$ADR" "$U" 1)"
run "$r" "$E" --release;                expect 1 "a token that also holds an unnamed value is a finding" "$V1" "$V2" "$T"
exc "$E" "$(rootof "$r")" "$(line 1 "$DF" "$T" 1)" "$(line 2 "$ADR" "$U" 1)"
run "$r" "$E" --release;                expect 1 "naming the wrong value for the token excuses nothing" "$V1" "$V2" "$T"

# --- metadata, names and binaries are never excused ----------------------------------------------
base msg; E=$(nextexc); GOOD "$E" "$r"; commit "$r" "see $U for the source"
run "$r" "$E" --release;                expect 1 "the URL in a commit message is a finding" "$V2" "$U"; kind "commit-metadata" "  ...as commit metadata"
base author; E=$(nextexc); GOOD "$E" "$r"; git -C "$r" -c user.name="$V2" commit -q --allow-empty -m x
run "$r" "$E" --release;                expect 1 "the value in an author name is a finding" "$V2" "$U"
base tag; E=$(nextexc); GOOD "$E" "$r"; git -C "$r" tag -a v1 -m "from $U"
run "$r" "$E" --release;                expect 1 "the URL in an annotated tag is a finding" "$V2" "$U"; kind "tag-metadata" "  ...as tag metadata"
base branch; E=$(nextexc); GOOD "$E" "$r"; git -C "$r" branch "b-$V2"
run "$r" "$E" --release;                expect 1 "the value in a branch name is a finding" "$V2" "$U"; kind "ref-name" "  ...as a ref name"
base fname; E=$(nextexc); GOOD "$E" "$r"; printf 'x\n' > "$r/$V2.txt"; commit "$r" name
run "$r" "$E" --release;                expect 1 "the value in a file name is a finding" "$V2" "$U"; kind "file-name" "  ...as a file name"
base dname; E=$(nextexc); GOOD "$E" "$r"; mkdir "$r/d-$V2"; printf 'x\n' > "$r/d-$V2/f.txt"; commit "$r" dir
run "$r" "$E" --release;                expect 1 "the value in a directory name is a finding" "$V2" "$U"
base binary; E=$(nextexc); GOOD "$E" "$r"; printf '\000\001%s\000' "$U" > "$r/$DF"; commit "$r" bin
run "$r" "$E";                          expect 1 "the URL inside a binary at a reviewed path is a finding (default mode: no binary gate to hide behind)" "$V2" "$U"
base badutf; E=$(nextexc); GOOD "$E" "$r"; printf 'FROM x\nARG U=%s\n\377\n' "$U" > "$r/$DF"; commit "$r" latin
run "$r" "$E" --release;                expect 1 "a reviewed path that is not valid UTF-8 is a finding" "$V2" "$U"
base utf16; E=$(nextexc); GOOD "$E" "$r"; printf '%s' "$U" | iconv -f UTF-8 -t UTF-16LE > "$r/$DF"; commit "$r" u16
run "$r" "$E" --release;                expect 1 "the URL encoded as UTF-16 at a reviewed path is a finding" "$V2" "$U"
base link; E=$(nextexc); GOOD "$E" "$r"; rm "$r/$ADR"; ln -s "$U" "$r/$ADR"; commit "$r" link
run "$r" "$E" --release;                expect 1 "a symlink at a reviewed path (target text) is a finding" "$V2" "$U"

# --- forged repository, ref or path ---------------------------------------------------------------
base forged; E=$(nextexc); exc "$E" "$(printf 'forged\n' | git hash-object --stdin)" "$(line 2 "$DF" "$U" 2)" "$(line 2 "$ADR" "$U" 1)"
run "$r" "$E" --release;                expect 2 "a manifest bound to another repository root fails closed" "$V2" "$U"
base foreignref; E=$(nextexc); GOOD "$E" "$r"
git -C "$r" checkout -q --orphan other; git -C "$r" rm -q -rf . ; echo hi > "$r/x.txt"; commit "$r" foreign; git -C "$r" checkout -q main
run "$r" "$E" --release;                expect 2 "a ref carrying a foreign root fails closed (exceptions never reach unrelated history)" "$V2" "$U"
base ontip; E=$(nextexc); GOOD "$E" "$r"; echo more >> "$r/$ADR"; commit "$r" next
run "$r" "$E" --release;                expect 0 "a descendant of the bound root is fine" "$V2" "$U"

badpath() { # LABEL PATH
  base "bp$n"; E=$(nextexc); exc "$E" "$(rootof "$r")" "$(line 2 "$2" "$U" 1)"
  run "$r" "$E" --release; expect 2 "forged path: $1" "$V2" "$U"
}
badpath "dot-slash prefix" "./Dockerfile"
badpath "parent segment" "docs/../Dockerfile"
badpath "absolute" "/Dockerfile"
badpath "doubled slash" "docs//adr/9999-a-door.md"
badpath "trailing slash (a directory)" "docs/"
badpath "wildcard" "docs/*"
badpath "glob class" "docs/adr/[0-9]*.md"
badpath "question mark" "Dockerfil?"
badpath "empty segment at the end" "docs/adr/"
badpath "dot segment" "docs/./adr/x.md"
badpath "a space" "Docker file"
badpath "a backslash" 'docs\adr'
badpath "the whole tree" "."

# --- malformed or unknown input fails closed ------------------------------------------------------
base mal; rt=$(rootof "$r"); okl=$(line 2 "$DF" "$U" 2)
try() { # LABEL FILE-CONTENT-WRITER...  (a function name that writes $E)
  E=$(nextexc); "$2"; run "$r" "$E" --release; expect 2 "malformed: $1" "$V2" "$U"
}
m_none() { : > "$E"; }
m_noroot() { printf '%s\n' "$okl" > "$E"; }
m_tworoot() { printf 'root\t%s\nroot\t%s\n%s\n' "$rt" "$rt" "$okl" > "$E"; }
m_badroot() { printf 'root\tnothex\n%s\n' "$okl" > "$E"; }
m_shortroot() { printf 'root\t%s\n%s\n' "$(printf %s "$rt" | cut -c1-39)" "$okl" > "$E"; }
m_unknown() { printf 'root\t%s\nallow\tall\n%s\n' "$rt" "$okl" > "$E"; }
m_extra() { printf 'root\t%s\n%s\textra\n' "$rt" "$okl" > "$E"; }
m_fewer() { printf 'root\t%s\n2\t%s\t%s\t%s\n' "$rt" "$DF" "$(sha "$U")" "${#U}" > "$E"; }
m_pos0() { printf 'root\t%s\n0\t%s\t%s\t%s\t1\tr\n' "$rt" "$DF" "$(sha "$U")" "${#U}" > "$E"; }
m_pos3() { printf 'root\t%s\n3\t%s\t%s\t%s\t1\tr\n' "$rt" "$DF" "$(sha "$U")" "${#U}" > "$E"; }
m_posneg() { printf 'root\t%s\n-1\t%s\t%s\t%s\t1\tr\n' "$rt" "$DF" "$(sha "$U")" "${#U}" > "$E"; }
m_posalpha() { printf 'root\t%s\n%s\t%s\t%s\t%s\t1\tr\n' "$rt" "x" "$DF" "$(sha "$U")" "${#U}" > "$E"; }
m_digest() { printf 'root\t%s\n2\t%s\tabc\t%s\t1\tr\n' "$rt" "$DF" "${#U}" > "$E"; }
m_digestup() { printf 'root\t%s\n2\t%s\t%s\t%s\t1\tr\n' "$rt" "$DF" "$(sha "$U" | tr a-f A-F)" "${#U}" > "$E"; }
m_len0() { printf 'root\t%s\n2\t%s\t%s\t0\t1\tr\n' "$rt" "$DF" "$(sha "$U")" > "$E"; }
m_lenbig() { printf 'root\t%s\n2\t%s\t%s\t99999\t1\tr\n' "$rt" "$DF" "$(sha "$U")" > "$E"; }
m_cnt0() { printf 'root\t%s\n2\t%s\t%s\t%s\t0\tr\n' "$rt" "$DF" "$(sha "$U")" "${#U}" > "$E"; }
m_cntbig() { printf 'root\t%s\n2\t%s\t%s\t%s\t1000\tr\n' "$rt" "$DF" "$(sha "$U")" "${#U}" > "$E"; }
m_cntany() { printf 'root\t%s\n2\t%s\t%s\t%s\t*\tr\n' "$rt" "$DF" "$(sha "$U")" "${#U}" > "$E"; }
m_id() { printf 'root\t%s\n2\t%s\t%s\t%s\t1\tbad id!\n' "$rt" "$DF" "$(sha "$U")" "${#U}" > "$E"; }
m_dup() { printf 'root\t%s\n%s\n%s\n' "$rt" "$okl" "$okl" > "$E"; }
m_crlf() { printf 'root\t%s\r\n%s\r\n' "$rt" "$okl" > "$E"; }
m_spaces() { printf 'root %s\n%s\n' "$rt" "$okl" > "$E"; }
m_nul() { printf 'root\t%s\n%s\n\000\n' "$rt" "$okl" > "$E"; }
m_big() { { printf 'root\t%s\n' "$rt"; i=0; while [ $i -lt 4000 ]; do printf '# padding padding padding padding\n'; i=$((i + 1)); done; } > "$E"; }
try "an empty file" m_none
try "no root line" m_noroot
try "two root lines" m_tworoot
try "a root that is not hex" m_badroot
try "a root of 39 digits" m_shortroot
try "an unknown directive" m_unknown
try "an extra field" m_extra
try "a missing field" m_fewer
try "position 0" m_pos0
try "a position beyond the values given" m_pos3
try "a negative position" m_posneg
try "a non-numeric position" m_posalpha
try "a short digest" m_digest
try "an upper-case digest" m_digestup
try "a zero token length" m_len0
try "an oversized token length" m_lenbig
try "a zero count" m_cnt0
try "an oversized count" m_cntbig
try "a wildcard count" m_cntany
try "a review id with a space" m_id
try "the same entry twice" m_dup
try "CRLF line ends" m_crlf
try "spaces where tabs belong" m_spaces
try "a NUL byte" m_nul
try "an oversized file" m_big

E=$(nextexc); GOOD "$E" "$r"
set +e; (cd "$r" && ENDORA_OWNER_VALUES="$VALS" ENDORA_OWNER_VALUE_EXCEPTIONS_FILE="" node "$chk" --release >"$tmp/o.empty" 2>&1); code=$?; set -e; out="$tmp/o.empty"
expect 2 "an empty file-name variable fails closed" "$V2" "$U"
cp "$E" "$r/exc.rel"
set +e; (cd "$r" && ENDORA_OWNER_VALUES="$VALS" ENDORA_OWNER_VALUE_EXCEPTIONS_FILE="exc.rel" node "$chk" --release >"$tmp/o.rel" 2>&1); code=$?; set -e; out="$tmp/o.rel"
expect 2 "a relative file name fails closed" "$V2" "$U"
run "$r" "$tmp/does-not-exist" --release; expect 2 "a missing file fails closed" "$V2" "$U"
run "$r" "$tmp" --release;                expect 2 "a directory instead of a file fails closed" "$V2" "$U"
ln -s "$E" "$tmp/exc.link"
run "$r" "$tmp/exc.link" --release;       expect 2 "a symlinked file fails closed" "$V2" "$U"
chmod 000 "$E"; run "$r" "$E" --release;  expect 2 "an unreadable file fails closed" "$V2" "$U"; chmod 644 "$E"

# no values given: nothing to except, and release still refuses
set +e; (cd "$r" && env -u ENDORA_OWNER_VALUES ENDORA_OWNER_VALUE_EXCEPTIONS_FILE="$E" node "$chk" --release >"$tmp/o.nov" 2>&1); code=$?; set -e; out="$tmp/o.nov"
expect 2 "release with no values still refuses, file or not" "$V2" "$U"

# --- a stale entry, and the same file does not weaken the other checks ---------------------------
# --- the out-of-band pin of the exception file: release fails closed without the owner's digest ----
base pin; E=$(nextexc); GOOD "$E" "$r"
run "$r" "$E" --release;                expect 0 "pin: the owner's digest of the exact file passes (release)" "$V2" "$U"
run "$r" "$E";                          expect 0 "pin: the same digest passes in default mode" "$V2" "$U"
NOPIN=1; run "$r" "$E" --release;       expect 2 "pin: release without a digest fails closed" "$V2" "$U"
NOPIN=1; run "$r" "$E";                 expect 0 "pin: default mode without a digest is a local check, as before" "$V2" "$U"
PINVAL=$(sha "not the file"); run "$r" "$E" --release; expect 2 "pin: a digest of other bytes fails closed (release)" "$V2" "$U"
PINVAL=$(sha "not the file"); run "$r" "$E"; expect 2 "pin: a wrong digest fails closed in default mode too" "$V2" "$U"
PINVAL=""; run "$r" "$E" --release;     expect 2 "pin: an empty digest is no digest (release)" "$V2" "$U"
pin_up=$(shasum -a 256 "$E" | cut -d' ' -f1 | tr a-f A-F); PINVAL=$pin_up; run "$r" "$E" --release; expect 2 "pin: the right digest in upper case fails closed" "$V2" "$U"
E2=$(nextexc); GOOD "$E2" "$r"; pin_now=$(shasum -a 256 "$E2" | cut -d' ' -f1); printf '\n' >> "$E2"
PINVAL=$pin_now; run "$r" "$E2" --release; expect 2 "pin: one byte added after the digest was taken fails closed" "$V2" "$U"
E3=$(nextexc); exc "$E3" "$(rootof "$r")" "$(line 2 "$ADR" "$U" 1)"
PINVAL=$(shasum -a 256 "$E" | cut -d' ' -f1); run "$r" "$E3" --release; expect 2 "pin: the digest of another exception file fails closed" "$V2" "$U"

# --- the exception test is required by name in the gate ------------------------------------------
grep -q -- '--require owner-value-exceptions' "$root/scripts/gate.sh" && ok "gate: names owner-value-exceptions as required" || bad "gate: does not require owner-value-exceptions by name"
grep -q '^owner-value-exceptions|owner-value exceptions tests passed|sh scripts/test-owner-value-exceptions.sh$' "$root/scripts/required-checks.txt" && ok "gate: the manifest line carries its marker" || bad "gate: manifest line missing or changed"
grep -v '^owner-value-exceptions|' "$root/scripts/required-checks.txt" > "$tmp/m-without.txt"
set +e; sh "$root/scripts/check-required.sh" --require owner-value-exceptions "$tmp/m-without.txt" > "$tmp/o.req" 2>&1; rc=$?; set -e
[ "$rc" != 0 ] && ok "gate: a manifest without the exception test stops the required run" || bad "gate: a missing required name did not stop the run"

# --- URL boundaries and the bound on the token walk -----------------------------------------------
H="http://example.invalid/$V2/plain.git"
base hp; printf 'ARG H=%s\n' "$H" >> "$r/$DF"; commit "$r" http; E=$(nextexc)
exc "$E" "$(rootof "$r")" "$(line 2 "$DF" "$U" 2)" "$(line 2 "$ADR" "$U" 1)" "$(line 2 "$DF" "$H" 1)"
run "$r" "$E" --release;                expect 0 "boundary: an exact http:// token passes" "$V2" "$U" "$H"
base brk; E=$(nextexc); exc "$E" "$(rootof "$r")" "$(line 2 "$DF" "$U" 2)" "$(line 2 "$ADR" "$U" 2)"
printf 'See <%s> and "%s" here\n' "$U" "$U" > "$r/$ADR"; commit "$r" brackets
run "$r" "$E" --release;                expect 0 "boundary: the token inside angle brackets and quotes passes" "$V2" "$U"
nearmiss2() { # LABEL TEXT
  base "nb$n"; E=$(nextexc); GOOD "$E" "$r"; printf '%s\n' "$2" > "$r/$ADR"; commit "$r" nb
  run "$r" "$E" --release; expect 1 "near-miss: $1" "$V2" "$U"
}
nearmiss2 "a digit glued before the scheme" "See 1$U now"
nearmiss2 "an underscore glued before the scheme" "See _$U now"
eq=$(head -c 3000 /dev/zero | tr '\0' '=')
nearmiss2 "scheme further back than the token bound" "See $eq$U now"
base walkok; E=$(nextexc); GOOD "$E" "$r"; eq2=$(head -c 100 /dev/zero | tr '\0' '='); printf 'See %s%s now\n' "$eq2" "$U" > "$r/$ADR"; commit "$r" walk
run "$r" "$E" --release;                expect 0 "boundary: a run of URL characters within the bound still passes" "$V2" "$U"

# --- nested sanctioned URLs: two whole tokens that end at the same byte --------------------------
# The outer token holds the value and so does the inner one; both end at the same byte. The masking
# cut them one after the other by original offsets, so the bytes after the shared end were dropped
# and a bare value there was excused. Overlapping spans must fail closed, with or without the value.
OUT="https://example.invalid/$V2/x=https://example.invalid/$V2/upstream.git"
# A repository whose only history is the reviewed ADR (no other version of the Dockerfile carries the URL).
fresh() { r=$(newrepo "$1"); mkdir -p "$r/docs/adr"; printf '# A door\n\nIt builds from [upstream](%s).\n' "$U" > "$r/$ADR"; commit "$r" one; }
fresh nest; E=$(nextexc); exc "$E" "$(rootof "$r")" "$(line 2 "$DF" "$OUT" 1)" "$(line 2 "$DF" "$U" 1)" "$(line 2 "$ADR" "$U" 1)"
printf 'ARG UPSTREAM=%s and a bare %s here\n' "$OUT" "$V2" > "$r/$DF"; commit "$r" nest
run "$r" "$E" --release;                expect 1 "nested sanctioned URLs: a bare value after their shared end is a finding" "$V2" "$U" "$OUT"
fresh nest2; E=$(nextexc); exc "$E" "$(rootof "$r")" "$(line 2 "$DF" "$OUT" 1)" "$(line 2 "$DF" "$U" 1)" "$(line 2 "$ADR" "$U" 1)"
printf 'ARG UPSTREAM=%s\n' "$OUT" > "$r/$DF"; commit "$r" nest2
run "$r" "$E" --release;                expect 1 "nested sanctioned URLs with no bare value fail closed (overlapping spans are not excused)" "$V2" "$U" "$OUT"

# --- the 2048 bound on a whole token, and the right-side walk bound --------------------------------
PRE="https://example.invalid/$V2/"
pad() { head -c "$1" /dev/zero | tr '\0' 'a'; }
T2048="$PRE$(pad $((2048 - ${#PRE})))"; T2049="$PRE$(pad $((2049 - ${#PRE})))"
fresh b2048; E=$(nextexc); exc "$E" "$(rootof "$r")" "$(line 2 "$DF" "$T2048" 1)" "$(line 2 "$ADR" "$U" 1)"
printf 'ARG B=%s\n' "$T2048" > "$r/$DF"; commit "$r" b2048
run "$r" "$E" --release;                expect 0 "bound: a whole token of exactly 2048 characters passes" "$V2" "$T2048"
fresh b2049; E=$(nextexc); exc "$E" "$(rootof "$r")" "$(line 2 "$DF" "$T2048" 1)" "$(line 2 "$ADR" "$U" 1)"
printf 'ARG B=%s\n' "$T2049" > "$r/$DF"; commit "$r" b2049
run "$r" "$E" --release;                expect 1 "bound: a 2049-character token is not the pinned 2048-character token" "$V2" "$T2049"
fresh b2049e; E=$(nextexc); exc "$E" "$(rootof "$r")" "$(line 2 "$DF" "$T2049" 1)" "$(line 2 "$ADR" "$U" 1)"
run "$r" "$E" --release;                expect 2 "bound: an entry naming a 2049-character token is malformed (fails closed)" "$V2" "$T2049"
fresh wide; E=$(nextexc); exc "$E" "$(rootof "$r")" "$(line 2 "$DF" "$T2048" 1)" "$(line 2 "$ADR" "$U" 1)"
printf 'ARG B=%s\n' "$PRE$(pad 2100)" > "$r/$DF"; commit "$r" wide
run "$r" "$E" --release;                expect 1 "right-side bound: a run of URL characters 2100 long after the value is not excused" "$V2" "$T2048"

# --- the occurrence limit is per file version: one version over it is a finding ---------------------
base hist; E=$(nextexc); exc "$E" "$(rootof "$r")" "$(line 2 "$DF" "$U" 2)" "$(line 2 "$ADR" "$U" 1)"
printf 'It builds from %s and again %s.\n' "$U" "$U" > "$r/$ADR"; commit "$r" over-in-history
printf 'It builds from [upstream](%s).\n' "$U" > "$r/$ADR"; commit "$r" within-now
run "$r" "$E" --release;                expect 1 "history: one version over the occurrence limit is a finding though another version is within it" "$V2" "$U"
base hist2; E=$(nextexc); exc "$E" "$(rootof "$r")" "$(line 2 "$DF" "$U" 2)" "$(line 2 "$ADR" "$U" 1)"
printf 'It builds from [upstream](%s) again.\n' "$U" > "$r/$ADR"; commit "$r" within-one
printf 'It builds from [upstream](%s).\n' "$U" > "$r/$ADR"; commit "$r" within-two
run "$r" "$E" --release;                expect 0 "history: two versions each within the occurrence limit pass" "$V2" "$U"

# --- a historical symlink is never excused, even when the file is clean at HEAD -------------------
hs=$(newrepo hsym); ln -s "$U" "$hs/$DF"; mkdir -p "$hs/docs/adr"; printf '# A door\n\nIt builds from [upstream](%s).\n' "$U" > "$hs/$ADR"; commit "$hs" one
rm "$hs/$DF"; printf 'FROM scratch\nARG UPSTREAM=%s\nRUN git clone %s /src\n' "$U" "$U" > "$hs/$DF"; commit "$hs" two; E=$(nextexc); GOOD "$E" "$hs"
r=$hs; run "$r" "$E" --release;         expect 1 "history: a symlink at a reviewed path, gone at HEAD, is still a finding" "$V2" "$U"; kind "file-content" "  ...as file content of the historical symlink's blob"
hc=$(newrepo hsymc); mkdir -p "$hc/docs/adr"; printf 'FROM scratch\nARG UPSTREAM=%s\nRUN git clone %s /src\n' "$U" "$U" > "$hc/$DF"; printf '# A door\n\nIt builds from [upstream](%s).\n' "$U" > "$hc/$ADR"; commit "$hc" one
E=$(nextexc); GOOD "$E" "$hc"; r=$hc; run "$r" "$E" --release; expect 0 "control: the same tree without the historical symlink passes" "$V2" "$U"

base stale; E=$(nextexc); exc "$E" "$(rootof "$r")" "$(line 2 "$DF" "$U" 2)" "$(line 2 "$ADR" "$U" 1)" "$(line 2 "other.md" "$U" 1)"
run "$r" "$E" --release;                expect 1 "release: an entry matching nothing is reported stale, with no token hash" "$V2" "$U" "$(sha "$U" | cut -c1-12)" "$(sha "$U")"; kind "stale-exception" "  ...named as a stale exception"
run "$r" "$E";                          expect 0 "default mode: a stale entry is not a failure" "$V2" "$U"
# a PNG text chunk is metadata of a binary: the pinned URL inside one is still found, at the reviewed path too
gen="$tmp/gen.js"
cat > "$gen" <<'JS'
const z=require("zlib");
const crc=(b)=>{let c,t=[];for(let n=0;n<256;n++){c=n;for(let k=0;k<8;k++)c=c&1?0xedb88320^(c>>>1):c>>>1;t[n]=c>>>0}let r=0xffffffff;for(const x of b)r=t[(r^x)&255]^(r>>>8);return(r^0xffffffff)>>>0};
const ch=(ty,d)=>{const l=Buffer.alloc(4);l.writeUInt32BE(d.length);const td=Buffer.concat([Buffer.from(ty),d]);const c=Buffer.alloc(4);c.writeUInt32BE(crc(td));return Buffer.concat([l,td,c])};
const ihdr=Buffer.alloc(13);ihdr.writeUInt32BE(1,0);ihdr.writeUInt32BE(1,4);ihdr[8]=8;ihdr[9]=2;
process.stdout.write(Buffer.concat([Buffer.from([137,80,78,71,13,10,26,10]),ch("IHDR",ihdr),ch("zTXt",Buffer.concat([Buffer.from("Comment\0\0"),z.deflateSync(Buffer.from(process.argv[2]))])),ch("IDAT",z.deflateSync(Buffer.from([0,255,0,0]))),ch("IEND",Buffer.alloc(0))]));
JS
base pngtext; E=$(nextexc); GOOD "$E" "$r"; node "$gen" "source $U" > "$r/$DF"; commit "$r" png
run "$r" "$E";                          expect 1 "the pinned URL inside a PNG text chunk at a reviewed path is a finding" "$V2" "$U"; kind "png-text-value" "  ...as a PNG text value"
base nopngs; E=$(nextexc); GOOD "$E" "$r"; printf '\211PNG\r\n\032\n\000' > "$r/pic.png"; commit "$r" png
run "$r" "$E" --release;                expect 1 "an unreviewed binary is still a finding with the file set" "$V2" "$U"; kind "unreviewed-binary" "  ...as an unreviewed binary"

# --- punctuation after a reviewed URL: the walk is bounded, and a short run is trimmed back ---------
# Dots are URL characters, so a run of them after the sanctioned URL is walked. Past URL_WALK_MAX the
# token is not excused (the right-walk guard), and a token longer than that bound is not excused either
# (the end-limit guard). Shorter runs are trimmed back to the URL by the trailing-punctuation rule.
dots() { head -c "$1" /dev/zero | tr '\0' '.'; }
base dotsbig; E=$(nextexc); GOOD "$E" "$r"; printf 'It builds from %s%s\n' "$U" "$(dots 2100)" > "$r/$ADR"; commit "$r" dotsbig
run "$r" "$E" --release;                expect 1 "punctuation: a reviewed URL followed by 2100 dots is a finding (walk bounded)" "$V2" "$U"
base dots100; E=$(nextexc); GOOD "$E" "$r"; printf 'It builds from %s%s\n' "$U" "$(dots 100)" > "$r/$ADR"; commit "$r" dots100
run "$r" "$E" --release;                expect 0 "punctuation: a reviewed URL followed by 100 dots still passes (as designed)" "$V2" "$U"

# --- a pinned row whose token length disagrees with the token excuses nothing ---------------------
# The row's digest is the real token's, so only the length comparison separates it from a match.
base lenlong; E=$(nextexc); exc "$E" "$(rootof "$r")" "$(line 2 "$DF" "$U" 2)" "$(printf '2\t%s\t%s\t%s\t1\tr\n' "$ADR" "$(sha "$U")" "$((${#U} + 1))")"
run "$r" "$E" --release;                expect 1 "length: a pinned row one character longer than its token excuses nothing" "$V2" "$U"
base lenshort; E=$(nextexc); exc "$E" "$(rootof "$r")" "$(line 2 "$DF" "$U" 2)" "$(printf '2\t%s\t%s\t%s\t1\tr\n' "$ADR" "$(sha "$U")" "$((${#U} - 1))")"
run "$r" "$E" --release;                expect 1 "length: a pinned row one character shorter than its token excuses nothing" "$V2" "$U"
base lenok; E=$(nextexc); GOOD "$E" "$r"
run "$r" "$E" --release;                expect 0 "length: the same row with the token's own length passes (control)" "$V2" "$U"

echo "$n checks, $fail failed"
[ "$fail" = 0 ] && echo "owner-value exceptions tests passed"
[ "$fail" = 0 ]
