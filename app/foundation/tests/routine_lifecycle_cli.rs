//! `routine pause|resume|retire` and the bounded `status --short`, through the real binary.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::Arc;

use endora_foundation::application::Core;
use endora_foundation::domain::GoalId;
use endora_foundation::infrastructure::SystemClock;

struct Lab {
    dir: PathBuf,
    db: PathBuf,
    cfg: PathBuf,
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

impl Lab {
    fn new(name: &str) -> Self {
        let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("lifecycle-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let cfg = dir.join("foundation.conf");
        std::fs::write(&cfg, "# a temp config\n").unwrap();
        let lab = Self {
            db: dir.join("live.db"),
            cfg,
            dir,
        };
        let init = lab.plain(&["init", "--token-out", lab.token().to_str().unwrap()]);
        assert_eq!(code(&init), 0, "{}", text(&init));
        let goal = lab.run(&[
            "goal",
            "add",
            "--title",
            "t",
            "--why",
            "w",
            "--criteria",
            "looks right",
        ]);
        assert_eq!(code(&goal), 0, "{}", text(&goal));
        lab
    }
    fn token(&self) -> PathBuf {
        self.dir.join("owner.token")
    }
    fn plain(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_endora-foundation"))
            .arg("--db")
            .arg(&self.db)
            .arg("--config")
            .arg(&self.cfg)
            .args(args)
            .output()
            .unwrap()
    }
    fn run(&self, args: &[&str]) -> Output {
        let token = self.token();
        let mut full = vec!["--token-file", token.to_str().unwrap()];
        full.extend_from_slice(args);
        self.plain(&full)
    }
    fn routine(&self) -> Output {
        let o = self.run(&["routine", "add", "--goal", "1", "--every", "60"]);
        assert_eq!(code(&o), 0, "{}", text(&o));
        o
    }
}

#[test]
fn pause_resume_and_retire_need_the_owner_token_like_every_owner_verb() {
    let lab = Lab::new("token");
    lab.routine();
    for verb in ["pause", "resume", "retire"] {
        let o = lab.plain(&["routine", verb, "1"]);
        assert_eq!(code(&o), 2, "{verb}: {}", text(&o));
        assert!(text(&o).contains("owner token"), "{verb}: {}", text(&o));
    }
    // A wrong token is refused too, and nothing changed.
    let wrong = lab.dir.join("wrong.token");
    std::fs::write(&wrong, "e".repeat(64)).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&wrong, std::fs::Permissions::from_mode(0o600)).unwrap();
    let w = lab.plain(&[
        "--token-file",
        wrong.to_str().unwrap(),
        "routine",
        "retire",
        "1",
    ]);
    assert_eq!(code(&w), 2, "{}", text(&w));
    let st = text(&lab.run(&["status"]));
    assert!(st.contains("running, failures 0/5"), "{st}");
    assert!(!st.contains("RETIRED") && !st.contains("PAUSED"), "{st}");
}

#[test]
fn run_due_skips_a_paused_routine_and_a_retired_one_and_the_chain_records_both() {
    let lab = Lab::new("skip");
    lab.routine(); // routine 1
    lab.routine(); // routine 2
    lab.routine(); // routine 3
    for args in [["routine", "pause", "1"], ["routine", "retire", "2"]] {
        let o = lab.run(&args);
        assert_eq!(code(&o), 0, "{args:?}: {}", text(&o));
    }
    let d = lab.run(&["run-due"]);
    assert_eq!(code(&d), 0, "{}", text(&d));
    let out = text(&d);
    assert!(out.contains("routine 3: item"), "{out}");
    assert!(
        !out.contains("routine 1:") && !out.contains("routine 2:"),
        "{out}"
    );
    let st = text(&lab.run(&["status"]));
    assert!(
        st.contains("routine 1 goal #1 every 60 s: PAUSED by the owner"),
        "{st}"
    );
    assert!(st.contains("routine 2 goal #1 every 60 s: RETIRED"), "{st}");
    assert!(
        st.contains("core.pause_routine") && st.contains("routine 1 paused"),
        "{st}"
    );
    assert!(
        st.contains("core.retire_routine") && st.contains("routine 2 retired"),
        "{st}"
    );
    let v = lab.run(&["verify-evidence"]);
    assert_eq!(code(&v), 0, "{}", text(&v));

    // Resume the paused one: it fires; the retired one cannot be resumed (exit 1, says why).
    assert_eq!(code(&lab.run(&["routine", "resume", "1"])), 0);
    let r = lab.run(&["routine", "resume", "2"]);
    assert_eq!(code(&r), 1, "{}", text(&r));
    assert!(text(&r).contains("retired"), "{}", text(&r));
    let p = lab.run(&["routine", "pause", "2"]);
    assert_eq!(code(&p), 1, "{}", text(&p));
    let none = lab.run(&["routine", "retire", "9"]);
    assert_eq!(code(&none), 1, "{}", text(&none));
    assert!(text(&none).contains("no routine 9"), "{}", text(&none));
    assert!(!text(&lab.run(&["status"])).contains("routine 2 resumed"));
}

/// Gives goal 1 `n` items titled `work 01`.. in-process (the CLI has no verb for a bare item).
fn add_items(db: &Path, n: usize) {
    let core = Core::open(db.to_str().unwrap(), Arc::new(SystemClock)).unwrap();
    for i in 1..=n {
        core.add_item(GoalId(1), &format!("work {i:02}")).unwrap();
    }
}

#[test]
fn status_short_shows_totals_and_only_the_most_recent_items_per_goal() {
    let lab = Lab::new("short");
    add_items(&lab.db, 25);
    let short = text(&lab.run(&["status", "--short"]));
    assert!(short.contains("25 items"), "totals per goal:\n{short}");
    assert!(short.contains("25 open"), "{short}");
    for i in 16..=25 {
        assert!(short.contains(&format!("work {i}")), "work {i}:\n{short}");
    }
    assert!(
        !short.contains("work 15") && !short.contains("work 01"),
        "{short}"
    );
    assert!(short.contains("15 older items not shown"), "{short}");
    assert!(short.contains("--all"), "says how to see them:\n{short}");
    assert!(
        !short.contains("core.start"),
        "still no evidence tail:\n{short}"
    );
    assert!(
        short.contains("ROUTINES") && short.contains("OPEN DECISIONS"),
        "{short}"
    );

    let few = text(&lab.run(&["status", "--short", "--recent", "3"]));
    assert!(few.contains("work 25") && few.contains("work 23"), "{few}");
    assert!(!few.contains("work 22"), "{few}");
    assert!(few.contains("22 older items not shown"), "{few}");

    // `--all` is the old behaviour: every item, still without the tail.
    let all = text(&lab.run(&["status", "--short", "--all"]));
    assert!(all.contains("work 01") && all.contains("work 25"), "{all}");
    assert!(
        !all.contains("older items not shown") && !all.contains("core.start"),
        "{all}"
    );

    // Plain `status` is unchanged: all items and the tail.
    let full = text(&lab.run(&["status"]));
    assert!(
        full.contains("work 01") && full.contains("EVIDENCE (tail)"),
        "{full}"
    );
}

#[test]
fn status_short_bounds_each_goal_separately_and_prints_no_note_when_nothing_is_hidden() {
    let lab = Lab::new("goals");
    let g2 = lab.run(&[
        "goal",
        "add",
        "--title",
        "second",
        "--why",
        "w",
        "--criteria",
        "ok",
    ]);
    assert_eq!(code(&g2), 0, "{}", text(&g2));
    add_items(&lab.db, 12);
    {
        let core = Core::open(lab.db.to_str().unwrap(), Arc::new(SystemClock)).unwrap();
        for i in 1..=2 {
            core.add_item(GoalId(2), &format!("other {i}")).unwrap();
        }
    }
    let short = text(&lab.run(&["status", "--short"]));
    assert!(
        short.contains("12 items") && short.contains("2 items"),
        "{short}"
    );
    assert!(
        short.contains("other 1") && short.contains("other 2"),
        "{short}"
    );
    assert!(
        short.contains("work 12") && short.contains("work 03"),
        "{short}"
    );
    assert!(!short.contains("work 02"), "{short}");
    assert_eq!(short.matches("older items not shown").count(), 1, "{short}");
}

#[test]
fn recent_needs_short_and_a_number() {
    let lab = Lab::new("recent");
    for args in [
        vec!["status", "--recent", "3"],
        vec!["status", "--short", "--recent", "0"],
        vec!["status", "--short", "--recent", "x"],
        vec!["status", "--short", "--all", "--recent", "3"],
    ] {
        let o = lab.run(&args);
        assert_eq!(code(&o), 1, "{args:?}: {}", text(&o));
        assert!(
            text(&o).contains("--recent") || text(&o).contains("--all"),
            "{args:?}: {}",
            text(&o)
        );
    }
}
