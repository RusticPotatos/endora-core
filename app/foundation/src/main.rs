//! `endora-foundation`: the foundation's command line. Plain text in and out over ONE SQLite file
//! (`--db <path>` or `ENDORA_FOUNDATION_DB`). Capabilities come from a small config file
//! (`--config <path>` or `ENDORA_FOUNDATION_CONFIG`) that is not kept in the repository.
//! Exit codes: 0 ok, 1 error (a usage mistake, an unknown command, option or id, a failed run),
//! 2 refused by authority (the reason is printed). `--help` and `--version` print to stdout, exit 0
//! and open no database. Every command line is checked against [`SPECS`] before a database is
//! opened: an unknown command, option or extra word is refused (exit 1) naming itself.

mod probes;

/// Set when a write to stdout failed (a closed pipe, a full disk): the exit code then says so.
static OUT_FAILED: AtomicBool = AtomicBool::new(false);

// `println!` and `print!` panic when stdout is closed (`endora-foundation status | head -0`).
// These versions never panic: a failed write is remembered and ends the run with exit 1 (nothing
// more to say on a stream that is gone). `eprint!`/`eprintln!` ignore a failed write.
macro_rules! println {
    () => { println!("") };
    ($($a:tt)*) => {{
        use std::io::Write as _;
        if writeln!(std::io::stdout(), $($a)*).is_err() {
            OUT_FAILED.store(true, Ordering::Relaxed);
        }
    }};
}
macro_rules! print {
    ($($a:tt)*) => {{
        use std::io::Write as _;
        if write!(std::io::stdout(), $($a)*).is_err() {
            OUT_FAILED.store(true, Ordering::Relaxed);
        }
    }};
}
macro_rules! eprintln {
    ($($a:tt)*) => {{
        use std::io::Write as _;
        let _ = writeln!(std::io::stderr(), $($a)*);
    }};
}

use std::path::{Path, PathBuf};
use std::process::exit;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use signal_hook::consts::{SIGINT, SIGTERM};

use endora_foundation::application::core::ITEM_TAKEN;
use endora_foundation::application::{
    Acceptance, Core, NewGoal, NewItem, Outcome, OwnerProof, WatchEnd,
};
use endora_foundation::domain::{
    AcceptancePolicy, Actor, Check, Criterion, DEFAULT_MAX_FAILURES, GoalId, ItemId, ItemState,
    PolicyRow, RoutineId,
};
use endora_foundation::infrastructure::SystemClock;
use endora_foundation_git::{
    AgentExecutor, ClaudeCode, ClaudeLimits, Config as GitConfig, Confinement, GitChange, Options,
    canonical_deny_paths, mac_profile_can_read,
};
use endora_kernel::{AppError, Clock, RepositoryError, Reversibility, Tier};

use probes::{Kind, Probe, Probes};

/// A command that failed: `Refused` for authority (exit 2), `Error` for anything else (exit 1).
enum Fail {
    Refused(String),
    Error(String),
}
type Out = Result<(), Fail>;

fn err<E: std::fmt::Display>(e: E) -> Fail {
    Fail::Error(e.to_string())
}

/// What the config file asks for.
#[derive(Default)]
struct Settings {
    probes: Vec<Probe>,
    /// Variables a command probe may inherit (`probe-env NAME...`); every other one is cleared.
    probe_env: Vec<String>,
    git: Option<GitConfig>,
    /// `claude-code <binary> <model> <max-micro-usd> <max-turns> [max-seconds]` (amounts in micro-USD: 1 USD = 1,000,000): the real executor for `git.change`.
    claude: Option<(ClaudeCode, ClaudeLimits, Option<Duration>)>,
    /// `claude-credential <file>`: where the CLI's access token is read from at call time.
    credential: Option<PathBuf>,
    /// `claude-arg <word>` (repeatable): extra command-line words for the CLI.
    claude_args: Vec<String>,
}

/// Reads the line-based config: `probe <name> <tcp|http|command> <target...> <interval-seconds>`
/// `probe-env <NAME>...` (variables a command probe may inherit) and
/// `git <scratch-root> <template> -- <agent command line...>`. `#` starts a comment.
fn read_config(path: &Path) -> Result<Settings, String> {
    let text =
        std::fs::read_to_string(path).map_err(|e| format!("config {}: {e}", path.display()))?;
    let mut s = Settings::default();
    for (n, line) in text.lines().enumerate() {
        let line = line.split('#').next().unwrap_or("").trim();
        let t: Vec<&str> = line.split_whitespace().collect();
        let at = |m: &str| format!("config line {}: {m}", n + 1);
        match t.first().copied() {
            None => {}
            Some("probe") if t.len() >= 5 => {
                let kind = match t[2] {
                    "tcp" => Kind::Tcp,
                    "http" => Kind::Http,
                    "command" => Kind::Command,
                    other => return Err(at(&format!("unknown probe kind `{other}`"))),
                };
                let interval_secs = t[t.len() - 1]
                    .parse()
                    .map_err(|_| at("the interval must be seconds"))?;
                s.probes.push(Probe {
                    name: t[1].into(),
                    kind,
                    target: t[3..t.len() - 1].join(" "),
                    interval_secs,
                });
            }
            Some("probe-env") if t.len() >= 2 => {
                s.probe_env.extend(t[1..].iter().map(|n| (*n).to_owned()));
            }
            Some("git") if t.len() >= 5 && t[3] == "--" => {
                s.git = Some(GitConfig {
                    scratch_root: t[1].into(),
                    template: t[2].into(),
                    agent_command: t[4..].iter().map(|w| (*w).to_owned()).collect(),
                    test_command: vec!["sh".into(), "test.sh".into()],
                    protected: vec!["test.sh".into()],
                    agent_limit: Duration::from_secs(600),
                    test_limit: Duration::from_secs(60),
                    confinement: Confinement::Auto,
                });
            }
            Some("claude-code") if t.len() == 5 || t.len() == 6 => {
                let micros = t[3].parse().map_err(|_| {
                    at("max-micro-usd must be a number (micro-USD: 1 USD = 1,000,000)")
                })?;
                let turns = t[4].parse().map_err(|_| at("max-turns must be a number"))?;
                s.claude = Some((
                    ClaudeCode {
                        binary: t[1].into(),
                        model: t[2].into(),
                        extra_args: Vec::new(),
                    },
                    ClaudeLimits {
                        max_micros: micros,
                        max_turns: turns,
                    },
                    t.get(5)
                        .map(|v| {
                            let n: u64 = v.parse().map_err(|_| at("max-seconds must be a number"))?;
                            // The same bound as `--every`: the Docker backend adds a second to it.
                            if (1..=MOST_SECONDS as u64).contains(&n) {
                                Ok(n)
                            } else {
                                Err(at(&format!(
                                    "max-seconds must be between 1 and {MOST_SECONDS} seconds (366 days), not {n}"
                                )))
                            }
                        })
                        .transpose()?
                        .map(Duration::from_secs),
                ));
            }
            Some("claude-credential") if t.len() == 2 => s.credential = Some(t[1].into()),
            Some("claude-arg") if t.len() == 2 => s.claude_args.push(t[1].to_owned()),
            Some(other) => return Err(at(&format!("cannot read `{other}`"))),
        }
    }
    Ok(s)
}

