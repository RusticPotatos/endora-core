#!/bin/sh
# Tests for scripts/gen-third-party-notices.sh that need no Docker: a stand-in `docker` runs the
# generator's own collect step against a fixture root file system (fictional packages only), and a
# stand-in `ldd` answers for the fixture's binaries. What they prove: packages are read from the
# dpkg status file (a removed package is skipped; a Source: field and its version are honoured);
# licences come from the `License:` lines AND from a text scan, so a free-form copyright file that
# wraps "GNU Lesser General Public License" across two lines is still found; the copyleft table
# lists such a package with its basis; the Rust crates come from the image's own index; the shared
# libraries of Endora's programs are mapped to their packages; the output is the same twice; and an
# image with no packages, or a docker that fails, fails the generator instead of printing a file.
#
#   sh scripts/test-gen-third-party-notices.sh
set -eu

here=$(cd "$(dirname "$0")" && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
root="$work/root"; shim="$work/shim"
mkdir -p "$shim" "$root/var/lib/dpkg/info" "$root/usr/share/doc" "$root/usr/local/bin" \
         "$root/usr/local/lib/endora" "$root/usr/share/doc/endora-core/licenses"
fail=0

# --- the fixture: four installed packages, one removed, none real ---------------------------------
cat > "$root/var/lib/dpkg/status" <<'EOF'
Package: libjohn1
Status: install ok installed
Architecture: amd64
Version: 2.1-3+b2
Source: john-src (2.1-3)

Package: jane-tool
Status: install ok installed
Architecture: amd64
Version: 1:1.0-1

Package: doe-free
Status: install ok installed
Architecture: amd64
Version: 0.9-1

Package: doe-gone
Status: deinstall ok config-files
Architecture: amd64
Version: 7-1

Package: doe-nocopy
Status: install ok installed
Architecture: amd64
Version: 3-1
EOF
printf '%s\n' /usr/lib/x86_64-linux-gnu/libjohn.so.1 /lib64/ld-linux-x86-64.so.2 /lib/ld-linux-aarch64.so.1 \
  /usr/share/doc/libjohn1/copyright > "$root/var/lib/dpkg/info/libjohn1:amd64.list"
printf '%s\n' /usr/bin/jane /usr/share/doc/jane-tool/copyright > "$root/var/lib/dpkg/info/jane-tool.list"
printf '%s\n' /usr/share/doc/doe-free/copyright > "$root/var/lib/dpkg/info/doe-free.list"
for p in libjohn1 jane-tool doe-free; do mkdir -p "$root/usr/share/doc/$p"; done
cat > "$root/usr/share/doc/libjohn1/copyright" <<'EOF'
Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/
Files: *
Copyright: 2020 John Doe
License: LGPL-2.1+

License: LGPL-2.1+
 This library is free software. On Debian systems the full text of the GNU Lesser General Public
 License can be found in /usr/share/common-licenses/LGPL-2.1.
EOF
cat > "$root/usr/share/doc/jane-tool/copyright" <<'EOF'
Files: *
Copyright: 2021 Jane Doe
License: MIT

License: MIT
 Permission is hereby granted, free of charge, to any person obtaining a copy.
EOF
cat > "$root/usr/share/doc/doe-free/copyright" <<'EOF'
This package is part of Doe Free, copyright 2019 John Doe.

It may be copied under the terms of the GNU Lesser General
Public License, version 2.1 or later.
EOF
# doe-nocopy has no copyright file at all: the generator must say so.
printf '#!/bin/sh\n' > "$root/usr/local/bin/endora-node"; chmod 755 "$root/usr/local/bin/endora-node"
printf '#!/bin/sh\n' > "$root/usr/local/lib/endora/homelab-docker-proxy.py"
printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
  fictional-crate 0.1.0 "MIT OR Apache-2.0" "2022 John Doe" fictional-crate-0.1.0 2 "" \
  no-text-crate 0.2.0 "MIT" "(supplied text)" no-text-crate-0.2.0 1 "NEEDS-OWNER-REVIEW" \
  > "$root/usr/share/doc/endora-core/licenses/INDEX.tsv"

# --- stand-ins ---------------------------------------------------------------------------------------
# docker: `run ... -v SCRIPT:/gen.sh:ro IMAGE /gen.sh --collect /` becomes the same collect against
# the fixture. FAKE_DOCKER: ok, fail, empty (a root with no dpkg database).
cat > "$shim/docker" <<'SH'
#!/bin/sh
case "$FAKE_DOCKER" in
  fail) echo "docker: no such image" >&2; exit 125 ;;
esac
src=""
while [ $# -gt 0 ]; do
  case "$1" in -v) src=${2%%:*}; shift 2 ;; *) shift ;; esac
