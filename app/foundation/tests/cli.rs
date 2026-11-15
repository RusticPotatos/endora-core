//! The command line, driven as a real process over one SQLite file and a temp config.

use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// True when some confinement backend can run on this host; otherwise prints why and returns
/// false, and the test returns early (a skip with a reason, not a failure).
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

struct Lab {
    dir: PathBuf,
}

impl Lab {
    fn new(name: &str) -> Self {
        let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("cli-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self { dir }
    }
    fn config(&self, probe: &str) -> PathBuf {
        let p = self.dir.join("foundation.conf");
        std::fs::write(&p, format!("# a temp config\n{probe}\n")).unwrap();
        p
    }
    /// The owner token file of this lab (made by the first run, through `init`).
    fn token(&self) -> PathBuf {
        self.dir.join("owner.token")
    }
    fn run_on(&self, db: &Path, cfg: &Path, args: &[&str]) -> Output {
        let token = self.token();
        if !token.exists() && args.first() != Some(&"init") {
            let init = self.run_plain(db, cfg, &["init", "--token-out", token.to_str().unwrap()]);
            assert_eq!(init.status.code(), Some(0), "{}", text(&init));
        }
        let mut full: Vec<&str> = vec!["--token-file", token.to_str().unwrap()];
        full.extend_from_slice(args);
        self.run_plain(db, cfg, &full)
    }
    /// A run with exactly these arguments (no token file added).
    fn run_plain(&self, db: &Path, cfg: &Path, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_endora-foundation"))
            .arg("--db")
            .arg(db)
            .arg("--config")
            .arg(cfg)
            .args(args)
            .output()
            .unwrap()
    }
}

fn text(o: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}
fn code(o: &Output) -> i32 {
    o.status.code().unwrap()
}

#[test]
fn goal_routine_run_due_accept_stop_backup_restore_end_to_end() {
    let lab = Lab::new("e2e");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let cfg = lab.config(&format!("probe web tcp 127.0.0.1:{port} 60"));
    let db = lab.dir.join("live.db");
    let run = |args: &[&str]| lab.run_on(&db, &cfg, args);

    let caps = run(&["capabilities"]);
    assert_eq!(code(&caps), 0, "{}", text(&caps));
    assert!(
        text(&caps).contains("probe.run") && text(&caps).contains("observe"),
        "{}",
        text(&caps)
    );

    let g = run(&[
        "goal",
        "add",
        "--title",
        "web answers",
        "--why",
        "the house needs it",
        "--criteria",
        "port is open",
        "--check",
        "probe.run",
        "web",
        "up",
        "true",
    ]);
    assert_eq!(code(&g), 0, "{}", text(&g));
    assert!(text(&g).contains("goal 1"), "{}", text(&g));
    for every in ["3600", "7200"] {
        let r = run(&["routine", "add", "--goal", "1", "--every", every]);
        assert_eq!(code(&r), 0, "{}", text(&r));
    }

    for _ in 0..2 {
        let d = run(&["run-due"]);
        assert_eq!(code(&d), 0, "{}", text(&d));
    }
    let status = run(&["status"]);
    let st = text(&status);
    assert_eq!(code(&status), 0, "{st}");
    assert!(
        st.contains("#1 [running]") && st.contains("#2 [running]"),
        "two items, awaiting the owner:\n{st}"
    );
    assert!(
        !st.contains("#3 ["),
        "the second run-due fired nothing new:\n{st}"
    );
    assert!(
        st.contains("probe.run") && st.contains("up=true"),
        "evidence of the checks:\n{st}"
    );
    assert!(st.contains("accept 1"), "an open decision is listed:\n{st}");
    let ev = run(&["evidence", "--item", "1"]);
    assert!(
        text(&ev).contains("probe.run") && text(&ev).contains("[facts] up=true"),
        "{}",
        text(&ev)
    );

    let a = run(&["accept", "1"]);
    assert_eq!(code(&a), 0, "{}", text(&a));
    assert!(text(&run(&["status"])).contains("#1 [done]"));

    let stop = run(&["stop"]);
    assert_eq!(code(&stop), 0);
    let refused = run(&["run-due"]);
    assert_eq!(code(&refused), 2, "{}", text(&refused));
    assert!(text(&refused).contains("stop switch"), "{}", text(&refused));
    assert_eq!(code(&run(&["resume"])), 0);
    assert_eq!(code(&run(&["run-due"])), 0);

    // Backup, then restore by pointing --db at the copy: the same status, word for word.
    let copy = lab.dir.join("backup").join("copy.db");
    let b = run(&["backup", "--to", copy.to_str().unwrap()]);
    assert_eq!(code(&b), 0, "{}", text(&b));
    let live = text(&run(&["status"]));
    let restored = text(&lab.run_on(&copy, &cfg, &["status"]));
    assert_eq!(live, restored);
    drop(listener);
}

#[test]
fn an_unmet_criterion_cannot_be_accepted_and_the_refusal_says_why() {
    let lab = Lab::new("unmet");
    let closed = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port(); // listener dropped: closed
    let cfg = lab.config(&format!("probe web tcp 127.0.0.1:{closed} 60"));
    let db = lab.dir.join("live.db");
    let run = |args: &[&str]| lab.run_on(&db, &cfg, args);
    assert_eq!(
        code(&run(&[
            "goal",
            "add",
            "--title",
            "t",
            "--why",
            "w",
            "--criteria",
            "open",
            "--check",
            "probe.run",
            "web",
            "up",
            "true"
        ])),
        0
    );
    assert_eq!(
        code(&run(&["routine", "add", "--goal", "1", "--every", "60"])),
        0
    );
    assert_eq!(code(&run(&["run-due"])), 0);
    // Slice 5a: a failing probe ends its item as failed instead of leaving it running, so it can
    // neither be accepted nor block the routine.
    assert!(text(&run(&["status"])).contains("#1 [failed]"));
    let a = run(&["accept", "1"]);
    assert_ne!(code(&a), 0, "{}", text(&a));
    assert!(text(&run(&["status"])).contains("#1 [failed]"));
    assert_eq!(code(&run(&["nonsense"])), 1);
}

#[test]
fn an_auto_observe_goal_keeps_its_watch_running_without_the_owner() {
    let lab = Lab::new("auto");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let cfg = lab.config(&format!("probe web tcp 127.0.0.1:{port} 60"));
    let db = lab.dir.join("live.db");
    let run = |args: &[&str]| lab.run_on(&db, &cfg, args);
    let bad = run(&[
        "goal",
        "add",
        "--title",
        "t",
        "--probe",
        "web",
        "--acceptance",
        "whenever",
    ]);
    assert_eq!(code(&bad), 1, "{}", text(&bad));
    let g = run(&[
        "goal",
        "add",
        "--title",
        "web answers",
        "--probe",
        "web",
        "--acceptance",
        "auto-observe",
    ]);
    assert_eq!(code(&g), 0, "{}", text(&g));
    assert_eq!(
        code(&run(&["routine", "add", "--goal", "1", "--every", "1"])),
        0
    );
    for _ in 0..3 {
        let d = run(&["run-due"]);
        assert_eq!(code(&d), 0, "{}", text(&d));
        assert!(
            text(&d).contains("accepted by policy:auto-observe"),
            "{}",
            text(&d)
        );
        std::thread::sleep(std::time::Duration::from_millis(1100));
    }
    let st = text(&run(&["status"]));
    assert!(
        st.contains("#1 [done]") && st.contains("#2 [done]") && st.contains("#3 [done]"),
        "{st}"
    );
    assert!(st.contains("accepted_by policy:auto-observe"), "{st}");
    // A closed port: the criterion is unmet, so the item ends failed (slice 5a), and the watch keeps going.
    drop(listener);
    std::thread::sleep(std::time::Duration::from_millis(1100));
    let d = run(&["run-due"]);
    assert!(text(&d).contains("item 4 failed"), "{}", text(&d));
    assert!(text(&run(&["status"])).contains("#4 [failed]"));
    std::thread::sleep(std::time::Duration::from_millis(1100));
    let d = run(&["run-due"]);
    assert!(
        text(&d).contains("item 5"),
        "the watch kept running: {}",
        text(&d)
    );
}

#[test]
fn a_failing_watch_pauses_at_its_cap_and_the_owner_resumes_it() {
    let lab = Lab::new("pause");
    let closed = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let cfg = lab.config(&format!("probe web tcp 127.0.0.1:{closed} 60"));
    let db = lab.dir.join("live.db");
    let run = |args: &[&str]| lab.run_on(&db, &cfg, args);
    assert_eq!(
        code(&run(&["goal", "add", "--title", "t", "--probe", "web"])),
        0
    );
    for bad in ["0", "999999999999"] {
        let r = run(&["routine", "add", "--goal", "1", "--every", bad]);
        assert_eq!(code(&r), 1, "--every {bad}: {}", text(&r));
    }
    let r = run(&[
        "routine",
        "add",
        "--goal",
        "1",
        "--every",
        "1",
        "--max-failures",
        "2",
    ]);
    assert_eq!(code(&r), 0, "{}", text(&r));
    let first = run(&["run-due"]);
    assert!(
        text(&first).contains("failure 1 in a row"),
        "{}",
        text(&first)
    );
    std::thread::sleep(std::time::Duration::from_millis(1100));
    let second = run(&["run-due"]);
    assert!(text(&second).contains("PAUSED"), "{}", text(&second));
    assert!(text(&run(&["status"])).contains("PAUSED after 2 failures"));
    std::thread::sleep(std::time::Duration::from_millis(1100));
    assert!(
        !text(&run(&["run-due"])).contains("routine 1:"),
        "a paused routine fires nothing"
    );
    let resumed = run(&["routine", "resume", "1"]);
    assert_eq!(code(&resumed), 0, "{}", text(&resumed));
    assert!(text(&run(&["status"])).contains("running, failures 0/2"));
}

/// A repo template whose `add` is wrong, its own test, and a scripted stand-in agent (a shell
/// script: no model, no network).
fn git_lab(name: &str, agent_fixes: bool) -> (Lab, PathBuf, PathBuf) {
    let lab = Lab::new(name);
    let tpl = lab.dir.join("template");
    std::fs::create_dir_all(&tpl).unwrap();
    std::fs::write(tpl.join("lib.sh"), "add() { echo $(( $1 - $2 )); }\n").unwrap();
    std::fs::write(
        tpl.join("test.sh"),
        ". ./lib.sh\n[ \"$(add 2 3)\" = 5 ] || { echo \"FAIL: add 2 3 gave $(add 2 3)\"; exit 1; }\necho PASS\n",
    )
    .unwrap();
    let agent = lab.dir.join("agent.sh");
    let body = if agent_fixes {
        "printf 'add() { echo $(( $1 + $2 )); }\\n' > lib.sh\necho fixed lib.sh\n"
    } else {
        "echo i did nothing\n"
    };
    std::fs::write(&agent, body).unwrap();
    let cfg = lab.config(&format!(
        "git {} {} -- sh {}",
        lab.dir.join("scratch").display(),
        tpl.display(),
        agent.display()
    ));
    (lab, tpl, cfg)
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

#[test]
fn git_task_runs_prepare_change_verify_and_waits_for_the_owner_and_publish_is_refused() {
    if !confined_or_skip() {
        return;
    }
    let (lab, tpl, cfg) = git_lab("gittask", true);
    let db = lab.dir.join("live.db");
    let run = |args: &[&str]| lab.run_on(&db, &cfg, args);
    let g = run(&GIT_GOAL);
    assert_eq!(code(&g), 0, "{}", text(&g));
    let t = run(&[
        "git-task",
        "--goal",
        "1",
        "--repo",
        tpl.to_str().unwrap(),
        "--instruction",
        "fix add",
        "--test-cmd",
        "sh test.sh",
    ]);
    assert_eq!(code(&t), 0, "{}", text(&t));
    let said = text(&t);
    assert!(
        said.contains("tests=fail")
            && said.contains("agent_exit=0")
            && said.contains("every criterion met")
            && said.contains("awaiting owner acceptance: accept 1"),
        "{said}"
    );
    // Even on an auto-observe goal a git task is never auto-accepted: run-due leaves it alone.
    assert_eq!(code(&run(&["run-due"])), 0);
    let st = text(&run(&["status"]));
    assert!(
        st.contains("#1 [running]") && st.contains("accept 1"),
        "{st}"
    );
    let ev = text(&run(&["evidence", "--item", "1"]));
    assert!(
        ev.contains("+add() { echo $(( $1 + $2 )); }"),
        "the diff is evidence: {ev}"
    );
    // The profile the change ran under is printed from its own evidence column.
    assert!(
        ev.contains(if cfg!(target_os = "macos") {
            "profile=scripted-agent"
        } else {
            "profile="
        }),
        "{ev}"
    );
    // publish is refused by authority.
    let p = run(&["publish", "1"]);
    assert_eq!(code(&p), 2, "{}", text(&p));
    // The owner accepts: done, by the owner.
    let a = run(&["accept", "1"]);
    assert_eq!(code(&a), 0, "{}", text(&a));
    let st = text(&run(&["status"]));
    assert!(
        st.contains("#1 [done]") && st.contains("accepted_by owner"),
        "{st}"
    );
}

#[test]
fn git_task_without_a_goal_makes_one_and_a_failing_agent_leaves_the_item_for_the_owner() {
    if !confined_or_skip() {
        return;
    }
    let (lab, tpl, cfg) = git_lab("gittask2", false);
    let db = lab.dir.join("live.db");
    let run = |args: &[&str]| lab.run_on(&db, &cfg, args);
    let t = run(&[
        "git-task",
        "--repo",
        tpl.to_str().unwrap(),
        "--instruction",
        "fix add",
        "--test-cmd",
        "sh test.sh",
    ]);
    assert_eq!(code(&t), 1, "tests still fail: {}", text(&t));
    assert!(text(&t).contains("criterion not met"), "{}", text(&t));
    let st = text(&run(&["status"]));
    assert!(st.contains("#1 [running]") && st.contains("0/1"), "{st}");
    let a = run(&["accept", "1"]);
    assert_eq!(
        code(&a),
        2,
        "an unmet criterion cannot be accepted: {}",
        text(&a)
    );
    assert_eq!(code(&run(&["decline", "1"])), 0);
    // A task needs the instruction and a test command.
    assert_eq!(
        code(&run(&["git-task", "--repo", tpl.to_str().unwrap()])),
        1
    );
}

#[test]
fn serve_runs_due_work_in_a_loop_and_exits_cleanly_on_sigterm() {
    let lab = Lab::new("serve");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let cfg = lab.config(&format!("probe web tcp 127.0.0.1:{port} 60"));
    let db = lab.dir.join("live.db");
    let run = |args: &[&str]| lab.run_on(&db, &cfg, args);
    assert_eq!(
        code(&run(&[
            "goal",
            "add",
            "--title",
            "web",
            "--probe",
            "web",
            "--acceptance",
            "auto-observe"
        ])),
        0
    );
    assert_eq!(
        code(&run(&["routine", "add", "--goal", "1", "--every", "1"])),
        0
    );
    // A bad rhythm is refused.
    assert_eq!(code(&run(&["serve", "--every", "0"])), 1);
    let mut child = Command::new(env!("CARGO_BIN_EXE_endora-foundation"))
        .arg("--db")
        .arg(&db)
        .arg("--config")
        .arg(&cfg)
        .args(["serve", "--every", "1"])
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(3200));
    // The owner's stop switch pauses the loop without killing it.
    assert_eq!(code(&run(&["stop"])), 0);
    std::thread::sleep(std::time::Duration::from_millis(1500));
    assert!(
        child.try_wait().unwrap().is_none(),
        "still serving under the stop switch"
    );
    assert_eq!(code(&run(&["resume"])), 0);
    let kill = Command::new("kill")
        .args(["-TERM", &child.id().to_string()])
        .status()
        .unwrap();
    assert!(kill.success());
    let t = std::time::Instant::now();
    let status = loop {
        if let Some(s) = child.try_wait().unwrap() {
            break s;
        }
        assert!(t.elapsed().as_secs() < 5, "serve did not exit on SIGTERM");
        std::thread::sleep(std::time::Duration::from_millis(50));
    };
    assert_eq!(status.code(), Some(0), "a clean exit on SIGTERM");
    let mut out = String::new();
    std::io::Read::read_to_string(&mut child.stdout.take().unwrap(), &mut out).unwrap();
    assert!(
        out.contains("accepted by policy:auto-observe") && out.contains("stopping"),
        "{out}"
    );
    assert!(out.contains("stop switch"), "the pause is said: {out}");
    let st = text(&run(&["status"]));
    assert!(
        st.contains("#1 [done]") && st.contains("#2 [done]"),
        "the loop fired several slots:\n{st}"
    );
}

// ---- the owner token (slice 5c) -------------------------------------------------------------

/// A lab with a goal and a routine whose first run leaves item 1 running, awaiting the owner.
fn owner_lab(name: &str) -> (Lab, PathBuf, PathBuf) {
    let lab = Lab::new(name);
    let cfg = lab.config("");
    let db = lab.dir.join("live.db");
    let run = |args: &[&str]| lab.run_on(&db, &cfg, args);
    let g = run(&[
        "goal",
        "add",
        "--title",
        "t",
        "--why",
        "w",
        "--criteria",
        "looks right",
    ]);
    assert_eq!(code(&g), 0, "{}", text(&g));
    assert_eq!(
        code(&run(&["routine", "add", "--goal", "1", "--every", "60"])),
        0
    );
    assert_eq!(code(&run(&["run-due"])), 0);
    (lab, cfg, db)
}

fn evidence_text(lab: &Lab, db: &Path, cfg: &Path) -> String {
    text(&lab.run_on(db, cfg, &["evidence", "--item", "1"]))
}

#[test]
fn init_makes_a_mode_600_token_file_once_and_never_prints_the_token() {
    let lab = Lab::new("init");
    let cfg = lab.config("");
    let db = lab.dir.join("live.db");
    let token = lab.token();
    let out = lab.run_plain(&db, &cfg, &["init", "--token-out", token.to_str().unwrap()]);
    assert_eq!(code(&out), 0, "{}", text(&out));
    let secret = std::fs::read_to_string(&token).unwrap();
    assert_eq!(secret.trim().len(), 64);
    assert!(
        !text(&out).contains(secret.trim()),
        "the token is never printed"
    );
    use std::os::unix::fs::PermissionsExt;
    let mode = std::fs::metadata(&token).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
    // A second init refuses to overwrite the file (and makes no second credential).
    let again = lab.run_plain(&db, &cfg, &["init", "--token-out", token.to_str().unwrap()]);
    assert_eq!(
        std::fs::read_to_string(&token).unwrap(),
        secret,
        "file untouched"
    );
    assert!(!text(&again).contains(secret.trim()));
    // Even into a new path: the credential exists, so no token is written.
    let other = lab.dir.join("other.token");
    let third = lab.run_plain(&db, &cfg, &["init", "--token-out", other.to_str().unwrap()]);
    // Slice 7 F4 (REVIEW3 finding 7): never exit 0 without a token; enrolment is a trusted setup
    // step, and a same-user process that enrolled first must not look like success to the owner.
    assert_eq!(code(&third), 2, "{}", text(&third));
    assert!(
        text(&third).contains("already enrolled: no token written"),
        "{}",
        text(&third)
    );
    assert!(text(&third).contains("trusted setup step"));
    assert!(!other.exists(), "no second token is made");
    // The same refusal on the path of the first token (which exists): exit 2 as well, file kept.
    assert_eq!(code(&again), 2, "{}", text(&again));
    // The database holds neither the token nor its raw bytes.
    let bytes = std::fs::read(&db).unwrap();
    assert!(!String::from_utf8_lossy(&bytes).contains(secret.trim()));
    // A stale token file at the path stops init before any credential is made.
    let lab2 = Lab::new("init2");
    let (db2, t2) = (lab2.dir.join("a.db"), lab2.dir.join("t"));
    std::fs::write(&t2, "keep").unwrap();
    let r = lab2.run_plain(&db2, &cfg, &["init", "--token-out", t2.to_str().unwrap()]);
    assert_eq!(code(&r), 1, "{}", text(&r));
    assert_eq!(std::fs::read_to_string(&t2).unwrap(), "keep");
}

#[test]
fn owner_commands_exit_2_without_a_token_or_with_a_wrong_one_and_work_with_the_right_one() {
    let (lab, cfg, db) = owner_lab("proof");
    let secret = std::fs::read_to_string(lab.token()).unwrap();
    let secret = secret.trim().to_owned();
    let mut seen = String::new();
    // No token file named at all.
    let none = lab.run_plain(&db, &cfg, &["accept", "1"]);
    assert_eq!(code(&none), 2, "{}", text(&none));
    assert!(text(&none).contains("owner token"));
    seen += &text(&none);
    // The token itself as an argument is not a token file path: refused, and not echoed.
    let arg = lab.run_plain(&db, &cfg, &["--token-file", &secret, "accept", "1"]);
    assert_eq!(code(&arg), 2);
    // A wrong token (right mode, wrong content): exit 2 and `auth-failed` is recorded.
    let wrong = lab.dir.join("wrong.token");
    std::fs::write(&wrong, "e".repeat(64)).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&wrong, std::fs::Permissions::from_mode(0o600)).unwrap();
    let w = lab.run_plain(
        &db,
        &cfg,
        &["--token-file", wrong.to_str().unwrap(), "accept", "1"],
    );
    assert_eq!(code(&w), 2, "{}", text(&w));
    seen += &text(&w);
    let status = text(&lab.run_on(&db, &cfg, &["status"]));
    assert!(
        status.contains("#1 [running]"),
        "item not accepted: {status}"
    );
    // A token file other users can read is refused.
    let loose = lab.dir.join("loose.token");
    std::fs::write(&loose, &secret).unwrap();
    std::fs::set_permissions(&loose, std::fs::Permissions::from_mode(0o644)).unwrap();
    let l = lab.run_plain(
        &db,
        &cfg,
        &["--token-file", loose.to_str().unwrap(), "accept", "1"],
    );
    assert_eq!(code(&l), 2, "{}", text(&l));
    seen += &text(&l);
    // The env var names a PATH too.
    let viaenv = Command::new(env!("CARGO_BIN_EXE_endora-foundation"))
        .env("ENDORA_OWNER_TOKEN_FILE", lab.token())
        .arg("--db")
        .arg(&db)
        .arg("--config")
        .arg(&cfg)
        .args(["confirm-criterion", "1", "1"])
        .output()
        .unwrap();
    assert_eq!(code(&viaenv), 0, "{}", text(&viaenv));
    // The right token accepts.
    let ok = lab.run_on(&db, &cfg, &["accept", "1"]);
    assert_eq!(code(&ok), 0, "{}", text(&ok));
    seen += &text(&ok);
    assert!(text(&lab.run_on(&db, &cfg, &["status"])).contains("#1 [done]"));
    // The token never appears in any captured output, evidence, or the database file.
    assert!(!seen.contains(&secret));
    assert!(!evidence_text(&lab, &db, &cfg).contains(&secret));
    assert!(!String::from_utf8_lossy(&std::fs::read(&db).unwrap()).contains(&secret));
}

