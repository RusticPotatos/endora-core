#!/bin/sh
# Tests for scripts/check-owner-values.mjs: the check that none of the owner's own values is in
# a repository's tree, its whole history, its metadata, or an unreviewed binary.
# Fictional values only. Every case that finds something also proves the value is never echoed.
#
# Exit codes of the checker: 0 clean, 1 found something, 2 could not verify (fails closed).
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
chk="$root/scripts/check-owner-values.mjs"
tmp=$(mktemp -d); trap 'chmod -R u+rwx "$tmp" 2>/dev/null; rm -rf "$tmp"' EXIT
V="zq-fictional-ovalue"
fail=0; n=0
ok() { n=$((n + 1)); echo "  ok    $1"; }
bad() { n=$((n + 1)); fail=$((fail + 1)); echo "FAIL  $1"; }

# newrepo NAME: an empty repository with a fictional identity.
newrepo() { d="$tmp/$1"; mkdir "$d"; git -C "$d" init -q -b main; git -C "$d" config user.name tester; git -C "$d" config user.email t@example.com; git -C "$d" config commit.gpgsign false; echo "$d"; }
commit() { git -C "$1" add -A >/dev/null; git -C "$1" commit -q --allow-empty -m "${2:-c}"; }
# run REPO VALUES [args]: sets $code and $out (stdout+stderr in one file). Never echoes anything.
run() {
  r=$1; vals=$2; shift 2
  out="$tmp/out.$n"
  set +e
  if [ "$vals" = "<unset>" ]; then
    (cd "$r" && env -u ENDORA_OWNER_VALUES -u ENDORA_REVIEWED_BINARIES_SHA256 node "$chk" "$@" >"$out" 2>&1); code=$?
  else
    (cd "$r" && env -u ENDORA_REVIEWED_BINARIES_SHA256 ENDORA_OWNER_VALUES="$vals" node "$chk" "$@" >"$out" 2>&1); code=$?
  fi
  set -e
}
# expect CODE LABEL VALUE-THAT-MUST-NOT-APPEAR...
expect() {
  want=$1; label=$2; shift 2
  [ "$code" = "$want" ] || { bad "$label (exit $code, wanted $want)"; return; }
  for secret in "$@"; do
    if grep -qiF -- "$secret" "$out"; then bad "$label: output echoed a value"; return; fi
  done
  ok "$label"
}

# kind WORD LABEL: the report must name this finding kind (so a pass by another route is not mistaken for this one).
kind() { grep -q -- "$1" "$out" && ok "$2" || bad "$2 (no $1 in report)"; }

# --- configuration fails closed in release mode --------------------------------------------------
r=$(newrepo clean); echo "a harmless line" > "$r/a.txt"; commit "$r" one; echo more >> "$r/a.txt"; commit "$r" two
run "$r" "<unset>" --release;        expect 2 "release: unset variable is refused"
run "$r" "" --release;               expect 2 "release: empty variable is refused"
run "$r" " , ,, " --release;         expect 2 "release: only separators is refused"
run "$r" "q9" --release;             expect 2 "release: a too-short value is refused, not skipped" q9
run "$r" "$V,q9" --release;          expect 2 "release: one short value among good ones is refused" q9
run "$r" "<unset>";                  expect 0 "default mode: unset passes but says it did not check"
grep -q "NOT CHECKED" "$out" && ok "default mode: unset is loud about not checking" || bad "default mode: unset is silent"
run "$r" "q9";                       expect 0 "default mode: a short value is skipped with a count" q9
grep -q "1 skipped" "$out" && ok "default mode: skipped count reported" || bad "default mode: skipped count missing"
run "$r" "$V" --release;             expect 0 "clean multi-commit history passes" "$V"
grep -q "2 commits" "$out" && ok "report states commits scanned" || bad "report lacks commit count"
old=ENDORA_PERSONAL; old="${old}_VALUES"
set +e; (cd "$r" && env "$old=$V" node "$chk" --release >"$tmp/o.old" 2>&1); code=$?; set -e; out="$tmp/o.old"
expect 2 "the previous variable name alone is refused" "$V"
run "$r" "$V" --release --nonsense;  expect 2 "unknown option is refused"
run "$tmp" "$V" --release;           expect 2 "not a git repository is refused"

