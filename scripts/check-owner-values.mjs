#!/usr/bin/env node
// None of the owner's own values is in this repository: not in the tree, not in any commit
// that history carries, not in a commit's metadata, and not behind a binary nobody read.
//
//   node scripts/check-owner-values.mjs             a local check; unset values are said out loud
//   node scripts/check-owner-values.mjs --release   the check before a push: it fails CLOSED
//
// Exit codes: 0 clean · 1 found something · 2 could not verify. A check that could not look is
// never a pass, so every refusal below is exit 2, not 0.
//
// What it will never do is print what it found. The values are the owner's secret; a report
// that quotes them copies them into a log, a terminal scrollback and an agent's context. A
// finding names its KIND, the object id, and which value by position (#1, #2 ...), nothing else.
// A path or ref name that itself contains a value is shown as a short hash instead.
//
// Where it looks (each of these has hidden a value from a simpler check before):
//   - every blob of every commit reachable from any ref, HEAD, the stash or a replace ref
//     (replace refs are switched off so a `git replace` cannot hide a commit);
//   - every commit and tag object: author, committer, message, signature;
//   - every file and directory name in every tree, every ref name;
//   - the index and the working tree, including untracked files not ignored;
//   - bytes, not just text: UTF-8, Latin-1 and UTF-16 readings of every object, matched
//     case-insensitively, with whitespace in a value free to wrap across lines;
//   - PNG text chunks, after inflating them.
//
// What it cannot see, and says so: a value that is only in an image's pixels, encoded
// (base64, percent), or in an unreachable or reflog-only object (those are not published).
// Pixels are the reason for the reviewed-binary gate below.
//
// Reviewed-binary gate (--release only). A binary cannot be read by a text search, so each one
// in history must be PINNED: its SHA-256 and size in scripts/reviewed-binaries.tsv, a person
// having looked at it. Editing that file cannot approve anything by itself, because its own
// SHA-256 must equal ENDORA_REVIEWED_BINARIES_SHA256, which the reviewer supplies out of band
// and which is never in the repository. No pin variable means the manifest approves nothing.
// A pinned PNG must still be well formed and carry no ancillary chunk but a short allowlist.
//
// Reviewed value exceptions (optional, out of band). An owner value can sit inside an exact upstream
// URL that has to stay in a file (a Dockerfile, an ADR). The owner can excuse that one URL token, at
// one exact path, a stated number of times, through a private file named by
// ENDORA_OWNER_VALUE_EXCEPTIONS_FILE. The file is never in the repository: a published list of "the
// value is inside this URL" would itself point at the value. Without the variable nothing is excused.
// Entries are strict (see loadExceptions); a malformed, unreadable or foreign-bound file is exit 2.
// The file is pinned: ENDORA_OWNER_VALUE_EXCEPTIONS_SHA256 holds the SHA-256 of its exact bytes, supplied
// out of band by the owner. A mismatch is exit 2 in every mode; a missing pin is exit 2 with --release.
// The max count is per file VERSION: each distinct blob (history) or working-tree content is counted
// on its own, so two versions each within the count pass, and one version over it is a finding.
// A URL token is walked at most URL_WALK_MAX characters either side of the value; a longer run is not
// excused. Only file content is ever excused: commit and tag metadata, file and directory names, ref
// names, PNG text, binaries, symlinks and any other occurrence of the value stay findings.

import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { lstatSync, readFileSync, readlinkSync } from "node:fs";
import { inflateSync } from "node:zlib";

const MIN_VALUE_LENGTH = 3;
const MANIFEST = "scripts/reviewed-binaries.tsv";
const PNG_ALLOWED = new Set(["IHDR", "PLTE", "IDAT", "IEND", "tRNS", "sRGB", "gAMA", "pHYs"]);
const PNG_TEXT = new Set(["tEXt", "zTXt", "iTXt"]);
const PNG_SIGNATURE = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]);