#[test]
fn a_wrong_token_leaves_an_auth_failed_row_and_decline_records_the_owner() {
    let (lab, cfg, db) = owner_lab("decline");
    let wrong = lab.dir.join("wrong.token");
    std::fs::write(&wrong, "d".repeat(64)).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&wrong, std::fs::Permissions::from_mode(0o600)).unwrap();
    let w = lab.run_plain(
        &db,
        &cfg,
        &["--token-file", wrong.to_str().unwrap(), "decline", "1"],
    );
    assert_eq!(code(&w), 2, "{}", text(&w));
    let conn = rusqlite::Connection::open(&db).unwrap();
    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM evidence WHERE call='core.auth' AND output LIKE 'auth-failed%'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 1);
    let leaked: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM evidence WHERE output LIKE '%dddddddd%'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(leaked, 0);
    drop(conn);
    let d = lab.run_on(&db, &cfg, &["decline", "1"]);
    assert_eq!(code(&d), 0, "{}", text(&d));
    let conn = rusqlite::Connection::open(&db).unwrap();
    let (actor, kind): (String, String) = conn
        .query_row(
            "SELECT actor, actor_kind FROM evidence WHERE call='core.decline'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((actor.as_str(), kind.as_str()), ("owner", "owner"));
}

#[test]
fn confirm_is_a_deprecated_alias_of_confirm_criterion_and_stop_needs_no_token() {
    let (lab, cfg, db) = owner_lab("alias");
    let c = lab.run_on(&db, &cfg, &["confirm", "1", "1"]);
    assert_eq!(code(&c), 0, "{}", text(&c));
    assert!(text(&c).contains("deprecated") && text(&c).contains("confirm-criterion"));
    let s = lab.run_plain(&db, &cfg, &["stop"]);
    assert_eq!(code(&s), 0, "anyone may stop: {}", text(&s));
    let r = lab.run_plain(&db, &cfg, &["resume"]);
    assert_eq!(code(&r), 2, "resuming needs the owner: {}", text(&r));
    let r = lab.run_on(&db, &cfg, &["resume"]);
    assert_eq!(code(&r), 0, "{}", text(&r));
}