/// The command words after the global flags, and the flags' values.
struct Args {
    words: Vec<String>,
    db: Option<PathBuf>,
    config: Option<PathBuf>,
    token_file: Option<PathBuf>,
    token_out: Option<PathBuf>,
    /// The first thing wrong with the arguments themselves (a global option without its value or
    /// given twice, an argument that is not UTF-8). Reported after help and version: `--help`
    /// (`-h`) is honoured even with such a problem on the line (see `wants_help`), `--version`
    /// only as the sole word.
    problem: Option<String>,
}

/// Reads the arguments as `OsString` (never panics on bytes that are not UTF-8): a word that is
/// not UTF-8 is kept lossily, so it can never equal a command or an option, and is reported.
fn parse_args() -> Args {
    let mut a = Args {
        words: Vec::new(),
        db: None,
        config: None,
        token_file: None,
        token_out: None,
        problem: None,
    };
    let note = |a: &mut Args, m: String| {
        a.problem.get_or_insert(m);
    };
    let mut it = std::env::args_os().skip(1).enumerate();
    let text = |a: &mut Args, n: usize, raw: std::ffi::OsString| -> String {
        raw.into_string().unwrap_or_else(|raw| {
            let shown = raw.to_string_lossy().into_owned();
            a.problem.get_or_insert(format!(
                "argument {} is not valid UTF-8 (shown as {shown:?}): every argument must be UTF-8 text",
                n + 1
            ));
            shown
        })
    };
    while let Some((n, raw)) = it.next() {
        let w = text(&mut a, n, raw);
        let name = w.as_str();
        if !matches!(name, "--db" | "--config" | "--token-file" | "--token-out") {
            a.words.push(w);
            continue;
        }
        let Some((m, value)) = it.next() else {
            note(&mut a, format!("{name} needs a value (a path)"));
            continue;
        };
        let value = PathBuf::from(text(&mut a, m, value));
        let slot = match name {
            "--db" => &mut a.db,
            "--config" => &mut a.config,
            "--token-file" => &mut a.token_file,
            _ => &mut a.token_out,
        };
        if slot.is_some() {
            note(&mut a, format!("{name} given twice"));
        } else {
            *slot = Some(value);
        }
    }
    a
}

/// One command's accepted words: its name (one or two words), each option with the number of
/// values it takes, and how many plain words (ids, amounts) may follow. The global options
/// (`--db`, `--config`, `--token-file`, `--token-out`) are taken out before this.
struct Spec {
    name: &'static str,
    options: &'static [(&'static str, usize)],
    /// The options that must be there.
    required: &'static [&'static str],
    /// The plain words (ids, amounts) the command takes, all required, in order.
    plain: &'static [&'static str],
    /// The options that may be given more than once.
    repeat: &'static [&'static str],
}

const fn spec(
    name: &'static str,
    options: &'static [(&'static str, usize)],
    required: &'static [&'static str],
    plain: &'static [&'static str],
) -> Spec {
    Spec {
        name,
        options,
        required,
        plain,
        repeat: &[],
    }
}

const SPECS: &[Spec] = &[
    Spec {
        repeat: &["--criteria", "--check", "--probe"],
        ..spec(
            "goal add",
            &[
                ("--title", 1),
                ("--why", 1),
                ("--criteria", 1),
                ("--check", 4),
                ("--probe", 1),
                ("--scope", 1),
                ("--tier", 1),
                ("--cap", 1),
                ("--acceptance", 1),
            ],
            &["--title"],
            &[],
        )
    },
    spec(
        "routine add",
        &[
            ("--goal", 1),
            ("--every", 1),
            ("--probe", 1),
            ("--title", 1),
            ("--max-failures", 1),
            ("--auto-accept", 0),
        ],
        &["--goal"],
        &[],
    ),
    spec("routine resume", &[], &[], &["ROUTINE"]),
    spec(
        "git-task",
        &[
            ("--goal", 1),
            ("--repo", 1),
            ("--instruction", 1),
            ("--test-cmd", 1),
            ("--cap", 1),
        ],
        &["--instruction", "--test-cmd"],
        &[],
    ),
    spec("publish", &[], &[], &["ITEM"]),
    spec("run-due", &[], &[], &[]),
    spec("serve", &[("--every", 1)], &["--every"], &[]),
    spec("status", &[("--short", 0)], &[], &[]),
    spec("init", &[], &[], &[]),
    spec("accept", &[], &[], &["ITEM"]),
    spec("decline", &[], &[], &["ITEM"]),
    spec("confirm-criterion", &[], &[], &["ITEM", "N"]),
    spec("confirm", &[], &[], &["ITEM", "N"]),
    spec("stop", &[], &[], &[]),
    spec("resume", &[], &[], &[]),
    spec("overall-cap", &[], &[], &["MICRO_USD"]),
    spec("resume-goal", &[], &[], &["GOAL"]),
    spec("verify-evidence", &[], &[], &[]),
    spec("evidence", &[("--item", 1)], &["--item"], &[]),
    spec("backup", &[("--to", 1)], &["--to"], &[]),
    spec("capabilities", &[], &[], &[]),
];

/// The one-line usage an unknown command is answered with.
const USAGE_LINE: &str = "usage: endora-foundation [--db F] [--config F] [--token-file F] goal add|routine add|routine resume|git-task|publish|run-due|serve|status|init|accept|decline|confirm-criterion|stop|resume|resume-goal|overall-cap|evidence|verify-evidence|backup|capabilities (amounts are micro-USD: 1 USD = 1,000,000; --cap and overall-cap take them; see --help)";

/// What `--help`, `-h` and `help` print (to stdout, exit 0, no database opened).
const HELP: &str = "endora-foundation: the foundation's command line (plain text over one SQLite file)

usage: endora-foundation [--db FILE] [--config FILE] [--token-file FILE] <command> [options]
       endora-foundation --help | -h | help       this text
       endora-foundation --version | -V           the version and the database schema it writes

global options (take a value; may stand anywhere on the line):
  --db FILE           the database (or ENDORA_FOUNDATION_DB)
  --config FILE       the capabilities config (or ENDORA_FOUNDATION_CONFIG)
  --token-file FILE   the owner token file (or ENDORA_OWNER_TOKEN_FILE): a PATH, never the token
  --token-out FILE    init only: where the new owner token is written (mode 600, never overwritten)

commands (OWNER = needs the owner token file; without it, or with a wrong one: exit 2):
  goal add --title T --criteria TEXT [--check CALL INPUT FIELD EXPECT] [--criteria TEXT ...]
           [--why W] [--probe NAME] [--scope a,b] [--tier public|internal|confidential|restricted]
           [--cap MICRO_USD] [--acceptance owner|auto-observe]            OWNER
                      a --check belongs to the --criteria before it
  routine add --goal ID (--every SECONDS | --probe NAME) [--title T] [--max-failures N]
           [--auto-accept]                                                OWNER
  routine resume ID   lift the pause after repeated failures              OWNER
  git-task --instruction T --test-cmd CMD [--goal ID] [--repo PATH] [--cap MICRO_USD]   OWNER
                      prepare, change (the configured agent), verify; waits for the owner
  run-due             fire due routines, start new items, run their checks
  serve --every SECONDS   run-due in a loop; SIGTERM ends it cleanly (SECONDS: 1 to 31622400, as for routine add)
  status [--short]    goals, items, evidence tail (--short: without the tail), open decisions
  init --token-out FILE   once: make the owner credential
  accept ITEM         the owner accepts an item whose criteria are all met        OWNER
  decline ITEM        the owner declines an item                                 OWNER
  confirm-criterion ITEM N   the owner confirms criterion N (numbered from 1)    OWNER
                      (`confirm` is a deprecated alias that warns)
  stop                stop switch on (anyone may)
  resume              stop switch off                                            OWNER
  resume-goal GOAL    lift the stop a cost violation put on a goal               OWNER
  overall-cap MICRO_USD   the most all goals may spend together (starts at 0)    OWNER
  publish ITEM        refused (exit 2): nothing here publishes; a missing or non-numeric ITEM
                      is a usage error (exit 1)
  evidence --item ID  every record about an item, in full
  verify-evidence     recompute the evidence hash chain; exit 1 naming the first broken row
  backup --to FILE    consistent copy (mode 600; an existing FILE is never overwritten)
  capabilities        every registered call with its band, tier and maximum cost

