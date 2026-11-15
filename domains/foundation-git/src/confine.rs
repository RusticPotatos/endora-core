//! Confinement for untrusted commands: the macOS `sandbox-exec` profile, the Linux `bwrap`
//! argument vector, the minimal environment, process-group clean-up, and the redaction and caps
//! applied to everything that reaches append-only evidence.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Most bytes of one output field (agent output, test output, diff) kept in evidence. A longer
/// field is cut with a marker that records its full length.
pub const EVIDENCE_CAP: usize = 16 * 1024;

/// Names that mark a `NAME=value` as a secret: a name that contains one of these (any case), or
/// that ends in `_KEY`, has its value replaced. One list, here; extend it here.
pub const SECRET_NAMES: &[&str] = &[
    "TOKEN",
    "SECRET",
    "PASSWORD",
    "PASSWD",
    "API_KEY",
    "APIKEY",
    "ACCESS_KEY",
    "PRIVATE_KEY",
    "CREDENTIAL",
    "AWS_SESSION",
    "COOKIE",
];

/// Well-known secret prefixes: a word that starts with one (and is at least 16 bytes long) is
/// replaced wherever it appears, whatever the name in front of it.
pub const SECRET_PREFIXES: &[&str] = &[
    "sk-ant-",
    "sk-",
    "ghp_",
    "gho_",
    "ghs_",
    "github_pat_",
    "xoxb-",
    "xoxp-",
    "AKIA",
    "AIza",
];

fn secret_name(name: &str) -> bool {
    let up = name.to_ascii_uppercase();
    let ident = !up.is_empty()
        && up
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'));
    ident && (up.ends_with("_KEY") || up == "KEY" || SECRET_NAMES.iter().any(|s| up.contains(s)))
}

/// `text` with private-key blocks, secret `NAME=value` pairs and well-known secret tokens
/// replaced by `[redacted]`. Line structure is kept.
#[must_use]
pub fn redact(text: &str) -> String {
    let mut out = Vec::new();
    let mut in_key = false;
    for line in text.lines() {
        if in_key {
            if line.contains("-----END") {
                in_key = false;
            }
            continue;
        }
        if line.contains("-----BEGIN") && line.contains("PRIVATE KEY") {
            out.push("[redacted: private key]".to_owned());
            in_key = !line.contains("-----END");
            continue;
        }
        out.push(redact_line(line));
    }
    out.join("\n")
}

fn redact_line(line: &str) -> String {
    let words: Vec<&str> = line.split(' ').collect();
    let mut kept: Vec<String> = Vec::with_capacity(words.len());
    let mut i = 0;
    while i < words.len() {
        let w = words[i];
        i += 1;
        if let Some((name, value)) = w.split_once('=') {
            if secret_name(name) {
                // A quoted value may hold spaces: swallow words until the closing quote.
                if let Some(q) = value.chars().next().filter(|c| matches!(c, '"' | '\'')) {
                    let mut closed = value.len() > 1 && value.ends_with(q);
                    while !closed && i < words.len() {
                        closed = words[i].ends_with(q);
                        i += 1;
                    }
                }
                kept.push(format!("{name}=[redacted]"));
                continue;
            }
        }
        let bare = w.trim_matches(|c: char| matches!(c, '"' | '\'' | ',' | ';' | '(' | ')'));
        if bare.len() >= 16 && SECRET_PREFIXES.iter().any(|p| bare.starts_with(p)) {
            kept.push(w.replace(bare, "[redacted]"));
        } else {
            kept.push(w.to_owned());
        }
    }
    kept.join(" ")
}

/// `text` redacted, then cut to [`EVIDENCE_CAP`] bytes with a marker naming how much there was.
#[must_use]
pub fn cap_text(text: &str) -> String {
    finish(&redact(text), text.len() as u64, text.len() > EVIDENCE_CAP)
}

fn finish(redacted: &str, total: u64, cut_before: bool) -> String {
    let mut end = redacted.len().min(EVIDENCE_CAP);
    while !redacted.is_char_boundary(end) {
        end -= 1;
    }
    if end < redacted.len() || cut_before {
        format!(
            "{}\n[truncated: kept {end} of {total} bytes]",
            &redacted[..end]
        )
    } else {
        redacted.to_owned()
    }
}

/// The first [`EVIDENCE_CAP`] bytes of a log file, redacted, with a marker if there was more.
#[must_use]
pub fn read_capped(path: &Path) -> String {
    let Ok(f) = File::open(path) else {
        return String::new();
    };
    let total = f.metadata().map_or(0, |m| m.len());
    let mut b = Vec::new();
    let _ = f.take(EVIDENCE_CAP as u64).read_to_end(&mut b);
    let more = total > b.len() as u64;
    if more {
        // Never keep half a line: a secret cut in the middle would escape the line rules.
        if let Some(nl) = b.iter().rposition(|c| *c == b'\n') {
            b.truncate(nl + 1);
        }
    }
    let text = String::from_utf8_lossy(&b).into_owned();
    let mut r = redact(&text);
    if more {
        r = format!("{r}\n[truncated: kept {} of {total} bytes]", b.len());
    }
    r
}

/// How untrusted commands are confined.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confinement {
    /// Pick what this machine has, in this order: macOS `sandbox-exec`; Linux `bwrap` if a real
    /// probe with the real flags passes; Docker if the daemon answers and the executor image is
    /// present locally; else fail closed.
    Auto,
    /// macOS `sandbox-exec`: no writes outside the job directory, no reads outside it and the
    /// system paths, no network.
    MacSandbox,
    /// Linux bubblewrap: a root that holds only read-only system paths and the writable job
    /// directory, no network. NOT exercised by this slice's tests (written on a Mac); the
    /// argument list is unit-tested.
    Bubblewrap,
    /// A plain unprivileged Docker container per executor call: only the job directory is
    /// mounted, no network, read-only root, no capabilities, bounded pids, memory and cpu. For
    /// hosts where `bwrap` is missing or user namespaces are restricted. The Docker daemon is
    /// root: whoever may start containers is trusted (see `docs/foundation/README.md`).
    Docker,
    /// Nothing available: untrusted commands are refused.
    Unavailable,
}

/// What a confined command may additionally read (read-only), beyond its job directory and the
/// system paths.
#[derive(Debug, Clone, Default)]
pub struct Reads {
    /// Whole directories (for example a toolchain the job config names).
    pub dirs: Vec<PathBuf>,
    /// Single files (for example the agent script named in the config).
    pub files: Vec<PathBuf>,
    /// Paths denied by name after the system allow, whatever prefix they lie under (the owner's
    /// database and token file; the macOS profile adds the `-journal`, `-wal` and `-shm` files of
    /// each). The scripted and the real-agent profile both honour it.
    pub deny: Vec<PathBuf>,
}