#[test]
fn verify_evidence_passes_on_a_clean_file_and_exits_1_naming_the_row_when_it_is_broken() {
    let (lab, cfg, db) = owner_lab("chain");
    let ok = lab.run_on(&db, &cfg, &["verify-evidence"]);
    assert_eq!(code(&ok), 0, "{}", text(&ok));
    assert!(text(&ok).contains("intact"));
    // Raw SQL on the file after dropping the triggers: detected, row named, exit 1.
    {
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute_batch(
            "DROP TRIGGER evidence_no_change; UPDATE evidence SET output = 'forged' WHERE id = 2;",
        )
        .unwrap();
    }
    let bad = lab.run_on(&db, &cfg, &["verify-evidence"]);
    assert_eq!(code(&bad), 1, "{}", text(&bad));
    assert!(text(&bad).contains("BROKEN at row 2"), "{}", text(&bad));
}

#[test]
fn amounts_are_micro_usd_in_the_cli_and_its_output() {
    let (lab, cfg, db) = owner_lab("units");
    let o = lab.run_on(&db, &cfg, &["overall-cap", "2500000"]);
    assert_eq!(code(&o), 0, "{}", text(&o));
    assert!(
        text(&o).contains("overall cap 2500000 micro-USD"),
        "{}",
        text(&o)
    );
    let usage = text(&lab.run_on(&db, &cfg, &["no-such-command"]));
    assert!(
        usage.contains("micro-USD") && usage.contains("1 USD = 1,000,000"),
        "{usage}"
    );
}