amounts are micro-USD: 1 USD = 1,000,000.

environment:
  ENDORA_FOUNDATION_DB                the database, when --db is not given
  ENDORA_FOUNDATION_CONFIG            the capabilities config, when --config is not given
  ENDORA_OWNER_TOKEN_FILE             the path of the owner token file, when --token-file is not given
  ENDORA_FOUNDATION_EXECUTOR_IMAGE    the container image of the Docker confinement backend

exit codes: 0 ok, 1 error (a mistake in the command line, an unknown id, a failed run),
            2 refused by authority (the reason is printed).
An unknown command, option, repeated option, extra or missing word, or an argument that is not
UTF-8 is refused with exit 1 and named; nothing is opened.
";

/// The command's [`Spec`], by its first (and for `goal` and `routine` second) word.
fn spec_of(w: &[String]) -> Option<&'static Spec> {
    SPECS.iter().find(|s| {
        let mut name = s.name.split(' ');
        w.first().map(String::as_str) == name.next()
            && name
                .next()
                .is_none_or(|second| w.get(1).map(String::as_str) == Some(second))
    })
}

/// True when help was asked for: `help` or a help flag as the first word, or a help flag anywhere
/// on the line of a command that exists (never as the value of an option). `bogus --help` is not
/// help: it is an unknown command.
fn wants_help(w: &[String]) -> bool {
    let is_help = |x: &str| x == "--help" || x == "-h";
    if w.first().is_some_and(|x| x == "help" || is_help(x)) {
        return true;
    }
    let Some(spec) = spec_of(w) else {
        // `goal --help` and `routine -h`: the first word of a two-word command.
        return matches!(w.first().map(String::as_str), Some("goal" | "routine"))
            && w.len() == 2
            && is_help(&w[1]);
    };
    let mut i = spec.name.split(' ').count();
    while i < w.len() {
        if is_help(&w[i]) {
            return true;
        }
        i += 1 + spec
            .options
            .iter()
            .find(|(n, _)| *n == w[i])
            .map_or(0, |(_, n)| *n);
    }
    false
}

/// Checks a command line against its [`Spec`] before anything is opened. An unknown command, an
/// unknown option, an option without its value or given twice, an extra word or a missing one is
/// an error (exit 1) that names the offender.
fn check_words(w: &[String]) -> Result<(), Fail> {
    let Some(spec) = spec_of(w) else {
        return Err(Fail::Error(match w.first() {
            None => format!("no command given; {USAGE_LINE}"),
            Some(_) => format!(
                "unknown command `{}`; {USAGE_LINE}",
                w.iter()
                    .take(
                        if matches!(w[0].as_str(), "goal" | "routine")
                            && !w[1..].is_empty()
                            && !w[1].starts_with('-')
                        {
                            2
                        } else {
                            1
                        }
                    )
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(" ")
            ),
        }));
    };
    let takes = |o: &str| spec.options.iter().find(|(n, _)| *n == o).map(|(_, n)| *n);
    let (mut plain, mut seen, mut i) = (0, Vec::new(), spec.name.split(' ').count());
    while i < w.len() {
        let x = w[i].as_str();
        if x.starts_with("--") {
            let n = takes(x).ok_or_else(|| {
                Fail::Error(format!("unknown option {x} for {} (see --help)", spec.name))
            })?;
            if i + n >= w.len() {
                return Err(Fail::Error(if x == "--check" {
                    "--check needs CALL INPUT FIELD EXPECT".into()
                } else if n == 1 {
                    format!("{x} needs a value")
                } else {
                    format!("{x} needs {n} values")
                }));
            }
            if seen.contains(&x) && !spec.repeat.contains(&x) {
                return Err(Fail::Error(format!(
                    "{x} given twice for {} (see --help)",
                    spec.name
                )));
            }
            seen.push(x);
            i += 1 + n;
        } else {
            plain += 1;
            if plain > spec.plain.len() {
                return Err(Fail::Error(format!(
                    "unexpected argument `{x}` for {} (see --help)",
                    spec.name
                )));
            }
            i += 1;
        }
    }
    if let Some(name) = spec.plain.get(plain) {
        return Err(Fail::Error(format!("missing {name} for {}", spec.name)));
    }
    if let Some(name) = spec.required.iter().find(|r| !seen.contains(r)) {
        return Err(Fail::Error(format!("missing {name} for {}", spec.name)));
    }
    Ok(())
}

/// `rusqlite` says `Query returned no rows` for an id that is not there.
fn is_gone(e: &RepositoryError) -> bool {
    matches!(e, RepositoryError::Backend(m) if m.contains("no rows"))
}

/// The item's state, or `no item N` (exit 1) instead of the engine's words.
fn need_item(core: &Core, id: ItemId) -> Result<ItemState, Fail> {
    core.store().item(id).map(|i| i.state).map_err(|e| {
        if is_gone(&e) {
            Fail::Error(format!("no item {}", id.0))
        } else {
            err(e)
        }
    })
}

fn need_goal(core: &Core, id: GoalId) -> Result<(), Fail> {
    core.store().goal(id).map(|_| ()).map_err(|e| {
        if is_gone(&e) {
            Fail::Error(format!("no goal {}", id.0))
        } else {
            err(e)
        }
    })
}

fn need_routine(core: &Core, id: RoutineId) -> Result<(), Fail> {
    core.store().routine(id).map(|_| ()).map_err(|e| {
        if is_gone(&e) {
            Fail::Error(format!("no routine {}", id.0))
        } else {
            err(e)
        }
    })
}

/// A move the item's state does not allow (the engine calls it `Corrupt`, because the store
/// cannot tell a lost race from damage) is said as what it is, from the item's own state; only
/// a real storage fault keeps the engine's words. `verb` is what the owner asked for.
fn refused_move(core: &Core, id: ItemId, e: AppError, verb: &str) -> Fail {
    let AppError::Repository(RepositoryError::Corrupt(m)) = &e else {
        return err(e);
    };
    let a_move = m.starts_with("cannot transition")
        || m.contains("only a running item")
        || m.contains("changed under the move")
        || m.contains("done is reached only");
    let Some(state) = a_move
        .then(|| core.store().item(id).ok().map(|i| i.state))
        .flatten()
    else {
        return err(e);
    };
    let name = state.name();
    Fail::Error(match (verb, state) {
        (_, ItemState::Done | ItemState::Failed) if verb == "decline" => {
            format!("item {} is already {name}", id.0)
        }
        ("accept", ItemState::Done) => format!("item {} is already done", id.0),
        ("accept", _) => format!(
            "item {} is {name}, not running: only a running item can be accepted",
            id.0
        ),
        ("confirm", _) => format!(
            "item {} is {name}: only a running item's criteria can be confirmed",
            id.0
        ),
        _ => format!("item {} is {name}: it cannot be {verb}d", id.0),
    })
}