/// System paths every command may read (read-only on both platforms).
const SYSTEM_READ: [&str; 7] = [
    "/usr",
    "/bin",
    "/sbin",
    "/System",
    "/Library",
    "/private/var/db",
    "/dev",
];

/// Places the macOS profile denies explicitly (it also denies everything not allowed, so these
/// are belt and braces against a later allow that happens to cover them).
const MAC_DENY: [&str; 13] = [
    "/Library/Keychains",
    // Where a package manager or an installer keeps service data and configuration (an owner may
    // keep a database, a token or a backup there): denied as a whole, not left to file placement.
    "/Library/Application Support",
    "/usr/local/var",
    "/usr/local/etc",
    "/opt/homebrew/var",
    "/opt/homebrew/etc",
    "/Users",
    "/private/tmp",
    "/tmp",
    "/private/var/folders",
    "/private/var/root",
    "/var/root",
    "/Volumes",
];

/// Operations denied because the profile starts from `(allow default)`: each one lets a confined
/// process ask a service that runs outside the sandbox to act for it (weekly review B, WF-01 and
/// WF-04). Each line was tried on its own on macOS 26: `(deny lsopen)` is the one that stops
/// LaunchServices from starting an applet the agent built (the other two alone did not);
/// `(deny appleevent-send)` and the LaunchServices mach lookup are belt and braces for asking an
/// already running app to act; `user-preference-*` or the cfprefsd lookups alone each stop
/// preference reads and writes (both are kept); the pasteboard lookup is the only line that closes
/// the pasteboard. Mach lookups and exec otherwise stay open (see TRUST-BOUNDARY.md).
const MAC_ROUTE_DENY: &str = concat!(
    "(deny lsopen)(deny appleevent-send)",
    "(deny user-preference-read)(deny user-preference-write)",
    "(deny mach-lookup",
    " (global-name \"com.apple.coreservices.launchservicesd\")",
    " (global-name \"com.apple.cfprefsd.daemon\")",
    " (global-name \"com.apple.cfprefsd.agent\")",
    " (global-name \"com.apple.pasteboard.1\"))",
    // The Shortcuts service (shortcuts run outside the sandbox, as the owner).
    "(deny mach-lookup (global-name-prefix \"com.apple.siri\"))",
    // A speed bump only, not a boundary: `launchctl print` shows other launchd jobs' environment
    // variables, and no mach-lookup or other operation deny closed that (tried); a process can
    // still reach launchd without this binary.
    "(deny process-exec (literal \"/bin/launchctl\"))",
);

/// The parts of `/opt/homebrew` a confined command may read.
const HOMEBREW_READ: [&str; 5] = ["bin", "lib", "opt", "Cellar", "share"];

/// `/opt/homebrew/etc` is denied, but Homebrew's Node reads its OpenSSL configuration there and
/// exits with an error without it; this one directory is allowed again after the deny.
const HOMEBREW_OPENSSL: &str = "/opt/homebrew/etc/openssl@3";

/// A path as an SBPL string literal body: backslash and double quote escaped, so a path can never
/// end the literal and add rules of its own (weekly review A, WF-A-25).
fn esc(p: &Path) -> String {
    p.to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
}

fn q(p: &Path) -> String {
    format!("(subpath \"{}\")", esc(p))
}

/// `(deny file-read-data ...)` for paths named by the owner, each with its SQLite sidecar files
/// (`-journal`, `-wal`, `-shm`); empty when there is nothing to deny.
fn deny_by_name(paths: &[PathBuf]) -> String {
    use std::fmt::Write as _;
    if paths.is_empty() {
        return String::new();
    }
    let mut out = String::from("(deny file-read-data");
    for d in paths {
        let _ = write!(out, " {}", q(d));
        for side in ["-journal", "-wal", "-shm"] {
            let mut n = d.clone().into_os_string();
            n.push(side);
            let _ = write!(out, " {}", q(Path::new(&n)));
        }
    }
    out.push(')');
    out
}

/// The macOS Data volume's firmlink prefix: a path and the same path with this prefix in front are
/// the same file, and the sandbox checks the path it is given, so both spellings must be denied.
const DATA_VOLUME: &str = "/System/Volumes/Data";

fn same_file(a: &Path, b: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    // Compare the nearest existing ancestors (the tails are the same names).
    let (mut a, mut b) = (a.to_path_buf(), b.to_path_buf());
    while !a.exists() || !b.exists() {
        if !a.pop() || !b.pop() {
            return false;
        }
    }
    match (a.metadata(), b.metadata()) {
        (Ok(x), Ok(y)) => x.dev() == y.dev() && x.ino() == y.ino(),
        _ => false,
    }
}

/// `p` in both spellings: the plain one (without the Data-volume prefix) first when it is the same
/// file as the prefixed one, then the other. A path that is not on the Data volume gets the
/// prefixed spelling added (denying a spelling that does not exist is harmless).
fn both_volume_spellings(p: &Path) -> Vec<PathBuf> {
    if let Ok(rest) = p.strip_prefix(DATA_VOLUME) {
        let plain = Path::new("/").join(rest);
        if rest.as_os_str().is_empty() {
            return vec![p.to_path_buf()];
        }
        return if same_file(&plain, p) {
            vec![plain, p.to_path_buf()]
        } else {
            vec![p.to_path_buf(), plain]
        };
    }
    let data = Path::new(DATA_VOLUME).join(p.strip_prefix("/").unwrap_or(p));
    vec![p.to_path_buf(), data]
}

/// Every spelling of each path that the sandbox could be asked to open it by: the canonical path
/// first (absolute, symlinks resolved, `.`, `..` and `//` gone; a file that does not exist yet
/// gets its nearest existing ancestor canonicalised), then the lexical absolute path and the path
/// as typed when they differ. The sandbox matches the canonical path, so the first one is the one
/// that denies; the others are belt and braces.
#[must_use]
pub fn canonical_deny_paths(paths: &[PathBuf]) -> Vec<PathBuf> {
    let cwd = std::env::current_dir().unwrap_or_default();
    let mut out: Vec<PathBuf> = Vec::new();
    let mut add = |p: PathBuf| {
        if !out.contains(&p) {
            out.push(p);
        }
    };
    for typed in paths {
        let abs = if typed.is_absolute() {
            typed.clone()
        } else {
            cwd.join(typed)
        };
        // Lexical normalisation: no `.`, no empty segments, `..` pops a segment.
        let mut lex = PathBuf::new();
        for c in abs.components() {
            match c {
                std::path::Component::CurDir => {}
                std::path::Component::ParentDir => {
                    lex.pop();
                }
                other => lex.push(other),
            }
        }
        // Canonical: the whole path if it exists, else the nearest existing ancestor plus the rest.
        let mut tail = Vec::new();
        let mut head = abs.clone();
        let canonical = loop {
            if let Ok(c) = head.canonicalize() {
                break Some(
                    tail.iter()
                        .rev()
                        .fold(c, |acc: PathBuf, t: &std::ffi::OsString| acc.join(t)),
                );
            }
            match (head.file_name().map(ToOwned::to_owned), head.parent()) {
                (Some(n), Some(par)) if n != ".." => {
                    tail.push(n);
                    head = par.to_path_buf();
                }
                _ => break None,
            }
        };
        let mut candidates: Vec<PathBuf> = canonical.into_iter().collect();
        candidates.push(lex);
        if typed.is_absolute() {
            candidates.push(typed.clone());
        }
        for c in candidates {
            // A spelling the profile could not match (a `..` segment, say) is dropped: the
            // canonical one covers it.
            for spelling in both_volume_spellings(&c) {
                if check_deny_path(&spelling).is_ok() {
                    add(spelling);
                }
            }
        }
    }
    out
}