// ---- CLI usability (weekly review A findings 07, 08, 15, 22; D1 follow-ups; review REC2 N-c) ----

/// A SKIP is not a pass: with `ENDORA_STRICT_TESTS=1` a skipped test fails.
fn skip(why: &str) {
    eprintln!("SKIP: {why}");
    assert!(
        std::env::var_os("ENDORA_STRICT_TESTS").is_none(),
        "ENDORA_STRICT_TESTS is set and the test cannot run: {why}"
    );
}

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_endora-foundation"))
}

const COMMANDS: [&str; 19] = [
    "goal add",
    "routine add",
    "routine resume",
    "git-task",
    "publish",
    "run-due",
    "serve",
    "status",
    "init",
    "accept",
    "decline",
    "confirm-criterion",
    "stop",
    "resume",
    "resume-goal",
    "overall-cap",
    "evidence",
    "verify-evidence",
    "backup",
];

#[test]
fn help_and_version_print_to_stdout_exit_0_and_never_create_a_database() {
    let lab = Lab::new("help");
    let db = lab.dir.join("never.db");
    let db_s = db.to_str().unwrap();
    for args in [
        vec!["--help"],
        vec!["-h"],
        vec!["help"],
        vec!["--db", db_s, "--help"],
        vec!["--db", db_s, "help"],
        vec!["--db", db_s, "goal", "add", "--help"],
        vec!["--db", db_s, "status", "-h"],
    ] {
        let o = bin()
            .args(&args)
            .env_remove("ENDORA_FOUNDATION_DB")
            .output()
            .unwrap();
        assert_eq!(code(&o), 0, "{args:?}: {}", text(&o));
        assert!(o.stderr.is_empty(), "{args:?}: stderr {}", text(&o));
        let out = String::from_utf8_lossy(&o.stdout);
        for c in COMMANDS.iter().chain(&["capabilities"]) {
            assert!(out.contains(c), "{args:?}: help lacks `{c}`:\n{out}");
        }
        for f in [
            "--db",
            "--config",
            "--token-file",
            "--token-out",
            "--title",
            "--criteria",
            "--check",
            "--probe",
            "--scope",
            "--tier",
            "--cap",
            "--acceptance",
            "--every",
            "--max-failures",
            "--auto-accept",
            "--repo",
            "--instruction",
            "--test-cmd",
            "--item",
            "--to",
            "ENDORA_FOUNDATION_DB",
            "ENDORA_FOUNDATION_CONFIG",
            "ENDORA_OWNER_TOKEN_FILE",
            "micro-USD",
            "0 ok",
            "1 error",
            "2 refused",
        ] {
            assert!(out.contains(f), "{args:?}: help lacks `{f}`:\n{out}");
        }
    }
    assert!(!db.exists(), "asking for help created a database file");
    let v = bin()
        .arg("--version")
        .env_remove("ENDORA_FOUNDATION_DB")
        .output()
        .unwrap();
    assert_eq!(code(&v), 0, "{}", text(&v));
    let line = String::from_utf8_lossy(&v.stdout).trim().to_owned();
    assert_eq!(
        line,
        format!(
            "endora-foundation {} (schema {})",
            env!("CARGO_PKG_VERSION"),
            endora_foundation::infrastructure::SCHEMA_VERSION
        )
    );
    let v2 = bin().args(["--db", db_s, "--version"]).output().unwrap();
    assert_eq!(code(&v2), 0);
    assert!(!db.exists(), "--version created a database file");
}

#[test]
fn an_unknown_command_is_refused_before_a_database_is_created() {
    let lab = Lab::new("unkcmd");
    let db = lab.dir.join("never.db");
    let o = bin()
        .arg("--db")
        .arg(&db)
        .arg("frobnicate")
        .output()
        .unwrap();
    assert_eq!(code(&o), 1, "{}", text(&o));
    assert!(
        text(&o).contains("unknown command `frobnicate`") && text(&o).contains("--help"),
        "{}",
        text(&o)
    );
    assert!(!db.exists(), "an unknown command created a database");
    // The old one-line usage still names the unit.
    let none = bin().arg("--db").arg(&db).output().unwrap();
    assert_eq!(code(&none), 1);
    assert!(!db.exists());
}