/// The value after `--flag`, if present.
fn flag<'a>(w: &'a [String], name: &str) -> Option<&'a str> {
    w.iter()
        .position(|x| x == name)
        .and_then(|i| w.get(i + 1))
        .map(String::as_str)
}

fn need<'a>(w: &'a [String], name: &str) -> Result<&'a str, Fail> {
    flag(w, name).ok_or_else(|| Fail::Error(format!("missing {name}")))
}

/// The longest rhythm a routine or a `serve` loop may have: 366 days, the engine's own limit.
const MOST_SECONDS: i64 = 366 * 24 * 3600;

/// A rhythm in seconds: a whole number from 1 to [`MOST_SECONDS`].
fn seconds(s: &str, what: &str) -> Result<i64, Fail> {
    let n = num(s, what)?;
    if (1..=MOST_SECONDS).contains(&n) {
        Ok(n)
    } else {
        Err(Fail::Error(format!(
            "{what} must be between 1 and {MOST_SECONDS} seconds (366 days), not {n}"
        )))
    }
}

fn num(s: &str, what: &str) -> Result<i64, Fail> {
    s.parse()
        .map_err(|_| Fail::Error(format!("{what} must be a whole number, not `{s}`")))
}

fn tier_of(name: &str) -> Result<Tier, Fail> {
    Ok(match name {
        "public" => Tier::Public,
        "internal" => Tier::Internal,
        "confidential" => Tier::Confidential,
        "restricted" => Tier::Restricted,
        other => return Err(Fail::Error(format!("unknown tier `{other}`"))),
    })
}

/// The criterion a probe implies: tcp and http are up (http: status 200), a command exits 0.
fn probe_criterion(p: &Probe) -> Criterion {
    let (field, expect) = match p.kind {
        Kind::Tcp => ("up", "true"),
        Kind::Http => ("status", "200"),
        Kind::Command => ("exit", "0"),
    };
    Criterion {
        text: format!("probe {} is healthy", p.name),
        check: Some(Check {
            call: "probe.run".into(),
            input: p.name.clone(),
            field: field.into(),
            expect: expect.into(),
        }),
    }
}

fn goal_add(core: &Core, proof: &OwnerProof, s: &Settings, w: &[String]) -> Out {
    let title = need(w, "--title")?;
    let mut criteria: Vec<Criterion> = Vec::new();
    let mut i = 0;
    while i < w.len() {
        match w[i].as_str() {
            "--criteria" => {
                let text = w
                    .get(i + 1)
                    .ok_or_else(|| Fail::Error("--criteria needs a text".into()))?;
                criteria.push(Criterion {
                    text: text.clone(),
                    check: None,
                });
                i += 2;
            }
            "--check" => {
                let [call, input, field, expect] = w
                    .get(i + 1..i + 5)
                    .and_then(|x| <&[String; 4]>::try_from(x).ok())
                    .ok_or_else(|| Fail::Error("--check needs CALL INPUT FIELD EXPECT".into()))?
                    .clone();
                let last = criteria
                    .last_mut()
                    .ok_or_else(|| Fail::Error("--check follows a --criteria".into()))?;
                last.check = Some(Check {
                    call,
                    input,
                    field,
                    expect,
                });
                i += 5;
            }
            "--probe" => {
                let name = w
                    .get(i + 1)
                    .ok_or_else(|| Fail::Error("--probe needs a name".into()))?;
                let p =
                    s.probes.iter().find(|p| &p.name == name).ok_or_else(|| {
                        Fail::Error(format!("`{name}` is not a probe in the config"))
                    })?;
                criteria.push(probe_criterion(p));
                i += 2;
            }
            _ => i += 1,
        }
    }
    let tier = tier_of(flag(w, "--tier").unwrap_or("internal"))?;
    let mut scope: Vec<String> = flag(w, "--scope")
        .map(|v| v.split(',').map(str::to_owned).collect())
        .unwrap_or_default();
    let checks: Vec<String> = criteria
        .iter()
        .filter_map(|c| c.check.as_ref().map(|k| k.call.clone()))
        .collect();
    for c in &checks {
        if !scope.contains(c) {
            scope.push(c.clone());
        }
    }
    let cap = flag(w, "--cap").map_or(Ok(0), |v| num(v, "--cap"))?;
    let cap = u64::try_from(cap).map_err(|_| Fail::Error("--cap must not be negative".into()))?;
    let acceptance = match flag(w, "--acceptance") {
        None => AcceptancePolicy::Owner,
        Some(name) => AcceptancePolicy::from_name(name).ok_or_else(|| {
            Fail::Error(format!(
                "unknown acceptance `{name}` (owner or auto-observe)"
            ))
        })?,
    };
    let id = core
        .add_goal_with_policy(
            proof,
            NewGoal {
                title: title.into(),
                why: flag(w, "--why").unwrap_or(title).into(),
                criteria,
                scope,
                tier,
                spend_cap_micros: cap,
            },
            acceptance,
        )
        .map_err(err)?;
    // The owner wrote these checks, so Core may make their observe-band calls to verify them.
    for call in checks {
        if let Some(spec) = core
            .spec(&call)
            .filter(|sp| sp.band == Some(Reversibility::Observe))
        {
            core.grant(
                proof,
                PolicyRow {
                    actor: "core".into(),
                    call,
                    band: Reversibility::Observe,
                    tier: tier.max(spec.tier),
                    max_uses_per_item: None,
                },
            )
            .map_err(err)?;
        }
    }
    println!("goal {}", id.0);
    Ok(())
}

/// Runs one call as the git task's runner and returns its result line, or why it did not run.
fn task_call(
    core: &Core,
    runner: &Actor,
    item: ItemId,
    call: &str,
    input: &str,
) -> Result<String, Fail> {
    match core.call(runner, item, call, input).map_err(err)? {
        Outcome::Done(r) => Ok(r.output.result),
        Outcome::Refused(why) => {
            let _ = core.fail(runner, item, why.said());
            Err(Fail::Refused(format!("{call}: {}", why.said())))
        }
        Outcome::Failed(why) => {
            let _ = core.fail(runner, item, &format!("{call} failed: {why}"));
            Err(Fail::Error(format!("{call} failed: {why}")))
        }
        Outcome::Violation { .. } => Err(Fail::Error(format!("{call} broke its cost declaration"))),
        Outcome::Contract(why) => Err(Fail::Error(format!(
            "{call} broke its declared contract: {why}"
        ))),
    }
}

/// Applies `git-task`'s `--repo` and `--test-cmd` to the configured git capability. Protected
/// files are the test command's words that are files of the repo, else `test.sh`.
fn task_config(mut cfg: GitConfig, w: &[String]) -> Result<GitConfig, Fail> {
    if let Some(repo) = flag(w, "--repo") {
        cfg.template = repo.into();
    }
    let cmd = need(w, "--test-cmd")?;
    cfg.test_command = cmd.split_whitespace().map(str::to_owned).collect();
    if cfg.test_command.is_empty() {
        return Err(Fail::Error("--test-cmd is empty".into()));
    }
    let protected: Vec<String> = cfg
        .test_command
        .iter()
        .filter(|x| cfg.template.join(x).is_file())
        .cloned()
        .collect();
    cfg.protected = if protected.is_empty() {
        vec!["test.sh".into()]
    } else {
        protected
    };
    Ok(cfg)
}