/// Rejects a deny path the profile would not match: it must be absolute and have no `.` or `..`
/// segment (use [`canonical_deny_paths`]).
///
/// # Errors
/// Names the path.
pub fn check_deny_path(p: &Path) -> Result<(), String> {
    // `Path::components` hides `.` and `//` inside a path, so the text itself is checked.
    let text = p.to_string_lossy();
    let plain = text.strip_prefix('/').is_some_and(|rest| {
        rest.split('/')
            .all(|seg| !seg.is_empty() && seg != "." && seg != "..")
    });
    if plain {
        Ok(())
    } else {
        Err(format!(
            "the denied path {} is not absolute and normalised: the sandbox would not match it",
            p.display()
        ))
    }
}

/// Refuses a layout where a by-name deny cannot hold: a denied path inside a directory the job may
/// write. The profile denies reading only, so an agent that may write in that directory can rename
/// the file, or a parent directory of it, to a name that is not denied and read it there. Fail
/// closed instead of claiming a deny. The Data-volume prefix is ignored when comparing.
///
/// # Errors
/// Names the denied path and the writable directory.
pub fn check_deny_outside_writable(deny: &[PathBuf], writable: &[&Path]) -> Result<(), String> {
    let plain = |p: &Path| -> PathBuf {
        p.strip_prefix(DATA_VOLUME)
            .map_or_else(|_| p.to_path_buf(), |r| Path::new("/").join(r))
    };
    for d in deny {
        for w in writable {
            if plain(d).starts_with(plain(w)) {
                return Err(format!(
                    "refusing to start: the denied path {} lies inside {}, where the job may write; \
                     a denied name cannot hold against a rename, so keep the database and the token \
                     outside every job directory",
                    d.display(),
                    w.display()
                ));
            }
        }
    }
    Ok(())
}

/// Rejects a path the macOS profile cannot carry safely: not UTF-8, or holding a control
/// character (a newline would end the rule). Quotes and backslashes are escaped, not rejected.
///
/// # Errors
/// Names the path and the reason.
pub fn check_profile_path(p: &Path) -> Result<(), String> {
    let Some(s) = p.to_str() else {
        return Err(format!(
            "the path {} is not UTF-8: the sandbox profile cannot carry it",
            p.display()
        ));
    };
    if s.chars().any(char::is_control) {
        return Err(format!(
            "the path {s:?} holds a control character: the sandbox profile cannot carry it"
        ));
    }
    Ok(())
}

/// Would the macOS profile let a confined command read `p`, before any by-name deny? (A
/// start-up warning uses it: a database, token or backup kept there is placement-dependent.)
#[must_use]
pub fn mac_profile_can_read(p: &Path) -> Option<String> {
    let denied = MAC_DENY.iter().any(|d| p.starts_with(d));
    if denied {
        return None;
    }
    let homebrew = HOMEBREW_READ.map(|d| format!("/opt/homebrew/{d}"));
    SYSTEM_READ
        .iter()
        .map(|s| (*s).to_owned())
        .chain(homebrew)
        .find(|a| p.starts_with(a))
}

/// The macOS `sandbox-exec` profile for a job directory (already canonical).
#[must_use]
pub fn mac_profile(dir: &Path, reads: &Reads) -> String {
    use std::fmt::Write as _;
    // `(deny signal (target others))`: the agent may signal itself, its children and its own
    // process group (the call's group kill), but not the engine or any other process.
    let mut p = String::from(
        "(version 1)(allow default)(deny signal (target others))(deny network*)(deny file-write*)",
    );
    p.push_str(MAC_ROUTE_DENY);
    let _ = write!(
        p,
        "(allow file-write* {} (literal \"/dev/null\") (literal \"/dev/dtracehelper\"))",
        q(dir)
    );
    // Contents and listings are denied everywhere, then opened for the system, then closed
    // again for the places that hold secrets, then opened for the job and the named reads.
    // Metadata (stat) stays open: path walking needs it and it holds no content.
    p.push_str("(deny file-read-data)");
    // `(literal "/")` is the root directory itself (the dynamic loader opens it), not its content.
    p.push_str("(allow file-read-data (literal \"/\")");
    for s in SYSTEM_READ {
        let _ = write!(p, " (subpath \"{s}\")");
    }
    // Only the toolchain parts of Homebrew, never its `var` or `etc` (service data, configuration).
    if Path::new("/opt/homebrew").exists() {
        for d in HOMEBREW_READ {
            let _ = write!(p, " (subpath \"/opt/homebrew/{d}\")");
        }
    }
    p.push(')');
    p.push_str("(deny file-read-data");
    for d in MAC_DENY {
        let _ = write!(p, " (subpath \"{d}\")");
    }
    p.push(')');
    if Path::new("/opt/homebrew").exists() {
        let _ = write!(p, "(allow file-read-data (subpath \"{HOMEBREW_OPENSSL}\"))");
    }
    let _ = write!(p, "(allow file-read-data {}", q(dir));
    for d in &reads.dirs {
        let _ = write!(p, " {}", q(d));
    }
    for f in &reads.files {
        let _ = write!(p, " (literal \"{}\")", esc(f));
    }
    p.push(')');
    // Last of all (the last matching rule wins): the owner's database and token stay denied even
    // when they lie inside a named read directory (read-only, so nothing can rename them away).
    // NOT inside the job directory: the job may write there and could rename the file or its parent
    // out of the denied name; `run` refuses that layout (`check_deny_outside_writable`).
    p.push_str(&deny_by_name(&reads.deny));
    p
}

/// Which sandbox profile a command runs under.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Profile {
    /// No network, reads only the job, the system and the named files (scripted agents, tests,
    /// our own git calls).
    #[default]
    Scripted,
    /// A real agent CLI: outbound HTTPS and DNS only, plus reads of the system paths; the
    /// listed paths (the owner's secrets: home folders, the token file, the database) are
    /// denied by name as well as by the general home deny.
    RealAgent {
        /// Paths denied by name, in addition to `/Users`.
        deny: Vec<PathBuf>,
    },
}

