#!/bin/sh
# Regenerates THIRD-PARTY-NOTICES.md from a built Core image, so the file describes the image that
# is published and not a hand-read copy of an earlier build.
#
#   sh scripts/gen-third-party-notices.sh <image> [--crates FILE] > THIRD-PARTY-NOTICES.md
#
# It runs this same script inside the image (`--collect`, no network, read-only mount of the
# script, nothing installed in the image) to read the installed packages from /var/lib/dpkg/status,
# the licences from BOTH the machine-readable `License:` lines and a text scan of every
# /usr/share/doc/<package>/copyright (free-form ones, such as glibc's, name no `License:` line),
# Endora's own programs and the shared libraries they load, and the Rust crate index the image
# carries at /usr/share/doc/endora-core/licenses/INDEX.tsv. An image without that index needs
# `--crates FILE` (tab-separated: name, version, licence, copyright). It prints nothing and fails
# when the image cannot be read or holds no packages.
# scripts/test-gen-third-party-notices.sh tests it without Docker, on a fictional root file system.
#
# Internal: `--collect ROOT` prints one tab-separated fact a line from the file system at ROOT.
set -eu
export LC_ALL=C
tab=$(printf '\t')

die() { echo "gen-third-party-notices: $*" >&2; exit 1; }

# --- the step that runs inside the image ----------------------------------------------------------
collect() {
  root=${1%/}
  status=$root/var/lib/dpkg/status
  [ -s "$status" ] || die "no dpkg status under ${root:-/}"
  info=$root/var/lib/dpkg/info
  # One line a package that is installed (a removed package keeps a `config-files` entry):
  # name, version, source package, source version, architecture.
  awk 'BEGIN { RS = ""; FS = "\n" }
  { p = ""; s = ""; v = ""; a = ""; src = ""
    for (i = 1; i <= NF; i++) {
      if ($i ~ /^Package: /) p = substr($i, 10)
      else if ($i ~ /^Status: /) s = substr($i, 9)
      else if ($i ~ /^Version: /) v = substr($i, 10)
      else if ($i ~ /^Architecture: /) a = substr($i, 15)
      else if ($i ~ /^Source: /) src = substr($i, 9)
    }
    if (s !~ / installed$/ || p == "") next
    sn = p; sv = v
    if (src != "") {
      n = index(src, " (")
      if (n > 0) { sn = substr(src, 1, n - 1); sv = substr(src, n + 2); sub(/\)$/, "", sv) } else sn = src
    }
    printf "%s\t%s\t%s\t%s\t%s\n", p, v, sn, sv, a }' "$status" > "${TMPDIR:-/tmp}/gen-pkgs.$$"
  [ -s "${TMPDIR:-/tmp}/gen-pkgs.$$" ] || die "no installed package under ${root:-/}"
  while IFS="$tab" read -r name ver sn sv arch; do
    doc=$root/usr/share/doc/$name/copyright
    lic=""; fam=""; st=none
    if [ -r "$doc" ]; then
      lic=$({ grep '^License:' "$doc" || true; } | sed 's/^License:[[:space:]]*//; s/[[:space:]]*$//' \
            | { grep -v '^$' || true; } | sort -u | tr '\n' ';' | sed 's/;$//')
      st=free; [ -n "$lic" ] && st=dep5
      # Letters only, one space between words, so a phrase wrapped over two lines still matches.
      t=" $(tr 'A-Z' 'a-z' < "$doc" | tr -c 'a-z' ' ' | tr -s ' ') "
      has() { printf '%s' "$t" | grep -Eq "$1"; }
      for pair in 'AGPL:affero general public licen| agpl ' \
                  'CDDL:common development and distribution licen| cddl ' \
                  'EPL:eclipse public licen| epl ' \
                  'GPL:gnu general public licen| gpl ' \
                  'LGPL:gnu (lesser|library) general public licen| lgpl ' \
                  'MPL:mozilla public licen| mpl '; do
        if has "${pair#*:}"; then fam="${fam:+$fam,}${pair%%:*}"; fi
      done
    fi
    list=$info/$name.list; [ -r "$list" ] || list=$info/$name:$arch.list
    code=no
    if [ -r "$list" ] && { grep -v '^/usr/share/' "$list" || true; } \
         | grep -Eq '(^|/)(s?bin|libexec)/[^/]+$|\.so(\.[0-9A-Za-z_.-]*)?$'; then code=yes; fi
    printf 'pkg\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$name" "$ver" "$sn" "$sv" "$lic" "$fam" "$code" "$st"
  done < "${TMPDIR:-/tmp}/gen-pkgs.$$"
  rm -f "${TMPDIR:-/tmp}/gen-pkgs.$$"

  # Endora's programs and the shared libraries each loads (ldd reads the ELF headers; Endora's own
  # binaries, built here, are all it is run on).
  for f in "$root"/usr/local/bin/* "$root"/usr/local/lib/endora/*; do
    [ -e "$f" ] || [ -L "$f" ] || continue
    rel=${f#"$root"}
    if [ -L "$f" ]; then printf 'bin\t%s\tlink to %s\n' "$rel" "$(readlink "$f")"; continue; fi
    printf 'bin\t%s\tfile\n' "$rel"
    out=$("${LDD:-ldd}" "$f" 2>/dev/null) || continue
    printf '%s\n' "$out" | awk '
      /=>/ { if ($3 ~ /^\//) print $1 "\t" $3; next }
      $1 ~ /^\// { n = split($1, a, "/"); print a[n] "\t" $1 }' | while IFS="$tab" read -r lib path; do
      owner=$({ grep -l -x -F -e "$path" -e "/usr$path" -e "${path#/usr}" "$info"/*.list 2>/dev/null || true; } | head -1)
      owner=${owner##*/}; owner=${owner%.list}; owner=${owner%%:*}
      # The loader's file name names the CPU (ld-linux-x86-64.so.2, ld-linux-aarch64.so.1); the
      # notices must read the same from an amd64 and an arm64 build of the one image.
      case $lib in ld-linux*.so* | ld.so*) lib="the glibc dynamic loader" ;; esac
      printf 'need\t%s\t%s\t%s\n' "$rel" "$lib" "${owner:-(no package owns it)}"
    done
  done

  idx=$root/usr/share/doc/endora-core/licenses/INDEX.tsv
  if [ -r "$idx" ]; then while IFS= read -r line; do printf 'crate\t%s\n' "$line"; done < "$idx"; fi
}

