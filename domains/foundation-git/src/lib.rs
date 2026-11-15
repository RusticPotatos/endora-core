//! Capability B for the foundation: a **git change task**. It lives outside the domain crate: it
//! implements the one port and is registered with `Core::register`; Core never names it.
//!
//! Calls offered (each with its declared band):
//! - `git.prepare` (reversible): a disposable repository in a scratch directory, from a template.
//! - `git.change` (reversible): run the configured agent command in that repository, with a time
//!   limit. The command is configuration, not code here; tests use a scripted stand-in. The
//!   agent's resulting changes are captured as a patch against the template, outside anything
//!   the agent can write.
//! - `git.verify` (observe): run the repo's own test command on a FRESH copy (the template as it
//!   was at prepare, plus that patch) and keep the output. Its result string comes from the exit
//!   code and a completion marker alone, never from text the agent wrote. It is observe-band
//!   because that copy is confined like the agent (no writes outside it, no reads outside it and
//!   the system, no network: tested) and is deleted afterwards.
//! - `git.publish` (outward-reversible): declared so that Core refuses it. Even if Core allowed
//!   it, this capability has no credentials and no remote and fails.
//!
//! Sandboxing is this crate's job: every command that touches the repository (the agent, the
//! tests and our own git calls) runs confined to its own directory, with no network, no reads
//! outside it and the system paths, an explicit minimal environment and no credentials; the whole
//! process group is killed when the call ends. With no confinement available the capability
//! fails closed.
//!
//! What decides acceptance is never read from the agent's tree or its `.git`: the base snapshot,
//! its tree hash and the protected-path rules live in `<scratch>/.meta/<job>/`, which the
//! confinement hides from the agent. The agent's `.git` (config, hooks, replace refs,
//! attributes) is never consulted: its files are copied (symlinks as symlinks, every `.git`
//! skipped) into a fresh repository this crate created, and git runs there with the safe flags
//! in [`GitChange`]'s `git` helper.
//!
//! Known limit: if the test command runs the code under test in the same process (a shell test
//! that sources the code), code written to forge the harness marker could still look like a
//! pass. The owner's acceptance remains the gate.

mod claude;
mod confine;

use std::fs::{self, File};
use std::io::Read;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use endora_foundation::application::{CallFailure, CallOutput, Capability};
use endora_foundation::domain::{Budget as CallBudget, BudgetMode, CallSpec, Fact, Field};
use endora_kernel::{Reversibility, Tier};

pub use claude::{
    AgentExecutor, ClaudeCode, ClaudeLimits, MOST_ENFORCEABLE_MICROS, SYSTEM_PROMPT, Stop,
    claude_args, micros_up, parse_report,
};
use claude::{Report, granted};
pub use confine::{
    Confinement, DEFAULT_IMAGE, DockerCall, EVIDENCE_CAP, Profile, Reads, SECRET_NAMES,
    SECRET_PREFIXES, bwrap_args, canonical_deny_paths, cap_text, check_deny_outside_writable,
    check_deny_path, check_profile_path, detect, docker_args, mac_profile, mac_profile_can_read,
    mac_profile_real_agent, reap_containers, redact,
};
use confine::{Job, Ran};

/// Most files and bytes copied from an agent's tree: a bigger one is refused, not truncated.
const MOST_FILES: usize = 20_000;
const MOST_BYTES: u64 = 256 * 1024 * 1024;

/// What the capability may touch and run.
#[derive(Debug, Clone)]
pub struct Config {
    /// Parent of every job directory.
    pub scratch_root: PathBuf,
    /// The directory a new repository starts from.
    pub template: PathBuf,
    /// The agent: a program and its arguments, run in the repository.
    pub agent_command: Vec<String>,
    /// The repo's own test command, run in a fresh copy.
    pub test_command: Vec<String>,
    /// Paths (relative to the repo root; a directory covers everything under it) the agent may
    /// not change if its tests are to count. Added to the default protected set.
    pub protected: Vec<String>,
    /// How long the agent may run.
    pub agent_limit: Duration,
    /// How long the tests may run.
    pub test_limit: Duration,
    /// How untrusted commands are confined.
    pub confinement: Confinement,
}

/// Per-job settings with safe defaults (kept apart from [`Config`] so existing callers stay as
/// they are).
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// Names of parent environment variables the child may see. Default: none.
    pub env_allow: Vec<String>,
    /// Directories (a toolchain, say) the child may read, read-only. Default: none.
    pub read_dirs: Vec<PathBuf>,
    /// More protected paths: files the tests source or run, config they read.
    pub protected_extra: Vec<String>,
    /// More `PATH` entries (they also need to be in `read_dirs`).
    pub extra_path: Vec<PathBuf>,
    /// Trust the test command's exit code alone. Default false: a shell test script must also
    /// run to its end (no `exit 0` of its own), which the harness proves with a random marker.
    pub trust_exit_code: bool,
    /// The Docker backend's executor image (it must already exist locally; nothing is pulled).
    /// Default: `ENDORA_FOUNDATION_EXECUTOR_IMAGE`, else [`DEFAULT_IMAGE`] (`alpine:3.23`: `sh`
    /// and busybox tools, no git, no node: a job that needs them names its own image).
    pub executor_image: Option<String>,
    /// What runs the agent of `git.change`. Default: the configured command (scripted).
    pub executor: AgentExecutor,
    /// The per-call ceilings for the `ClaudeCode` executor (declared maximum cost, turns).
    pub claude_limits: ClaudeLimits,
    /// A file holding the CLI's access token. The capability reads it at call time and gives it
    /// to the `ClaudeCode` child as `CLAUDE_CODE_OAUTH_TOKEN` (never in an argument, evidence or
    /// a file inside the job). Default: none.
    pub credential_file: Option<PathBuf>,
    /// Paths both macOS profiles deny reading by name (the token file, the database; the profile
    /// adds each one's `-journal`, `-wal` and `-shm` files), whatever prefix they lie under.
    pub deny_paths: Vec<PathBuf>,
}