/// A change task from the command line: an item on a goal, then prepare, a first test run, the
/// configured agent's change, and the engine's own verification. The item is left awaiting the
/// OWNER: a git task is reversible work, which no policy accepts.
fn git_task(core: &Core, proof: &OwnerProof, w: &[String]) -> Out {
    let runner = core.capability_actor("runner").map_err(err)?;
    let instruction = need(w, "--instruction")?;
    need(w, "--test-cmd")?;
    if core.spec("git.change").is_none() {
        return Err(Fail::Error(
            "git-task needs a `git` line in the config (scratch root, template, agent command)"
                .into(),
        ));
    }
    let goal = match flag(w, "--goal") {
        Some(g) => GoalId(num(g, "--goal")?),
        None => {
            let id = core
                .add_goal(
                    proof,
                    NewGoal {
                        title: format!(
                            "git task: {}",
                            instruction.chars().take(50).collect::<String>()
                        ),
                        why: instruction.into(),
                        criteria: vec![Criterion {
                            text: "the repo's own tests pass".into(),
                            check: Some(Check {
                                call: "git.verify".into(),
                                input: "{subject}".into(),
                                field: "tests".into(),
                                expect: "pass".into(),
                            }),
                        }],
                        scope: ["git.prepare", "git.change", "git.verify"]
                            .map(str::to_owned)
                            .to_vec(),
                        tier: Tier::Internal,
                        spend_cap_micros: match flag(w, "--cap") {
                            Some(c) => u64::try_from(num(c, "--cap")?)
                                .map_err(|_| Fail::Error("--cap must not be negative".into()))?,
                            None => 0,
                        },
                    },
                )
                .map_err(err)?;
            println!("goal {}", id.0);
            id
        }
    };
    need_goal(core, goal)?;
    let now = SystemClock.now().unix_millis();
    // The owner is running this command, so the owner grants the runner exactly what a task needs.
    for (actor, call, band, uses) in [
        ("runner", "git.prepare", Reversibility::Reversible, Some(1)),
        ("runner", "git.change", Reversibility::Reversible, Some(1)),
        ("runner", "git.verify", Reversibility::Observe, None),
        ("core", "git.verify", Reversibility::Observe, None),
    ] {
        core.grant(
            proof,
            PolicyRow {
                actor: actor.into(),
                call: call.into(),
                band,
                tier: Tier::Internal,
                max_uses_per_item: uses,
            },
        )
        .map_err(err)?;
    }
    // One repository per job: the name is unique per run, and it is the item's subject.
    let job = format!("job-{}-{}", now / 1000, std::process::id());
    let item = core
        .add_item_with(NewItem {
            goal,
            title: instruction.chars().take(80).collect(),
            subject: Some(job.clone()),
            criteria: None,
        })
        .map_err(err)?;
    core.start(&runner, item).map_err(err)?;
    println!("item {} job {job}", item.0);
    println!(
        "prepare: {}",
        task_call(core, &runner, item, "git.prepare", &job)?
    );
    println!(
        "before: {}",
        task_call(core, &runner, item, "git.verify", &job)?
    );
    let changed = task_call(
        core,
        &runner,
        item,
        "git.change",
        &format!("{job}\n{instruction}"),
    )?;
    println!("change: {changed}");
    let met = core.verify(item).map_err(err)?;
    let n = met.iter().filter(|m| **m).count();
    println!(
        "after: {}",
        task_call(core, &runner, item, "git.verify", &job)?
    );
    if n == met.len() {
        println!(
            "every criterion met; awaiting owner acceptance: accept {}",
            item.0
        );
        Ok(())
    } else {
        Err(Fail::Error(format!(
            "criterion not met ({n}/{}): item {} stays for the owner (decline {} or confirm)",
            met.len(),
            item.0,
            item.0
        )))
    }
}

fn routine_add(core: &Core, proof: &OwnerProof, s: &Settings, w: &[String]) -> Out {
    let goal = GoalId(num(need(w, "--goal")?, "--goal")?);
    need_goal(core, goal)?;
    let secs = match (flag(w, "--every"), flag(w, "--probe")) {
        (Some(v), _) => seconds(v, "--every")?,
        (None, Some(name)) => {
            let found = s
                .probes
                .iter()
                .find(|p| p.name == name)
                .map(|p| p.interval_secs)
                .ok_or_else(|| Fail::Error(format!("`{name}` is not a probe in the config")))?;
            seconds(
                &found.to_string(),
                &format!("the interval of probe `{name}`"),
            )?
        }
        (None, None) => {
            return Err(Fail::Error(
                "routine add needs --every <seconds> or --probe <name>".into(),
            ));
        }
    };
    let title = match flag(w, "--title") {
        Some(t) => t.to_owned(),
        None => core.store().goal(goal).map_err(err)?.title,
    };
    let now = SystemClock.now().unix_millis();
    let max_failures = flag(w, "--max-failures")
        .map_or(Ok(i64::from(DEFAULT_MAX_FAILURES)), |v| {
            num(v, "--max-failures")
        })?;
    let max_failures = u32::try_from(max_failures).map_err(|_| {
        Fail::Error(if max_failures < 0 {
            "--max-failures must not be negative".into()
        } else {
            format!("--max-failures is too large (at most {})", u32::MAX)
        })
    })?;
    let id = core
        .add_routine_limited(
            proof,
            goal,
            &title,
            secs * 1000,
            w.iter().any(|x| x == "--auto-accept"),
            now,
            max_failures,
        )
        .map_err(err)?;
    println!("routine {}", id.0);
    Ok(())
}

/// Runs what is due and prints what it did. `Ok(true)` when it did or said anything, `Ok(false)`
/// when there was nothing to do (`run-due` then says so; `serve` stays quiet).
fn run_due(core: &Core) -> Result<bool, Fail> {
    let mut acted = false;
    let runner = core.capability_actor("runner").map_err(err)?;
    if core.store().control("stop").map_err(err)? != 0 {
        return Err(Fail::Refused(
            "refused: the owner's stop switch is on (run `resume`)".into(),
        ));
    }
    for r in core.recover_interrupted().map_err(err)? {
        acted = true;
        println!("recovered item {} -> {}: {}", r.item.0, r.now.name(), r.why);
    }
    // An auto-observe item a killed process left after its last check and before its
    // acceptance is settled now, not left for the owner for ever.
    for (id, got) in core.settle_stranded_auto_observe().map_err(err)? {
        acted = true;
        match got {
            Acceptance::Accepted { by } => println!("item {} accepted by {by}", id.0),
            Acceptance::Refused(why) => {
                println!("item {} stays for the owner: {}", id.0, why.said());
            }
        }
    }
    for f in core.fire_due().map_err(err)? {
        acted = true;
        println!(
            "routine {}: item {} missed {} skipped_open {}",
            f.routine.0,
            f.item.map_or("-".into(), |i| i.0.to_string()),
            f.missed,
            f.skipped_open
        );
    }
    // Open items start; routine items a dead process left running with an unmet check are
    // re-checked and closed too, so a routine never waits on one forever.
    let mut todo: Vec<(ItemId, Option<RoutineId>, bool)> = Vec::new();
    for it in core.store().items_in(ItemState::Open).map_err(err)? {
        todo.push((it.id, it.routine, true));
    }
    for id in core.stuck_watch_items().map_err(err)? {
        let routine = core.store().item(id).map_err(err)?.routine;
        todo.push((id, routine, false));
    }
    for (id, routine, fresh) in todo {
        acted = true;
        if fresh {
            match core.start(&runner, id) {
                Ok(()) => {}
                // Another process listed the same open item and started it first. Nothing was
                // written here; the item is theirs. Skip it and carry on (exit status is not
                // affected): a lost race on one item is not a failure of the run.
                Err(AppError::Conflict { code, message }) if code == ITEM_TAKEN => {
                    println!("item {}: skipped: {message}", id.0);
                    continue;
                }
                Err(e) => return Err(err(e)),
            }
        }
        let (met, end) = core.check_watch(id).map_err(err)?;
        println!(
            "item {}: {}/{} criteria met",
            id.0,
            met.iter().filter(|m| **m).count(),
            met.len()
        );
        if let WatchEnd::Failed { failures, paused } = end {
            println!("item {} failed (failure {failures} in a row)", id.0);
            if paused {
                println!(
                    "routine {} PAUSED after {failures} failures in a row: `routine resume {}`",
                    routine.map_or(0, |r| r.0),
                    routine.map_or(0, |r| r.0)
                );
            }
            continue;
        }
        // A goal set to `auto-observe` settles a verified observe-only item itself; anything
        // else stays running as an open decision for the owner.
        match core.settle_by_policy(id).map_err(err)? {
            Some(Acceptance::Accepted { by }) => {
                println!("item {} accepted by {by}", id.0);
                continue;
            }
            Some(Acceptance::Refused(why)) => {
                println!("item {} stays for the owner: {}", id.0, why.said());
                continue;
            }
            None => {}
        }
        let auto = match routine {
            Some(r) => core.store().routine(r).map_err(err)?.auto_accept,
            None => false,
        };
        if auto {
            if let Acceptance::Accepted { by } = core.settle_routine_by_policy(id).map_err(err)? {
                println!("item {} accepted by {by}", id.0);
            }
        }
    }
    Ok(acted)
}

