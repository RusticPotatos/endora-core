#!/bin/sh
# Collects the licence texts of the Rust crates built into Endora's programs, in the Dockerfile's
# build stage (no network, no new tool: cargo and the registry sources it already holds).
#
#   sh scripts/collect-crate-licences.sh <target-triple> <output-dir>
#
# For every third-party crate `cargo tree` names for the four programs the node image ships, it
# copies each LICENSE*, LICENCE*, COPYING*, NOTICE*, UNLICENSE*, COPYRIGHT*, PATENTS* file and each
# file under a licenses/ folder from the crate's source (nested ones too: ring, aws-lc-sys,
# chrono-tz, libsqlite3-sys, tracing-core) to <output-dir>/<crate>-<version>/, and writes
# <output-dir>/INDEX.tsv: name, version, SPDX licence, first copyright line, folder, file count, note.
# A crate whose package has no licence file gets SUPPLIED-NO-LICENCE-FILE.txt: the standard text of
# the licence its manifest declares, the copyright line found in its README or manifest, and the
# note NEEDS-OWNER-REVIEW. It fails (and the image build with it) on anything it cannot place.
set -eu
triple=${1:?usage: collect-crate-licences.sh <target-triple> <output-dir>}
out=${2:?usage: collect-crate-licences.sh <target-triple> <output-dir>}
reg=${CARGO_HOME:-/usr/local/cargo}/registry/src
tab=$(printf '\t')
die() { echo "collect-crate-licences: $*" >&2; exit 1; }

mkdir -p "$out"
list=$(mktemp)
trap 'rm -f "$list"' EXIT
# `-e normal` is what the binaries link; workspace crates (path dependencies) are Endora's own.
cargo tree --offline --locked -p endora-node -p endora-hq -p endora-worker -p endora-runner \
  -e normal --target "$triple" --prefix none -f '{p}|{l}' \
  | sed 's/ (\*)//; s/ (proc-macro)//' | grep -v '^$' | grep -v ' (/' | sort -u > "$list" \
  || die "cargo tree failed"
[ -s "$list" ] || die "cargo tree named no crates"
if cut -d'|' -f1 "$list" | grep -q ' ('; then
  die "a crate from a source other than the registry: $(cut -d'|' -f1 "$list" | grep ' (' | head -1)"
fi

: > "$out/INDEX.tsv"
while IFS='|' read -r pv lic; do
  name=${pv% v*}; ver=${pv##* v}
  src=$(ls -d "$reg"/*/"$name-$ver" 2>/dev/null | head -1)
  [ -d "$src" ] || die "no source for $name $ver under $reg"
  dest=$out/$name-$ver
  mkdir -p "$dest"
  (cd "$src" && find . -type f \( -iname 'LICENSE*' -o -iname 'LICENCE*' -o -iname 'COPYING*' \
     -o -iname 'NOTICE*' -o -iname 'UNLICENSE*' -o -iname 'COPYRIGHT*' -o -iname 'PATENTS*' \
     -o -ipath '*/licenses/*' \) -exec cp --parents -t "$dest" {} +)
  n=$(find "$dest" -type f | wc -l | tr -d ' ')
  note=""
  if [ "$n" -eq 0 ]; then
    note=NEEDS-OWNER-REVIEW
    cr=$(cat "$src"/README* 2>/dev/null | grep -i -m1 -E '^[[:space:]]*(copyright|\(c\))' || true)
    [ -n "$cr" ] || cr=$(sed -n 's/^authors *= *\[\(.*\)\]/\1/p' "$src/Cargo.toml" | head -1 \
                         | sed 's/ *<[^>]*>//g; s/"//g' | sed 's/^/Copyright (c) /')
    [ -n "$cr" ] || cr="Copyright (c) the authors of $name (no copyright line found)"
    {
      echo "SUPPLIED TEXT, NOT FROM THE CRATE. NEEDS-OWNER-REVIEW."
      echo "The published package of $name $ver contains no licence file."
      echo "SPDX identifier from its Cargo.toml: $lic"
      echo "Copyright line as found in its README or Cargo.toml: $cr"
      echo
      case $lic in *MIT*)
        echo "MIT License (standard text, SPDX template)"; echo
        echo "$cr"; echo
        echo "Permission is hereby granted, free of charge, to any person obtaining a copy of this software"
        echo "and associated documentation files (the \"Software\"), to deal in the Software without"
        echo "restriction, including without limitation the rights to use, copy, modify, merge, publish,"
        echo "distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the"
        echo "Software is furnished to do so, subject to the following conditions:"; echo
        echo "The above copyright notice and this permission notice shall be included in all copies or"
        echo "substantial portions of the Software."; echo
        echo "THE SOFTWARE IS PROVIDED \"AS IS\", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING"
        echo "BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND"
        echo "NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM,"
        echo "DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,"
        echo "OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE." ;;
      esac
      case $lic in *Apache-2.0*)
        echo; echo "Apache License 2.0: the standard text follows."; echo
        cat /usr/share/common-licenses/Apache-2.0 ;;
      esac
    } > "$dest/SUPPLIED-NO-LICENCE-FILE.txt"
    n=1
    first=$cr
  else
    first=$(find "$dest" -type f | sort | while read -r f; do
              grep -h -i -m1 -E '^[[:space:]]*(copyright|\(c\))[[:space:]]+(\(c\)|©|[0-9])' "$f" || true
            done | head -1)
  fi
  # The index is read into a tracked file: a third party's mailbox, in a copyright line, stays out of it.
  first=$(printf '%s' "$first" | tr "$tab" ' ' | sed 's/ *<[^>]*@[^>]*>//g; s/[A-Za-z0-9._%+-]*@[A-Za-z0-9.-]*//g; s/ *()//g; s/^[[:space:]]*//; s/[[:space:]]*$//' | cut -c1-200)
  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$name" "$ver" "$lic" "$first" "$name-$ver" "$n" "$note" >> "$out/INDEX.tsv"
done < "$list"
echo "collect-crate-licences: $(wc -l < "$out/INDEX.tsv" | tr -d ' ') crates, $(find "$out" -type f | wc -l | tr -d ' ') files, $(du -sk "$out" | cut -f1) KiB, $(grep -c NEEDS-OWNER-REVIEW "$out/INDEX.tsv") need owner review"