# --- found where the old checker never looked ----------------------------------------------------
r=$(newrepo head); echo "x $V y" > "$r/a.txt"; commit "$r"
run "$r" "$V" --release; expect 1 "value in a tracked file" "$V"
r=$(newrepo gone); echo "x $V y" > "$r/a.txt"; commit "$r" add; echo "clean" > "$r/a.txt"; commit "$r" scrub
run "$r" "$V" --release; expect 1 "value only in an older commit's blob" "$V"
r=$(newrepo branch); echo base > "$r/a.txt"; commit "$r"; git -C "$r" checkout -q -b side; echo "$V" > "$r/b.txt"; commit "$r" side; git -C "$r" checkout -q main
run "$r" "$V" --release; expect 1 "value only on a non-HEAD branch" "$V"
r=$(newrepo msg); echo base > "$r/a.txt"; commit "$r" "note about $V here"
run "$r" "$V" --release; expect 1 "value in a commit message" "$V"
r=$(newrepo author); echo base > "$r/a.txt"; git -C "$r" add -A; git -C "$r" -c user.name="$V" -c user.email=ok@example.com commit -q -m c
run "$r" "$V" --release; expect 1 "value in an author name" "$V"
r=$(newrepo email); echo base > "$r/a.txt"; git -C "$r" add -A; git -C "$r" -c user.name=ok -c user.email="$V@example.com" commit -q -m c
run "$r" "$V" --release; expect 1 "value in a committer email" "$V"
r=$(newrepo tag); echo base > "$r/a.txt"; commit "$r"; git -C "$r" tag -a v1 -m "release for $V"
run "$r" "$V" --release; expect 1 "value in an annotated tag message" "$V"
r=$(newrepo refname); echo base > "$r/a.txt"; commit "$r"; git -C "$r" branch "feature-$V"
run "$r" "$V" --release; expect 1 "value in a branch name" "$V"
r=$(newrepo path); echo base > "$r/$V.txt"; commit "$r"
run "$r" "$V" --release; expect 1 "value in a file name (path is redacted in the report)" "$V"
r=$(newrepo subdir); mkdir -p "$r/d-$V"; echo base > "$r/d-$V/f.txt"; commit "$r"; git -C "$r" rm -q -r "d-$V"; commit "$r" gone
run "$r" "$V" --release; expect 1 "value in a directory name removed in a later commit" "$V"; kind "file-name" "  ...as a file name"
r=$(newrepo replace); echo "$V" > "$r/a.txt"; commit "$r" leak; git -C "$r" rm -q a.txt; commit "$r" clean
leak=$(git -C "$r" rev-parse HEAD~1); git -C "$r" replace "$leak" HEAD
run "$r" "$V" --release; expect 1 "value hidden behind a git replace ref is still found" "$V"; kind "file-content" "  ...found as file content, not by another route"
r=$(newrepo stash); echo base > "$r/a.txt"; commit "$r"; echo "$V" > "$r/a.txt"; git -C "$r" stash -q
run "$r" "$V" --release; expect 1 "value only in a stash is found" "$V"
r=$(newrepo staged); echo base > "$r/a.txt"; commit "$r"; echo "$V" > "$r/s.txt"; git -C "$r" add s.txt; echo clean > "$r/s.txt"; rm -f "$r/s.txt"
run "$r" "$V" --release; expect 1 "value only in the index (staged, then gone from the working tree)" "$V"; kind "file-content" "  ...as file content"
r=$(newrepo untracked); echo base > "$r/a.txt"; commit "$r"; echo "$V" > "$r/new.txt"
run "$r" "$V" --release; expect 1 "value in an untracked, not-yet-added file" "$V"
r=$(newrepo ignored); echo base > "$r/a.txt"; echo "ign.txt" > "$r/.gitignore"; commit "$r"; echo "$V" > "$r/ign.txt"
run "$r" "$V" --release; expect 0 "a git-ignored file is out of scope, as git itself carries it nowhere" "$V"
r=$(newrepo upper); echo "UPPER $(echo "$V" | tr a-z A-Z) UPPER" > "$r/a.txt"; commit "$r"
run "$r" "$V" --release; expect 1 "case differences do not hide a value" "$V"
r=$(newrepo wrapped); printf 'first\n   second\n' > "$r/a.txt"; commit "$r"
run "$r" "first second" --release; expect 1 "a multi-word value split across a line wrap" "first second"
r=$(newrepo meta); echo "axb and a.b" > "$r/a.txt"; commit "$r"; echo "axb only" > "$r/a.txt"; commit "$r" two
run "$r" "a.b.c" --release; expect 0 "regex characters in a value are literal" "a.b.c"
r=$(newrepo literal); echo "axb only" > "$r/a.txt"; commit "$r"
run "$r" "a.bz" --release; expect 0 "a dot does not match any character" "a.bz"
r=$(newrepo list); echo "keep $V keep" > "$r/a.txt"; commit "$r"
run "$r" "other-fictional, $V ,third-fictional" --release; expect 1 "values are trimmed and separated by commas" "$V" other-fictional third-fictional