#[test]
fn an_unknown_flag_or_extra_word_is_refused_naming_it_and_changes_nothing() {
    let (lab, cfg, db) = owner_lab("unkflag");
    let before = text(&lab.run_on(&db, &cfg, &["status"]));
    let cases: [(&[&str], &str); 14] = [
        (
            &[
                "goal",
                "add",
                "--title",
                "T",
                "--criteria",
                "c",
                "--bogus-flag",
                "x",
            ],
            "unknown option --bogus-flag for goal add",
        ),
        (
            &[
                "goal",
                "add",
                "--title",
                "T",
                "--criteria",
                "c",
                "--cpa",
                "5",
            ],
            "unknown option --cpa for goal add",
        ),
        (
            &["goal", "add", "--title", "T", "--criteria", "c", "stray"],
            "unexpected argument `stray` for goal add",
        ),
        (
            &[
                "routine",
                "add",
                "--goal",
                "1",
                "--every",
                "60",
                "--auto-acept",
            ],
            "unknown option --auto-acept for routine add",
        ),
        (
            &["routine", "resume", "1", "--now"],
            "unknown option --now for routine resume",
        ),
        (&["run-due", "--fast"], "unknown option --fast for run-due"),
        (&["status", "--json"], "unknown option --json for status"),
        (
            &["accept", "1", "--force"],
            "unknown option --force for accept",
        ),
        (
            &["decline", "1", "extra"],
            "unexpected argument `extra` for decline",
        ),
        (
            &["overall-cap", "5", "--x"],
            "unknown option --x for overall-cap",
        ),
        (
            &["evidence", "--item", "1", "--all"],
            "unknown option --all for evidence",
        ),
        (
            &["backup", "--to", "x.db", "--zip"],
            "unknown option --zip for backup",
        ),
        (
            &["serve", "--every", "1", "--once"],
            "unknown option --once for serve",
        ),
        (
            &["git-task", "--instruction", "i", "--test-cmd", "t", "--bad"],
            "unknown option --bad for git-task",
        ),
    ];
    for (args, want) in cases {
        let o = lab.run_on(&db, &cfg, args);
        assert_eq!(code(&o), 1, "{args:?}: {}", text(&o));
        assert!(text(&o).contains(want), "{args:?}: {}", text(&o));
    }
    let after = text(&lab.run_on(&db, &cfg, &["status"]));
    assert_eq!(before, after, "a refused command changed the database");
    // A flag that lacks its value says so, whichever command it belongs to.
    let o = lab.run_on(&db, &cfg, &["goal", "add", "--title"]);
    assert_eq!(code(&o), 1, "{}", text(&o));
    assert!(text(&o).contains("--title needs a value"), "{}", text(&o));
    let o = lab.run_plain(&db, &cfg, &["--token-file"]);
    assert_eq!(code(&o), 1, "{}", text(&o));
    assert!(
        text(&o).contains("--token-file needs a value"),
        "{}",
        text(&o)
    );
}

#[test]
fn every_documented_flag_is_still_accepted() {
    let lab = Lab::new("known");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let cfg = lab.config(&format!("probe web tcp 127.0.0.1:{port} 60"));
    let db = lab.dir.join("live.db");
    let run = |args: &[&str]| lab.run_on(&db, &cfg, args);
    let g = run(&[
        "goal",
        "add",
        "--title",
        "T",
        "--why",
        "W",
        "--criteria",
        "c",
        "--check",
        "probe.run",
        "web",
        "up",
        "true",
        "--criteria",
        "d",
        "--probe",
        "web",
        "--scope",
        "a,b",
        "--tier",
        "internal",
        "--cap",
        "5",
        "--acceptance",
        "owner",
    ]);
    assert_eq!(code(&g), 0, "{}", text(&g));
    let r = run(&[
        "routine",
        "add",
        "--goal",
        "1",
        "--every",
        "60",
        "--title",
        "R",
        "--max-failures",
        "2",
        "--auto-accept",
    ]);
    assert_eq!(code(&r), 0, "{}", text(&r));
    let r = run(&["routine", "add", "--goal", "1", "--probe", "web"]);
    assert_eq!(code(&r), 0, "{}", text(&r));
    assert_eq!(code(&run(&["status", "--short"])), 0);
    assert_eq!(code(&run(&["capabilities"])), 0);
}

#[test]
fn run_due_says_what_it_did_when_nothing_was_due_or_no_routine_exists() {
    let lab = Lab::new("nothing");
    let cfg = lab.config("");
    let db = lab.dir.join("live.db");
    let none = lab.run_on(&db, &cfg, &["run-due"]);
    assert_eq!(code(&none), 0, "{}", text(&none));
    assert!(
        text(&none).contains("no routines: a goal needs a routine to make items"),
        "{}",
        text(&none)
    );
    lab.run_on(
        &db,
        &cfg,
        &["goal", "add", "--title", "t", "--criteria", "looks right"],
    );
    lab.run_on(
        &db,
        &cfg,
        &["routine", "add", "--goal", "1", "--every", "3600"],
    );
    let first = lab.run_on(&db, &cfg, &["run-due"]);
    assert_eq!(code(&first), 0, "{}", text(&first));
    assert!(text(&first).contains("item 1"), "{}", text(&first));
    let again = lab.run_on(&db, &cfg, &["run-due"]);
    assert_eq!(code(&again), 0, "{}", text(&again));
    assert!(
        text(&again).contains("nothing due (1 routine)"),
        "{}",
        text(&again)
    );
}

#[test]
fn an_unknown_item_goal_or_criterion_says_so_and_a_negative_amount_says_why() {
    let (lab, cfg, db) = owner_lab("unknown-ids");
    let run = |args: &[&str]| lab.run_on(&db, &cfg, args);
    for (args, want) in [
        (vec!["accept", "9"], "no item 9"),
        (vec!["decline", "9"], "no item 9"),
        (vec!["confirm-criterion", "9", "1"], "no item 9"),
        (vec!["evidence", "--item", "999"], "no item 999"),
        (
            vec!["routine", "add", "--goal", "99", "--every", "300"],
            "no goal 99",
        ),
        (vec!["routine", "resume", "99"], "no routine 99"),
        (vec!["resume-goal", "99"], "no goal 99"),
        (
            vec!["confirm-criterion", "1", "7"],
            "item 1 has 1 criterion: there is no criterion 7",
        ),
        (
            vec!["confirm-criterion", "1", "0"],
            "criteria are numbered from 1",
        ),
        (
            vec![
                "goal",
                "add",
                "--title",
                "T",
                "--criteria",
                "c",
                "--cap",
                "-5",
            ],
            "--cap must not be negative",
        ),
        (
            vec![
                "routine",
                "add",
                "--goal",
                "1",
                "--every",
                "60",
                "--max-failures",
                "-1",
            ],
            "--max-failures must not be negative",
        ),
    ] {
        let o = run(&args);
        assert_eq!(code(&o), 1, "{args:?}: {}", text(&o));
        let t = text(&o);
        assert!(t.contains(want), "{args:?}: {t}");
        assert!(
            !t.contains("storage backend")
                && !t.contains("Query returned")
                && !t.contains("FOREIGN KEY")
                && !t.contains("out of range"),
            "{args:?}: a raw engine error: {t}"
        );
    }
    // Authority still comes first: without the token these are refused (2), not "no item".
    let o = lab.run_plain(&db, &cfg, &["accept", "9"]);
    assert_eq!(code(&o), 2, "{}", text(&o));
    // An item that exists but has no evidence yet is not an error.
    let ok = run(&["evidence", "--item", "1"]);
    assert_eq!(code(&ok), 0, "{}", text(&ok));
}

#[test]
fn publish_refuses_with_exit_2_one_prefix_and_no_engine_text_even_for_a_missing_item() {
    let (lab, cfg, db) = owner_lab("publish");
    for item in ["1", "999"] {
        let o = lab.run_on(&db, &cfg, &["publish", item]);
        assert_eq!(code(&o), 2, "item {item}: {}", text(&o));
        let t = text(&o);
        assert!(t.starts_with("refused: "), "{t}");
        assert_eq!(t.matches("refused:").count(), 1, "a doubled prefix: {t}");
        assert!(
            !t.contains("storage backend") && !t.contains("Query returned"),
            "{t}"
        );
    }
}