if [ "${1:-}" = "--collect" ]; then collect "${2:-/}"; exit 0; fi

# --- the part that runs on the host ----------------------------------------------------------------
image=${1:-}; [ -n "$image" ] || die "usage: gen-third-party-notices.sh <image> [--crates FILE]"
shift
crates_file=""
while [ $# -gt 0 ]; do
  case $1 in
    --crates) crates_file=${2:-}; [ -r "$crates_file" ] || die "--crates needs a readable file"; shift 2 ;;
    *) die "unknown argument: $1" ;;
  esac
done

self=$(cd "$(dirname "$0")" && pwd)/$(basename "$0")
w=$(mktemp -d "${TMPDIR:-/tmp}/gen-notices.XXXXXX")
trap 'rm -rf "$w"' EXIT

"${GEN_DOCKER:-docker}" run --rm --network none --entrypoint sh -v "$self:/gen.sh:ro" "$image" \
  /gen.sh --collect / > "$w/facts" || die "could not read the image $image"

awk -F'\t' -v d="$w" '{ k = $1; sub(/^[^\t]*\t/, ""); print > (d "/" k) }' "$w/facts"
[ -s "$w/pkg" ] || die "the image $image has no installed packages"
: >> "$w/bin"; : >> "$w/need"; : >> "$w/crate"
if [ -s "$w/crate" ]; then :
elif [ -n "$crates_file" ]; then cp "$crates_file" "$w/crate"
else die "no crate list: the image has no licenses/INDEX.tsv and no --crates file was given"
fi
sort -t "$tab" -k1,1 "$w/pkg" > "$w/pkg.s"
sort -t "$tab" -k1,1 -k2,2 "$w/crate" > "$w/crate.s"
npkg=$(wc -l < "$w/pkg.s" | tr -d ' ')
ncrate=$(wc -l < "$w/crate.s" | tr -d ' ')

cat <<EOF
# Third-party notices

