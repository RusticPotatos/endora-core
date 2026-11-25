//! Owner notices through the real binary (ADR 0152): a watch that fails for the grace says so
//! once into the local notice file and the `notices` listing; recovery says so once; the owner
//! acknowledges with the token; a delivery that fails retries without a duplicate, even with the
//! stop switch on. Nothing here sends anywhere.

use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use rusqlite::Connection;

const BACKDATE_MS: i64 = 700_000;

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

struct Lab {
    dir: PathBuf,
    db: PathBuf,
    port: u16,
}

impl Lab {
    fn new(name: &str) -> Self {
        let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("notices-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let port = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let lab = Self {
            db: dir.join("live.db"),
            dir,
            port,
        };
        let cfg = lab.config(&lab.dir.join("notices.log"));
        let init = lab.plain(
            &cfg,
            &["init", "--token-out", lab.token().to_str().unwrap()],
        );
        assert_eq!(code(&init), 0, "{}", text(&init));
        let goal = lab.run(
            &cfg,
            &[
                "goal",
                "add",
                "--title",
                "web answers",
                "--probe",
                "web",
                "--acceptance",
                "auto-observe",
            ],
        );
        assert_eq!(code(&goal), 0, "{}", text(&goal));
        let routine = lab.run(
            &cfg,
            &[
                "routine",
                "add",
                "--goal",
                "1",
                "--every",
                "1",
                "--title",
                "web front",
                "--max-failures",
                "50",
            ],
        );
        assert_eq!(code(&routine), 0, "{}", text(&routine));
        lab
    }
    fn config(&self, notice_file: &Path) -> PathBuf {
        // One config per notice file, named after its whole path.
        let tag: String = notice_file
            .to_string_lossy()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect();
        let p = self.dir.join(format!(
            "foundation-{}.conf",
            &tag[tag.len().saturating_sub(40)..]
        ));
        std::fs::write(
            &p,
            format!(
                "probe web tcp 127.0.0.1:{} 60\nnotice-file {}\n",
                self.port,
                notice_file.display()
            ),
        )
        .unwrap();
        p
    }
    fn token(&self) -> PathBuf {
        self.dir.join("owner.token")
    }
    fn plain(&self, cfg: &Path, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_endora-foundation"))
            .arg("--db")
            .arg(&self.db)
            .arg("--config")
            .arg(cfg)
            .args(args)
            .output()
            .unwrap()
    }
    fn run(&self, cfg: &Path, args: &[&str]) -> Output {
        let token = self.token();
        let mut full = vec!["--token-file", token.to_str().unwrap()];
        full.extend_from_slice(args);
        self.plain(cfg, &full)
    }
    /// The web port answers (true) or is closed (false).
    fn serve(&self, up: bool) -> Option<TcpListener> {
        up.then(|| TcpListener::bind(("127.0.0.1", self.port)).unwrap())
    }
    /// Makes the routine due now, as the next slot would be a second later.
    fn due_now(&self) {
        Connection::open(&self.db)
            .unwrap()
            .execute(
                "UPDATE routines SET next_due_ms = 1 + COALESCE((SELECT MAX(due_ms) FROM firings WHERE routine_id = routines.id), 0)",
                [],
            )
            .unwrap();
    }
    /// Moves the failing run's start back, as ten minutes passing would.
    fn backdate_run(&self) {
        Connection::open(&self.db)
            .unwrap()
            .execute(
                "UPDATE watch_state SET failing_since_ms = failing_since_ms - ?1",
                [BACKDATE_MS],
            )
            .unwrap();
    }
    fn run_due(&self, cfg: &Path) -> Output {
        self.due_now();
        self.run(cfg, &["run-due"])
    }
}

fn lines(p: &Path) -> Vec<String> {
    std::fs::read_to_string(p)
        .map(|t| t.lines().map(str::to_owned).collect())
        .unwrap_or_default()
}