const EXC_VARIABLE = "ENDORA_OWNER_VALUE_EXCEPTIONS_FILE";
const EXC_PIN_VARIABLE = "ENDORA_OWNER_VALUE_EXCEPTIONS_SHA256";
const EXC_MAX_BYTES = 65536;
const URL_WALK_MAX = 2048; // the longest token an entry may name (loadExceptions: chars <= 2048)
const URL_CHAR = /[A-Za-z0-9\-._~:/?#@&=%+]/;
const GLUED_BEFORE = /[A-Za-z0-9_\-.~%+@&?#:/]/; // a URL that starts glued to one of these is a different string
const REVIEW_PATH = /^[A-Za-z0-9._-]+(\/[A-Za-z0-9._-]+)*$/;
const REGULAR_MODES = new Set(["100644", "100755"]);

/** A reason to stop with exit 2. The text is fixed: it must never carry file or value text. */
class Refuse extends Error {}
const refuse = (why) => {
  throw new Refuse(why);
};

const args = process.argv.slice(2);
const release = args.includes("--release");

const sha256 = (buf) => createHash("sha256").update(buf).digest("hex");
const escapeRegex = (s) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

function git(gitArgs, { input, cwd } = {}) {
  const run = spawnSync("git", ["-c", "core.quotepath=off", ...gitArgs], {
    cwd,
    input,
    maxBuffer: 2 ** 31 - 1,
    // A replace ref must not be able to hide a real commit from the walk.
    env: { ...process.env, GIT_NO_REPLACE_OBJECTS: "1", GIT_OPTIONAL_LOCKS: "0" },
  });
  if (run.error || run.status !== 0) refuse(`git ${gitArgs[0]} failed`);
  return run.stdout;
}

// ---- the values -----------------------------------------------------------------------------

const OLD_VARIABLE = ["ENDORA", "PERSONAL", "VALUES"].join("_");
const given = [...new Set((process.env.ENDORA_OWNER_VALUES ?? "").split(",").map((v) => v.trim()).filter(Boolean))];
const usable = given.filter((v) => [...v].length >= MIN_VALUE_LENGTH);
const skipped = given.length - usable.length;

function main() {
  if (args.some((a) => a !== "--release")) refuse("unknown option");
  // The variable was renamed when Core got its own repository. An old name left in a shell would
  // be ignored and the check would quietly look for nothing, so refuse instead.
  if (process.env[OLD_VARIABLE] && !process.env.ENDORA_OWNER_VALUES) {
    refuse("the owner-values variable is now ENDORA_OWNER_VALUES; set that and unset the old name");
  }
  // Two names with different values: one list would be read and the other never, so refuse.
  const asSet = (text) => [...new Set(text.split(",").map((v) => v.trim()).filter(Boolean))].sort().join("\u0000");
  if (process.env[OLD_VARIABLE] && asSet(process.env[OLD_VARIABLE]) !== asSet(process.env.ENDORA_OWNER_VALUES ?? "")) {
    refuse("ENDORA_OWNER_VALUES and the previous variable name hold different values; keep only ENDORA_OWNER_VALUES");
  }
  if (release && given.length === 0) refuse("release mode needs ENDORA_OWNER_VALUES; none is set");
  if (release && skipped > 0) refuse(`release mode refuses a value shorter than ${MIN_VALUE_LENGTH} characters (${skipped} given)`);
  if (usable.length === 0) {
    console.log(`check-owner-values: NOT CHECKED — no usable owner values (${skipped} skipped as too short). Only a machine that holds them can run this check.`);
    return 0;
  }
  const patterns = usable.map((v) => new RegExp(escapeRegex(v).replace(/\s+/g, "\\s+"), "i"));
  const exc = loadExceptions(usable.length);

  /** Which values (by 1-based position) occur in these bytes, in any reading. */
  const valuesIn = (buf) => {
    const readings = [buf.toString("utf8"), buf.toString("latin1")];
    if (buf.length >= 2 * MIN_VALUE_LENGTH) {
      readings.push(buf.toString("utf16le"));
      if (buf.length > 2 * MIN_VALUE_LENGTH) readings.push(buf.subarray(1).toString("utf16le"));
    }
    const hits = [];
    patterns.forEach((p, i) => {
      if (readings.some((r) => p.test(r))) hits.push(i + 1);
    });
    return hits;
  };
  const valuesInText = (text) => valuesIn(Buffer.from(text, "utf8"));
  /** A name that is safe to print: itself, or a hash if it holds a value. */
  const safe = (name) => (valuesInText(name).length ? `<redacted:${sha256(Buffer.from(name)).slice(0, 8)}>` : name);

  /**
   * The entries that excuse every value hit in these bytes, or null. All of: the bytes are one regular,
   * valid-UTF-8, non-binary file that sits at exactly one path (a blob also stored at another path is
   * never excused); that path has entries; each hit is a value named by an entry whose exact URL token
   * (whole-token, hash and length match) contains it; no token occurs more often than reviewed; and
   * with every sanctioned token removed no value is left in any reading of the bytes.
   */
  const excuse = (paths, data, hits, regular) => {
    if (!exc || !regular || paths.size !== 1) return null;
    const entries = exc.byPath.get([...paths][0]);
    if (!entries) return null;
    if (data.subarray(0, 8000).includes(0) || data.subarray(0, 8).equals(PNG_SIGNATURE)) return null;
    const text = data.toString("utf8");
    if (!Buffer.from(text, "utf8").equals(data)) return null;
    const spans = new Map(); // start -> { end, entries }
    const starts = new Map(); // entry -> Set of starts
    for (const e of entries) {
      for (const m of text.matchAll(new RegExp(patterns[e.position - 1].source, "gi"))) {
        const t = urlTokenAround(text, m.index, m.index + m[0].length);
        if (!t || t.end - t.start !== e.chars) continue;
        if (sha256(Buffer.from(text.slice(t.start, t.end), "utf8")) !== e.digest) continue;
        const span = spans.get(t.start) ?? { end: t.end, entries: new Set() };
        span.entries.add(e);
        spans.set(t.start, span);
        starts.set(e, (starts.get(e) ?? new Set()).add(t.start));
      }
    }
    if (spans.size === 0) return null;
    // Masking below cuts by the original offsets, so spans must not overlap. A URL nested in another
    // sanctioned URL shares its end: masking the two in turn shifts the offsets and drops the bytes after
    // the shared end, which could excuse a bare value there. Overlapping spans are never excused.
    let reach = -1;
    for (const [start, span] of [...spans].sort((x, y) => x[0] - y[0])) {
      if (start < reach) return null;
      reach = Math.max(reach, span.end);
    }
    for (const [e, set] of starts) if (set.size > e.max) return null;
    let masked = text;
    for (const [start, span] of [...spans].sort((x, y) => y[0] - x[0])) masked = masked.slice(0, start) + "\u0001" + masked.slice(span.end);
    if (valuesInText(masked).length) return null;
    const named = new Set([...starts.keys()].map((e) => e.position));
    if (!hits.every((h) => named.has(h))) return null;
    return [...starts].map(([e, set]) => [e, set.size]);
  };
  const excused = { objects: 0, files: 0 }; // distinct objects / working-tree files fully excused
  const tally = (used, kind) => {
    excused[kind === "object" ? "objects" : "files"]++;
    for (const [e, n] of used) {
      if (kind === "object") { e.objects++; e.objectOccurrences += n; } else { e.files++; e.fileOccurrences += n; }
    }
  };

  const findings = [];
  const find = (kind, where, hits, note = "") => {
    findings.push(`  ${kind} ${where}${hits.length ? ` value${hits.length > 1 ? "s" : ""} ${hits.map((h) => `#${h}`).join(",")}` : ""}${note ? ` — ${note}` : ""}`);
  };

  // ---- repository ---------------------------------------------------------------------------
  const top = git(["rev-parse", "--show-toplevel"]).toString().trim();
  if (!top) refuse("not a git repository");
  if (git(["rev-parse", "--is-shallow-repository"], { cwd: top }).toString().trim() === "true") {
    if (release) refuse("this clone is shallow: history is incomplete, so nothing can be proven");
    console.log("check-owner-values: warning — shallow clone, older history was not checked");
  }

  const refNames = git(["for-each-ref", "--format=%(refname)%00"], { cwd: top }).toString().split("\0").map((s) => s.trim()).filter(Boolean);
  const hasHead = spawnSync("git", ["rev-parse", "-q", "--verify", "HEAD"], { cwd: top }).status === 0;
  // (rev-list -z does not NUL-separate the object records; a path with a newline in it can add a bogus
  // line, which the object listing below reports as a missing object: a refusal, never a pass.)
  const walk = git(["rev-list", "--objects", "--all", ...(hasHead ? ["HEAD"] : [])], { cwd: top }).toString("latin1").split("\n").filter(Boolean);
  const pathOf = new Map();
  for (const rec of walk) {
    const oid = rec.slice(0, rec.indexOf(" ") === -1 ? undefined : rec.indexOf(" "));
    if (rec.includes(" ") && !pathOf.has(oid)) pathOf.set(oid, Buffer.from(rec.slice(oid.length + 1), "latin1").toString("utf8"));
  }
  const oids = new Set(walk.map((r) => r.slice(0, r.indexOf(" ") === -1 ? undefined : r.indexOf(" "))));
  // Staged but not committed content is not in history yet, and is about to be.
  const pathsOf = new Map(); // blob oid -> every path it sits at (trees, index); only kept with exceptions
  const notRegular = new Set(); // blob oids stored anywhere as a symlink
  const addPath = (oid, mode, path) => {
    if (!pathsOf.has(oid)) pathsOf.set(oid, new Set());
    pathsOf.get(oid).add(path);
    if (!REGULAR_MODES.has(mode)) notRegular.add(oid);
  };
  for (const line of git(["ls-files", "-z", "-s"], { cwd: top }).toString("latin1").split("\0").filter(Boolean)) {
    const m = /^(\d+) ([0-9a-f]+) \d\t/.exec(line);
    if (m && m[1] !== "160000") oids.add(m[2]);
    if (m && exc && m[1] !== "160000") addPath(m[2], m[1], Buffer.from(line.slice(m[0].length), "latin1").toString("utf8"));
  }

  for (const name of refNames) {
    const hits = valuesInText(name);
    if (hits.length) find("ref-name", safe(name), hits);
  }

  // ---- every object -------------------------------------------------------------------------
  const ordered = [...oids];
  const batch = git(["cat-file", "--batch"], { cwd: top, input: ordered.join("\n") + "\n" });
  const counts = { commits: 0, tags: 0, trees: 0, blobs: 0, binaries: 0 };
  const roots = new Set(); // root commits in scope (exceptions are bound to one repository)
  const commitTrees = new Set();
  const treeEntries = new Map(); // tree oid -> [{ mode, name, oid }]; only kept with exceptions
  const deferred = []; // file-content hits waiting for every path of their blob to be known
  const binaries = []; // {oid, size, digest, data}
  let at = 0;
  for (const oid of ordered) {
    const eol = batch.indexOf(10, at);
    if (eol === -1) refuse("object listing ended early");
    const header = batch.subarray(at, eol).toString("latin1").split(" ");
    if (header[1] === "missing") refuse("an object is missing from this repository, so it cannot be checked");
    const size = Number(header[2]);
    if (header[0] !== oid || !Number.isInteger(size)) refuse("object listing is malformed");
    const data = batch.subarray(eol + 1, eol + 1 + size);
    if (data.length !== size) refuse("object listing ended early");
    at = eol + 1 + size + 1;
    const type = header[1];
    const where = `${type} ${oid.slice(0, 10)}`;

    if (type === "commit" || type === "tag") {
      counts[type === "commit" ? "commits" : "tags"]++;
      // The tree, parent and object lines are hex ids; a short value would match inside them.
      const text = data.toString("latin1");
      const cut = text.indexOf("\n\n");
      const headLines = (cut === -1 ? text : text.slice(0, cut)).split("\n");
      if (type === "commit") {
        if (!headLines.some((l) => /^parent [0-9a-f]+$/.test(l))) roots.add(oid);
        const t = /^tree ([0-9a-f]+)$/.exec(headLines[0] ?? "");
        if (t) commitTrees.add(t[1]);
      }
      const head = headLines.filter((l) => !/^(tree|parent|object) [0-9a-f]+$/.test(l)).join("\n");
      const hits = valuesIn(Buffer.concat([Buffer.from(head, "latin1"), Buffer.from("\n\n"), data.subarray(cut === -1 ? data.length : cut + 2)]));
      if (hits.length) find(`${type}-metadata`, where, hits);
    } else if (type === "tree") {
      counts.trees++;
      let i = 0;
      while (i < data.length) {
        const sp = data.indexOf(32, i);
        const nul = data.indexOf(0, sp);
        if (sp === -1 || nul === -1 || nul + 21 > data.length) refuse("a tree is malformed");
        const hits = valuesIn(data.subarray(sp + 1, nul));
        if (hits.length) find("file-name", where, hits);
        if (exc) {
          if (!treeEntries.has(oid)) treeEntries.set(oid, []);
          treeEntries.get(oid).push({ mode: data.toString("latin1", i, sp), name: data.toString("utf8", sp + 1, nul), oid: data.subarray(nul + 1, nul + 21).toString("hex") });
        }
        i = nul + 21;
      }
    } else if (type === "blob") {
      counts.blobs++;
      const hits = valuesIn(data);
      const known = pathOf.get(oid);
      const label = `${where}${known ? ` (${safe(known)})` : ""}`;
      if (hits.length && exc) deferred.push({ oid, label, data, hits });
      else if (hits.length) find("file-content", label, hits);
      if (data.subarray(0, 8000).includes(0)) {
        counts.binaries++;
        binaries.push({ oid, label, size, digest: sha256(data), data });
      }
      if (data.subarray(0, 8).equals(PNG_SIGNATURE)) {
        const png = readPng(data);
        for (const text of png.texts) {
          const inner = valuesIn(text);
          if (inner.length) find("png-text-value", label, inner);
        }
      }
    }
  }

  // ---- reviewed value exceptions: bound to one repository, then applied per path ----------------
  if (exc) {
    // Foreign history in scope (another root) or no root at all (a shallow edge) means the file
    // is being used on something it was not reviewed for.
    if (roots.size !== 1 || !roots.has(exc.root)) refuse("the value-exceptions file is bound to a different repository");
    if (deferred.length) {
      const child = new Set();
      for (const list of treeEntries.values()) for (const en of list) if (en.mode === "40000") child.add(en.oid);
      const seenTrees = new Set();
      const visit = (tree, prefix) => {
        const key = `${tree}\0${prefix}`;
        if (seenTrees.has(key)) return;
        seenTrees.add(key);
        for (const en of treeEntries.get(tree) ?? []) {
          if (en.mode === "40000") visit(en.oid, `${prefix}${en.name}/`);
          else if (en.mode !== "160000") addPath(en.oid, en.mode, `${prefix}${en.name}`);
        }
      };
      // every commit's root tree, and any tree nothing else contains (a tag may point at one)
      for (const tree of new Set([...commitTrees, ...[...treeEntries.keys()].filter((t) => !child.has(t))])) visit(tree, "");
      for (const d of deferred) {
        const used = excuse(pathsOf.get(d.oid) ?? new Set(), d.data, d.hits, !notRegular.has(d.oid));
        if (used) tally(used, "object");
        else find("file-content", d.label, d.hits);
      }
    }
  }

  // ---- index paths and the working tree -----------------------------------------------------
  const files = git(["ls-files", "-z", "--cached", "--others", "--exclude-standard"], { cwd: top }).toString("utf8").split("\0").filter(Boolean);
  let scannedFiles = 0;
  let removed = 0;
  for (const path of new Set(files)) {
    const pathHits = valuesInText(path);
    if (pathHits.length) find("file-name", `worktree ${safe(path)}`, pathHits);
    let data;
    const full = `${top}/${path}`;
    try {
      const st = lstatSync(full);
      if (st.isDirectory()) continue; // a submodule's folder: its own repository's job
      data = st.isSymbolicLink() ? Buffer.from(readlinkSync(full)) : readFileSync(full);
    } catch (e) {
      if (e && e.code === "ENOENT") {
        removed++; // tracked and deleted from the working tree: its content is in the objects
        continue;
      }
      refuse("a working-tree file cannot be read, so it cannot be checked");
    }
    scannedFiles++;
    const hits = valuesIn(data);
    if (hits.length) {
      const used = excuse(new Set([path]), data, hits, !lstatSync(full).isSymbolicLink());
      if (used) tally(used, "file");
      else find("worktree-content", safe(path), hits);
    }
  }

  // ---- reviewed-binary gate -----------------------------------------------------------------
  let gated = "not gated (default mode)";
  if (release) {
    const manifest = readManifest(top);
    const pin = process.env.ENDORA_REVIEWED_BINARIES_SHA256 ?? "";
    if (pin && pin !== manifest.digest) refuse("the manifest does not match the out-of-band pin ENDORA_REVIEWED_BINARIES_SHA256");
    const trusted = pin !== "";
    const byDigest = new Map(manifest.entries.map((e) => [e.digest, e]));
    const seen = new Set();
    for (const b of binaries) {
      seen.add(b.digest);
      const entry = trusted ? byDigest.get(b.digest) : undefined;
      if (!entry || entry.size !== b.size) {
        find("unreviewed-binary", b.label, [], trusted ? "no matching pin (hash and size)" : "no out-of-band pin supplied, so no manifest line approves anything");
        continue;
      }
      const png = readPng(b.data);
      if (!png.ok) find("png-malformed", b.label, [], png.why);
      for (const t of png.chunks) if (!PNG_ALLOWED.has(t)) find("png-chunk", b.label, [], `chunk ${t} is not on the allowlist`);
    }
    if (trusted) for (const e of manifest.entries) if (!seen.has(e.digest)) find("stale-pin", `pin ${e.digest.slice(0, 12)}`, [], "pins a binary found in no commit");
    gated = `${binaries.length} binaries against ${manifest.entries.length} pins (${trusted ? "pinned" : "NO out-of-band pin"})`;
  }

  if (exc && release) {
    // Named by its line order in the file, never by any part of its token hash.
    exc.entries.forEach((e, i) => {
      if (e.objects + e.files === 0) find("stale-exception", `entry ${i + 1}`, [], "names a token found in no scanned file");
    });
  }
  console.log(
    `check-owner-values (${release ? "release" : "default"}): ${counts.commits} commits, ${counts.tags} tags, ${counts.trees} trees, ${counts.blobs} blobs ` +
      `(${counts.binaries} binary; ${gated}), ${refNames.length} refs, ${scannedFiles} working-tree files (${removed} deleted), ${usable.length} values (${skipped} skipped)`,
  );
  if (exc) {
    const sum = (key) => exc.entries.reduce((a, e) => a + e[key], 0);
    console.log(
      `check-owner-values: reviewed value exceptions: ${exc.entries.length} entries; ` +
        `excused ${sum("objectOccurrences")} occurrence(s) in ${excused.objects} object(s), ` +
        `${sum("fileOccurrences")} in ${excused.files} working-tree file(s)`,
    );
  }
  if (findings.length > 0) {
    console.error(`check-owner-values: ${findings.length} finding${findings.length > 1 ? "s" : ""} (values are never printed; #n is a position in ENDORA_OWNER_VALUES)`);
    for (const f of findings) console.error(f);
    return 1;
  }
  return 0;
}

// ---- reviewed value exceptions ----------------------------------------------------------------

/**
 * The out-of-band exception file, strictly parsed, or null when the variable is not set.
 * Lines (tab separated, LF, "#" comments and blank lines allowed):
 *   root <40 hex>      once: the repository root commit this file is bound to
 *   <position> <path> <sha256 of the exact URL token> <token length> <max occurrences> <review id>
 * A position is the 1-based place of a usable value in ENDORA_OWNER_VALUES. A path is exact: no
 * wildcard, no ".", "..", empty or trailing segment, no leading slash. Anything else is exit 2.
 */
function loadExceptions(valueCount) {
  const file = process.env[EXC_VARIABLE];
  if (file === undefined) return null;
  if (!file.startsWith("/") || file.includes("\0")) refuse("the value-exceptions file must be named by an absolute path");
  let st;
  try {
    st = lstatSync(file);
  } catch {
    refuse("the value-exceptions file cannot be read");
  }
  if (!st.isFile()) refuse("the value-exceptions file must be a regular file (not a link or a directory)");
  if (st.size > EXC_MAX_BYTES) refuse("the value-exceptions file is too large");
  let raw;
  try {
    raw = readFileSync(file);
  } catch {
    refuse("the value-exceptions file cannot be read");
  }
  // The owner's out-of-band digest of these exact bytes. A file that changed, or a digest that is not
  // this file's, is never used. Release mode needs one; a supplied digest that does not match always refuses.
  const pin = process.env[EXC_PIN_VARIABLE] ?? "";
  if (pin !== "" && pin !== sha256(raw)) refuse("the value-exceptions file does not match the out-of-band pin ENDORA_OWNER_VALUE_EXCEPTIONS_SHA256");
  if (pin === "" && release) refuse("release mode needs ENDORA_OWNER_VALUE_EXCEPTIONS_SHA256 to pin the value-exceptions file");
  if (raw.length > EXC_MAX_BYTES || raw.includes(0) || raw.includes(13)) refuse("the value-exceptions file is malformed");
  const text = raw.toString("utf8");
  if (!Buffer.from(text, "utf8").equals(raw)) refuse("the value-exceptions file is malformed");
  let root = null;
  const entries = [];
  const seen = new Set();
  for (const line of text.split("\n")) {
    if (line === "" || line.startsWith("#")) continue;
    const f = line.split("\t");
    if (f[0] === "root") {
      if (root !== null || f.length !== 2 || !/^[0-9a-f]{40}$/.test(f[1])) refuse("the value-exceptions file is malformed");
      root = f[1];
      continue;
    }
    const [position, path, digest, chars, max, review] = f;
    const pathOk = f.length === 6 && REVIEW_PATH.test(path) && path.split("/").every((seg) => !/^\.+$/.test(seg));
    const fieldsOk =
      /^[1-9]\d{0,2}$/.test(position) && /^[0-9a-f]{64}$/.test(digest) && /^[1-9]\d{0,3}$/.test(chars) && /^[1-9]\d?$/.test(max) && /^[A-Za-z0-9._-]{1,40}$/.test(review);
    if (!pathOk || !fieldsOk) refuse("the value-exceptions file is malformed");
    if (Number(position) > valueCount) refuse("the value-exceptions file names a value that was not given");
    if (Number(max) > 16 || Number(chars) < 8 || Number(chars) > 2048) refuse("the value-exceptions file is malformed");
    const key = `${position}\0${path}\0${digest}`;
    if (seen.has(key)) refuse("the value-exceptions file lists the same entry twice");
    seen.add(key);
    const counters = { objects: 0, objectOccurrences: 0, files: 0, fileOccurrences: 0 };
    entries.push({ position: Number(position), path, digest, chars: Number(chars), max: Number(max), ...counters });
  }
  if (root === null) refuse("the value-exceptions file has no root line");
  const byPath = new Map();
  for (const e of entries) byPath.set(e.path, [...(byPath.get(e.path) ?? []), e]);
  return { root, entries, byPath };
}

/**
 * The whole-token http(s) URL that contains text[a, b), as {start, end}, or null if it is not inside one.
 * Bounded: a run of URL characters longer than URL_WALK_MAX on either side, or a token longer than
 * URL_WALK_MAX, is never excused (the scheme or the end may lie beyond the bound).
 */
function urlTokenAround(text, a, b) {
  let left = a;
  while (left > 0 && a - left < URL_WALK_MAX && URL_CHAR.test(text[left - 1])) left--;
  if (a - left >= URL_WALK_MAX && left > 0 && URL_CHAR.test(text[left - 1])) return null;
  const lead = text.slice(left, a);
  const scheme = Math.max(lead.lastIndexOf("https://"), lead.lastIndexOf("http://"));
  if (scheme === -1) return null;
  const start = left + scheme;
  if (start > 0 && GLUED_BEFORE.test(text[start - 1])) return null;
  let end = b;
  while (end < text.length && end - b < URL_WALK_MAX && URL_CHAR.test(text[end])) end++;
  if (end - b >= URL_WALK_MAX && end < text.length && URL_CHAR.test(text[end])) return null;
  while (end > b && /[.,:;]/.test(text[end - 1])) end--;
  if (end - start > URL_WALK_MAX) return null;
  return { start, end };
}

// ---- helpers that need nothing from the repository -----------------------------------------

function crc32(buf) {
  let r = 0xffffffff;
  for (const x of buf) {
    r ^= x;
    for (let k = 0; k < 8; k++) r = r & 1 ? 0xedb88320 ^ (r >>> 1) : r >>> 1;
  }
  return (r ^ 0xffffffff) >>> 0;
}

/** Chunks of a PNG, the bytes of its text chunks once inflated, and whether it is well formed. */
function readPng(data) {
  const out = { ok: false, why: "", chunks: [], texts: [] };
  if (!data.subarray(0, 8).equals(PNG_SIGNATURE)) return { ...out, why: "no PNG signature" };
  let i = 8;
  while (i + 12 <= data.length) {
    const len = data.readUInt32BE(i);
    const type = data.subarray(i + 4, i + 8).toString("latin1");
    if (i + 12 + len > data.length) return { ...out, why: "a chunk runs past the end" };
    const body = data.subarray(i + 8, i + 8 + len);
    if (crc32(data.subarray(i + 4, i + 8 + len)) !== data.readUInt32BE(i + 8 + len)) return { ...out, why: "a chunk has a bad checksum" };
    out.chunks.push(type);
    if (PNG_TEXT.has(type)) {
      out.texts.push(body);
      try {
        const nul = body.indexOf(0);
        if (type === "zTXt") out.texts.push(inflateSync(body.subarray(nul + 2)));
        if (type === "iTXt" && body[nul + 1] === 1) {
          // keyword NUL flag method language NUL translated NUL text
          const l = body.indexOf(0, nul + 3);
          const t = body.indexOf(0, l + 1);
          out.texts.push(inflateSync(body.subarray(t + 1)));
        }
      } catch {
        // a text chunk that will not inflate is already off the allowlist
      }
    }
    i += 12 + len;
    if (type === "IEND") break;
  }
  if (out.chunks[0] !== "IHDR") return { ...out, why: "IHDR is not first" };
  if (out.chunks.at(-1) !== "IEND") return { ...out, why: "no IEND" };
  if (i !== data.length) return { ...out, why: "bytes after IEND" };
  return { ...out, ok: true };
}

/** The manifest as committed at HEAD (never the working copy), strictly parsed. */
function readManifest(top) {
  const probe = spawnSync("git", ["cat-file", "-e", `HEAD:${MANIFEST}`], { cwd: top });
  const raw = probe.status === 0 ? git(["show", `HEAD:${MANIFEST}`], { cwd: top }) : Buffer.alloc(0);
  const entries = [];
  const seen = new Set();
  for (const line of raw.toString("utf8").split("\n")) {
    if (line === "" || line.startsWith("#")) continue;
    const m = /^([0-9a-f]{64})\t(\d+)\t(png)\t([A-Za-z0-9._-]+)$/.exec(line);
    if (!m) refuse("the reviewed-binary manifest has a malformed line");
    if (seen.has(m[1])) refuse("the reviewed-binary manifest pins the same binary twice");
    seen.add(m[1]);
    entries.push({ digest: m[1], size: Number(m[2]), kind: m[3], review: m[4] });
  }
  return { digest: sha256(raw), entries };
}

try {
  process.exitCode = main();
} catch (e) {
  // Fixed text only: an error message from git or the filesystem can carry a path or a value.
  console.error(`check-owner-values: cannot verify — ${e instanceof Refuse ? e.message : `internal error${e && e.code ? ` (${e.code})` : ""}`}`);
  process.exitCode = 2;
}
