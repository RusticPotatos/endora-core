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
