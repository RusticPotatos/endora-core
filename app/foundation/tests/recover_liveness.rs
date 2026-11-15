//! Two real processes on one database (weekly review B, WF-02): a second `run-due`, a `serve`
//! tick or an overlapping cron run must leave alone an item a LIVE process is running, and must
//! still recover the item once that process is dead (kill -9). Waits poll with a timeout; the
//! slow work is released by a file, not a sleep the test has to outlast.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

const BIN: &str = env!("CARGO_BIN_EXE_endora-foundation");
const LIMIT: Duration = Duration::from_secs(60);

struct Lab {
    dir: PathBuf,
    db: PathBuf,
    cfg: PathBuf,
    /// When set, `--db` is this RELATIVE name and the commands run with `dir` as their directory.
    rel: Option<String>,
}

impl Lab {
    /// A lab in the (already made) directory `dir`, with this config text.
    fn at(dir: PathBuf, config: &str) -> Self {
        let cfg = dir.join("foundation.conf");
        std::fs::write(&cfg, config).unwrap();
        Self {
            db: dir.join("live.db"),
            dir,
            cfg,
            rel: None,
        }
    }
    /// The database is `name` relative to the lab directory, as `--db name` from inside it.
    fn relative(mut self, name: &str) -> Self {
        self.db = self.dir.join(name);
        self.rel = Some(name.to_owned());
        self
    }
    fn token(&self) -> PathBuf {
        self.dir.join("owner.token")
    }
    fn command(&self, args: &[&str]) -> Command {
        let mut c = Command::new(BIN);
        match &self.rel {
            Some(name) => c.arg("--db").arg(name).current_dir(&self.dir),
            None => c.arg("--db").arg(&self.db),
        };
        c.arg("--config").arg(&self.cfg);
        if self.token().exists() {
            c.arg("--token-file").arg(self.token());
        }
        c.args(args);
        c
    }
    fn init(&self) {
        let o = self
            .command(&["init", "--token-out", self.token().to_str().unwrap()])
            .output()
            .unwrap();
        assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    }
    fn run(&self, args: &[&str]) -> Output {
        self.command(args).output().unwrap()
    }
    fn spawn(&self, args: &[&str]) -> Child {
        self.command(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap()
    }
    fn evidence(&self) -> String {
        text(&self.run(&["evidence", "--item", "1"]))
    }
    fn status(&self) -> String {
        text(&self.run(&["status"]))
    }
}

/// A new, empty directory for one test.
fn fresh_dir(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("liveness-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn text(o: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

fn wait_until(what: &str, mut ready: impl FnMut() -> bool) {
    let t = Instant::now();
    while !ready() {
        assert!(t.elapsed() < LIMIT, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(25));
    }
}

fn finish(mut c: Child) -> (Option<i32>, String) {
    let t = Instant::now();
    loop {
        if c.try_wait().unwrap().is_some() {
            let o = c.wait_with_output().unwrap();
            return (o.status.code(), text(&o));
        }
        assert!(t.elapsed() < LIMIT, "a child did not finish");
        std::thread::sleep(Duration::from_millis(25));
    }
}

/// Waits for the child to exit, but for no longer than `grace` (a second process that WOULD
/// block on the live item must not hang the test): `None` when it was still running.
fn finish_within(mut c: Child, grace: Duration) -> Option<(Option<i32>, String)> {
    let t = Instant::now();
    while t.elapsed() < grace {
        if c.try_wait().unwrap().is_some() {
            let o = c.wait_with_output().unwrap();
            return Some((o.status.code(), text(&o)));
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    let _ = c.kill();
    let _ = c.wait();
    None
}

fn lines(p: &Path) -> Vec<String> {
    std::fs::read_to_string(p)
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .collect()
}

fn starts(log: &Path) -> usize {
    lines(log).iter().filter(|l| l.starts_with("start")).count()
}

/// A probe command that logs its start, then waits for the `release` file (or 60 s), then logs
/// its end. Nothing here sleeps for a time the test must out-wait.
fn slow_lab(name: &str) -> (Lab, PathBuf, PathBuf) {
    slow_lab_db(name, None)
}

/// As [`slow_lab`]; with `Some(file)` the database is that RELATIVE name (`--db file`).
fn slow_lab_db(name: &str, rel: Option<&str>) -> (Lab, PathBuf, PathBuf) {
    slow_lab_with(name, rel, Some("auto-observe"), "1")
}

/// The general form: `acceptance` is the goal's `--acceptance` (`None` = the default, the owner
/// accepts) and `every` the routine's period in seconds.
fn slow_lab_with(
    name: &str,
    rel: Option<&str>,
    acceptance: Option<&str>,
    every: &str,
) -> (Lab, PathBuf, PathBuf) {
    let dir = fresh_dir(name);
    let log = dir.join("probe.log");
    let release = dir.join("release");
    let script = dir.join("slow.sh");
    std::fs::write(
        &script,
        format!(
            "echo \"start $$\" >> {log}\ni=0\nwhile [ ! -e {release} ] && [ $i -lt 600 ]; do /bin/sleep 0.1; i=$((i+1)); done\necho \"end $$\" >> {log}\n",
            log = log.display(),
            release = release.display()
        ),
    )
    .unwrap();
    let mut lab = Lab::at(
        dir,
        &format!("probe slow command sh {} 60\n", script.display()),
    );
    if let Some(file) = rel {
        lab = lab.relative(file);
    }
    lab.init();
    let mut goal = vec!["goal", "add", "--title", "slow watch", "--probe", "slow"];
    if let Some(a) = acceptance {
        goal.extend(["--acceptance", a]);
    }
    let o = lab.run(&goal);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    let o = lab.run(&["routine", "add", "--goal", "1", "--every", every]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    (lab, log, release)
}

#[test]
fn a_second_run_due_leaves_a_live_observe_call_alone_and_the_probe_runs_once() {
    let (lab, log, release) = slow_lab("observe");
    let a = lab.spawn(&["run-due"]);
    wait_until("the probe to start", || starts(&log) == 1);
    // A is alive and inside the probe. B must neither recover its item nor start it again.
    let b = lab.spawn(&["run-due"]);
    let b_out = finish_within(b, Duration::from_secs(20));
    std::fs::write(&release, b"go").unwrap();
    let (a_code, a_out) = finish(a);
    let (b_code, b_said) = b_out.expect("the second run-due must not wait on the live item");
    assert_eq!(a_code, Some(0), "{a_out}");
    assert_eq!(b_code, Some(0), "{b_said}");
    assert!(
        !b_said.contains("recovered"),
        "B recovered a live item: {b_said}"
    );
    assert!(!a_out.contains("recovered"), "{a_out}");
    assert_eq!(starts(&log), 1, "the probe ran twice: {:?}", lines(&log));
    let ev = lab.evidence();
    assert!(!ev.contains("core.recover"), "a false recover row:\n{ev}");
    assert!(
        a_out.contains("accepted by policy:auto-observe"),
        "A settled its own item: {a_out}"
    );
}

#[test]
fn after_the_holder_is_killed_the_next_run_due_recovers_its_item() {
    let (lab, log, release) = slow_lab("observe-kill");
    let mut a = lab.spawn(&["run-due"]);
    wait_until("the probe to start", || starts(&log) == 1);
    a.kill().unwrap(); // SIGKILL: no clean-up of any kind
    let _ = a.wait();
    // The orphaned probe still runs; release it so the recovered run is quick.
    std::fs::write(&release, b"go").unwrap();
    let b = lab.run(&["run-due"]);
    let said = text(&b);
    assert_eq!(b.status.code(), Some(0), "{said}");
    assert!(
        said.contains("recovered item 1 -> open: interrupted on attempt 1"),
        "the dead holder's item was not recovered: {said}"
    );
    assert!(lab.evidence().contains("core.recover"));
    assert!(said.contains("accepted by policy:auto-observe"), "{said}");
    // Neither the killed holder's file nor the recovering process's own is left behind.
    let mut holders = lab.db.as_os_str().to_owned();
    holders.push(".holders");
    let left: Vec<_> = std::fs::read_dir(&holders).unwrap().flatten().collect();
    assert!(left.is_empty(), "holder files left: {left:?}");
}

#[test]
fn a_serve_tick_leaves_a_live_observe_call_alone() {
    let (lab, log, release) = slow_lab("observe-serve");
    let a = lab.spawn(&["run-due"]);
    wait_until("the probe to start", || starts(&log) == 1);
    let mut serve = lab.spawn(&["serve", "--every", "1"]);
    // The first tick runs at once; give it a few more.
    std::thread::sleep(Duration::from_millis(2500));
    assert!(serve.try_wait().unwrap().is_none(), "serve is still up");
    assert!(
        Command::new("kill")
            .args(["-TERM", &serve.id().to_string()])
            .status()
            .unwrap()
            .success()
    );
    let (serve_code, serve_out) = finish(serve);
    std::fs::write(&release, b"go").unwrap();
    let (a_code, a_out) = finish(a);
    assert_eq!(serve_code, Some(0), "{serve_out}");
    assert!(
        !serve_out.contains("recovered"),
        "serve recovered a live item: {serve_out}"
    );
    assert_eq!(a_code, Some(0), "{a_out}");
    assert_eq!(starts(&log), 1, "the probe ran again: {:?}", lines(&log));
    assert!(!lab.evidence().contains("core.recover"));
}

// ---- the reversible band: a git task whose agent is still working ------------------------------

fn confined_or_skip() -> bool {
    let image = std::env::var("ENDORA_FOUNDATION_EXECUTOR_IMAGE")
        .unwrap_or_else(|_| endora_foundation_git::DEFAULT_IMAGE.to_owned());
    let (c, note) = endora_foundation_git::detect(&image);
    if c == endora_foundation_git::Confinement::Unavailable {
        eprintln!("SKIPPED: no confinement backend can run on this host: {note}");
        return false;
    }
    true
}

/// A git lab whose agent fixes the repository only after it sees nothing in its way for a while:
/// it writes `agent-started` into its job directory, waits (bounded) and then fixes `lib.sh`.
fn git_lab(name: &str, agent_seconds: u32) -> (Lab, PathBuf) {
    let dir = fresh_dir(name);
    let tpl = dir.join("template");
    std::fs::create_dir_all(&tpl).unwrap();
    std::fs::write(tpl.join("lib.sh"), "add() { echo $(( $1 - $2 )); }\n").unwrap();
    std::fs::write(
        tpl.join("test.sh"),
        ". ./lib.sh\n[ \"$(add 2 3)\" = 5 ] || { echo \"FAIL: add 2 3 gave $(add 2 3)\"; exit 1; }\necho PASS\n",
    )
    .unwrap();
    let agent = dir.join("agent.sh");
    std::fs::write(
        &agent,
        format!(
            "echo started > agent-started\n/bin/sleep {agent_seconds}\nprintf 'add() {{ echo $(( $1 + $2 )); }}\\n' > lib.sh\necho fixed lib.sh\n"
        ),
    )
    .unwrap();
    let config = format!(
        "git {} {} -- sh {}\n",
        dir.join("scratch").display(),
        tpl.display(),
        agent.display()
    );
    let lab = Lab::at(dir, &config);
    lab.init();
    (lab, tpl)
}

fn agent_started(lab: &Lab) -> bool {
    fn walk(p: &Path) -> bool {
        std::fs::read_dir(p).is_ok_and(|rd| {
            rd.flatten()
                .any(|e| e.file_name() == "agent-started" || (e.path().is_dir() && walk(&e.path())))
        })
    }
    walk(&lab.dir.join("scratch"))
}

const GIT_GOAL: [&str; 17] = [
    "goal",
    "add",
    "--title",
    "fix add",
    "--why",
    "add is wrong",
    "--criteria",
    "the repo's own tests pass",
    "--check",
    "git.verify",
    "{subject}",
    "tests",
    "pass",
    "--scope",
    "git.prepare,git.change,git.verify",
    "--acceptance",
    "auto-observe",
];

fn git_task_args(tpl: &Path) -> Vec<String> {
    [
        "git-task",
        "--goal",
        "1",
        "--repo",
        tpl.to_str().unwrap(),
        "--instruction",
        "fix add",
        "--test-cmd",
        "sh test.sh",
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect()
}

#[test]
fn a_run_due_during_a_live_git_task_leaves_it_alone_and_the_task_verifies() {
    if !confined_or_skip() {
        return;
    }
    // The agent works for 8 s; the second run-due starts as soon as the agent is seen working,
    // so it overlaps by about 8 s of slack (it does not need to outlast the agent).
    let (lab, tpl) = git_lab("git-live", 8);
    assert_eq!(lab.run(&GIT_GOAL).status.code(), Some(0));
    let args = git_task_args(&tpl);
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let a = lab.spawn(&args);
    wait_until("the agent to start", || agent_started(&lab));
    let b = lab.run(&["run-due"]);
    let b_said = text(&b);
    let (a_code, a_out) = finish(a);
    assert!(
        !b_said.contains("recovered"),
        "B failed a live task: {b_said}"
    );
    assert_eq!(b.status.code(), Some(0), "{b_said}");
    assert_eq!(
        a_code,
        Some(0),
        "the task was cut by the other process: {a_out}"
    );
    assert!(
        a_out.contains("after: ")
            && a_out.contains("tests=pass")
            && a_out.contains("awaiting owner acceptance"),
        "the task did not complete and verify: {a_out}"
    );
    assert!(
        !lab.evidence().contains("core.recover"),
        "a false recover row"
    );
    assert!(lab.status().contains("#1 [running]"), "{}", lab.status());
}

#[test]
fn after_a_git_task_is_killed_the_next_run_due_fails_it_as_before() {
    if !confined_or_skip() {
        return;
    }
    let (lab, tpl) = git_lab("git-kill", 30);
    assert_eq!(lab.run(&GIT_GOAL).status.code(), Some(0));
    let args = git_task_args(&tpl);
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let mut a = lab.spawn(&args);
    wait_until("the agent to start", || agent_started(&lab));
    a.kill().unwrap();
    let _ = a.wait();
    // The agent may outlive Core (a known limit on macOS): it must not keep the item "alive",
    // which also shows that the holder lock is not inherited by children.
    let b = lab.run(&["run-due"]);
    let said = text(&b);
    assert!(
        said.contains(
            "recovered item 1 -> failed: interrupted inside a call above the observe band"
        ),
        "{said}"
    );
}

// ---- WF-06: an auto-observe item killed between its last check and settling ---------------------

#[test]
fn run_due_settles_an_auto_observe_item_stranded_after_its_last_check() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let lab = Lab::at(
        fresh_dir("stranded"),
        &format!("probe web tcp 127.0.0.1:{port} 60\n"),
    );
    lab.init();
    let o = lab.run(&[
        "goal",
        "add",
        "--title",
        "web answers",
        "--probe",
        "web",
        "--acceptance",
        "auto-observe",
    ]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    let o = lab.run(&["routine", "add", "--goal", "1", "--every", "3600"]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    let o = lab.run(&["run-due"]);
    assert!(
        text(&o).contains("item 1 accepted by policy:auto-observe"),
        "{}",
        text(&o)
    );
    // Put item 1 back into the state a kill leaves it in: probe result and met criterion are
    // recorded, acceptance never happened.
    rusqlite::Connection::open(&lab.db)
        .unwrap()
        .execute(
            "UPDATE items SET state='running', accepted_by=NULL, accepted_ms=NULL WHERE id=1",
            [],
        )
        .unwrap();
    assert!(lab.status().contains("#1 [running]"));
    let o = lab.run(&["run-due"]);
    let said = text(&o);
    assert_eq!(o.status.code(), Some(0), "{said}");
    assert!(
        said.contains("item 1 accepted by policy:auto-observe"),
        "{said}"
    );
    assert!(lab.status().contains("#1 [done]"), "{}", lab.status());
}

// ---- round 2: a SQLite URI path is refused, not opened with the lease off ------------------------

#[test]
fn a_uri_spelling_of_the_database_is_refused_with_a_clear_error() {
    let lab = Lab::at(fresh_dir("uri"), "probe web tcp 127.0.0.1:1 60\n");
    lab.init();
    let uri = format!("file:{}", lab.db.display());
    let o = Command::new(BIN)
        .arg("--db")
        .arg(&uri)
        .arg("--config")
        .arg(&lab.cfg)
        .arg("run-due")
        .output()
        .unwrap();
    let said = text(&o);
    assert_eq!(o.status.code(), Some(1), "{said}");
    assert!(said.contains("URI"), "{said}");
}

// ---- round 3: non-ASCII relative paths and the exact URI rule ------------------------------------

#[test]
fn a_non_ascii_relative_database_path_opens_and_the_lease_works() {
    // Round 2 sliced the path at byte 5 and panicked (exit 101) when that fell inside a character.
    for (i, file) in ["日本.db", "ÄÖé.db", "fileé.db", "日本日本日本.db"]
        .iter()
        .enumerate()
    {
        let (lab, log, release) = slow_lab_db(&format!("nonascii{i}"), Some(file));
        let a = lab.spawn(&["run-due"]);
        wait_until("the probe to start", || starts(&log) == 1);
        let b = lab.spawn(&["run-due"]);
        let b_out = finish_within(b, Duration::from_secs(20));
        std::fs::write(&release, b"go").unwrap();
        let (a_code, a_out) = finish(a);
        let (b_code, b_said) = b_out.expect("the second run-due must not wait on the live item");
        assert_eq!(a_code, Some(0), "{file}: {a_out}");
        assert_eq!(b_code, Some(0), "{file}: {b_said}");
        assert!(!b_said.contains("recovered"), "{file}: {b_said}");
        assert!(!b_said.contains("panicked"), "{file}: {b_said}");
        assert_eq!(starts(&log), 1, "{file}: the probe ran twice");
        assert!(
            lab.dir.join(format!("{file}.holders")).is_dir(),
            "{file}: the lease was on"
        );
    }
}

#[test]
fn only_a_lowercase_file_colon_prefix_is_a_uri_and_a_plain_file_of_that_name_opens() {
    // SQLite treats a name as a URI only when it begins with the lowercase bytes `file:`.
    let lab = Lab::at(fresh_dir("uri-rule"), "probe web tcp 127.0.0.1:1 60\n");
    let at = |name: &str, args: &[&str]| {
        Command::new(BIN)
            .current_dir(&lab.dir)
            .args(["--db", name, "--config"])
            .arg(&lab.cfg)
            .args(args)
            .output()
            .unwrap()
    };
    for uri in ["file:x.db", "file:./x.db?mode=rwc", "file:"] {
        let o = at(uri, &["status"]);
        let said = text(&o);
        assert_eq!(o.status.code(), Some(1), "{uri}: {said}");
        assert!(
            said.contains("URI") && !said.contains("panicked"),
            "{uri}: {said}"
        );
    }
    assert!(
        !lab.dir.join("x.db").exists(),
        "a refused URI created nothing"
    );
    // Other spellings are plain file names: `./file:x.db` (the workaround), `FILE:x.db`, `File:x.db`.
    for plain in ["./file:x.db", "FILE:x.db", "File:x.db", "fil", "file"] {
        let o = at(plain, &["status"]);
        assert_eq!(o.status.code(), Some(0), "{plain}: {}", text(&o));
    }
    assert!(lab.dir.join("FILE:x.db").exists() || lab.dir.join("File:x.db").exists());
}

// ---- recovery counts attempts (review of the integrated branch, F1 and F2) -----------------------

/// How many `core.recover` rows the file holds, from the CLI's own evidence listing.
fn recover_rows(lab: &Lab) -> usize {
    lab.evidence().matches("core.recover").count()
}

/// Kills `a` (SIGKILL, no clean-up of any kind) once its probe has started `n` times in all.
fn kill_in_probe(mut a: Child, log: &Path, n: usize) {
    wait_until("the probe to start", || starts(log) == n);
    a.kill().unwrap();
    let _ = a.wait();
}

/// The tutorial's step 7 and then more of the same: an item interrupted once whose re-run met its
/// criteria waits for the owner, however many `run-due` follow, and can still be accepted.
#[test]
fn a_recovered_item_that_met_its_criteria_stays_waiting_over_further_run_dues() {
    let (lab, log, release) = slow_lab_with("again", None, None, "300");
    kill_in_probe(lab.spawn(&["run-due"]), &log, 1);
    std::fs::write(&release, b"go").unwrap();
    let b = lab.run(&["run-due"]);
    let said = text(&b);
    assert_eq!(b.status.code(), Some(0), "{said}");
    assert!(
        said.contains("recovered item 1 -> open: interrupted on attempt 1"),
        "{said}"
    );
    assert!(said.contains("item 1: 1/1 criteria met"), "{said}");
    for pass in 0..3 {
        let o = lab.run(&["run-due"]);
        let said = text(&o);
        assert_eq!(o.status.code(), Some(0), "pass {pass}: {said}");
        assert!(
            !said.contains("recovered"),
            "pass {pass} recovered it again: {said}"
        );
    }
    assert_eq!(
        recover_rows(&lab),
        1,
        "false core.recover rows:\n{}",
        lab.evidence()
    );
    let status = lab.status();
    assert!(
        status.contains("[running]") && status.contains("attempts 2"),
        "{status}"
    );
    let o = lab.run(&["accept", "1"]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    assert!(lab.status().contains("[done]"));
    assert_eq!(lab.run(&["verify-evidence"]).status.code(), Some(0));
}

/// The same under `serve`, whose own ticks may adopt its own items: three ticks (and more) leave
/// the waiting item alone.
#[test]
fn serve_ticks_leave_a_recovered_item_that_met_its_criteria_waiting() {
    let (lab, log, release) = slow_lab_with("again-serve", None, None, "300");
    kill_in_probe(lab.spawn(&["run-due"]), &log, 1);
    std::fs::write(&release, b"go").unwrap();
    let mut serve = lab.spawn(&["serve", "--every", "1"]);
    std::thread::sleep(Duration::from_millis(4500)); // ticks at 0, 1, 2, 3 and 4 s
    assert!(serve.try_wait().unwrap().is_none(), "serve is still up");
    assert!(
        Command::new("kill")
            .args(["-TERM", &serve.id().to_string()])
            .status()
            .unwrap()
            .success()
    );
    let (code, out) = finish(serve);
    assert_eq!(code, Some(0), "{out}");
    assert_eq!(out.matches("recovered item").count(), 1, "{out}");
    assert_eq!(recover_rows(&lab), 1, "{}", lab.evidence());
    let status = lab.status();
    assert!(
        status.contains("[running]") && status.contains("attempts 2"),
        "{status}"
    );
}

/// What the fix must not weaken: an item killed inside its call three times in a row fails.
#[test]
fn an_item_killed_inside_its_call_three_times_still_fails_after_three_attempts() {
    let (lab, log, release) = slow_lab_with("thrice", None, None, "300");
    kill_in_probe(lab.spawn(&["run-due"]), &log, 1);
    // Each later run-due recovers the one before and starts the item again (attempts 2, 3).
    for n in [2, 3] {
        kill_in_probe(lab.spawn(&["run-due"]), &log, n);
    }
    std::fs::write(&release, b"go").unwrap();
    let o = lab.run(&["run-due"]);
    let said = text(&o);
    assert_eq!(o.status.code(), Some(0), "{said}");
    assert!(
        said.contains("recovered item 1 -> failed: interrupted again after 3 attempts"),
        "{said}"
    );
    assert_eq!(recover_rows(&lab), 3, "{}", lab.evidence());
    let status = lab.status();
    assert!(
        status.contains("[failed]") && status.contains("attempts 3"),
        "{status}"
    );
    assert_eq!(lab.run(&["verify-evidence"]).status.code(), Some(0));
}

/// F2: several overlapping `run-due`, all listing the same open items, race to start them. A
/// lost race is skipped, not "corrupt stored data": every process exits 0, every item starts and
/// runs exactly once, and the chain is intact.
#[test]
fn overlapping_run_dues_skip_a_lost_start_and_every_item_runs_once() {
    const ITEMS: usize = 6;
    let dir = fresh_dir("race");
    let log = dir.join("probe.log");
    let release = dir.join("release");
    let script = dir.join("slow.sh");
    std::fs::write(
        &script,
        format!(
            "echo \"start $$\" >> {log}\ni=0\nwhile [ ! -e {release} ] && [ $i -lt 600 ]; do /bin/sleep 0.1; i=$((i+1)); done\n",
            log = log.display(),
            release = release.display()
        ),
    )
    .unwrap();
    let lab = Lab::at(
        dir,
        &format!("probe slow command sh {} 60\n", script.display()),
    );
    lab.init();
    for g in 1..=ITEMS {
        let o = lab.run(&[
            "goal",
            "add",
            "--title",
            &format!("g{g}"),
            "--probe",
            "slow",
        ]);
        assert_eq!(o.status.code(), Some(0), "{}", text(&o));
        let o = lab.run(&["routine", "add", "--goal", &g.to_string(), "--every", "300"]);
        assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    }
    let procs: Vec<Child> = (0..ITEMS).map(|_| lab.spawn(&["run-due"])).collect();
    // Wait until every item has started, or every process has ended (the failure case).
    let t = Instant::now();
    let mut procs = procs;
    while starts(&log) < ITEMS && t.elapsed() < Duration::from_secs(30) {
        if procs.iter_mut().all(|c| c.try_wait().unwrap().is_some()) {
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    std::fs::write(&release, b"go").unwrap();
    let mut said = String::new();
    for (i, c) in procs.into_iter().enumerate() {
        let (code, out) = finish(c);
        assert_eq!(code, Some(0), "process {i} exited {code:?}: {out}");
        assert!(!out.contains("corrupt stored data"), "process {i}: {out}");
        said.push_str(&out);
    }
    assert_eq!(starts(&log), ITEMS, "{:?}", lines(&log));
    let status = lab.status();
    assert_eq!(status.matches("attempts 1").count(), ITEMS, "{status}");
    assert!(!status.contains("attempts 2"), "{status}");
    assert_eq!(lab.run(&["verify-evidence"]).status.code(), Some(0));
    eprintln!(
        "lost races skipped: {}",
        said.matches(": skipped: item ").count()
    );
}

/// A storm of overlapping `run-due` with random SIGKILLs, repeated (`ENDORA_STORM_REPS`, default
/// 10): per repetition 8 goals (half owner-accepted, half `auto-observe`), 6 staggered `run-due`,
/// one or two killed at random, then recovery passes and two more that must change nothing.
/// Whatever the timing: no false `core.recover` row (one row per call really cut off, none for
/// an item whose holder was alive or whose criteria were met), no call run again beyond the number
/// of kills (a kill can cost one call: cut off, or died before its criterion was marked met), no
/// exit status other than 0 (or the kill), the chain intact.
#[test]
fn a_storm_of_overlapping_run_dues_and_kills_leaves_no_false_recovery() {
    let reps: usize = std::env::var("ENDORA_STORM_REPS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10);
    const GOALS: usize = 8;
    let (mut kills, mut runs, mut recovered, mut cut) = (0usize, 0usize, 0i64, 0i64);
    for rep in 0..reps {
        let dir = fresh_dir(&format!("storm{rep}"));
        let mut cfg = String::new();
        for g in 1..=GOALS {
            let script = dir.join(format!("p{g}.sh"));
            std::fs::write(
                &script,
                format!(
                    "echo \"start $$\" >> {}\n/bin/sleep 0.6\n",
                    dir.join(format!("p{g}.log")).display()
                ),
            )
            .unwrap();
            cfg.push_str(&format!("probe p{g} command sh {} 60\n", script.display()));
        }
        let lab = Lab::at(dir.clone(), &cfg);
        lab.init();
        for g in 1..=GOALS {
            let mut a = vec!["goal", "add", "--title", "storm", "--probe"];
            let p = format!("p{g}");
            a.push(&p);
            if g % 2 == 1 {
                a.extend(["--acceptance", "auto-observe"]);
            }
            assert_eq!(lab.run(&a).status.code(), Some(0));
            let o = lab.run(&["routine", "add", "--goal", &g.to_string(), "--every", "300"]);
            assert_eq!(o.status.code(), Some(0), "{}", text(&o));
        }
        // A cheap deterministic generator: the schedule differs per repetition, and a failure can
        // be re-run with the same repetition number (the OS still adds its own timing).
        let mut x =
            0x9E37_79B9_7F4A_7C15_u64 ^ (rep as u64 + 1).wrapping_mul(0xD1B5_4A32_D192_ED03);
        let mut next = |n: u64| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x % n
        };
        let mut wave: Vec<Child> = Vec::new();
        for _ in 0..6 {
            wave.push(lab.spawn(&["run-due"]));
            std::thread::sleep(Duration::from_millis(80 + next(120)));
        }
        let mut killed = Vec::new();
        for _ in 0..(1 + next(2)) {
            std::thread::sleep(Duration::from_millis(100 + next(500)));
            let live: Vec<usize> = (0..wave.len())
                .filter(|i| !killed.contains(i) && wave[*i].try_wait().unwrap().is_none())
                .collect();
            if live.is_empty() {
                break;
            }
            let v = live[usize::try_from(next(live.len() as u64)).unwrap()];
            wave[v].kill().unwrap();
            killed.push(v);
        }
        kills += killed.len();
        for (i, c) in wave.into_iter().enumerate() {
            let (code, out) = finish(c);
            runs += 1;
            if killed.contains(&i) {
                continue;
            }
            assert_eq!(
                code,
                Some(0),
                "rep {rep}: run-due {i} exited {code:?}: {out}"
            );
            assert!(!out.contains("corrupt stored data"), "rep {rep}: {out}");
        }
        // Recovery passes, then two that must be quiet.
        for pass in 0..3 {
            let o = lab.run(&["run-due"]);
            assert_eq!(
                o.status.code(),
                Some(0),
                "rep {rep} pass {pass}: {}",
                text(&o)
            );
        }
        let settled = recover_rows_all(&lab);
        for pass in 0..2 {
            let o = lab.run(&["run-due"]);
            let said = text(&o);
            assert_eq!(o.status.code(), Some(0), "rep {rep}: {said}");
            assert!(
                !said.contains("recovered"),
                "rep {rep} quiet pass {pass}: {said}"
            );
        }
        assert_eq!(
            recover_rows_all(&lab),
            settled,
            "rep {rep}: rows after quiet passes"
        );
        // The file: one recover row per call really cut off, and per item exactly one finished call.
        let c = rusqlite::Connection::open_with_flags(
            &lab.db,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        let mut extra = 0i64;
        let items: Vec<(i64, String)> = c
            .prepare("SELECT id, state FROM items ORDER BY id")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        assert_eq!(items.len(), GOALS, "rep {rep}");
        for (id, st) in &items {
            let n = |sql: &str| -> i64 { c.query_row(sql, [id], |r| r.get(0)).unwrap() };
            let intents = n("SELECT COUNT(*) FROM evidence WHERE item_id=?1 AND kind='intent'");
            let unsettled = n(
                "SELECT COUNT(*) FROM evidence WHERE item_id=?1 AND kind='intent' AND id NOT IN (SELECT intent_id FROM evidence WHERE intent_id IS NOT NULL)",
            );
            let recovers =
                n("SELECT COUNT(*) FROM evidence WHERE item_id=?1 AND call='core.recover'");
            assert_eq!(
                recovers, unsettled,
                "rep {rep} item {id}: a false (or missing) core.recover row"
            );
            let finished = intents - unsettled;
            assert!(
                finished >= 1,
                "rep {rep} item {id}: no finished call after the recovery passes"
            );
            assert!(
                st == "done" || st == "running",
                "rep {rep} item {id} is {st}"
            );
            // A call that ran again is a call a kill cut off (an unsettled intent, recovered), or
            // one whose process died between its result row and the "criterion met" mark (the
            // stuck-watch re-check of a dead holder's routine item). Neither happens without a kill.
            extra += (finished - 1) + unsettled;
            recovered += recovers;
            cut += unsettled;
        }
        assert!(
            extra <= i64::try_from(killed.len()).unwrap(),
            "rep {rep}: {extra} calls ran again but only {} processes were killed (a live item ran twice)",
            killed.len()
        );
        assert_eq!(
            lab.run(&["verify-evidence"]).status.code(),
            Some(0),
            "rep {rep}"
        );
    }
    eprintln!(
        "storm: {reps} repetitions, {runs} run-due, {kills} kills, {cut} calls cut off, {recovered} recover rows (equal), 0 false"
    );
}

fn recover_rows_all(lab: &Lab) -> usize {
    text(&lab.run(&["evidence"]))
        .matches("core.recover")
        .count()
}