#[test]
fn an_impossible_transition_gets_its_own_message_never_corrupt_stored_data() {
    let (lab, cfg, db) = owner_lab("transitions");
    let run = |args: &[&str]| lab.run_on(&db, &cfg, args);
    // Item 1 is running (criterion unmet): decline it, then every further move is impossible.
    let d = run(&["decline", "1"]);
    assert_eq!(code(&d), 0, "{}", text(&d));
    for (args, want) in [
        (vec!["decline", "1"], "item 1 is already failed"),
        (
            vec!["accept", "1"],
            "item 1 is failed, not running: only a running item can be accepted",
        ),
    ] {
        let o = run(&args);
        // `accept` stays a refusal by the rules (2); the others were errors (1) and stay so.
        let want_code = if args[0] == "accept" { 2 } else { 1 };
        assert_eq!(code(&o), want_code, "{args:?}: {}", text(&o));
        let t = text(&o);
        assert!(t.contains(want), "{args:?}: {t}");
        assert!(
            !t.contains("corrupt stored data") && !t.contains("cannot transition"),
            "{args:?}: {t}"
        );
    }
    // An owner accept that comes second to a settle: a done item.
    let g = run(&[
        "goal",
        "add",
        "--title",
        "t2",
        "--criteria",
        "c2",
        "--acceptance",
        "auto-observe",
    ]);
    assert_eq!(code(&g), 0, "{}", text(&g));
    assert_eq!(
        code(&run(&["routine", "add", "--goal", "2", "--every", "60"])),
        0
    );
    assert_eq!(code(&run(&["run-due"])), 0);
    assert_eq!(code(&run(&["confirm-criterion", "2", "1"])), 0);
    let a = run(&["accept", "2"]);
    assert_eq!(code(&a), 0, "{}", text(&a));
    for args in [vec!["accept", "2"], vec!["decline", "2"]] {
        let o = run(&args);
        assert_eq!(
            code(&o),
            if args[0] == "accept" { 2 } else { 1 },
            "{args:?}: {}",
            text(&o)
        );
        assert!(
            text(&o).contains("item 2 is already done"),
            "{args:?}: {}",
            text(&o)
        );
        assert!(!text(&o).contains("corrupt stored data"), "{}", text(&o));
    }
}

#[test]
fn status_short_omits_the_evidence_tail() {
    let (lab, cfg, db) = owner_lab("short");
    let full = text(&lab.run_on(&db, &cfg, &["status"]));
    let short = text(&lab.run_on(&db, &cfg, &["status", "--short"]));
    assert!(
        full.contains("ROUTINES") && short.contains("ROUTINES") && short.contains("OPEN DECISIONS")
    );
    assert!(
        short.len() < full.len(),
        "--short is not shorter:\n{short}\n---\n{full}"
    );
    assert!(!short.contains("core.start"), "{short}");
}

/// The cost-violation path through the real command line: a fake `claude` reports more than the
/// grant, the goal stops, `resume-goal` needs the owner, and the goal then takes calls again.
#[test]
fn resume_goal_needs_the_token_and_lifts_a_cost_stop() {
    if !confined_or_skip() {
        skip("no confinement backend on this host");
        return;
    }
    let lab = Lab::new("resume-goal");
    let tpl = lab.dir.join("template");
    std::fs::create_dir_all(&tpl).unwrap();
    std::fs::write(tpl.join("lib.sh"), "add() { echo $(( $1 - $2 )); }\n").unwrap();
    std::fs::write(
        tpl.join("test.sh"),
        ". ./lib.sh\n[ \"$(add 2 3)\" = 5 ] || { echo FAIL; exit 1; }\necho PASS\n",
    )
    .unwrap();
    let fake = lab.dir.join("fake-claude");
    std::fs::write(
        &fake,
        "#!/bin/sh\nprintf 'add() { echo $(( $1 + $2 )); }\\n' > lib.sh\necho '{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"num_turns\":1,\"total_cost_usd\":0.50,\"result\":\"fixed\"}'\n",
    )
    .unwrap();
    std::process::Command::new("chmod")
        .arg("+x")
        .arg(&fake)
        .status()
        .unwrap();
    let cred = lab.dir.join("cred");
    std::fs::write(&cred, "DUMMY-CREDENTIAL-NOT-A-SECRET\n").unwrap();
    let cfg = lab.config(&format!(
        "git {} {} -- unused\nclaude-code {} dummy-model 300000 3\nclaude-credential {}",
        lab.dir.join("scratch").display(),
        tpl.display(),
        fake.display(),
        cred.display()
    ));
    let db = lab.dir.join("live.db");
    let run = |args: &[&str]| lab.run_on(&db, &cfg, args);
    assert_eq!(code(&run(&["overall-cap", "5000000"])), 0);
    let task = |extra: &[&str]| {
        let mut a = vec![
            "git-task",
            "--instruction",
            "fix add",
            "--test-cmd",
            "sh test.sh",
            "--repo",
            tpl.to_str().unwrap(),
        ];
        a.extend_from_slice(extra);
        run(&a)
    };
    // 0.50 USD against a 0.30 USD declaration: a violation, so the goal stops.
    let first = task(&["--cap", "1000000"]);
    assert_eq!(code(&first), 1, "{}", text(&first));
    assert!(
        text(&first).contains("broke its cost declaration"),
        "{}",
        text(&first)
    );
    let st = text(&run(&["status"]));
    assert!(
        st.contains("goal 1 is stopped after a cost violation. resume-goal 1"),
        "{st}"
    );
    // A stopped goal takes no call.
    let blocked = task(&["--goal", "1"]);
    assert_eq!(code(&blocked), 2, "{}", text(&blocked));
    assert!(
        text(&blocked).contains("the goal is stopped after a cost violation"),
        "{}",
        text(&blocked)
    );
    // Only the owner lifts it: no token, or a wrong one, exits 2 and the stop stays.
    let none = lab.run_plain(&db, &cfg, &["resume-goal", "1"]);
    assert_eq!(code(&none), 2, "{}", text(&none));
    assert!(text(&run(&["status"])).contains("goal 1 is stopped"));
    let wrong = lab.dir.join("wrong.token");
    std::fs::write(&wrong, "DUMMY-WRONG-TOKEN\n").unwrap();
    std::process::Command::new("chmod")
        .arg("600")
        .arg(&wrong)
        .status()
        .unwrap();
    let w = lab.run_plain(
        &db,
        &cfg,
        &["--token-file", wrong.to_str().unwrap(), "resume-goal", "1"],
    );
    assert_eq!(code(&w), 2, "{}", text(&w));
    assert!(text(&run(&["status"])).contains("goal 1 is stopped"));
    // With the token it lifts the stop, and the goal takes calls again (prepare runs).
    let ok = run(&["resume-goal", "1"]);
    assert_eq!(code(&ok), 0, "{}", text(&ok));
    assert!(text(&ok).contains("goal 1 resumed"), "{}", text(&ok));
    assert!(!text(&run(&["status"])).contains("goal 1 is stopped"));
    let again = task(&["--goal", "1"]);
    assert!(
        text(&again).contains("prepare:"),
        "the goal still takes no call: {}",
        text(&again)
    );
    assert!(
        !text(&again).contains("the goal is stopped"),
        "{}",
        text(&again)
    );
}

// ---- round 2: review CLI1 (B1, B2, D1, D2, D3 and the nits) ----

fn no_panic(o: &Output, what: &str) {
    let t = text(o);
    assert!(
        !t.contains("panicked") && matches!(o.status.code(), Some(0..=2)),
        "{what}: a panic or a signal (code {:?}): {t}",
        o.status.code()
    );
}