/// The profile the evidence records for a call confined by `c` under `p`: the macOS profile's
/// name (`scripted-agent`, `real-agent`), or the backend's own name where the profile is not a
/// macOS one (`bwrap`, `docker`). `None` when nothing confined the call.
#[must_use]
pub fn profile_label(c: Confinement, p: &Profile) -> Option<String> {
    Some(match c {
        Confinement::MacSandbox => p.name().to_owned(),
        Confinement::Bubblewrap => "bwrap".to_owned(),
        Confinement::Docker => "docker".to_owned(),
        Confinement::Auto | Confinement::Unavailable => return None,
    })
}

/// What a real executor adds to a confined run: its profile, environment and readable files.
#[derive(Default)]
struct Extra {
    profile: Profile,
    env: Vec<(String, String)>,
    files: Vec<PathBuf>,
}

/// The capability.
pub struct GitChange {
    config: Config,
    options: Options,
    confinement: Confinement,
    /// Why no backend was available, or which was picked (what `Auto` tried).
    note: String,
    image: String,
    git_bin: PathBuf,
}

fn fail(m: impl Into<String>) -> CallFailure {
    CallFailure::new(m)
}

fn io(e: &std::io::Error) -> CallFailure {
    fail(e.to_string())
}

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn unique() -> String {
    format!(
        "{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}

/// Files and bytes copied so far from a tree we do not trust.
#[derive(Default)]
struct Budget {
    files: usize,
    bytes: u64,
}

/// Copies a tree without following symlinks (they are recreated as symlinks), skipping every
/// `.git` when asked, and anything that is not a file, directory or symlink.
fn copy_tree(from: &Path, to: &Path, skip_git: bool, b: &mut Budget) -> std::io::Result<()> {
    fs::create_dir_all(to)?;
    let mut entries: Vec<_> = fs::read_dir(from)?.collect::<Result<_, _>>()?;
    entries.sort_by_key(fs::DirEntry::file_name);
    for e in entries {
        if skip_git && e.file_name() == ".git" {
            continue;
        }
        let (src, dst, ft) = (e.path(), to.join(e.file_name()), e.file_type()?);
        b.files += 1;
        if b.files > MOST_FILES || b.bytes > MOST_BYTES {
            return Err(std::io::Error::other(
                "the tree is too large to snapshot (files or bytes)",
            ));
        }
        if ft.is_symlink() {
            symlink(fs::read_link(&src)?, &dst)?;
        } else if ft.is_dir() {
            copy_tree(&src, &dst, skip_git, b)?;
        } else if ft.is_file() {
            b.bytes += fs::copy(&src, &dst)?;
        }
    }
    Ok(())
}

fn clear_worktree(repo: &Path) -> std::io::Result<()> {
    for e in fs::read_dir(repo)? {
        let e = e?;
        if e.file_name() == ".git" {
            continue;
        }
        if e.file_type()?.is_dir() {
            fs::remove_dir_all(e.path())?;
        } else {
            fs::remove_file(e.path())?;
        }
    }
    Ok(())
}

/// A random 128-bit hex string for the completion marker.
fn random_marker() -> String {
    let mut b = [0u8; 16];
    if let Ok(mut f) = File::open("/dev/urandom") {
        let _ = f.read_exact(&mut b);
    }
    b.iter().fold(String::from("ENDORA-DONE-"), |mut s, x| {
        s.push_str(&format!("{x:02x}"));
        s
    })
}

/// A fresh copy of the template at the recorded base, deleted when dropped.
struct Fresh {
    dir: PathBuf,
}

impl Fresh {
    fn repo(&self) -> PathBuf {
        self.dir.join("repo")
    }
}

impl Drop for Fresh {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

/// Does `path` fall under one of the protected `rules` (a rule names a file or a directory)?
/// Case-insensitive: the Mac's file system is, and `Test.sh` is `test.sh` there.
fn protected_by<'a>(rules: &'a [String], path: &str) -> Option<&'a String> {
    let p = path.to_ascii_lowercase();
    rules.iter().find(|r| {
        let r = r.trim_end_matches('/').to_ascii_lowercase();
        !r.is_empty() && (p == r || p.starts_with(&format!("{r}/")))
    })
}

/// A shell test script is run as `sh -c '. "$0" "$@"; rc=$?; echo MARKER; exit $rc' ./script`:
/// an `exit 0` inside the script or in anything it sources ends that shell before the marker.
fn wrap_test(argv: &[String], marker: &str) -> Vec<String> {
    let shell = argv.first().map(String::as_str);
    let script = argv.get(1).filter(|a| !a.starts_with('-'));
    match (shell, script) {
        (Some("sh" | "bash"), Some(script)) => {
            let path = if script.contains('/') {
                script.clone()
            } else {
                format!("./{script}")
            };
            let mut v = vec![
                argv[0].clone(),
                "-c".into(),
                format!(". \"$0\" \"$@\"; rc=$?; echo '{marker}'; exit $rc"),
                path,
            ];
            v.extend(argv[2..].iter().cloned());
            v
        }
        _ => argv.to_vec(),
    }
}

impl GitChange {
    /// A capability confined to `config`, with default [`Options`].
    #[must_use]
    pub fn new(config: Config) -> Self {
        Self::with_options(config, Options::default())
    }

    /// A capability confined to `config`, with these per-job `options`.
    #[must_use]
    pub fn with_options(config: Config, options: Options) -> Self {
        let image = options
            .executor_image
            .clone()
            .or_else(|| std::env::var("ENDORA_FOUNDATION_EXECUTOR_IMAGE").ok())
            .unwrap_or_else(|| DEFAULT_IMAGE.to_owned());
        let (confinement, note) = match config.confinement {
            Confinement::Auto => detect(&image),
            other => (other, "chosen by the caller".to_owned()),
        };
        if confinement == Confinement::Docker {
            // A Core killed mid-call may have left an executor container running: remove what
            // the last one recorded before starting anything new.
            let _ = reap_containers(&config.scratch_root.join(".meta/.containers"));
        }
        // macOS's /usr/bin/git is a shim that writes a cache outside the job directory (which the
        // confinement forbids); prefer the real binary.
        let git_bin = [
            "/opt/homebrew/bin/git",
            "/usr/local/bin/git",
            "/Library/Developer/CommandLineTools/usr/bin/git",
            "/usr/bin/git",
        ]
        .into_iter()
        .map(PathBuf::from)
        .find(|p| p.exists())
        .unwrap_or_else(|| PathBuf::from("git"));
        // In a container git is whatever the image has, at the usual place.
        let git_bin = if confinement == Confinement::Docker {
            PathBuf::from("/usr/bin/git")
        } else {
            git_bin
        };
        Self {
            config,
            options,
            confinement,
            note,
            image,
            git_bin,
        }
    }

    /// The confinement in force (`Auto` is already resolved).
    #[must_use]
    pub fn confinement(&self) -> Confinement {
        self.confinement
    }

    /// What was picked, or, when nothing was, which backends were tried and why each failed.
    #[must_use]
    pub fn confinement_note(&self) -> &str {
        &self.note
    }

    /// Removes every executor container a previously killed Core left behind (Docker backend).
    /// Done at construction too; call it again from a recovery path. Names and whether each is gone.
    #[must_use]
    pub fn reap_leftovers(&self) -> Vec<(String, bool)> {
        reap_containers(&self.meta_root().join(".containers"))
    }

    fn job(&self, input: &str) -> Result<(String, PathBuf), CallFailure> {
        let name = input.lines().next().unwrap_or("").trim();
        let ok = !name.is_empty()
            && name.len() <= 40
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
        if !ok {
            return Err(fail(format!("`{name}` is not a plain job name")));
        }
        Ok((name.to_owned(), self.config.scratch_root.join(name)))
    }

    /// `<scratch>/.meta`: what the capability keeps that the agent must never reach.
    fn meta_root(&self) -> PathBuf {
        self.config.scratch_root.join(".meta")
    }

    fn meta(&self, name: &str) -> PathBuf {
        self.meta_root().join(name)
    }

    /// The read-only extras: toolchain directories from the options, and the files the config
    /// names by absolute path (the agent script, the test script).
    fn reads(&self) -> Reads {
        let files = self
            .config
            .agent_command
            .iter()
            .chain(&self.config.test_command)
            .map(Path::new)
            .filter(|p| p.is_absolute())
            .filter_map(|p| p.canonicalize().ok())
            .filter(|p| p.is_file())
            .collect();
        Reads {
            dirs: self.options.read_dirs.clone(),
            files,
            deny: self.options.deny_paths.clone(),
        }
    }

    /// The whole environment a child gets: a fixed `PATH`, a `HOME` and `TMPDIR` inside its
    /// directory, git's safety variables, and only the parent variables the options allow.
    fn env(&self, dir: &Path, task: &str) -> Vec<(String, String)> {
        let mut path: Vec<String> = Vec::new();
        if let Some(g) = self.git_bin.parent() {
            path.push(g.display().to_string());
        }
        path.extend(
            self.options
                .extra_path
                .iter()
                .map(|p| p.display().to_string()),
        );
        path.extend(["/usr/bin".into(), "/bin".into()]);
        let mut env = vec![
            ("PATH".to_owned(), path.join(":")),
            ("HOME".into(), dir.join("home").display().to_string()),
            ("TMPDIR".into(), dir.join("tmp").display().to_string()),
            ("GIT_CONFIG_GLOBAL".into(), "/dev/null".into()),
            ("GIT_CONFIG_NOSYSTEM".into(), "1".into()),
            ("GIT_TERMINAL_PROMPT".into(), "0".into()),
            ("AGENT_TASK".into(), task.into()),
        ];
        for name in &self.options.env_allow {
            if let Some(v) = std::env::var_os(name) {
                env.push((name.clone(), v.to_string_lossy().into_owned()));
            }
        }
        env
    }

    /// Runs `argv` in `dir/repo`, confined to `dir`, with the minimal environment and a time
    /// limit; the whole process group dies when it returns.
    fn confined(
        &self,
        dir: &Path,
        argv: &[String],
        task: &str,
        limit: Duration,
    ) -> Result<Ran, CallFailure> {
        self.confined_as(dir, argv, task, limit, &Extra::default())
    }

    /// As [`Self::confined`], under a chosen profile, with extra environment and extra readable
    /// files (the real agent's binary).
    fn confined_as(
        &self,
        dir: &Path,
        argv: &[String],
        task: &str,
        limit: Duration,
        extra: &Extra,
    ) -> Result<Ran, CallFailure> {
        let dir = dir
            .canonicalize()
            .map_err(|e| fail(format!("no such job: {e}")))?;
        // Logs live where the command cannot write (and so cannot plant a symlink).
        let logs = self.meta_root().join(".logs");
        fs::create_dir_all(&logs).map_err(|e| io(&e))?;
        let id = unique();
        let (out, err) = (
            logs.join(format!("{id}.out")),
            logs.join(format!("{id}.err")),
        );
        confine::run(&Job {
            confinement: self.confinement,
            dir: &dir,
            cwd: &dir.join("repo"),
            argv,
            env: &{
                let mut e = self.env(&dir, task);
                e.extend(extra.env.iter().cloned());
                e
            },
            reads: &{
                let mut r = self.reads();
                r.files.extend(extra.files.iter().cloned());
                r
            },
            logs: (&out, &err),
            limit,
            image: &self.image,
            records: &self.meta_root().join(".containers"),
            unavailable: &self.note,
            profile: &extra.profile,
        })
        .map_err(fail)
    }

    /// Runs git in `dir/repo` with the safe flags: no replace objects, no hooks, no fsmonitor, no
    /// attributes or ignore files from outside, no protocols, no pager, no system or global
    /// config (the environment says so too). Returns stdout, whole.
    fn git(&self, dir: &Path, args: &[&str]) -> Result<String, CallFailure> {
        let mut argv: Vec<String> = [
            self.git_bin.to_str().unwrap_or("git"),
            "--no-replace-objects",
            "--no-pager",
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.attributesFile=/dev/null",
            "-c",
            "core.excludesFile=/dev/null",
            "-c",
            "core.sshCommand=false",
            "-c",
            "core.autocrlf=false",
            "-c",
            "core.fileMode=true",
            "-c",
            "protocol.allow=never",
            "-c",
            "user.name=Agent",
            "-c",
            "user.email=agent@example.com",
        ]
        .map(String::from)
        .to_vec();
        argv.extend(args.iter().map(|a| (*a).to_owned()));
        let r = self.confined(dir, &argv, "", Duration::from_secs(60))?;
        if r.code != Some(0) {
            return Err(fail(format!("git {} failed: {}", args[0], r.all())));
        }
        Ok(fs::read_to_string(&r.out_path)
            .or_else(|_| fs::read(&r.out_path).map(|b| String::from_utf8_lossy(&b).into_owned()))
            .unwrap_or_default())
    }

    /// The flags every diff of an agent's change gets.
    const DIFF: [&'static str; 4] = [
        "--no-ext-diff",
        "--no-textconv",
        "--no-color",
        "--no-renames",
    ];

    /// A new fresh repository: the base snapshot committed by us, attributes neutralised. When
    /// `expect_tree` is given, the snapshot must still hash to it.
    fn fresh(&self, name: &str, expect_tree: Option<&str>) -> Result<Fresh, CallFailure> {
        let dir = self.meta(name).join(format!("fresh-{}", unique()));
        let f = Fresh { dir };
        for sub in ["repo", "tmp", "home"] {
            fs::create_dir_all(f.dir.join(sub)).map_err(|e| io(&e))?;
        }
        copy_tree(
            &self.meta(name).join("base-tree"),
            &f.repo(),
            true,
            &mut Budget::default(),
        )
        .map_err(|e| io(&e))?;
        self.git(&f.dir, &["init", "-q", "-b", "main"])?;
        // `info/attributes` outranks every `.gitattributes`: no filter, textconv, merge driver,
        // eol or binary override can come from the tree.
        let info = f.repo().join(".git/info");
        fs::create_dir_all(&info).map_err(|e| io(&e))?;
        fs::write(
            info.join("attributes"),
            "* !diff !merge !filter !ident !eol !working-tree-encoding -text\n",
        )
        .map_err(|e| io(&e))?;
        fs::write(info.join("exclude"), "").map_err(|e| io(&e))?;
        self.git(&f.dir, &["add", "-A", "-f"])?;
        self.git(&f.dir, &["commit", "-q", "--allow-empty", "-m", "base"])?;
        let tree = self.git(&f.dir, &["rev-parse", "HEAD^{tree}"])?;
        if expect_tree.is_some_and(|e| e != tree.trim()) {
            return Err(fail(
                "the base snapshot no longer matches its recorded hash: refusing to judge",
            ));
        }
        Ok(f)
    }

    /// The paths a job's rules protect: the config's, the options', `tests/` and `scripts/`, and
    /// every path the test command names that exists in the template. Taken from the template,
    /// never from the agent's tree.
    fn protected_rules(&self, base_tree: &Path) -> Vec<String> {
        let mut rules: Vec<String> = ["tests/", "scripts/"].map(String::from).to_vec();
        rules.extend(self.config.protected.iter().cloned());
        rules.extend(self.options.protected_extra.iter().cloned());
        for arg in &self.config.test_command {
            let rel = arg.trim_start_matches("./");
            if !rel.is_empty() && !arg.starts_with('/') && !rel.contains("..") {
                let p = base_tree.join(rel);
                if p.is_dir() {
                    rules.push(format!("{rel}/"));
                } else if p.exists() {
                    rules.push(rel.to_owned());
                }
            }
        }
        rules.sort();
        rules.dedup();
        rules
    }

    fn prepare(&self, input: &str) -> Result<CallOutput, CallFailure> {
        let (name, dir) = self.job(input)?;
        if dir.exists() || self.meta(&name).exists() {
            return Err(fail(format!(
                "job `{name}` already exists: one repository per job"
            )));
        }
        for sub in ["repo", "tmp", "home"] {
            fs::create_dir_all(dir.join(sub)).map_err(|e| io(&e))?;
        }
        copy_tree(
            &self.config.template,
            &dir.join("repo"),
            true,
            &mut Budget::default(),
        )
        .map_err(|e| io(&e))?;
        let meta = self.meta(&name);
        fs::create_dir_all(&meta).map_err(|e| io(&e))?;
        let base_tree = meta.join("base-tree");
        copy_tree(
            &self.config.template,
            &base_tree,
            true,
            &mut Budget::default(),
        )
        .map_err(|e| io(&e))?;
        for args in [
            &["init", "-q", "-b", "main"][..],
            &["add", "-A"],
            &["commit", "-q", "-m", "base"],
        ] {
            self.git(&dir, args)?;
        }
        let head = self.git(&dir, &["rev-parse", "HEAD"])?.trim().to_owned();
        // The recorded hash comes from the same fresh construction verify will use.
        let probe = self.fresh(&name, None)?;
        let tree = self.git(&probe.dir, &["rev-parse", "HEAD^{tree}"])?;
        drop(probe);
        fs::write(meta.join("base.sha"), &head).map_err(|e| io(&e))?;
        fs::write(meta.join("base.tree"), tree.trim()).map_err(|e| io(&e))?;
        fs::write(
            meta.join("protected"),
            self.protected_rules(&base_tree).join("\n"),
        )
        .map_err(|e| io(&e))?;
        Ok(CallOutput {
            cost_micros: 0,
            facts: Vec::new(),
            result: format!("prepared job={name} base={head}"),
            evidence: "disposable repository, no remote".into(),
            profile: self.label(&Profile::Scripted),
        })
    }

    fn recorded(&self, name: &str, what: &str) -> Result<String, CallFailure> {
        fs::read_to_string(self.meta(name).join(what))
            .map(|s| s.trim().to_owned())
            .map_err(|e| fail(format!("not prepared: {e}")))
    }

    /// Captures what the agent left as a patch against the template, from a fresh repository:
    /// the agent's files (not its `.git`) are laid over the base and diffed with the safe flags.
    /// Returns the patch (git's binary-safe form) and the changed paths.
    fn capture(&self, name: &str, dir: &Path) -> Result<(String, Vec<String>), CallFailure> {
        let tree = self.recorded(name, "base.tree")?;
        let f = self.fresh(name, Some(&tree))?;
        clear_worktree(&f.repo()).map_err(|e| io(&e))?;
        copy_tree(&dir.join("repo"), &f.repo(), true, &mut Budget::default())
            .map_err(|e| fail(format!("cannot snapshot the agent's tree: {e}")))?;
        self.git(&f.dir, &["add", "-A", "-f"])?;
        let mut names = vec!["diff", "--cached", "--name-only", "-z"];
        names.extend(Self::DIFF);
        let changed = self
            .git(&f.dir, &names)?
            .split('\0')
            .filter(|p| !p.is_empty())
            .map(str::to_owned)
            .collect();
        let mut patch = vec!["diff", "--cached", "--binary", "--full-index"];
        patch.extend(Self::DIFF);
        Ok((self.git(&f.dir, &patch)?, changed))
    }

    fn violations(&self, name: &str, changed: &[String]) -> Result<Vec<String>, CallFailure> {
        let rules: Vec<String> = self
            .recorded(name, "protected")?
            .lines()
            .map(str::to_owned)
            .collect();
        Ok(changed
            .iter()
            .filter(|p| protected_by(&rules, p).is_some())
            .cloned()
            .collect())
    }

    fn change(&self, input: &str) -> Result<CallOutput, CallFailure> {
        let (name, dir) = self.job(input)?;
        let task = input.split_once('\n').map_or("", |(_, t)| t);
        let ran = self.confined(
            &dir,
            &self.config.agent_command,
            task,
            self.config.agent_limit,
        )?;
        if ran.timed_out {
            return Err(fail(format!(
                "the agent was stopped after {:?}; its output so far: {}",
                self.config.agent_limit,
                ran.all()
            )));
        }
        // From here on nothing the agent started is alive (its process group was killed).
        let said = ran.all();
        self.finish_change(
            &name,
            &dir,
            ran.code,
            &said,
            0,
            Vec::new(),
            String::new(),
            String::new(),
        )
        .map(|o| CallOutput {
            profile: self.label(&Profile::Scripted),
            ..o
        })
    }

    /// Captures the agent's change as a patch and builds the call's output. `extra_facts`,
    /// `extra_result` and `header` carry what an executor adds (turns, cost, profile).
    #[allow(clippy::too_many_arguments)]
    fn finish_change(
        &self,
        name: &str,
        dir: &Path,
        code: Option<i32>,
        said: &str,
        cost_micros: u64,
        extra_facts: Vec<Fact>,
        extra_result: String,
        header: String,
    ) -> Result<CallOutput, CallFailure> {
        let (patch, changed) = self.capture(name, dir)?;
        let meta = self.meta(name);
        fs::write(meta.join("patch"), &patch).map_err(|e| io(&e))?;
        fs::write(meta.join("changed"), changed.join("\0")).map_err(|e| io(&e))?;
        let hits = self.violations(name, &changed)?;
        let flagged: String = hits
            .iter()
            .map(|p| format!("PROTECTED PATH TOUCHED: {p}\n"))
            .collect();
        let mut facts: Vec<Fact> = code
            .map(|c| Fact::int("agent_exit", i64::from(c)))
            .into_iter()
            .collect();
        facts.extend(extra_facts);
        Ok(CallOutput {
            cost_micros,
            facts,
            result: format!(
                "job={name} agent_exit={}{extra_result}",
                code.map_or("signal".to_owned(), |c| c.to_string())
            ),
            evidence: format!(
                "{header}agent said:\n{said}\n{flagged}diff:\n{}",
                cap_text(&patch)
            ),
            profile: None,
        })
    }

    /// The real executor: the Claude Code CLI, limited by the budget Core granted.
    fn change_claude(
        &self,
        cc: &ClaudeCode,
        input: &str,
        b: &CallBudget,
    ) -> Result<CallOutput, CallFailure> {
        let (name, dir) = self.job(input)?;
        let task = input.split_once('\n').map_or("", |(_, t)| t);
        let g = granted(b, self.options.claude_limits, self.config.agent_limit);
        if g.micros == 0 || g.turns == 0 || g.seconds == 0 {
            return Err(CallFailure {
                reason: "no budget granted for the real agent".into(),
                cost_micros: Some(0),
                facts: Vec::new(),
                profile: None,
            });
        }
        let argv = claude::claude_args(cc, &g, task);
        let mut env = vec![
            (
                "CLAUDE_CONFIG_DIR".to_owned(),
                dir.canonicalize()
                    .map_err(|e| io(&e))?
                    .join("home/.claude")
                    .display()
                    .to_string(),
            ),
            // The CLI wants a scratch folder under /tmp by default; put it inside the job.
            (
                "CLAUDE_CODE_TMPDIR".into(),
                dir.canonicalize()
                    .map_err(|e| io(&e))?
                    .join("tmp")
                    .display()
                    .to_string(),
            ),
            ("CLAUDE_CODE_SUBPROCESS_ENV_SCRUB".into(), "1".into()),
            (
                "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC".into(),
                "1".into(),
            ),
            ("DISABLE_AUTOUPDATER".into(), "1".into()),
            ("DISABLE_TELEMETRY".into(), "1".into()),
            ("DISABLE_ERROR_REPORTING".into(), "1".into()),
        ];
        if let Some(f) = &self.options.credential_file {
            let tok = fs::read_to_string(f).map_err(|e| fail(format!("credential file: {e}")))?;
            env.push(("CLAUDE_CODE_OAUTH_TOKEN".into(), tok.trim().to_owned()));
        }
        let mut deny: Vec<PathBuf> = self.options.deny_paths.clone();
        if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
            for sub in [
                ".ssh",
                ".config",
                "Library",
                ".claude",
                ".claude.json",
                ".gnupg",
                ".aws",
            ] {
                deny.push(home.join(sub));
            }
        }
        let extra = Extra {
            profile: Profile::RealAgent { deny },
            env,
            files: vec![],
        };
        let binary = cc
            .binary
            .canonicalize()
            .map_err(|e| fail(format!("claude binary: {e}")))?;
        let mut extra = extra;
        extra.files.push(binary);
        let ran = self.confined_as(&dir, &argv, task, Duration::from_secs(g.seconds), &extra)?;
        let label = self.label(&extra.profile);
        let header = format!(
            "executor: claude-code model={} granted: {} micro-USD, {} turns, {} s\n",
            cc.model, g.micros, g.turns, g.seconds
        );
        if ran.timed_out {
            // Nothing was read back: the cost is unknown, so Core charges the declared maximum.
            return Err(CallFailure {
                reason: format!(
                    "{header}the agent was killed at {} s (whole process group); cost unknown: {}",
                    g.seconds,
                    ran.all()
                ),
                cost_micros: None,
                facts: vec![Fact::word("agent_stop", "timeout")],
                profile: label,
            });
        }
        let raw = fs::read_to_string(&ran.out_path).unwrap_or_default();
        let report: Report = claude::parse_report(&raw).map_err(|e| CallFailure {
            reason: format!(
                "{header}no usable report from the CLI ({e}) exit={:?}: {}",
                ran.code,
                ran.all()
            ),
            cost_micros: None,
            facts: Vec::new(),
            profile: label.clone(),
        })?;
        let mut facts = vec![Fact::word("agent_stop", report.stop.word())];
        if let Some(t) = report.turns {
            facts.push(Fact::int("agent_turns", i64::from(t)));
        }
        let summary = format!(
            "{header}cli: cost_usd={} turns={} stop={}\n",
            report.cost_usd,
            report.turns.map_or("?".to_owned(), |t| t.to_string()),
            report.stop.word()
        );
        if report.stop != Stop::Finished || ran.code != Some(0) {
            if let Some(c) = ran.code {
                facts.push(Fact::int("agent_exit", i64::from(c)));
            }
            return Err(CallFailure {
                reason: format!(
                    "{summary}the agent did not finish: {}",
                    cap_text(&report.text)
                ),
                cost_micros: Some(report.cost_micros),
                facts,
                profile: label,
            });
        }
        let extra = format!(
            " agent_stop={} cost_micros={}",
            report.stop.word(),
            report.cost_micros
        );
        self.finish_change(
            &name,
            &dir,
            ran.code,
            &cap_text(&report.text),
            report.cost_micros,
            facts,
            extra,
            summary,
        )
        .map(|o| CallOutput {
            profile: label,
            ..o
        })
    }

    fn verify(&self, input: &str) -> Result<CallOutput, CallFailure> {
        let (name, _) = self.job(input)?;
        let (base, tree) = (
            self.recorded(&name, "base.sha")?,
            self.recorded(&name, "base.tree")?,
        );
        let meta = self.meta(&name);
        let patch = fs::read_to_string(meta.join("patch")).unwrap_or_default();
        let changed: Vec<String> = fs::read_to_string(meta.join("changed"))
            .unwrap_or_default()
            .split('\0')
            .filter(|p| !p.is_empty())
            .map(str::to_owned)
            .collect();
        // The tests only count if the agent left the repo's own tests as the template had them.
        let hits = self.violations(&name, &changed)?;
        if let Some(p) = hits.first() {
            return Ok(CallOutput {
                cost_micros: 0,
                facts: vec![Fact::word("tests", "tampered")],
                result: format!("tests=tampered path={p}"),
                evidence: format!(
                    "a protected path differs from the template: {}\ndiff:\n{}",
                    hits.join(", "),
                    cap_text(&patch)
                ),
                profile: self.label(&Profile::Scripted),
            });
        }
        let f = self.fresh(&name, Some(&tree))?;
        if !patch.is_empty() {
            fs::write(f.dir.join("patch.diff"), &patch).map_err(|e| io(&e))?;
            self.git(&f.dir, &["apply", "--whitespace=nowarn", "../patch.diff"])?;
        }
        let marker = random_marker();
        let wrapped = if self.options.trust_exit_code {
            self.config.test_command.clone()
        } else {
            wrap_test(&self.config.test_command, &marker)
        };
        let wrapped_in_use = wrapped != self.config.test_command;
        let ran = self.confined(&f.dir, &wrapped, "", self.config.test_limit)?;
        let complete = !wrapped_in_use || ran.ends_with_line(&marker);
        let shown = ran.all().replace(&marker, "");
        // Facts come from the exit status the capability observed, never from the output's text.
        let pass = ran.code == Some(0) && complete;
        let mut facts = vec![Fact::word("tests", if pass { "pass" } else { "fail" })];
        facts.extend(ran.code.map(|c| Fact::int("exit", i64::from(c))));
        let result = match (ran.code, ran.timed_out) {
            (Some(0), _) if complete => "tests=pass".to_owned(),
            (Some(0), _) => "tests=fail exit=0 incomplete (the test script ended early)".to_owned(),
            (_, true) => "tests=fail exit=timeout".to_owned(),
            (c, _) => format!(
                "tests=fail exit={}",
                c.map_or("signal".to_owned(), |c| c.to_string())
            ),
        };
        Ok(CallOutput {
            cost_micros: 0,
            facts,
            result,
            evidence: format!("base={base}\n{}", shown.trim()),
            profile: self.label(&Profile::Scripted),
        })
    }
}

