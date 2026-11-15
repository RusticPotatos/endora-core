#!/bin/sh
# How big the foundation is: the four crates, in lines of Rust. Read-only; prints, never fails on size.
#   sh scripts/foundation-size.sh
# "lines" is `wc -l` over every *.rs under a crate's src/ (comments and blanks included, `#[cfg(test)]` modules
# included). "code" is cloc's code lines (no comments or blanks) and is printed only if cloc is installed.
# "tests" is `wc -l` over the crate's tests/ directory. The two shared crates the foundation links
# (shared/kernel, shared/persistence) are not counted here; they serve the legacy code too.
set -eu
cd "$(dirname "$0")/.."
have_cloc=no; command -v cloc >/dev/null 2>&1 && have_cloc=yes
total=0; total_code=0; total_tests=0
printf '%-32s %8s %8s %8s\n' crate lines code tests
for dir in domains/foundation domains/foundation-observe domains/foundation-git app/foundation; do
    [ -d "$dir/src" ] || continue
    lines=$(find "$dir/src" -name '*.rs' -exec cat {} + | wc -l | tr -d ' ')
    tests=0
    [ -d "$dir/tests" ] && tests=$(find "$dir/tests" -name '*.rs' -exec cat {} + | wc -l | tr -d ' ')
    code=-
    if [ "$have_cloc" = yes ]; then
        code=$(cloc --quiet --csv --include-lang=Rust "$dir/src" 2>/dev/null | awk -F, 'END{print $NF + 0}')
        total_code=$((total_code + code))
    fi
    printf '%-32s %8s %8s %8s\n' "$dir" "$lines" "$code" "$tests"
    total=$((total + lines)); total_tests=$((total_tests + tests))
done
[ "$have_cloc" = yes ] || total_code=-
printf '%-32s %8s %8s %8s\n' total "$total" "$total_code" "$total_tests"