#[test]
fn a_non_utf8_argument_is_refused_with_exit_1_and_never_panics_but_help_still_works() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;
    let lab = Lab::new("utf8");
    let db = lab.dir.join("never.db");
    let bad = OsString::from_vec(vec![b'x', 0xff, b'y']);
    let s = |x: &str| OsString::from(x);
    let cases: Vec<Vec<OsString>> = vec![
        vec![bad.clone()],
        vec![s("status"), bad.clone()],
        vec![
            s("goal"),
            s("add"),
            s("--title"),
            bad.clone(),
            s("--criteria"),
            s("c"),
        ],
        vec![s("--db"), s(db.to_str().unwrap()), bad.clone()],
        vec![s("--db"), bad.clone(), s("status")],
        vec![s("--config"), bad.clone(), s("status")],
        vec![s("--token-file"), bad.clone(), s("accept"), s("1")],
        vec![s("accept"), bad.clone()],
        vec![s("--version"), bad.clone()],
    ];
    for args in &cases {
        // `--db` first, unless the case names one itself (a second one is refused).
        let mut cmd = bin();
        if !args.iter().any(|a| a == "--db") {
            cmd.arg("--db").arg(&db);
        }
        let o = cmd.args(args).output().unwrap();
        let t = text(&o);
        assert_eq!(code(&o), 1, "{args:?}: {t}");
        assert!(t.contains("not valid UTF-8"), "{args:?}: {t}");
        assert!(!t.contains("panicked"), "{args:?}: {t}");
    }
    assert!(!db.exists(), "a refused argument created a database");
    // Help and version never convert the other arguments first.
    for args in [
        vec![s("--help"), bad.clone()],
        vec![s("-h"), bad.clone()],
        vec![s("status"), s("--help"), bad.clone()],
        vec![bad.clone(), s("--help")],
    ] {
        let o = bin()
            .args(&args)
            .env_remove("ENDORA_FOUNDATION_DB")
            .output()
            .unwrap();
        // `bad --help` is an unknown command with a bad word; everything else is help.
        if args[0] == bad {
            assert_eq!(code(&o), 1, "{args:?}: {}", text(&o));
        } else {
            assert_eq!(code(&o), 0, "{args:?}: {}", text(&o));
            assert!(String::from_utf8_lossy(&o.stdout).contains("exit codes"));
        }
        assert!(!text(&o).contains("panicked"), "{args:?}: {}", text(&o));
    }
    let v = bin()
        .arg("--version")
        .env_remove("ENDORA_FOUNDATION_DB")
        .output()
        .unwrap();
    assert_eq!(code(&v), 0);
}

#[test]
fn every_numeric_option_survives_empty_negative_huge_and_overflowing_values() {
    let (lab, cfg, db) = owner_lab("numbers");
    let run = |args: &[&str]| lab.run_on(&db, &cfg, args);
    let values = [
        "",
        "-1",
        "0",
        "1",
        "+1",
        "9223372036854775807",
        "9223372036854775808",
        "18446744073709551615",
        "4294967296",
        "31622401",
        "99999999999",
        "abc",
        "1e9",
        " 1",
    ];
    for v in values {
        for args in [
            vec!["goal", "add", "--title", "T", "--criteria", "c", "--cap", v],
            vec!["routine", "add", "--goal", "1", "--every", v],
            vec!["routine", "add", "--goal", v, "--every", "60"],
            vec![
                "routine",
                "add",
                "--goal",
                "1",
                "--every",
                "60",
                "--max-failures",
                v,
            ],
            vec!["routine", "resume", v],
            vec![
                "git-task",
                "--goal",
                v,
                "--instruction",
                "i",
                "--test-cmd",
                "t",
            ],
            vec![
                "git-task",
                "--cap",
                v,
                "--instruction",
                "i",
                "--test-cmd",
                "t",
            ],
            vec!["accept", v],
            vec!["decline", v],
            vec!["publish", v],
            vec!["confirm-criterion", v, "1"],
            vec!["confirm-criterion", "1", v],
            vec!["resume-goal", v],
            vec!["overall-cap", v],
            vec!["evidence", "--item", v],
        ] {
            // Valid values may succeed (exit 0) and change the lab; none may panic.
            no_panic(&run(&args), &format!("{args:?}"));
        }
    }
    // `serve` never ends on a valid value, so only the refused ones run here.
    for v in [
        "",
        "-1",
        "0",
        "9223372036854775807",
        "9223372036854775808",
        "31622401",
        "abc",
        "1e9",
    ] {
        let o = run(&["serve", "--every", v]);
        assert_eq!(code(&o), 1, "serve --every {v:?}: {}", text(&o));
        no_panic(&o, v);
        assert!(
            text(&o).contains("--every must be a whole number")
                || text(&o).contains("--every must be between 1 and 31622400 seconds"),
            "serve --every {v:?}: {}",
            text(&o)
        );
    }
    let big = run(&[
        "routine",
        "add",
        "--goal",
        "1",
        "--every",
        "9223372036854775807",
    ]);
    assert_eq!(code(&big), 1, "{}", text(&big));
    assert!(
        text(&big).contains("--every must be between 1 and 31622400 seconds"),
        "{}",
        text(&big)
    );
    let mf = run(&[
        "routine",
        "add",
        "--goal",
        "1",
        "--every",
        "60",
        "--max-failures",
        "99999999999",
    ]);
    assert_eq!(code(&mf), 1);
    assert!(
        text(&mf).contains("--max-failures is too large"),
        "{}",
        text(&mf)
    );
    // And a routine that was accepted still runs.
    no_panic(&run(&["run-due"]), "run-due after the numbers");
}

#[test]
fn a_missing_argument_says_missing_and_opens_no_database() {
    let lab = Lab::new("missing");
    let db = lab.dir.join("never.db");
    for (args, want) in [
        (
            vec!["confirm-criterion", "1"],
            "missing N for confirm-criterion",
        ),
        (vec!["accept"], "missing ITEM for accept"),
        (vec!["publish"], "missing ITEM for publish"),
        (
            vec!["goal", "add", "--criteria", "c"],
            "missing --title for goal add",
        ),
        (vec!["evidence"], "missing --item for evidence"),
        (vec!["backup"], "missing --to for backup"),
        (vec!["serve"], "missing --every for serve"),
        (
            vec!["git-task", "--instruction", "i"],
            "missing --test-cmd for git-task",
        ),
        (vec!["overall-cap"], "missing MICRO_USD for overall-cap"),
    ] {
        let o = bin().arg("--db").arg(&db).args(&args).output().unwrap();
        assert_eq!(code(&o), 1, "{args:?}: {}", text(&o));
        assert!(text(&o).contains(want), "{args:?}: {}", text(&o));
        assert!(!text(&o).contains("whole number"), "{args:?}: {}", text(&o));
    }
    assert!(!db.exists(), "a missing argument created a database");
    // A word that is not a number is still said so.
    let o = bin()
        .arg("--db")
        .arg(&db)
        .args(["evidence", "--item", "abc"])
        .output()
        .unwrap();
    assert_eq!(code(&o), 1);
    assert!(text(&o).contains("whole number"), "{}", text(&o));
    // publish: a missing or non-numeric id is a usage error (1); a refusal is 2.
    let p = bin()
        .arg("--db")
        .arg(&db)
        .args(["publish", "abc"])
        .output()
        .unwrap();
    assert_eq!(code(&p), 1, "{}", text(&p));
}

