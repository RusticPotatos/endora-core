//! `endora-foundation`: the foundation's command line. Plain text in and out over ONE SQLite file
//! (`--db <path>` or `ENDORA_FOUNDATION_DB`). Capabilities come from a small config file
//! (`--config <path>` or `ENDORA_FOUNDATION_CONFIG`) that is not kept in the repository.
//! Exit codes: 0 ok, 1 error, 2 refused by authority (the reason is printed).

mod probes;

use std::path::{Path, PathBuf};
use std::process::exit;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use signal_hook::consts::{SIGINT, SIGTERM};

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
use endora_kernel::{Clock, Reversibility, Tier};

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
                        .map(|v| v.parse().map_err(|_| at("max-seconds must be a number")))
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
}

fn parse_args() -> Args {
    let mut a = Args {
        words: Vec::new(),
        db: None,
        config: None,
        token_file: None,
        token_out: None,
    };
    let mut it = std::env::args().skip(1);
    while let Some(w) = it.next() {
        match w.as_str() {
            "--db" => a.db = it.next().map(PathBuf::from),
            "--config" => a.config = it.next().map(PathBuf::from),
            "--token-file" => a.token_file = it.next().map(PathBuf::from),
            "--token-out" => a.token_out = it.next().map(PathBuf::from),
            _ => a.words.push(w),
        }
    }
    a
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
                spend_cap_micros: u64::try_from(cap).map_err(err)?,
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
    core.store().goal(goal).map_err(err)?;
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
    let secs = match (flag(w, "--every"), flag(w, "--probe")) {
        (Some(v), _) => num(v, "--every")?,
        (None, Some(name)) => s
            .probes
            .iter()
            .find(|p| p.name == name)
            .map(|p| p.interval_secs)
            .ok_or_else(|| Fail::Error(format!("`{name}` is not a probe in the config")))?,
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
    let id = core
        .add_routine_limited(
            proof,
            goal,
            &title,
            secs.saturating_mul(1000),
            w.iter().any(|x| x == "--auto-accept"),
            now,
            u32::try_from(max_failures).map_err(err)?,
        )
        .map_err(err)?;
    println!("routine {}", id.0);
    Ok(())
}

fn run_due(core: &Core) -> Out {
    let runner = core.capability_actor("runner").map_err(err)?;
    if core.store().control("stop").map_err(err)? != 0 {
        return Err(Fail::Refused(
            "refused: the owner's stop switch is on (run `resume`)".into(),
        ));
    }
    for r in core.recover_interrupted().map_err(err)? {
        println!("recovered item {} -> {}: {}", r.item.0, r.now.name(), r.why);
    }
    // An auto-observe item a killed process left after its last check and before its
    // acceptance is settled now, not left for the owner for ever.
    for (id, got) in core.settle_stranded_auto_observe().map_err(err)? {
        match got {
            Acceptance::Accepted { by } => println!("item {} accepted by {by}", id.0),
            Acceptance::Refused(why) => {
                println!("item {} stays for the owner: {}", id.0, why.said());
            }
        }
    }
    for f in core.fire_due().map_err(err)? {
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
        if fresh {
            core.start(&runner, id).map_err(err)?;
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
    Ok(())
}

/// `serve --every <seconds>`: `run-due` in a loop, for a container or a service manager. The owner's
/// stop switch pauses it (said once per pause) instead of ending it; SIGTERM or SIGINT ends it
/// cleanly between two runs (exit 0). A failed run is reported and the loop goes on.
fn serve(core: &Core, w: &[String]) -> Out {
    let every = num(need(w, "--every")?, "--every")?;
    if every < 1 {
        return Err(Fail::Error("--every must be at least 1 second".into()));
    }
    let stop = Arc::new(AtomicBool::new(false));
    for sig in [SIGTERM, SIGINT] {
        signal_hook::flag::register(sig, Arc::clone(&stop)).map_err(err)?;
    }
    println!("serving: run-due every {every} s (SIGTERM stops)");
    let mut paused = false;
    while !stop.load(Ordering::Relaxed) {
        match run_due(core) {
            Ok(()) => paused = false,
            Err(Fail::Refused(why)) => {
                if !paused {
                    println!("{why}");
                }
                paused = true;
            }
            Err(Fail::Error(why)) => println!("run-due failed: {why}"),
        }
        let until = Instant::now() + Duration::from_secs(u64::try_from(every).unwrap_or(1));
        while Instant::now() < until && !stop.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    println!("stopping on signal");
    Ok(())
}

fn status(core: &Core) -> Out {
    print!("{}", core.status_text(15).map_err(err)?);
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
            core.resume_routine(&auth.prove(core)?, id).map_err(err)?;
            println!("routine {} resumed", id.0);
            Ok(())
        }
        (Some("git-task"), _) => git_task(core, &auth.prove(core)?, w),
        (Some("publish"), _) => {
            // Publishing is refused by authority: there is no row, and the call is outward-reversible.
            let it = core.store().item(item(1)?).map_err(err)?;
            let job = it.subject.unwrap_or_default();
            match core.call(
                &core.capability_actor("publish-request").map_err(err)?,
                it.id,
                "git.publish",
                &job,
            ).map_err(err)? {
                Outcome::Refused(why) => Err(Fail::Refused(format!("refused: {}", why.said()))),
                other => Err(Fail::Refused(format!("refused: publishing is not enabled ({other:?})"))),
            }
        }
        (Some("run-due"), _) => run_due(core),
        (Some("serve"), _) => serve(core, w),
        (Some("status"), _) => status(core),
        (Some("accept"), _) => match core.accept(&auth.prove(core)?, item(1)?).map_err(err)? {
            Acceptance::Accepted { by } => {
                println!("item {} accepted by {by}", w[1]);
                Ok(())
            }
            Acceptance::Refused(why) => Err(Fail::Refused(why.said().into())),
        },
        (Some("decline"), _) => {
            core.decline(&auth.prove(core)?, item(1)?, "declined by the owner")
                .map_err(err)?;
            println!("item {} declined", w[1]);
            Ok(())
        }
        (Some("confirm-criterion"), _) | (Some("confirm"), _) => {
            if w[0] == "confirm" {
                eprintln!("warning: `confirm` is deprecated and will be removed in the next release: use `confirm-criterion` (it confirms a criterion of an item, never a call)");
            }
            let n = usize::try_from(num(w.get(2).map_or("", String::as_str), "criterion number")?).map_err(err)?;
            core.confirm_criterion(&auth.prove(core)?, item(1)?, n.saturating_sub(1)).map_err(err)?;
            println!("criterion {n} of item {} confirmed", w[1]);
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
            core.set_overall_cap(&auth.prove(core)?, micros).map_err(err)?;
            println!("overall cap {micros} micro-USD");
            Ok(())
        }
        (Some("resume-goal"), _) => {
            core.resume_goal(&auth.prove(core)?, GoalId(num(w.get(1).map_or("", String::as_str), "goal id")?)).map_err(err)?;
            println!("goal {} resumed", w[1]);
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
            for e in core.store().evidence_for(ItemId(num(need(w, "--item")?, "--item")?)).map_err(err)? {
                println!("e{} t={} {} {} {} {} {}{}{}:\n{}", e.id, e.at_ms, e.actor, e.call, e.band.name(), if e.allowed { "allow" } else { "DENY" }, e.kind.name(), if e.cost_micros > 0 { format!(" cost={} micro-USD", e.cost_micros) } else { String::new() }, e.profile.as_ref().map_or_else(String::new, |p| format!(" profile={p}")), e.output);
            }
            Ok(())
        }
        (Some("backup"), _) => {
            let to = PathBuf::from(need(w, "--to")?);
            endora_persistence::snapshot(db, &to).map_err(err)?;
            println!("backup written to {} (restore: point --db at it)", to.display());
            Ok(())
        }
        (Some("capabilities"), _) => {
            for sp in core.specs() {
                println!("{} band={} tier={:?} max_cost={} micro-USD in: {} out: {}", sp.name, sp.effective_band().name(), sp.tier, sp.max_cost_micros, sp.input, sp.output);
            }
            Ok(())
        }
        _ => Err(Fail::Error("usage: endora-foundation [--db F] [--config F] goal add|routine add|git-task|publish|run-due|serve|status|init|accept|decline|confirm-criterion|stop|resume|resume-goal|overall-cap|evidence|verify-evidence|backup|capabilities (amounts are micro-USD: 1 USD = 1,000,000; --cap and overall-cap take them)".into())),
    }
}

fn run() -> Out {
    let a = parse_args();
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
    match run() {
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