/// `run-due` as a command: what `run_due` printed, or one line saying why it printed nothing.
fn run_due_command(core: &Core) -> Out {
    if !run_due(core)? {
        let n = core.store().routines().map_err(err)?.len();
        if n == 0 {
            println!("no routines: a goal needs a routine to make items");
        } else {
            let routines = core.store().routines().map_err(err)?;
            let paused: Vec<String> = routines
                .iter()
                .filter(|r| r.paused)
                .map(|r| r.id.0.to_string())
                .collect();
            let waiting = core
                .store()
                .items_in(ItemState::Running)
                .map_err(err)?
                .len();
            let mut said = format!("nothing due ({n} routine{})", if n == 1 { "" } else { "s" });
            if !paused.is_empty() {
                said.push_str(&format!(
                    "; PAUSED: routine {} (routine resume ID)",
                    paused.join(", ")
                ));
            }
            if waiting > 0 {
                said.push_str(&format!(
                    "; {waiting} item{} waiting for the owner (see status)",
                    if waiting == 1 { "" } else { "s" }
                ));
            }
            println!("{said}");
        }
    }
    Ok(())
}

/// `serve --every <seconds>`: `run-due` in a loop, for a container or a service manager. The owner's
/// stop switch pauses it (said once per pause) instead of ending it; SIGTERM or SIGINT ends it
/// cleanly between two runs (exit 0). A failed run is reported and the loop goes on.
fn serve(core: &Core, w: &[String]) -> Out {
    let every = seconds(need(w, "--every")?, "--every")?;
    let stop = Arc::new(AtomicBool::new(false));
    for sig in [SIGTERM, SIGINT] {
        signal_hook::flag::register(sig, Arc::clone(&stop)).map_err(err)?;
    }
    println!("serving: run-due every {every} s (SIGTERM stops)");
    let mut paused = false;
    while !stop.load(Ordering::Relaxed) {
        match run_due(core) {
            Ok(_) => paused = false,
            Err(Fail::Refused(why)) => {
                if !paused {
                    println!("{why}");
                }
                paused = true;
            }
            Err(Fail::Error(why)) => println!("run-due failed: {why}"),
        }
        let until = Instant::now()
            .checked_add(Duration::from_secs(u64::try_from(every).unwrap_or(1)))
            .ok_or_else(|| Fail::Error("--every is too long for this clock".into()))?;
        while Instant::now() < until && !stop.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    println!("stopping on signal");
    Ok(())
}

fn status(core: &Core, short: bool) -> Out {
    let text = core.status_text(if short { 0 } else { 15 }).map_err(err)?;
    // `--short` has no evidence tail at all, not an empty header.
    print!(
        "{}",
        if short {
            text.split("\nEVIDENCE (tail)").next().unwrap_or(&text)
        } else {
            &text
        }
    );
    println!("\nROUTINES");
    for r in core.store().routines().map_err(err)? {
        println!(
            "  routine {} goal #{} every {} s: {}",
            r.id.0,
            r.goal.0,
            r.every_ms / 1000,
            if r.paused {
                format!(
                    "PAUSED after {} failures in a row (routine resume {})",
                    r.consecutive_failures, r.id.0
                )
            } else {
                format!(
                    "running, failures {}/{}",
                    r.consecutive_failures, r.max_failures
                )
            }
        );
    }
    println!("\nOPEN DECISIONS");
    let mut none = true;
    for it in core.store().items_in(ItemState::Running).map_err(err)? {
        let c = core.store().criteria(it.id).map_err(err)?;
        let met = c.iter().filter(|k| k.met_by.is_some()).count();
        none = false;
        if met == c.len() {
            println!(
                "  item {}: every criterion met. accept {} | decline {}",
                it.id.0, it.id.0, it.id.0
            );
        } else {
            println!(
                "  item {}: {met}/{} criteria met. confirm {} <n> | decline {}",
                it.id.0,
                c.len(),
                it.id.0,
                it.id.0
            );
        }
    }
    for g in core.store().goals().map_err(err)? {
        if core
            .store()
            .control(&format!("goal_stop_{}", g.id.0))
            .map_err(err)?
            != 0
        {
            none = false;
            println!(
                "  goal {} is stopped after a cost violation. resume-goal {}",
                g.id.0, g.id.0
            );
        }
    }
    if none {
        println!("  none");
    }
    Ok(())
}

/// Where the owner token file is named: `--token-file <path>` or `ENDORA_OWNER_TOKEN_FILE`
/// (a PATH only: the token itself is never an argument or an environment value).
struct Auth {
    token_file: Option<PathBuf>,
}

impl Auth {
    /// Reads the token file and proves the owner, or says (exit 2) why not. No message here
    /// contains the token.
    fn prove(&self, core: &Core) -> Result<OwnerProof, Fail> {
        let path = self
            .token_file
            .clone()
            .or_else(|| std::env::var_os("ENDORA_OWNER_TOKEN_FILE").map(PathBuf::from))
            .ok_or_else(|| {
                Fail::Refused(
                    "this command needs the owner token: pass --token-file <path> or set ENDORA_OWNER_TOKEN_FILE to the path of the token file (made by `init --token-out`)"
                        .into(),
                )
            })?;
        check_token_file_mode(&path)?;
        let token = std::fs::read_to_string(&path).map_err(|e| {
            Fail::Refused(format!(
                "cannot read the owner token file {}: {}",
                path.display(),
                e.kind()
            ))
        })?;
        core.authenticate_owner(token.trim())
            .map_err(|e| Fail::Refused(format!("owner authentication failed: {e}")))
    }
}

#[cfg(unix)]
fn check_token_file_mode(path: &Path) -> Result<(), Fail> {
    use std::os::unix::fs::PermissionsExt;
    let mode = std::fs::metadata(path)
        .map_err(|e| {
            Fail::Refused(format!(
                "cannot read the owner token file {}: {}",
                path.display(),
                e.kind()
            ))
        })?
        .permissions()
        .mode();
    if mode & 0o077 != 0 {
        return Err(Fail::Refused(format!(
            "the owner token file {} is readable by others (mode {:o}): run chmod 600 on it",
            path.display(),
            mode & 0o777
        )));
    }
    Ok(())
}

#[cfg(not(unix))]
fn check_token_file_mode(_path: &Path) -> Result<(), Fail> {
    Ok(())
}

/// `init --token-out <path>`: makes the owner credential if none exists. The token goes to the
/// file (mode 600, never overwritten) and nowhere else: not stdout, not stderr, not the log.
fn init(core: &Core, out: Option<&Path>) -> Out {
    if core.owner_enrolled().map_err(err)? {
        // Exit 2, never 0: the caller asked for a token and none was written (REVIEW3 finding 7).
        // Enrolment belongs to a trusted setup context; the engine does not try to prove who
        // enrolled first.
        return Err(Fail::Refused(
            "already enrolled: no token written; enrolment is a trusted setup step".into(),
        ));
    }
    let out = out.ok_or_else(|| Fail::Error("init needs --token-out <path>".into()))?;
    let token = Core::generate_owner_token().map_err(err)?;
    write_new_private_file(out, &token)?;
    if let Err(e) = core.enroll_owner(&token) {
        let _ = std::fs::remove_file(out);
        return Err(err(e));
    }
    println!(
        "owner credential created; the token is in {}",
        out.display()
    );
    Ok(())
}

fn write_new_private_file(path: &Path, text: &str) -> Result<(), Fail> {
    use std::io::Write;
    let mut o = std::fs::OpenOptions::new();
    o.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        o.mode(0o600);
    }
    let mut f = o.open(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::AlreadyExists {
            Fail::Error(format!(
                "refusing to overwrite {}: it already exists",
                path.display()
            ))
        } else {
            Fail::Error(format!("cannot create {}: {}", path.display(), e.kind()))
        }
    })?;
    f.write_all(text.as_bytes()).map_err(err)
}