#[test]
fn a_repeated_option_is_refused_and_so_is_token_out_outside_init_and_bogus_help() {
    let (lab, cfg, db) = owner_lab("repeat");
    let run = |args: &[&str]| lab.run_on(&db, &cfg, args);
    for (args, want) in [
        (
            vec![
                "goal",
                "add",
                "--title",
                "A",
                "--title",
                "B",
                "--criteria",
                "c",
            ],
            "--title given twice for goal add",
        ),
        (
            vec![
                "routine", "add", "--goal", "1", "--every", "60", "--every", "90",
            ],
            "--every given twice for routine add",
        ),
        (
            vec!["status", "--short", "--short"],
            "--short given twice for status",
        ),
        (
            vec!["status", "--token-out", "x.tok"],
            "--token-out is only for init",
        ),
        (vec!["bogus", "--help"], "unknown command `bogus`"),
        (
            vec!["status", "bogus"],
            "unexpected argument `bogus` for status",
        ),
    ] {
        let o = run(&args);
        assert_eq!(code(&o), 1, "{args:?}: {}", text(&o));
        assert!(text(&o).contains(want), "{args:?}: {}", text(&o));
    }
    let o = run(&["--db", db.to_str().unwrap(), "status"]);
    assert_eq!(code(&o), 1, "{}", text(&o));
    assert!(text(&o).contains("--db given twice"), "{}", text(&o));
    // Repeating the options that are documented to repeat is fine.
    let g = run(&[
        "goal",
        "add",
        "--title",
        "R",
        "--criteria",
        "a",
        "--criteria",
        "b",
        "--probe",
        "none-x",
    ]);
    assert_eq!(code(&g), 1, "{}", text(&g));
    assert!(text(&g).contains("`none-x` is not a probe"), "{}", text(&g));
    let g = run(&[
        "goal",
        "add",
        "--title",
        "R",
        "--criteria",
        "a",
        "--criteria",
        "b",
    ]);
    assert_eq!(code(&g), 0, "{}", text(&g));
    // `goal --help` is help for goal.
    let h = bin()
        .args(["goal", "--help"])
        .env_remove("ENDORA_FOUNDATION_DB")
        .output()
        .unwrap();
    assert_eq!(code(&h), 0, "{}", text(&h));
}

#[test]
fn messages_name_the_parsed_id_say_what_waits_and_status_short_has_no_tail_header() {
    let (lab, cfg, db) = owner_lab("tidy");
    let run = |args: &[&str]| lab.run_on(&db, &cfg, args);
    let nothing = text(&run(&["run-due"]));
    assert!(nothing.contains("nothing due (1 routine)"), "{nothing}");
    assert!(
        nothing.contains("1 item waiting for the owner"),
        "{nothing}"
    );
    let short = text(&run(&["status", "--short"]));
    assert!(!short.contains("EVIDENCE"), "{short}");
    assert!(short.contains("OPEN DECISIONS"), "{short}");
    let c = run(&["confirm-criterion", "+1", "+1"]);
    assert_eq!(code(&c), 0, "{}", text(&c));
    assert!(
        text(&c).contains("criterion 1 of item 1 confirmed"),
        "{}",
        text(&c)
    );
    let d = run(&["decline", "+1"]);
    assert_eq!(code(&d), 0, "{}", text(&d));
    assert!(
        text(&d).contains("item 1 declined") && !text(&d).contains("+1"),
        "{}",
        text(&d)
    );
    let r = run(&["resume-goal", "+1"]);
    assert_eq!(code(&r), 0, "{}", text(&r));
    assert!(text(&r).contains("goal 1 resumed"), "{}", text(&r));
}

#[test]
fn confirm_on_a_finished_item_is_refused_and_real_corruption_still_says_corruption() {
    let (lab, cfg, db) = owner_lab("finished");
    let run = |args: &[&str]| lab.run_on(&db, &cfg, args);
    assert_eq!(code(&run(&["decline", "1"])), 0);
    let before = text(&run(&["status"]));
    let o = run(&["confirm-criterion", "1", "1"]);
    assert_eq!(code(&o), 1, "{}", text(&o));
    assert!(
        text(&o).contains("item 1 is already failed: its criteria cannot be changed"),
        "{}",
        text(&o)
    );
    assert_eq!(
        before,
        text(&run(&["status"])),
        "a refused confirm changed the item"
    );
    // A row whose state is unreadable is real corruption: the engine's words stay for it where it
    // reads the state itself (`decline`). (`accept` and `status` show such a row as `failed`: a
    // known limit, root cause in the Core reader, planned fix CORE-2; not pinned here.)
    let (lab2, cfg2, db2) = owner_lab("corrupt");
    {
        let c = rusqlite::Connection::open(&db2).unwrap();
        c.execute_batch(
            "PRAGMA ignore_check_constraints=1; UPDATE items SET state='zzz' WHERE id=1;",
        )
        .unwrap();
    }
    let d = lab2.run_on(&db2, &cfg2, &["decline", "1"]);
    assert_eq!(code(&d), 1, "{}", text(&d));
    assert!(text(&d).contains("corrupt stored data"), "{}", text(&d));
}

#[test]
fn claude_max_seconds_is_bounded_and_help_and_version_claims_are_exact() {
    let lab = Lab::new("maxsec");
    let db = lab.dir.join("never.db");
    for (v, ok) in [
        ("18446744073709551615", false),
        ("31622401", false),
        ("0", false),
        ("-1", false),
        ("abc", false),
        ("1", true),
        ("31622400", true),
    ] {
        let cfg = lab.config(&format!("claude-code /bin/true haiku 1 1 {v}"));
        let o = bin()
            .arg("--db")
            .arg(&db)
            .arg("--config")
            .arg(&cfg)
            .arg("--version")
            .output()
            .unwrap();
        assert_eq!(code(&o), 0, "{v}: --version reads no config");
        let o = bin()
            .arg("--db")
            .arg(&db)
            .arg("--config")
            .arg(&cfg)
            .arg("capabilities")
            .output()
            .unwrap();
        if ok {
            assert_eq!(code(&o), 0, "{v}: {}", text(&o));
        } else {
            assert_eq!(code(&o), 1, "{v}: {}", text(&o));
            assert!(
                text(&o).contains("max-seconds must be"),
                "{v}: {}",
                text(&o)
            );
            assert!(!text(&o).contains("panicked"), "{v}: {}", text(&o));
        }
    }
    // The documented words: `--help` anywhere after a known command, `--version` only alone.
    assert_eq!(code(&bin().args(["status", "--help"]).output().unwrap()), 0);
    assert_eq!(
        code(&bin().args(["--version", "status"]).output().unwrap()),
        1
    );
    let o = bin().args(["--version", "status"]).output().unwrap();
    assert!(
        text(&o).contains("unknown command `--version`"),
        "{}",
        text(&o)
    );
}

#[test]
fn a_closed_stdout_after_a_change_says_so_on_stderr() {
    let lab = Lab::new("closedchange");
    let db = lab.dir.join("l.db");
    let cfg = lab.config("");
    assert_eq!(code(&lab.run_on(&db, &cfg, &["status"])), 0); // makes the token
    let mut child = bin()
        .arg("--db")
        .arg(&db)
        .arg("--token-file")
        .arg(lab.token())
        .args(["goal", "add", "--title", "t", "--criteria", "c"])
        .env_remove("ENDORA_FOUNDATION_CONFIG")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stdout.take());
    let o = child.wait_with_output().unwrap();
    // The reader may be gone before or after the write; either way no panic, and if it failed the
    // exit is 1 with the stderr line.
    let t = String::from_utf8_lossy(&o.stderr);
    assert!(!t.contains("panicked"), "{t}");
    match o.status.code() {
        Some(0) => {}
        Some(1) => assert!(
            t.contains("output closed: the change may have been applied"),
            "{t}"
        ),
        other => panic!("{other:?}: {t}"),
    }
    let st = lab.run_on(&db, &cfg, &["status"]);
    assert!(
        text(&st).contains("goal 1"),
        "the change must be applied: {}",
        text(&st)
    );
}

#[test]
fn a_closed_stdout_never_panics() {
    // `endora-foundation --help | head -0`: the reader is gone before the first write.
    for args in [vec!["--help"], vec!["--version"]] {
        for _ in 0..5 {
            let mut child = bin()
                .args(&args)
                .env_remove("ENDORA_FOUNDATION_DB")
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .unwrap();
            drop(child.stdout.take());
            let o = child.wait_with_output().unwrap();
            let t = String::from_utf8_lossy(&o.stderr);
            assert!(!t.contains("panicked"), "{args:?}: {t}");
            assert!(
                matches!(o.status.code(), Some(0 | 1)),
                "{args:?}: {:?}",
                o.status
            );
        }
    }
}
