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
        }
    }
    fn token(&self) -> PathBuf {
        self.dir.join("owner.token")
    }
    fn command(&self, args: &[&str]) -> Command {
        let mut c = Command::new(BIN);
        c.arg("--db").arg(&self.db).arg("--config").arg(&self.cfg);
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
    let lab = Lab::at(
        dir,
        &format!("probe slow command sh {} 60\n", script.display()),
    );
    lab.init();
    let o = lab.run(&[
        "goal",
        "add",
        "--title",
        "slow watch",
        "--probe",
        "slow",
        "--acceptance",
        "auto-observe",
    ]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    let o = lab.run(&["routine", "add", "--goal", "1", "--every", "1"]);
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