fn spec(name: &str, band: Reversibility, input: &str, output: &str) -> CallSpec {
    CallSpec {
        name: name.into(),
        band: Some(band),
        input: input.into(),
        output: output.into(),
        tier: Tier::Internal,
        max_cost_micros: 0,
        max_seconds: endora_foundation::domain::DEFAULT_MAX_SECONDS,
        max_turns: None,
        inputs: vec![Field::text("job")],
        facts: Vec::new(),
        // A scripted stand-in: nothing is paid, so nothing needs enforcing (a real agent
        // executor will declare `Enforced` and a non-zero maximum).
        budget: BudgetMode::Unenforced,
    }
}

impl GitChange {
    /// What the evidence records as the profile a call ran under (see [`profile_label`]).
    fn label(&self, p: &Profile) -> Option<String> {
        profile_label(self.confinement, p)
    }

    /// The declaration of `git.change`: free for a scripted agent; for the real executor, a
    /// non-zero maximum that the executor enforces (turns and cost by the CLI, time by us).
    fn change_spec(&self) -> CallSpec {
        let s = spec(
            "git.change",
            Reversibility::Reversible,
            "job name, then (after a newline) the task",
            "job=<name> agent_exit=<n>; the diff in evidence",
        );
        // One text field: the job name, optionally followed by a newline and the task.
        match &self.options.executor {
            AgentExecutor::Command => s.with_facts(vec![Field::int("agent_exit")]),
            AgentExecutor::ClaudeCode(_) => {
                let mut s = s.with_facts(vec![
                    Field::int("agent_exit"),
                    Field::int("agent_turns"),
                    Field::word(
                        "agent_stop",
                        &["finished", "max_turns", "max_budget", "error", "timeout"],
                    ),
                ]);
                s.max_cost_micros = self.options.claude_limits.max_micros;
                s.max_turns = Some(self.options.claude_limits.max_turns);
                s.budget = BudgetMode::Enforced {
                    units: MOST_ENFORCEABLE_MICROS,
                };
                s
            }
        }
    }
}