Endora Core is licensed under the Apache License 2.0 (\`LICENSE\`, \`NOTICE\`). This file lists the
third-party software that Core's image carries, under what licence, and where the source of the
copyleft parts can be had. It covers the final (default) stage of the Dockerfile, the node image.
It does not cover the Dockerfile's other targets (\`sound\`, \`senders\`, \`airplay2\`, \`worksite\`,
\`bench\`).

This file is generated from the image itself, by \`scripts/gen-third-party-notices.sh\`. The licence
texts of the Rust crates are in the image, in \`/usr/share/doc/endora-core/licenses/\`, and the
Debian packages' own notices in \`/usr/share/doc/<package>/copyright\`.

## 1. What the image contains

Endora's own programs and scripts (Apache-2.0):

EOF
awk -F'\t' '{ printf "- `%s` (%s)\n", $1, $2 }' "$w/bin"
cat <<EOF

Nothing in the image was patched: the Debian packages are installed as Debian builds them, and the
crates are built from the crates.io sources \`Cargo.lock\` names.

**glibc.** Endora's programs use the GNU C Library (glibc, LGPL-2.1 or later) at run time, as the
shared libraries \`libc.so.6\` and \`libm.so.6\`, and \`libgcc_s.so.1\` (GCC, with the GCC Runtime
Library Exception). They are not modified; the licence texts are in
\`/usr/share/doc/libc6/copyright\` and \`/usr/share/common-licenses/\`.

## 2. Rust crates

$ncrate third-party crates linked into Endora's programs (Endora's own workspace crates, all
Apache-2.0, are not listed). The licence column is the crate's declared SPDX expression; the
copyright column is the first copyright line of the crate's own licence files. The licence texts
themselves are in the image, in \`/usr/share/doc/endora-core/licenses/<crate>-<version>/\`.

| Crate | Version | Licence | Copyright |
|---|---|---|---|
EOF
awk -F'\t' '{ c = $4; if (NF >= 7 && $7 != "") c = c " " $7; print "| " $1 " | " $2 " | " $3 " | " c " |" }' "$w/crate.s"
cat <<EOF

## 3. Debian packages

The $npkg packages of the image, with the licences their own \`copyright\` files name (the
\`License:\` lines of a machine-readable file; a package that bundles other code names several).
A package marked free-form has a copyright file with no \`License:\` lines: it is still scanned for
copyleft licences in section 5. The authoritative text of each is in the image:
\`/usr/share/doc/<package>/copyright\`, and the full texts of the common licences in
\`/usr/share/common-licenses/\`.

| Package | Version | Source package | Licences named |
|---|---|---|---|
EOF
awk -F'\t' '{ l = $5; if (l == "") l = ($8 == "none") ? "(no copyright file)" : "(free-form copyright file)"
  print "| " $1 " | " $2 " | " $3 " | " l " |" }' "$w/pkg.s"
cat <<EOF

## 4. Shared libraries of Endora's programs

What each program of Endora's loads at run time (\`ldd\` in the image), and the Debian package that
owns it.

| Program | Library | Debian package |
|---|---|---|
EOF
sort -t "$tab" -k1,1 -k2,2 "$w/need" | awk -F'\t' '{ print "| " $1 " | " $2 " | " $3 " |" }'
cat <<EOF

## 5. Copyleft components and where their source is

The Debian source packages below have a GPL, LGPL, AGPL, MPL, EPL or CDDL licence in at least one
binary package's copyright file, found two ways: a \`License:\` line, or a scan of the whole text
(the Basis column; a free-form copyright file such as glibc's has no \`License:\` line, so only the
text scan finds it). The list is deliberately wide: it includes packages where the licence covers
only a part (a packaging script, a test, a bundled library). "Ships code" is a guess from the
installed file names (an executable in a \`bin\` directory, or a \`.so\` library), not from ELF headers.

| Source package | Version in the image | Binary packages in the image | Licences found | Basis | Ships code |
|---|---|---|---|---|---|
EOF
awk -F'\t' '
  function addfam(s, f) { fams[s, f] = 1 }
  { name = $1; sn = $3; sv = $4; lic = $5; fam = $6; code = $7
    ls = 0
    if (lic ~ /(^|[^A-Za-z])AGPL/) { addfam(sn, "AGPL"); ls = 1 }
    if (lic ~ /(^|[^A-Za-z])CDDL/) { addfam(sn, "CDDL"); ls = 1 }
    if (lic ~ /(^|[^A-Za-z])EPL/) { addfam(sn, "EPL"); ls = 1 }
    if (lic ~ /(^|[^A-Za-z])GPL/) { addfam(sn, "GPL"); ls = 1 }
    if (lic ~ /(^|[^A-Za-z])LGPL/) { addfam(sn, "LGPL"); ls = 1 }
    if (lic ~ /(^|[^A-Za-z])MPL/) { addfam(sn, "MPL"); ls = 1 }
    ts = 0
    if (fam != "") { n = split(fam, a, ","); for (i = 1; i <= n; i++) addfam(sn, a[i]); ts = 1 }
    if (!ls && !ts) next
    if (!(sn in seen)) { seen[sn] = 1; order[++cnt] = sn }
    if (ls) licline[sn] = 1
    if (ts) text[sn] = 1
    if (code == "yes") shipcode[sn] = 1
    bins[sn] = (sn in bins) ? bins[sn] ", " name : name
    if (!((sn, sv) in vseen)) { vseen[sn, sv] = 1; vers[sn] = (sn in vers) ? vers[sn] ", " sv : sv }
  }
  END {
    split("AGPL CDDL EPL GPL LGPL MPL", fl, " ")
    for (k = 1; k <= cnt; k++) {
      sn = order[k]; f = ""
      for (i = 1; i <= 6; i++) if ((sn, fl[i]) in fams) f = (f == "" ? "" : f ", ") fl[i]
      b = (sn in licline) ? "licence line" : ""
      if (sn in text) b = (b == "" ? "text" : b ", text")
      printf "%s\t%s\t%s\t%s\t%s\t%s\n", sn, vers[sn], bins[sn], f, b, ((sn in shipcode) ? "yes" : "no")
    }
  }' "$w/pkg.s" | sort -t "$tab" -k1,1 \
  | awk -F'\t' '{ print "| " $1 " | " $2 " | " $3 " | " $4 " | " $5 " | " $6 " |" }'
cat <<EOF

### Where to get the source

Every Debian package above is unmodified, so its corresponding source is Debian's source package
at the version in the tables:

- the Debian archive: \`https://deb.debian.org/debian/pool/main/\` (the \`.orig.tar.*\`, \`.debian.tar.*\`
  and \`.dsc\` of that version), or \`apt-get source <source-package>=<version>\` on a Debian 12 host
  with a \`deb-src\` line;
- \`https://snapshot.debian.org/\` for the exact version once the archive has moved on;
- \`https://sources.debian.org/\` to browse it.

The Rust crates are not copyleft, but their sources are the crates.io packages \`Cargo.lock\` names,
and Endora's own source is this repository at the commit the image's \`sha-<commit>\` tag names.

### Written offer

DRAFT, NEEDS-OWNER-REVIEW: the wording and whether to give it are the owner's decision.

For any component above licensed under the GNU General Public License or the GNU Lesser General
Public License, Endora will give anyone who asks the complete corresponding source code, or the
exact Debian source files and version it was built from, on a medium customarily used for software
interchange (a download is enough), for a charge no more than the cost of doing so (none, for a
download), for three years from the last time we distributed an image that contains it.
To ask, open an issue at \`https://github.com/RusticPotatos/endora-core/issues\` naming the image
tag (or its digest) and the component. The offer is in addition to the Debian locations above,
which anyone can use without asking.

## 6. How this was made

- \`sh scripts/gen-third-party-notices.sh <image> > THIRD-PARTY-NOTICES.md\`, which runs itself in the
  image with no network: \`/var/lib/dpkg/status\` for the packages, each \`/usr/share/doc/<package>/copyright\`
  for the licences (\`License:\` lines and a text scan), \`ldd\` for the libraries, and
  \`/usr/share/doc/endora-core/licenses/INDEX.tsv\` (written by the Dockerfile's build stage from the
  crates \`cargo tree\` names) for the Rust crates.
- The Debian part is right for the image it was read from. The base images are pinned by digest, but
  the packages the last stage installs come from the live Debian archive when the image is built, so a
  later build can carry newer ones: regenerate this file for the release and commit it before the
  release build. The file reads the same from an amd64 and an arm64 build (the dynamic loader, whose
  file name names the CPU, is given by role).
EOF