# --- binaries, encodings and unreadable input ----------------------------------------------------
r=$(newrepo bin); printf '\000\001%s\000\002' "$V" > "$r/b.dat"; commit "$r"
run "$r" "$V" --release; expect 1 "value inside a binary file" "$V"; kind "file-content" "  ...as file content"
r=$(newrepo utf16); printf '%s' "$V" | iconv -f UTF-8 -t UTF-16LE > "$r/u.dat"; commit "$r"
run "$r" "$V" --release; expect 1 "value encoded as UTF-16 is found" "$V"; kind "file-content" "  ...as file content (not merely as an unreviewed binary)"
r=$(newrepo unread); echo base > "$r/a.txt"; echo secret > "$r/b.txt"; commit "$r"; chmod 000 "$r/b.txt"
run "$r" "$V" --release; expect 2 "release: an unreadable working-tree file is a failure to verify" "$V"
chmod 644 "$r/b.txt"
r=$(newrepo dangling); echo base > "$r/a.txt"; ln -s /nonexistent-target "$r/link"; commit "$r"
run "$r" "$V" --release; expect 0 "a dangling symlink is checked by its target text, not skipped or crashed on" "$V"
r=$(newrepo linkval); echo base > "$r/a.txt"; ln -s "/x/$V" "$r/link"; commit "$r"
run "$r" "$V" --release; expect 1 "value in a symlink target" "$V"
r=$(newrepo shallowsrc); echo a > "$r/a.txt"; commit "$r" one; echo b > "$r/a.txt"; commit "$r" two
git clone -q --depth 1 "file://$r" "$tmp/shallow"
run "$tmp/shallow" "$V" --release; expect 2 "release: a shallow clone (history incomplete) is a failure to verify" "$V"
r=$(newrepo missing); echo "$V" > "$r/a.txt"; commit "$r" leak; echo ok > "$r/a.txt"; commit "$r" fixed
oid=$(git -C "$r" rev-parse HEAD~1:a.txt); rm -f "$r/.git/objects/${oid%"${oid#??}"}/${oid#??}"
run "$r" "$V" --release; expect 2 "a missing object is a failure to verify, not a pass" "$V"

# --- reviewed-binary gate ------------------------------------------------------------------------
# A real, minimal PNG (1x1), built in the test so it is a valid file with no text chunks.
gen="$tmp/gen.js"
cat > "$gen" <<'JS'
const z=require("zlib");
const crc=(b)=>{let c,t=[];for(let n=0;n<256;n++){c=n;for(let k=0;k<8;k++)c=c&1?0xedb88320^(c>>>1):c>>>1;t[n]=c>>>0}let r=0xffffffff;for(const x of b)r=t[(r^x)&255]^(r>>>8);return(r^0xffffffff)>>>0};
const ch=(ty,d)=>{const l=Buffer.alloc(4);l.writeUInt32BE(d.length);const td=Buffer.concat([Buffer.from(ty),d]);const c=Buffer.alloc(4);c.writeUInt32BE(crc(td));return Buffer.concat([l,td,c])};
const ihdr=Buffer.alloc(13);ihdr.writeUInt32BE(1,0);ihdr.writeUInt32BE(1,4);ihdr[8]=8;ihdr[9]=2;
const extra=process.argv[2]||"";
const out=[Buffer.from([137,80,78,71,13,10,26,10]),ch("IHDR",ihdr)];
if(extra==="text")out.push(ch("tEXt",Buffer.from("Comment\0hello")));
if(extra==="ztxt")out.push(ch("zTXt",Buffer.concat([Buffer.from("Comment\0\0"),z.deflateSync(Buffer.from(process.argv[3]))])));
if(extra==="itxt")out.push(ch("iTXt",Buffer.concat([Buffer.from("Comment\0\1\0\0\0"),z.deflateSync(Buffer.from(process.argv[3]))])));
out.push(ch("IDAT",z.deflateSync(Buffer.from([0,255,0,0]))),ch("IEND",Buffer.alloc(0)));
process.stdout.write(Buffer.concat(out));
JS
png="$tmp/ok.png"; node "$gen" > "$png"
sha() { shasum -a 256 "$1" | cut -d' ' -f1; }
bsize() { wc -c < "$1" | tr -d ' '; }
setrepo() { r=$(newrepo "$1"); echo base > "$r/a.txt"; mkdir -p "$r/scripts"; cp "$png" "$r/pic.png"; }
withpin() { # REPO PNGFILE: writes the manifest, returns its digest in $pin
  printf '%s\t%s\tpng\treview-test-1\n' "$(sha "$2")" "$(bsize "$2")" > "$1/scripts/reviewed-binaries.tsv"; pin=$(sha "$1/scripts/reviewed-binaries.tsv"); }
runpin() { r=$1; shift; out="$tmp/out.$n"; set +e; (cd "$r" && ENDORA_OWNER_VALUES="$V" ENDORA_REVIEWED_BINARIES_SHA256="$pin" node "$chk" "$@" >"$out" 2>&1); code=$?; set -e; }

setrepo nopin; commit "$r"
run "$r" "$V" --release; expect 1 "release: a binary with no pin is an unreviewed-binary finding" "$V"
grep -q "unreviewed-binary" "$out" && ok "the finding is named" || bad "finding not named"
run "$r" "$V"; expect 0 "default mode does not gate binaries" "$V"

setrepo pinned; withpin "$r" "$png"; commit "$r"
runpin "$r" --release; expect 0 "release: a binary pinned by hash with the matching manifest digest passes" "$V"
pin=$(printf 'x' | shasum -a 256 | cut -d' ' -f1)
runpin "$r" --release; expect 2 "release: the manifest digest not matching the out-of-band pin is a failure to verify" "$V"
pin=""; runpin "$r" --release; expect 1 "release: no out-of-band pin means the manifest approves nothing" "$V"

setrepo selfapprove; withpin "$r" "$png"; commit "$r"
printf '%s\t%s\tpng\tself\n' "$(sha "$png")" "$(bsize "$png")" >> "$r/scripts/reviewed-binaries.tsv"; commit "$r" tamper
runpin "$r" --release; expect 2 "negative control: editing the manifest after the pin was taken fails" "$V"

setrepo swapped; withpin "$r" "$png"; node -e 'process.stdout.write(require("fs").readFileSync(process.argv[1]))' "$png" > "$r/pic.png"; printf '\000trailing' >> "$r/pic.png"; commit "$r"
runpin "$r" --release; expect 1 "negative control: a changed binary no longer matches its pin" "$V"

setrepo stale; withpin "$r" "$png"; rm "$r/pic.png"; commit "$r"
runpin "$r" --release; expect 1 "negative control: a pin for a binary that is in no commit is reported stale" "$V"
grep -q "stale-pin" "$out" && ok "the stale pin is named" || bad "stale-pin not named"

setrepo hist; commit "$r" add; git -C "$r" rm -q pic.png; commit "$r" gone
run "$r" "$V" --release; expect 1 "release: a binary removed in a later commit is still in history and still needs a pin" "$V"

setrepo sizelie; printf '%s\t%s\tpng\tr\n' "$(sha "$png")" "$(( $(bsize "$png") + 1 ))" > "$r/scripts/reviewed-binaries.tsv"; pin=$(sha "$r/scripts/reviewed-binaries.tsv"); commit "$r"
runpin "$r" --release; expect 1 "negative control: a pin with the wrong size does not count" "$V"

setrepo wild; printf '*\t0\tpng\tr\n' > "$r/scripts/reviewed-binaries.tsv"; pin=$(sha "$r/scripts/reviewed-binaries.tsv"); commit "$r"
runpin "$r" --release; expect 2 "negative control: a wildcard pin is a malformed manifest" "$V"

setrepo dupe; withpin "$r" "$png"; cat "$r/scripts/reviewed-binaries.tsv" "$r/scripts/reviewed-binaries.tsv" > "$tmp/dup"; cp "$tmp/dup" "$r/scripts/reviewed-binaries.tsv"; pin=$(sha "$r/scripts/reviewed-binaries.tsv"); commit "$r"
runpin "$r" --release; expect 2 "a duplicated pin is a malformed manifest" "$V"

png2="$tmp/text.png"
node "$gen" text > "$png2"
setrepo anc; cp "$png2" "$r/pic.png"; withpin "$r" "$png2"; commit "$r"
runpin "$r" --release; expect 1 "a pinned PNG still fails if it carries a text chunk" "$V"
grep -q "png-chunk" "$out" && ok "the chunk finding is named" || bad "png-chunk finding not named"
node "$gen" ztxt "hidden $V inside" > "$tmp/z.png"
setrepo ztxt; cp "$tmp/z.png" "$r/pic.png"; withpin "$r" "$tmp/z.png"; commit "$r"
runpin "$r" --release; expect 1 "a value inside a compressed PNG text chunk is found" "$V"; kind "png-text-value" "  ...inflated and matched"
node "$gen" itxt "hidden $V inside" > "$tmp/i.png"
setrepo itxt; cp "$tmp/i.png" "$r/pic.png"; withpin "$r" "$tmp/i.png"; commit "$r"
runpin "$r" --release; expect 1 "a value inside a compressed iTXt chunk is found" "$V"; kind "png-text-value" "  ...inflated and matched"
printf 'not a png at all' > "$tmp/fake.png"; printf '\000' >> "$tmp/fake.png"
setrepo fake; cp "$tmp/fake.png" "$r/pic.png"; withpin "$r" "$tmp/fake.png"; commit "$r"
runpin "$r" --release; expect 1 "a pinned file that is not a well-formed PNG fails" "$V"

echo "$n checks, $fail failed"
[ "$fail" = 0 ] && echo "owner-values checker tests passed"
[ "$fail" = 0 ]