impl Capability for GitChange {
    fn offers(&self) -> Vec<CallSpec> {
        vec![
            spec(
                "git.prepare",
                Reversibility::Reversible,
                "job name",
                "prepared job=<name> base=<sha>",
            ),
            self.change_spec(),
            spec(
                "git.verify",
                Reversibility::Observe,
                "job name",
                "tests=pass | tests=fail exit=<n> | tests=tampered path=<file>",
            )
            .with_facts(vec![
                Field::word("tests", &["pass", "fail", "tampered"]),
                Field::int("exit"),
            ]),
            spec(
                "git.publish",
                Reversibility::OutwardReversible,
                "job name",
                "nothing: refused",
            ),
        ]
    }

    fn call_budgeted(
        &self,
        call: &str,
        input: &str,
        budget: &CallBudget,
    ) -> Result<CallOutput, CallFailure> {
        match (&self.options.executor, call) {
            (AgentExecutor::ClaudeCode(cc), "git.change") => self.change_claude(cc, input, budget),
            _ => self.call(call, input),
        }
    }

    fn call(&self, call: &str, input: &str) -> Result<CallOutput, CallFailure> {
        match call {
            "git.prepare" => self.prepare(input),
            "git.change" => match &self.options.executor {
                AgentExecutor::Command => self.change(input),
                AgentExecutor::ClaudeCode(cc) => self.change_claude(
                    cc,
                    input,
                    &CallBudget {
                        max_cost_micros: self.options.claude_limits.max_micros,
                        max_seconds: self.config.agent_limit.as_secs(),
                        max_turns: None,
                    },
                ),
            },
            "git.verify" => self.verify(input),
            "git.publish" => Err(fail(
                "publishing is not enabled: this capability holds no remote and no credentials",
            )),
            other => Err(fail(format!("`{other}` is not offered"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protected_rules_match_files_directories_and_case() {
        let rules = vec!["tests/".to_owned(), "test.sh".to_owned()];
        assert!(protected_by(&rules, "tests/a/b.sh").is_some());
        assert!(protected_by(&rules, "Test.sh").is_some());
        assert!(protected_by(&rules, "lib.sh").is_none());
        assert!(protected_by(&rules, "tests-extra/x").is_none());
    }

    #[test]
    fn a_shell_test_is_wrapped_with_the_marker_and_others_are_not() {
        let w = wrap_test(&["sh".into(), "test.sh".into(), "a".into()], "M");
        assert_eq!(w[1], "-c");
        assert!(w[2].contains("echo 'M'") && w[2].contains("exit $rc"));
        assert_eq!((w[3].as_str(), w[4].as_str()), ("./test.sh", "a"));
        let other = vec!["cargo".to_owned(), "test".to_owned()];
        assert_eq!(wrap_test(&other, "M"), other);
        let c = vec!["sh".to_owned(), "-c".to_owned(), "true".to_owned()];
        assert_eq!(wrap_test(&c, "M"), c);
    }

    #[test]
    fn the_marker_is_random_per_call() {
        assert_ne!(random_marker(), random_marker());
    }

    #[test]
    fn the_evidence_profile_names_the_macos_profile_or_the_backend() {
        let real = Profile::RealAgent { deny: vec![] };
        assert_eq!(
            profile_label(Confinement::MacSandbox, &Profile::Scripted).as_deref(),
            Some("scripted-agent")
        );
        assert_eq!(
            profile_label(Confinement::MacSandbox, &real).as_deref(),
            Some("real-agent")
        );
        assert_eq!(
            profile_label(Confinement::Docker, &Profile::Scripted).as_deref(),
            Some("docker")
        );
        assert_eq!(
            profile_label(Confinement::Bubblewrap, &Profile::Scripted).as_deref(),
            Some("bwrap")
        );
        assert_eq!(profile_label(Confinement::Unavailable, &real), None);
    }
}