done
r=$FIXTURE_ROOT; [ "$FAKE_DOCKER" = empty ] && r=$FIXTURE_EMPTY
exec sh "$src" --collect "$r"
SH
cat > "$shim/ldd" <<'SH'
#!/bin/sh
echo "	linux-vdso.so.1 (0x00007ffd0)"
echo "	libjohn.so.1 => /usr/lib/x86_64-linux-gnu/libjohn.so.1 (0x00007f0)"
echo "	libmissing.so.9 => /opt/nowhere/libmissing.so.9 (0x00007f1)"
# The dynamic loader is the one line that names the CPU: ARCH picks the amd64 or the arm64 spelling.
case "$ARCH" in
  arm64) echo "	/lib/ld-linux-aarch64.so.1 (0x00007f2)" ;;
  *)     echo "	/lib64/ld-linux-x86-64.so.2 (0x00007f2)" ;;
esac
SH
chmod +x "$shim/docker" "$shim/ldd"
mkdir -p "$work/empty/var/lib/dpkg"; : > "$work/empty/var/lib/dpkg/status"

gen() { ARCH="${ARCH:-amd64}" PATH="$shim:$PATH" FIXTURE_ROOT="$root" FIXTURE_EMPTY="$work/empty" FAKE_DOCKER="${FAKE:-ok}" \
          sh "$here/gen-third-party-notices.sh" fictional:image; }

expect() { # name, pattern (fixed string) in $work/out
  if grep -qF -e "$2" "$work/out"; then echo "ok   $1"; else echo "FAIL $1: no line with: $2"; fail=1; fi
}
reject() {
  if grep -qF -e "$2" "$work/out"; then echo "FAIL $1: found: $2"; fail=1; else echo "ok   $1"; fi
}

FAKE=ok gen > "$work/out" 2> "$work/err" || { echo "FAIL generator exited non-zero"; sed 's/^/     /' "$work/err"; fail=1; }

expect "debian row: Source: field and its version honoured" "| libjohn1 | 2.1-3+b2 | john-src | LGPL-2.1+ |"
expect "debian row: no Source: means the package itself" "| jane-tool | 1:1.0-1 | jane-tool | MIT |"
expect "debian row: free-form copyright file is said to be free-form" "| doe-free | 0.9-1 | doe-free | (free-form copyright file) |"
expect "debian row: a missing copyright file is said so, not hidden" "| doe-nocopy | 3-1 | doe-nocopy | (no copyright file) |"
reject "a removed package is not listed" "doe-gone"
expect "copyleft table: text-scan-only package is listed with its basis" "| doe-free | 0.9-1 | doe-free | LGPL | text |"
expect "copyleft table: licence-line package is listed" "| john-src | 2.1-3 | libjohn1 | LGPL | licence line, text | yes |"
reject "copyleft table: an MIT-only package is not listed" "| jane-tool | 1:1.0-1 | jane-tool | MIT | "
expect "crates: from the image's own index" "| fictional-crate | 0.1.0 | MIT OR Apache-2.0 | 2022 John Doe |"
expect "crates: a supplied-text crate is flagged for review" "NEEDS-OWNER-REVIEW"
expect "binaries: Endora's programs are listed" "endora-node"
expect "libraries: mapped to the Debian package" "| /usr/local/bin/endora-node | libjohn.so.1 | libjohn1 |"
expect "libraries: an unmapped one is said to be unmapped" "| /usr/local/bin/endora-node | libmissing.so.9 | (no package owns it) |"
expect "libraries: the loader is named by role, not by CPU" "| /usr/local/bin/endora-node | the glibc dynamic loader | libjohn1 |"
reject "libraries: no amd64 loader file name in the table" "ld-linux-x86-64"
reject "libraries: no arm64 loader file name in the table" "ld-linux-aarch64"
(ARCH=arm64; gen) > "$work/out-arm" 2>/dev/null || { echo "FAIL generator exited non-zero for arm64"; fail=1; }
if cmp -s "$work/out" "$work/out-arm"; then echo "ok   amd64 and arm64 images give the same notices"; else echo "FAIL notices differ between architectures"; diff "$work/out" "$work/out-arm" | sed 's/^/     /'; fail=1; fi
expect "glibc notice line is present" "glibc"
expect "written offer is present" "### Written offer"

FAKE=ok gen > "$work/out2" 2>/dev/null || true
if cmp -s "$work/out" "$work/out2"; then echo "ok   same input, same output"; else echo "FAIL output differs between two runs"; fail=1; fi

# Negative controls: a file must never be printed for an image the generator could not read.
if FAKE=fail gen > "$work/o3" 2>/dev/null; then echo "FAIL a failing docker must fail the generator"; fail=1; else echo "ok   a failing docker fails the generator"; fi
[ ! -s "$work/o3" ] && echo "ok   ...and prints nothing" || { echo "FAIL ...and printed a file"; fail=1; }
if FAKE=empty gen > "$work/o4" 2>/dev/null; then echo "FAIL an image with no packages must fail"; fail=1; else echo "ok   an image with no packages fails the generator"; fi
if sh "$here/gen-third-party-notices.sh" > /dev/null 2>&1; then echo "FAIL no image argument must fail"; fail=1; else echo "ok   no image argument fails"; fi

if [ "$fail" -ne 0 ]; then echo "gen-third-party-notices tests failed"; exit 1; fi
echo "gen-third-party-notices tests passed"