#[test]
fn a_failing_watch_is_said_once_then_recovered_once_and_the_owner_acknowledges() {
    let lab = Lab::new("flow");
    let file = lab.dir.join("notices.log");
    let cfg = lab.config(&file);
    // Down: three failures inside the grace say nothing.
    for _ in 0..3 {
        let d = lab.run_due(&cfg);
        assert_eq!(code(&d), 0, "{}", text(&d));
    }
    assert!(lines(&file).is_empty());
    assert!(text(&lab.plain(&cfg, &["notices"])).contains("no notices"));
    // The grace runs out: exactly one failing notice, however many passes follow.
    lab.backdate_run();
    let d = lab.run_due(&cfg);
    assert!(text(&d).contains("notice"), "{}", text(&d));
    lab.run_due(&cfg);
    lab.run_due(&cfg);
    let written = lines(&file);
    assert_eq!(written.len(), 1, "{written:?}");
    assert!(written[0].contains("state=failing") && written[0].contains("name=\"web front\""));
    let listed = lab.plain(&cfg, &["notices"]);
    assert_eq!(code(&listed), 0, "{}", text(&listed));
    assert!(text(&listed).contains("failing") && text(&listed).contains("web front"));
    // Acknowledging needs the owner token.
    let no_token = lab.plain(&cfg, &["notices", "ack", "1"]);
    assert_eq!(code(&no_token), 2, "{}", text(&no_token));
    let ack = lab.run(&cfg, &["notices", "ack", "1"]);
    assert_eq!(code(&ack), 0, "{}", text(&ack));
    assert!(text(&lab.plain(&cfg, &["notices"])).contains("no notices"));
    assert!(text(&lab.plain(&cfg, &["notices", "--all"])).contains("failing"));
    assert_eq!(
        code(&lab.run(&cfg, &["notices", "ack", "1"])),
        1,
        "twice is an error"
    );
    // Up again: one recovered notice, and no more after it.
    let _up = lab.serve(true);
    let d = lab.run_due(&cfg);
    assert_eq!(code(&d), 0, "{}", text(&d));
    lab.run_due(&cfg);
    let last = lab.run_due(&cfg);
    let written = lines(&file);
    assert_eq!(written.len(), 2, "{written:?} {} {}", text(&d), text(&last));
    assert!(written[1].contains("state=recovered"));
    assert!(text(&lab.plain(&cfg, &["notices"])).contains("recovered"));
}

#[test]
fn a_notice_carries_no_text_from_the_probe_or_its_target() {
    let lab = Lab::new("leak");
    let file = lab.dir.join("notices.log");
    let cfg = lab.config(&file);
    for _ in 0..3 {
        lab.run_due(&cfg);
    }
    lab.backdate_run();
    lab.run_due(&cfg);
    let all = format!(
        "{}{}",
        lines(&file).join("\n"),
        text(&lab.plain(&cfg, &["notices", "--all"]))
    );
    // The routine name, the state and the time only: no address, port or probe wording.
    assert!(
        !all.contains("127.0.0.1") && !all.contains(&lab.port.to_string()),
        "{all}"
    );
    assert!(!all.contains("tcp") && !all.contains("refused"), "{all}");
}

#[test]
fn a_delivery_that_fails_is_retried_once_the_sink_works_even_with_the_stop_switch_on() {
    let lab = Lab::new("retry");
    let bad = lab.dir.join("no-such-dir").join("notices.log");
    let good = lab.dir.join("notices.log");
    let (cfg_bad, cfg_good) = (lab.config(&bad), lab.config(&good));
    for _ in 0..3 {
        lab.run_due(&cfg_bad);
    }
    lab.backdate_run();
    let d = lab.run_due(&cfg_bad);
    assert_eq!(
        code(&d),
        0,
        "a sink that fails does not fail the run: {}",
        text(&d)
    );
    assert!(text(&d).contains("will retry"), "{}", text(&d));
    assert!(!bad.exists());
    // Stopped: the run is refused (exit 2), the owner is still told, once.
    assert_eq!(code(&lab.plain(&cfg_good, &["stop"])), 0);
    let d = lab.run_due(&cfg_good);
    assert_eq!(code(&d), 2, "{}", text(&d));
    assert_eq!(lines(&good).len(), 1, "{:?}", lines(&good));
    let d = lab.run_due(&cfg_good);
    assert_eq!(code(&d), 2);
    assert_eq!(lines(&good).len(), 1, "still one line");
}

#[test]
fn the_notices_verb_is_a_known_command_with_help_and_usage() {
    let lab = Lab::new("help");
    let cfg = lab.config(&lab.dir.join("notices.log"));
    let h = lab.plain(&cfg, &["--help"]);
    assert!(text(&h).contains("notices"), "{}", text(&h));
    let bad = lab.plain(&cfg, &["notices", "--bogus"]);
    assert_eq!(code(&bad), 1, "{}", text(&bad));
    let missing = lab.run(&cfg, &["notices", "ack"]);
    assert_eq!(code(&missing), 1, "{}", text(&missing));
}