impl Profile {
    /// The word recorded in evidence.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Scripted => "scripted-agent",
            Self::RealAgent { .. } => "real-agent",
        }
    }
}

/// The macOS profile for a real agent CLI (it starts from [`mac_profile`], so signals to other
/// processes are denied too). It differs from [`mac_profile`] in three ways: it
/// allows outbound TCP to port 443 and the DNS resolver socket (nothing inbound, no loopback
/// listener, no other port), it can read `/private/etc` (certificates, resolver settings), and
/// it denies the named secret paths explicitly before it opens the job and the named reads.
#[must_use]
pub fn mac_profile_real_agent(dir: &Path, reads: &Reads, deny: &[PathBuf]) -> String {
    use std::fmt::Write as _;
    let base = mac_profile(dir, &Reads::default());
    // `base` ends with the job-only read allow; rebuild the tail so the explicit denies sit
    // before the final allows (last match wins).
    let cut = base
        .rfind("(allow file-read-data (subpath")
        .unwrap_or(base.len());
    let mut p = base[..cut].to_owned();
    p.push_str(
        "(allow network-outbound (remote tcp \"*:443\") (literal \"/private/var/run/mDNSResponder\"))",
    );
    p.push_str("(allow file-read-data (subpath \"/private/etc\"))");
    p.push_str(&deny_by_name(deny));
    let _ = write!(p, "(allow file-read-data {}", q(dir));
    for d in &reads.dirs {
        let _ = write!(p, " {}", q(d));
    }
    for f in &reads.files {
        let _ = write!(p, " (literal \"{}\")", esc(f));
    }
    p.push(')');
    // Last of all (the last matching rule wins): the owner's database and token stay denied even
    // when they lie inside a named read directory (read-only, so nothing can rename them away).
    // NOT inside the job directory: the job may write there and could rename the file or its parent
    // out of the denied name; `run` refuses that layout (`check_deny_outside_writable`).
    p.push_str(&deny_by_name(&reads.deny));
    p
}

/// The Linux `bwrap` arguments for a job directory: a new root that holds only read-only system
/// paths, an empty tmpfs where home directories would be, no network, and the job directory as
/// the one writable path. Ends with `--` so the command can follow.
#[must_use]
pub fn bwrap_args(dir: &Path, cwd: &Path, reads: &Reads) -> Vec<String> {
    let mut a: Vec<String> = [
        "--die-with-parent",
        "--new-session",
        "--unshare-net",
        "--unshare-pid",
    ]
    .map(String::from)
    .to_vec();
    for s in ["/usr", "/bin", "/sbin", "/lib", "/lib32", "/lib64"] {
        a.extend(["--ro-bind-try".into(), s.into(), s.into()]);
    }
    for s in ["/etc/ld.so.cache", "/etc/alternatives"] {
        a.extend(["--ro-bind-try".into(), s.into(), s.into()]);
    }
    a.extend([
        "--proc".into(),
        "/proc".into(),
        "--dev".into(),
        "/dev".into(),
    ]);
    for t in ["/tmp", "/home", "/root"] {
        a.extend(["--tmpfs".into(), t.into()]);
    }
    let d = dir.display().to_string();
    a.extend(["--bind".into(), d.clone(), d]);
    for r in reads.dirs.iter().chain(&reads.files) {
        let r = r.display().to_string();
        a.extend(["--ro-bind-try".into(), r.clone(), r]);
    }
    a.extend(["--chdir".into(), cwd.display().to_string(), "--".into()]);
    a
}

/// What a command is run with.
pub struct Job<'a> {
    /// The confinement in force (never `Auto`).
    pub confinement: Confinement,
    /// The one writable (and readable) directory, canonical.
    pub dir: &'a Path,
    /// The working directory.
    pub cwd: &'a Path,
    /// Program and arguments.
    pub argv: &'a [String],
    /// The whole environment the command gets.
    pub env: &'a [(String, String)],
    /// Extra read-only paths.
    pub reads: &'a Reads,
    /// Where stdout and stderr go (outside anything the command can write).
    pub logs: (&'a Path, &'a Path),
    /// How long it may run.
    pub limit: Duration,
    /// The Docker backend's executor image (unused by the others).
    pub image: &'a str,
    /// Where the Docker backend records the containers it starts, so a later start can remove
    /// any a killed Core left behind (unused by the others).
    pub records: &'a Path,
    /// Why no backend was available (used in the refusal when `confinement` is `Unavailable`).
    pub unavailable: &'a str,
    /// The sandbox profile.
    pub profile: &'a Profile,
}

/// What a finished command left.
pub struct Ran {
    /// Exit code, if it exited.
    pub code: Option<i32>,
    /// It was stopped for taking too long.
    pub timed_out: bool,
    /// Where stdout went.
    pub out_path: PathBuf,
    /// Where stderr went.
    pub err_path: PathBuf,
}

impl Ran {
    /// Everything the command printed (stdout, then stderr), redacted and capped per field.
    #[must_use]
    pub fn all(&self) -> String {
        let (out, err) = (read_capped(&self.out_path), read_capped(&self.err_path));
        if err.trim().is_empty() {
            out.trim().to_owned()
        } else {
            format!("{}\n[stderr]\n{}", out.trim(), err.trim())
        }
    }
    /// The last line of stdout is exactly `marker`.
    #[must_use]
    pub fn ends_with_line(&self, marker: &str) -> bool {
        let Ok(mut f) = File::open(&self.out_path) else {
            return false;
        };
        let len = f.metadata().map_or(0, |m| m.len());
        let _ = f.seek(SeekFrom::Start(len.saturating_sub(512)));
        let mut b = Vec::new();
        let _ = f.read_to_end(&mut b);
        String::from_utf8_lossy(&b).trim_end().lines().last() == Some(marker)
    }
}

impl Drop for Ran {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.out_path);
        let _ = std::fs::remove_file(&self.err_path);
    }
}

/// Kills every process in the group `pid` led, with the shell's own `kill` builtin (no external
/// `kill` binary needed, and `unsafe` stays forbidden). A group that is already gone is not an
/// error; anything else is, and the caller must surface it.
fn kill_group(pid: u32) -> Result<(), String> {
    let out = Command::new("sh")
        .args(["-c", "kill -s KILL -- \"-$1\"", "sh", &pid.to_string()])
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("cannot run sh to stop process group {pid}: {e}"))?;
    if out.status.success() {
        return Ok(());
    }
    let said = String::from_utf8_lossy(&out.stderr).into_owned();
    if said.contains("No such process") {
        return Ok(());
    }
    Err(format!("cannot kill process group {pid}: {}", said.trim()))
}