fn dispatch(core: &Core, auth: &Auth, s: &Settings, db: &Path, w: &[String]) -> Out {
    let item = |i: usize| -> Result<ItemId, Fail> {
        Ok(ItemId(num(
            w.get(i)
                .ok_or_else(|| Fail::Error("missing item id".into()))?,
            "item id",
        )?))
    };
    match (w.first().map(String::as_str), w.get(1).map(String::as_str)) {
        (Some("goal"), Some("add")) => goal_add(core, &auth.prove(core)?, s, w),
        (Some("routine"), Some("add")) => routine_add(core, &auth.prove(core)?, s, w),
        (Some("routine"), Some("resume")) => {
            let id = RoutineId(num(w.get(2).map_or("", String::as_str), "routine id")?);
            let proof = auth.prove(core)?;
            need_routine(core, id)?;
            core.resume_routine(&proof, id).map_err(err)?;
            println!("routine {} resumed", id.0);
            Ok(())
        }
        (Some("git-task"), _) => git_task(core, &auth.prove(core)?, w),
        (Some("publish"), _) => {
            // Publishing is refused by authority: there is no row, and the call is outward-reversible.
            // Always exit 2, also for an item that does not exist (the refusal does not depend on it).
            let id = item(1)?;
            if let Err(Fail::Error(m)) = need_item(core, id) {
                // Only an absent item is "no item"; damaged data says so (still a refusal: exit 2).
                let why = if m.starts_with("no item") {
                    format!("there is no item {}", id.0)
                } else {
                    format!("item {} cannot be read: {m}", id.0)
                };
                return Err(Fail::Refused(format!(
                    "refused: nothing here publishes (and {why})"
                )));
            }
            let it = core.store().item(id).map_err(err)?;
            let job = it.subject.unwrap_or_default();
            match core
                .call(
                    &core.capability_actor("publish-request").map_err(err)?,
                    it.id,
                    "git.publish",
                    &job,
                )
                .map_err(err)?
            {
                // `said()` already starts with `refused:`.
                Outcome::Refused(why) => Err(Fail::Refused(why.said().to_owned())),
                other => Err(Fail::Refused(format!(
                    "refused: publishing is not enabled ({other:?})"
                ))),
            }
        }
        (Some("run-due"), _) => run_due_command(core),
        (Some("serve"), _) => serve(core, w),
        (Some("status"), _) => status(core, w.iter().any(|x| x == "--short")),
        (Some("accept"), _) => {
            let proof = auth.prove(core)?;
            let id = item(1)?;
            let state = need_item(core, id)?;
            match core
                .accept(&proof, id)
                .map_err(|e| refused_move(core, id, e, "accept"))?
            {
                Acceptance::Accepted { by } => {
                    println!("item {} accepted by {by}", id.0);
                    Ok(())
                }
                // Still exit 2 (an owner accept refused by the rules); the item's own state is said.
                Acceptance::Refused(_) if state == ItemState::Done => Err(Fail::Refused(format!(
                    "refused: item {} is already done",
                    id.0
                ))),
                Acceptance::Refused(_) if state != ItemState::Running => {
                    Err(Fail::Refused(format!(
                        "refused: item {} is {}, not running: only a running item can be accepted",
                        id.0,
                        state.name()
                    )))
                }
                Acceptance::Refused(why) => Err(Fail::Refused(why.said().into())),
            }
        }
        (Some("decline"), _) => {
            let proof = auth.prove(core)?;
            let id = item(1)?;
            need_item(core, id)?;
            core.decline(&proof, id, "declined by the owner")
                .map_err(|e| refused_move(core, id, e, "decline"))?;
            println!("item {} declined", id.0);
            Ok(())
        }
        (Some("confirm-criterion"), _) | (Some("confirm"), _) => {
            if w[0] == "confirm" {
                eprintln!(
                    "warning: `confirm` is deprecated and will be removed in the next release: use `confirm-criterion` (it confirms a criterion of an item, never a call)"
                );
            }
            let n = num(w.get(2).map_or("", String::as_str), "criterion number")?;
            let proof = auth.prove(core)?;
            let id = item(1)?;
            let state = need_item(core, id)?;
            // The engine would still change a finished item's criteria; the command line does not.
            if matches!(state, ItemState::Done | ItemState::Failed) {
                return Err(Fail::Error(format!(
                    "item {} is already {}: its criteria cannot be changed",
                    id.0,
                    state.name()
                )));
            }
            let have = core.store().criteria(id).map_err(err)?.len();
            let n = usize::try_from(n).ok().filter(|n| *n >= 1).ok_or_else(|| {
                Fail::Error(format!("criterion {n}: criteria are numbered from 1"))
            })?;
            if n > have {
                return Err(Fail::Error(format!(
                    "item {} has {have} {}: there is no criterion {n}",
                    id.0,
                    if have == 1 { "criterion" } else { "criteria" }
                )));
            }
            core.confirm_criterion(&proof, id, n - 1)
                .map_err(|e| refused_move(core, id, e, "confirm"))?;
            println!("criterion {n} of item {} confirmed", id.0);
            Ok(())
        }
        (Some("stop"), _) => {
            // Anyone may stop: a stop only makes the system safer. Recorded as the CLI.
            core.stop(&core.capability_actor("cli").map_err(err)?)
                .map_err(err)?;
            println!("stop switch ON");
            Ok(())
        }
        (Some("resume"), _) => {
            core.clear_stop(&auth.prove(core)?).map_err(err)?;
            println!("stop switch off");
            Ok(())
        }
        (Some("overall-cap"), _) => {
            // The owner sets the most all goals together may spend (it starts at zero).
            // The amount is in micro-USD (1 USD = 1,000,000).
            let micros = u64::try_from(num(w.get(1).map_or("", String::as_str), "micro-USD")?)
                .map_err(|_| Fail::Error("micro-USD must not be negative".into()))?;
            core.set_overall_cap(&auth.prove(core)?, micros)
                .map_err(err)?;
            println!("overall cap {micros} micro-USD");
            Ok(())
        }
        (Some("resume-goal"), _) => {
            let goal = GoalId(num(w.get(1).map_or("", String::as_str), "goal id")?);
            let proof = auth.prove(core)?;
            need_goal(core, goal)?;
            core.resume_goal(&proof, goal).map_err(err)?;
            println!("goal {} resumed", goal.0);
            Ok(())
        }
        (Some("verify-evidence"), _) => {
            // DETECTION only: a broken chain says a row was changed or removed; an intact one
            // does not prove the file was never written (see the README, "Trust").
            let r = core.verify_evidence().map_err(err)?;
            match r.broken {
                None => {
                    println!(
                        "evidence chain intact: {} rows, head {} (keep the head elsewhere to catch removal of the last rows)",
                        r.rows, r.head
                    );
                    Ok(())
                }
                Some(b) => Err(Fail::Error(format!(
                    "evidence chain BROKEN at row {}: {}",
                    b.id, b.why
                ))),
            }
        }
        (Some("evidence"), _) => {
            let id = ItemId(num(need(w, "--item")?, "--item")?);
            need_item(core, id)?;
            let rows = core.store().evidence_for(id).map_err(err)?;
            if rows.is_empty() {
                println!("item {} has no evidence yet", id.0);
            }
            for e in rows {
                println!(
                    "e{} t={} {} {} {} {} {}{}{}:\n{}",
                    e.id,
                    e.at_ms,
                    e.actor,
                    e.call,
                    e.band.name(),
                    if e.allowed { "allow" } else { "DENY" },
                    e.kind.name(),
                    if e.cost_micros > 0 {
                        format!(" cost={} micro-USD", e.cost_micros)
                    } else {
                        String::new()
                    },
                    e.profile
                        .as_ref()
                        .map_or_else(String::new, |p| format!(" profile={p}")),
                    e.output
                );
            }
            Ok(())
        }
        (Some("backup"), _) => {
            let to = PathBuf::from(need(w, "--to")?);
            endora_persistence::snapshot(db, &to).map_err(err)?;
            println!(
                "backup written to {} (restore: point --db at it)",
                to.display()
            );
            Ok(())
        }
        (Some("capabilities"), _) => {
            for sp in core.specs() {
                println!(
                    "{} band={} tier={:?} max_cost={} micro-USD in: {} out: {}",
                    sp.name,
                    sp.effective_band().name(),
                    sp.tier,
                    sp.max_cost_micros,
                    sp.input,
                    sp.output
                );
            }
            Ok(())
        }
        _ => Err(Fail::Error(USAGE_LINE.into())),
    }
}