/// Stops the command's process group; if that fails, kills the direct child as a last resort and
/// reports the failure, so a call never hangs on something it could not stop.
fn stop(child: &mut std::process::Child) -> Result<(), String> {
    let r = kill_group(child.id());
    // Always the child itself too: a group kill that found no group (ESRCH is not an error)
    // must not leave `wait` blocked on a live child.
    let _ = child.kill();
    let _ = child.wait();
    r
}

/// A random 64-bit hex string for a container name.
fn random_name() -> String {
    let mut b = [0u8; 8];
    if let Ok(mut f) = File::open("/dev/urandom") {
        let _ = f.read_exact(&mut b);
    }
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    let hex: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!("frx-{hex}{n:08x}")
}

/// A container name this code made: `frx-` and lower-case hex, nothing else.
fn our_name(n: &str) -> bool {
    n.strip_prefix("frx-")
        .is_some_and(|h| !h.is_empty() && h.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f')))
}

/// The `docker` binary on the parent's `PATH`, as an absolute path.
fn docker_bin() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join("docker"))
        .find(|p| p.is_file())
}

/// First line of what a probe command said, for the "tried and failed" message.
fn first_line(b: &[u8]) -> String {
    String::from_utf8_lossy(b)
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("no output")
        .trim()
        .chars()
        .take(160)
        .collect()
}

/// The default executor image of the Docker backend (a job needing git, node and so on names its
/// own through `Options::executor_image` or `ENDORA_FOUNDATION_EXECUTOR_IMAGE`).
pub const DEFAULT_IMAGE: &str = "alpine:3.23";

/// Picks a confinement for this machine: macOS `sandbox-exec`; Linux `bwrap` if a REAL probe with
/// the real flags passes; Docker if the daemon answers and `image` exists locally; else
/// [`Confinement::Unavailable`]. The second value names each backend tried and why it failed.
#[must_use]
pub fn detect(image: &str) -> (Confinement, String) {
    let mut tried: Vec<String> = Vec::new();
    match Command::new("sandbox-exec")
        .args(["-p", "(version 1)(allow default)", "true"])
        .output()
    {
        Ok(o) if o.status.success() => return (Confinement::MacSandbox, "sandbox-exec".into()),
        Ok(o) => tried.push(format!("sandbox-exec: {}", first_line(&o.stderr))),
        Err(e) => tried.push(format!("sandbox-exec: {e}")),
    }
    // The real argument vector, not `--ro-bind / / true`: a host can pass the cheap probe and
    // still refuse the namespaces, /proc and /dev the real profile needs.
    let probe_dir = std::env::temp_dir().join(format!("endora-bwrap-probe-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&probe_dir);
    let bw = Command::new("bwrap")
        .args(bwrap_args(&probe_dir, &probe_dir, &Reads::default()))
        .arg("true")
        .stdin(Stdio::null())
        .output();
    let _ = std::fs::remove_dir_all(&probe_dir);
    match bw {
        Ok(o) if o.status.success() => return (Confinement::Bubblewrap, "bwrap".into()),
        Ok(o) => tried.push(format!(
            "bwrap: the real flags failed ({})",
            first_line(&o.stderr)
        )),
        Err(e) => tried.push(format!("bwrap: {e}")),
    }
    match docker_ready(image) {
        Ok(()) => return (Confinement::Docker, format!("docker, image {image}")),
        Err(e) => tried.push(format!("docker: {e}")),
    }
    (
        Confinement::Unavailable,
        format!("tried {}", tried.join("; ")),
    )
}

/// The daemon answers and the image is present locally (nothing is ever pulled).
fn docker_ready(image: &str) -> Result<(), String> {
    let bin = docker_bin().ok_or("no `docker` on PATH")?;
    let v = Command::new(&bin)
        .args(["version", "--format", "{{.Server.Version}}"])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| e.to_string())?;
    if !v.status.success() {
        return Err(format!(
            "the daemon does not answer ({})",
            first_line(&v.stderr)
        ));
    }
    let i = Command::new(&bin)
        .args(["image", "inspect", "--format", "{{.Id}}", image])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| e.to_string())?;
    if !i.status.success() {
        return Err(format!(
            "image `{image}` is not present locally (never pulled)"
        ));
    }
    // The git capability runs `git` itself in the container: an image without it cannot serve.
    let g = Command::new(&bin)
        .args([
            "run",
            "--rm",
            "--pull",
            "never",
            "--network",
            "none",
            "--entrypoint",
            "sh",
            image,
            "-c",
            "command -v git",
        ])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| e.to_string())?;
    if !g.status.success() {
        return Err(format!(
            "image `{image}` has no `git` (the git capability runs git in the container: set \
             `Options::executor_image` or ENDORA_FOUNDATION_EXECUTOR_IMAGE to an image that has it)"
        ));
    }
    Ok(())
}

/// What one container run needs (see [`docker_args`]).
#[derive(Clone, Copy)]
pub struct DockerCall<'a> {
    /// The container name (`frx-` and hex).
    pub name: &'a str,
    /// The executor image.
    pub image: &'a str,
    /// The job directory: the one mount.
    pub dir: &'a Path,
    /// The working directory inside it.
    pub cwd: &'a Path,
    /// The (uid, gid) the command runs as (never root).
    pub user: (u32, u32),
    /// The file holding the allow-listed `NAME=value` lines.
    pub env_file: &'a Path,
    /// The names in that file: the only variables the command keeps (the image's own `ENV`
    /// and the runtime's `HOSTNAME` are removed before it starts).
    pub env_names: &'a [String],
    /// The wall-clock limit (the in-container `timeout` is one second longer).
    pub limit: Duration,
}

/// The `docker run` arguments for one executor call. Only `dir` is mounted (at the same path it
/// has on the host, so every path the job's environment and arguments already hold is valid);
/// nothing else: no Core database, no token file, no home, no Docker socket.
#[must_use]
pub fn docker_args(c: &DockerCall<'_>, argv: &[String]) -> Vec<String> {
    let DockerCall {
        name,
        image,
        dir,
        cwd,
        user,
        env_file,
        env_names,
        limit,
    } = *c;
    let d = dir.display().to_string();
    let mut a: Vec<String> = [
        "run",
        "--rm",
        "--pull",
        "never",
        "--name",
        name,
        "--network",
        "none",
        "--read-only",
        "--cap-drop",
        "ALL",
        "--security-opt",
        "no-new-privileges",
        "--pids-limit",
        "128",
        "--memory",
        "512m",
        "--memory-swap",
        "512m",
        "--cpus",
        "1",
        "--user",
    ]
    .map(String::from)
    .to_vec();
    a.push(format!("{}:{}", user.0, user.1));
    a.extend(["--tmpfs", "/tmp:rw,noexec,nosuid,size=64m", "--init", "-w"].map(String::from));
    a.push(cwd.display().to_string());
    a.extend(["-v".into(), format!("{d}:{d}:rw")]);
    a.extend(["--env-file".into(), env_file.display().to_string()]);
    a.push(image.to_owned());
    // The inner script first drops every variable that is not an allowed name (the image's own
    // `ENV`, `HOSTNAME`), then runs the command under `timeout`: the belt to `docker kill`'s
    // braces, so even a Core killed outright leaves a container that ends itself one second
    // after the limit. Everything is positional, so nothing needs quoting.
    a.extend(
        [
            "sh",
            "-c",
            "k=\" $1 \"; s=$2; shift 2; for v in $(env | sed -n 's/^\\([A-Za-z_][A-Za-z0-9_]*\\)=.*/\\1/p'); do case \"$k\" in *\" $v \"*) ;; *) unset \"$v\" 2>/dev/null;; esac; done; if command -v timeout >/dev/null 2>&1; then exec timeout -s KILL \"$s\" \"$@\"; fi; exec \"$@\"",
            "sh",
        ]
        .map(String::from),
    );
    a.push(env_names.join(" "));
    a.push((limit.as_secs() + 1).to_string());
    a.extend(argv.iter().cloned());
    a
}