fn run() -> Out {
    let a = parse_args();
    if matches!(
        a.words.first().map(String::as_str),
        Some("--version" | "-V")
    ) && a.words.len() == 1
    {
        println!(
            "endora-foundation {} (schema {})",
            env!("CARGO_PKG_VERSION"),
            endora_foundation::infrastructure::SCHEMA_VERSION
        );
        return Ok(());
    }
    if wants_help(&a.words) {
        print!("{HELP}");
        return Ok(());
    }
    if let Some(problem) = a.problem {
        return Err(Fail::Error(problem));
    }
    check_words(&a.words)?;
    if a.token_out.is_some() && a.words.first().map(String::as_str) != Some("init") {
        return Err(Fail::Error(
            "--token-out is only for init (see --help)".into(),
        ));
    }
    let db =
        a.db.or_else(|| std::env::var_os("ENDORA_FOUNDATION_DB").map(PathBuf::from))
            .ok_or_else(|| {
                Fail::Error("no database: pass --db <path> or set ENDORA_FOUNDATION_DB".into())
            })?;
    let settings = match a
        .config
        .or_else(|| std::env::var_os("ENDORA_FOUNDATION_CONFIG").map(PathBuf::from))
    {
        Some(p) => read_config(&p).map_err(Fail::Error)?,
        None => Settings::default(),
    };
    let mut core = Core::open(
        db.to_str()
            .ok_or_else(|| Fail::Error("the path is not UTF-8".into()))?,
        Arc::new(SystemClock),
    )
    .map_err(err)?;
    core.register(Arc::new(Probes(
        settings.probes.clone(),
        settings.probe_env.clone(),
    )))
    .map_err(err)?;
    if let Some(mut cfg) = settings.git.clone() {
        if a.words.first().map(String::as_str) == Some("git-task") {
            cfg = task_config(cfg, &a.words)?;
        }
        let mut options = Options::default();
        if let Some((mut cc, limits, wall)) = settings.claude.clone() {
            cc.extra_args.clone_from(&settings.claude_args);
            if let Some(w) = wall {
                cfg.agent_limit = w;
            }
            options.executor = AgentExecutor::ClaudeCode(cc);
            options.claude_limits = limits;
            options.credential_file = settings.credential.clone();
        }
        // No confined agent, scripted or real, may read the owner's token file or the database,
        // by name (the profile adds the database's -journal, -wal and -shm files).
        // The sandbox matches canonical paths: make each one absolute and canonical (symlinks
        // resolved; a file that does not exist yet gets its parent resolved), and keep the other
        // spellings too. The note uses the canonical path.
        let typed: Vec<PathBuf> = [a.token_file.clone(), Some(db.clone())]
            .into_iter()
            .flatten()
            .chain(std::env::var_os("ENDORA_OWNER_TOKEN_FILE").map(PathBuf::from))
            .collect();
        for t in &typed {
            let spellings = canonical_deny_paths(std::slice::from_ref(t));
            if let Some(prefix) = spellings.first().and_then(|c| mac_profile_can_read(c)) {
                eprintln!(
                    "note: {} lies under {prefix}, which the macOS sandbox profile can read; it is \
                     denied by name for confined agents, but a copy kept under that prefix would be readable",
                    spellings[0].display()
                );
            }
        }
        options.deny_paths = canonical_deny_paths(&typed);
        core.register(Arc::new(GitChange::with_options(cfg, options)))
            .map_err(err)?;
    }
    if a.words.first().map(String::as_str) == Some("init") {
        return init(&core, a.token_out.as_deref());
    }
    let auth = Auth {
        token_file: a.token_file,
    };
    dispatch(&core, &auth, &settings, &db, &a.words)
}

fn main() {
    let result = run();
    // Flush now: a buffered line that cannot be written is a failed run, not a panic at exit.
    let flushed = std::io::Write::flush(&mut std::io::stdout()).is_ok();
    if !flushed || OUT_FAILED.load(Ordering::Relaxed) {
        if let Err(Fail::Refused(_)) = result {
            // The refusal is still the answer (exit 2): authority comes before the output.
        } else {
            // The change, if there was one, is already committed: say so (stderr may be closed too).
            eprintln!("output closed: the change may have been applied");
            exit(1);
        }
    }
    match result {
        Ok(()) => {}
        Err(Fail::Refused(why)) => {
            eprintln!("{why}");
            exit(2);
        }
        Err(Fail::Error(why)) => {
            eprintln!("endora-foundation: {why}");
            exit(1);
        }
    }
}