/// Removes every container a previous (possibly killed) Core recorded in `records`, and the
/// records. Returns each name with whether it is gone. Only names this code made are touched.
pub fn reap_containers(records: &Path) -> Vec<(String, bool)> {
    let Ok(rd) = std::fs::read_dir(records) else {
        return Vec::new();
    };
    let Some(docker) = docker_bin() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if !our_name(&name) {
            continue;
        }
        let r = Command::new(&docker)
            .args(["rm", "-f", &name])
            .stdin(Stdio::null())
            .output();
        let gone = r.is_ok_and(|o| {
            o.status.success()
                || String::from_utf8_lossy(&o.stderr).contains("No such container")
                || String::from_utf8_lossy(&o.stderr).contains("already in progress")
        });
        if gone {
            let _ = std::fs::remove_file(e.path());
        }
        out.push((name, gone));
    }
    out
}

/// Removes the copied agent files when the call ends.
struct Cleanup(Vec<PathBuf>);

impl Drop for Cleanup {
    fn drop(&mut self) {
        for p in &self.0 {
            let _ = std::fs::remove_dir_all(p);
            let _ = std::fs::remove_file(p);
        }
    }
}

fn run_docker(j: &Job<'_>) -> Result<Ran, String> {
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
    if !j.reads.dirs.is_empty() {
        return Err(
            "the Docker backend mounts only the job directory: `read_dirs` cannot be \
                    offered; put the toolchain in the executor image"
                .into(),
        );
    }
    let ds = j.dir.display().to_string();
    if ds.contains([':', ',', '\n'])
        || j.env
            .iter()
            .any(|(k, v)| v.contains('\n') || k.contains(['=', '\n']))
    {
        return Err(
            "the Docker backend cannot carry a path or variable with `:`, `,` or a newline".into(),
        );
    }
    let docker = docker_bin().ok_or("no `docker` on PATH")?;
    let md = std::fs::metadata(j.dir).map_err(|e| e.to_string())?;
    // The container's user is the job directory's owner (never root), so what it writes is owned
    // by the Core's user and the job directory can still be removed afterwards.
    let user = if md.uid() == 0 {
        (65534, 65534)
    } else {
        (md.uid(), md.gid())
    };

    // The files the job's commands name by absolute path (the agent script) are copied into the
    // job directory, because nothing outside it is mounted; the arguments are rewritten.
    let exec_dir = j.dir.join(".exec");
    let mut cleanup = Cleanup(vec![exec_dir.clone()]);
    let mut argv: Vec<String> = j.argv.to_vec();
    for (i, f) in j.reads.files.iter().enumerate() {
        std::fs::create_dir_all(&exec_dir).map_err(|e| e.to_string())?;
        let base = f
            .file_name()
            .map_or_else(|| "file".into(), |n| n.to_string_lossy().into_owned());
        let dest = exec_dir.join(format!("{i}-{base}"));
        std::fs::copy(f, &dest).map_err(|e| format!("cannot copy {}: {e}", f.display()))?;
        for a in &mut argv {
            if Path::new(a.as_str()).is_absolute()
                && Path::new(a.as_str()).canonicalize().is_ok_and(|c| &c == f)
            {
                *a = dest.display().to_string();
            }
        }
    }

    let name = random_name();
    std::fs::create_dir_all(j.records).map_err(|e| e.to_string())?;
    let record = j.records.join(&name);
    // Recorded BEFORE the container exists: a Core killed at any point after this is cleaned up
    // by the next start (removing a name that never started is harmless).
    std::fs::write(&record, &name).map_err(|e| e.to_string())?;
    let env_file = j.logs.0.with_extension("env");
    let env_names: Vec<String> = j.env.iter().map(|(k, _)| k.clone()).collect();
    if env_names
        .iter()
        .any(|k| k.is_empty() || !k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
    {
        return Err("the Docker backend cannot carry a variable name with other than letters, digits and `_`".into());
    }
    let mut text = String::new();
    for (k, v) in j.env {
        text.push_str(&format!("{k}={v}\n"));
    }
    {
        use std::io::Write as _;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&env_file)
            .map_err(|e| e.to_string())?;
        f.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
    }
    cleanup.0.push(env_file.clone());
    let out = File::create(j.logs.0).map_err(|e| e.to_string())?;
    let err = File::create(j.logs.1).map_err(|e| e.to_string())?;
    let args = docker_args(
        &DockerCall {
            name: &name,
            image: j.image,
            dir: j.dir,
            cwd: j.cwd,
            user,
            env_file: &env_file,
            env_names: &env_names,
            limit: j.limit,
        },
        &argv,
    );
    let mut cmd = Command::new(&docker);
    cmd.args(args)
        .stdin(Stdio::null())
        .stderr(err)
        .stdout(out)
        .process_group(0);
    let finish = |res: Result<(Option<i32>, bool), String>| {
        // Whatever happened, the container must not outlive the call: remove it, then check it
        // is gone (`--rm` may be removing it at the same moment, so `rm -f` alone can say
        // "already in progress").
        let _ = Command::new(&docker)
            .args(["rm", "-f", &name])
            .stdin(Stdio::null())
            .output();
        let mut gone = false;
        for _ in 0..50 {
            let o = Command::new(&docker)
                .args(["container", "inspect", &name])
                .stdin(Stdio::null())
                .output();
            if o.is_ok_and(|o| String::from_utf8_lossy(&o.stderr).contains("No such container")) {
                gone = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        if gone {
            let _ = std::fs::remove_file(&record);
        }
        match (res, gone) {
            (Ok(r), true) => Ok(r),
            (Ok(_), false) => Err(format!("container {name} could not be removed")),
            (Err(e), _) => Err(e),
        }
    };
    // `finish` of an `Err` is always that `Err` (after the container is removed).
    let bail = |m: String| finish(Err(m)).err().unwrap_or_default();
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return Err(bail(format!("cannot start docker: {e}"))),
    };
    let started = Instant::now();
    let res = loop {
        match child.try_wait() {
            Ok(Some(s)) => break Ok((s.code(), false)),
            Ok(None) if started.elapsed() > j.limit => {
                // `docker kill` by name; the client process is not the container.
                let mut killed = false;
                for _ in 0..20 {
                    let k = Command::new(&docker)
                        .args(["kill", &name])
                        .stdin(Stdio::null())
                        .output();
                    if k.is_ok_and(|o| o.status.success()) {
                        killed = true;
                        break;
                    }
                    // Still being created, or gone already: look again shortly.
                    if child.try_wait().ok().flatten().is_some() {
                        killed = true;
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
                let stopped = stop(&mut child);
                if !killed {
                    break Err(format!("cannot stop container {name} at its time limit"));
                }
                break stopped.map(|()| (None, true));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(5)),
            Err(e) => {
                let _ = stop(&mut child);
                break Err(e.to_string());
            }
        }
    };
    if let Err(e) = kill_group(child.id()) {
        return Err(bail(e));
    }
    let (code, timed_out) = finish(res)?;
    // Docker itself failed (daemon, image): exit 125 and a `docker:` message, not the command.
    if code == Some(125) {
        let said = read_capped(j.logs.1);
        if said.starts_with("docker:") {
            return Err(format!(
                "docker run failed: {}",
                first_line(said.as_bytes())
            ));
        }
    }
    drop(cleanup);
    Ok(Ran {
        code,
        timed_out,
        out_path: j.logs.0.to_path_buf(),
        err_path: j.logs.1.to_path_buf(),
    })
}

/// Runs a command confined, with exactly `env`, and ALWAYS kills its whole process group when it
/// ends (success, failure or timeout), so nothing it started can write afterwards.
///
/// # Errors
/// When no confinement is available, the command cannot start, or it cannot be stopped.
pub fn run(j: &Job<'_>) -> Result<Ran, String> {
    let mut cmd = match j.confinement {
        Confinement::MacSandbox => {
            let named: &[PathBuf] = match j.profile {
                Profile::Scripted => &[],
                Profile::RealAgent { deny } => deny,
            };
            let mut all: Vec<&Path> = vec![j.dir, j.cwd];
            all.extend(j.reads.dirs.iter().map(PathBuf::as_path));
            all.extend(j.reads.files.iter().map(PathBuf::as_path));
            all.extend(j.reads.deny.iter().map(PathBuf::as_path));
            all.extend(named.iter().map(PathBuf::as_path));
            for p in all {
                check_profile_path(p)?;
            }
            for p in j.reads.deny.iter().chain(named) {
                check_deny_path(p)?;
            }
            let denied: Vec<PathBuf> = j.reads.deny.iter().chain(named).cloned().collect();
            check_deny_outside_writable(&denied, &[j.dir, j.cwd])?;
            let mut c = Command::new("sandbox-exec");
            let profile = match j.profile {
                Profile::Scripted => mac_profile(j.dir, j.reads),
                Profile::RealAgent { deny } => mac_profile_real_agent(j.dir, j.reads, deny),
            };
            c.args(["-p", &profile]);
            c
        }
        Confinement::Bubblewrap if j.profile != &Profile::Scripted => {
            return Err("the real-agent profile is written for macOS only in this slice".into());
        }
        Confinement::Bubblewrap => {
            let mut c = Command::new("bwrap");
            c.args(bwrap_args(j.dir, j.cwd, j.reads));
            c
        }
        Confinement::Docker => return run_docker(j),
        _ => {
            let why = if j.unavailable.is_empty() {
                String::new()
            } else {
                format!(" ({})", j.unavailable)
            };
            return Err(format!(
                "no confinement available: refusing to run an untrusted command{why}"
            ));
        }
    };
    let out = File::create(j.logs.0).map_err(|e| e.to_string())?;
    let err = File::create(j.logs.1).map_err(|e| e.to_string())?;
    cmd.args(j.argv)
        .current_dir(j.cwd)
        .env_clear()
        .envs(j.env.iter().map(|(k, v)| (k, v)))
        .stdin(Stdio::null())
        .stderr(err)
        .stdout(out)
        .process_group(0);
    let mut child = cmd.spawn().map_err(|e| format!("cannot start: {e}"))?;
    let started = Instant::now();
    let (code, timed_out) = loop {
        match child.try_wait() {
            Ok(Some(s)) => break (s.code(), false),
            Ok(None) if started.elapsed() > j.limit => {
                stop(&mut child)?;
                break (None, true);
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(5)),
            Err(e) => {
                let _ = stop(&mut child);
                return Err(e.to_string());
            }
        }
    };
    kill_group(child.id())?;
    Ok(Ran {
        code,
        timed_out,
        out_path: j.logs.0.to_path_buf(),
        err_path: j.logs.1.to_path_buf(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A denied path inside the job directory is refused before anything starts (a by-name deny
    /// cannot hold against a rename where the job may write).
    #[test]
    fn run_refuses_a_denied_path_inside_the_job_directory() {
        let dir = Path::new("/srv/sandbox-job");
        let reads = Reads {
            deny: vec![dir.join("data/core.db")],
            ..Reads::default()
        };
        let j = Job {
            confinement: Confinement::MacSandbox,
            dir,
            cwd: dir,
            argv: &["true".into()],
            env: &[],
            reads: &reads,
            logs: (dir, dir),
            limit: Duration::from_secs(1),
            image: "",
            records: dir,
            unavailable: "",
            profile: &Profile::Scripted,
        };
        let e = run(&j).err().expect("refused");
        assert!(
            e.contains("refusing to start") && e.contains("rename"),
            "{e}"
        );
    }

    fn has_pair(a: &[String], x: &str, y: &str) -> bool {
        a.windows(2).any(|w| w[0] == x && w[1] == y)
    }

    #[test]
    fn the_bwrap_vector_binds_only_system_paths_read_only_and_the_job_writable() {
        let dir = Path::new("/home/user/scratch/job1");
        let reads = Reads {
            dirs: vec!["/opt/toolchain".into()],
            files: vec!["/srv/agent.sh".into()],
            ..Reads::default()
        };
        let a = bwrap_args(dir, &dir.join("repo"), &reads);
        for flag in [
            "--die-with-parent",
            "--new-session",
            "--unshare-net",
            "--unshare-pid",
        ] {
            assert!(a.iter().any(|x| x == flag), "{flag}");
        }
        assert_eq!(a.last().map(String::as_str), Some("--"));
        assert!(has_pair(&a, "--tmpfs", "/home") && has_pair(&a, "--tmpfs", "/root"));
        assert!(a.windows(3).any(|w| w == ["--ro-bind-try", "/usr", "/usr"]));
        // The one writable bind is the job directory; nothing binds the root or a home.
        let binds: Vec<_> = a.windows(3).filter(|w| w[0] == "--bind").collect();
        assert_eq!(binds.len(), 1);
        assert_eq!(binds[0][1], "/home/user/scratch/job1");
        assert!(
            !a.iter()
                .any(|x| x == "/" || x == "/Users" || x == "--share-net")
        );
        assert!(
            a.windows(3)
                .any(|w| w == ["--ro-bind-try", "/opt/toolchain", "/opt/toolchain"])
        );
        assert!(
            a.windows(3)
                .any(|w| w == ["--ro-bind-try", "/srv/agent.sh", "/srv/agent.sh"])
        );
        // The job directory is bound after the tmpfs that would otherwise hide it.
        let pos = |n: &str| a.iter().position(|x| x == n).unwrap();
        assert!(pos("/home") < a.iter().position(|x| x == "--bind").unwrap());
        assert!(pos("/root") < a.len());
        assert!(has_pair(&a, "--chdir", "/home/user/scratch/job1/repo"));
    }

    #[test]
    fn the_mac_profile_denies_reads_then_opens_only_the_job_and_named_paths() {
        let reads = Reads {
            dirs: vec!["/Users/x/.rustup".into()],
            files: vec!["/Users/x/lab/agent.sh".into()],
            ..Reads::default()
        };
        let p = mac_profile(Path::new("/private/tmp/job"), &reads);
        assert!(p.contains("(deny file-read-data)") && p.contains("(deny network*)"));
        // Last match wins: the denies come after the system allow, the job allow comes last.
        let at = |s: &str| p.find(s).unwrap();
        assert!(
            at("(allow file-read-data (literal \"/\") (subpath \"/usr\")")
                < at("(deny file-read-data (subpath \"/Library/Keychains\")")
        );
        assert!(
            at("(deny file-read-data (subpath \"/Library/Keychains\")")
                < at("(allow file-read-data (subpath \"/private/tmp/job\")")
        );
        assert!(p.contains("(subpath \"/Users\")") && p.contains("(subpath \"/private/tmp\")"));
        assert!(p.ends_with("(literal \"/Users/x/lab/agent.sh\"))"));
    }

    #[test]
    fn the_docker_vector_mounts_only_the_job_and_drops_every_privilege() {
        let dir = Path::new("/srv/scratch/job1");
        let names = vec!["PATH".to_owned(), "HOME".to_owned()];
        let a = docker_args(
            &DockerCall {
                name: "frx-0123abcd",
                image: "alpine:3.23",
                dir,
                cwd: &dir.join("repo"),
                user: (1000, 1000),
                env_file: Path::new("/srv/scratch/.meta/.logs/1.env"),
                env_names: &names,
                limit: Duration::from_secs(60),
            },
            &["sh".into(), "agent.sh".into()],
        );
        let has = |x: &str, y: &str| a.windows(2).any(|w| w[0] == x && w[1] == y);
        for (x, y) in [
            ("--network", "none"),
            ("--cap-drop", "ALL"),
            ("--security-opt", "no-new-privileges"),
            ("--pids-limit", "128"),
            ("--memory", "512m"),
            ("--cpus", "1"),
            ("--user", "1000:1000"),
            ("--pull", "never"),
            ("--name", "frx-0123abcd"),
            ("-v", "/srv/scratch/job1:/srv/scratch/job1:rw"),
            ("-w", "/srv/scratch/job1/repo"),
        ] {
            assert!(has(x, y), "{x} {y} in {a:?}");
        }
        for f in ["--read-only", "--rm", "--init"] {
            assert!(a.iter().any(|x| x == f), "{f}");
        }
        // One mount, no privilege, no host namespaces, no socket, no secret on the command line.
        assert_eq!(a.iter().filter(|x| *x == "-v").count(), 1);
        for bad in [
            "--privileged",
            "--net",
            "host",
            "--pid",
            "--ipc",
            "--userns",
        ] {
            assert!(!a.iter().any(|x| x == bad), "{bad}");
        }
        assert!(!a.iter().any(|x| x.contains("docker.sock")));
        let img = a.iter().position(|x| x == "alpine:3.23").unwrap();
        assert!(a[..img].iter().any(|x| x == "--env-file") && img < a.len() - 2);
        // The positional arguments after the inner script: allowed names, seconds, the command.
        let tail: Vec<&str> = a[a.len() - 4..].iter().map(String::as_str).collect();
        assert_eq!(tail, ["PATH HOME", "61", "sh", "agent.sh"]);
    }

    #[test]
    fn container_names_are_ours_only_when_frx_and_hex() {
        assert!(our_name("frx-0123abcdef") && our_name(&random_name()));
        for bad in ["frx-", "frx-xyz", "other", "frx-0123; rm -rf", "FRX-01", ""] {
            assert!(!our_name(bad), "{bad}");
        }
        assert_ne!(random_name(), random_name());
    }

    #[test]
    fn kill_group_kills_a_group_and_a_group_already_gone_is_not_an_error() {
        let mut c = Command::new("sleep")
            .arg("30")
            .process_group(0)
            .spawn()
            .unwrap();
        assert!(kill_group(c.id()).is_ok());
        let st = c.wait().unwrap();
        assert!(st.code().is_none(), "killed by a signal: {st:?}");
        // Gone now: still fine (the final clean-up kills groups that already ended).
        assert!(kill_group(c.id()).is_ok());
    }

    #[test]
    fn stop_never_hangs_on_a_child_it_cannot_group_kill() {
        // A child that is not a group leader has no group of its own to kill (ESRCH): `stop`
        // must still end it promptly instead of waiting out the 30 s sleep.
        let mut c = Command::new("sleep").arg("30").spawn().unwrap();
        let t = Instant::now();
        let _ = stop(&mut c);
        assert!(t.elapsed() < Duration::from_secs(5));
        assert!(c.try_wait().unwrap().is_some());
    }

    #[test]
    fn redaction_and_caps() {
        let r = redact(
            "A=1 MY_API_KEY=abc OPENAI_KEY=\"two words\" ok=1\nexport GITHUB_TOKEN=ghp_xx\nsee sk-ant-api03-ABCDEFGHIJKLMNOP now\n-----BEGIN RSA PRIVATE KEY-----\nMIIB\n-----END RSA PRIVATE KEY-----\nlast",
        );
        assert!(
            r.contains("A=1") && r.contains("ok=1") && r.contains("last"),
            "{r}"
        );
        for gone in ["abc", "two words", "ghp_xx", "sk-ant", "MIIB"] {
            assert!(!r.contains(gone), "{gone} in {r}");
        }
        let big = "x".repeat(EVIDENCE_CAP + 10);
        let c = cap_text(&big);
        assert!(c.contains(&format!(
            "[truncated: kept {EVIDENCE_CAP} of {} bytes]",
            big.len()
        )));
    }
}
