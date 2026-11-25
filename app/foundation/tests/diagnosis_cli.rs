//! Advisory diagnosis through the real binary (ADR 0153). Off unless the config names a local
//! model: without the line, a failed watch makes no connection anywhere. With it (here a fake
//! model on a loopback port, replaying canned replies), run-due keeps a verified diagnosis and
//! `diagnoses` lists it, as advice. A non-loopback model host is refused at startup.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

fn text(o: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

struct Lab {
    dir: PathBuf,
    db: PathBuf,
    port: u16,
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

impl Lab {
    fn new(name: &str) -> Self {
        let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("diagnosis-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let lab = Self {
            db: dir.join("live.db"),
            dir,
            port: free_port(),
        };
        let cfg = lab.config("");
        let init = lab.plain(
            &cfg,
            &["init", "--token-out", lab.token().to_str().unwrap()],
        );
        assert_eq!(init.status.code(), Some(0), "{}", text(&init));
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
        assert_eq!(goal.status.code(), Some(0), "{}", text(&goal));
        let r = lab.run(
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
        assert_eq!(r.status.code(), Some(0), "{}", text(&r));
        lab
    }
    fn config(&self, extra: &str) -> PathBuf {
        let p = self.dir.join(format!("c{}.conf", extra.len()));
        std::fs::write(
            &p,
            format!("probe web tcp 127.0.0.1:{} 60\n{extra}", self.port),
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
    fn run_due(&self, cfg: &Path) -> Output {
        rusqlite::Connection::open(&self.db)
            .unwrap()
            .execute("UPDATE routines SET next_due_ms = 1 + COALESCE((SELECT MAX(due_ms) FROM firings WHERE routine_id = routines.id), 0)", [])
            .unwrap();
        self.run(cfg, &["run-due"])
    }
}

/// A fake model: counts connections and answers like Ollama. The first request of a pair gets a
/// found-answer quoting the first line of the first evidence block; the second (the checker) gets yes.
fn fake_model(hits: Arc<AtomicUsize>) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut s) = stream else { return };
            hits.fetch_add(1, Ordering::SeqCst);
            let mut buf = vec![0u8; 65536];
            let mut got = 0;
            loop {
                let n = s.read(&mut buf[got..]).unwrap_or(0);
                got += n;
                let seen = String::from_utf8_lossy(&buf[..got]).into_owned();
                if n == 0 {
                    break;
                }
                let Some((head, body)) = seen.split_once("\r\n\r\n") else {
                    continue;
                };
                let want: usize = head
                    .to_lowercase()
                    .split("content-length: ")
                    .nth(1)
                    .and_then(|r| r.split("\r\n").next())
                    .and_then(|v| v.trim().parse().ok())
                    .unwrap_or(0);
                if body.len() >= want {
                    break;
                }
            }
            let req = String::from_utf8_lossy(&buf[..got]).into_owned();
            let body = req.split_once("\r\n\r\n").map_or("", |x| x.1);
            let v: serde_json::Value = serde_json::from_str(body).unwrap_or_default();
            let prompt = v["prompt"].as_str().unwrap_or("");
            let reply = if prompt.starts_with("You are a strict checker") {
                "yes".to_owned()
            } else {
                // The block of the probe's output, and its line that names the cause.
                let block = prompt
                    .split("<<<EVIDENCE ")
                    .find(|b| b.contains("Connection refused"))
                    .unwrap_or("");
                let doc = block.lines().next().unwrap_or("");
                let line = block
                    .lines()
                    .find(|l| l.contains("Connection refused"))
                    .unwrap_or("");
                let line = line.trim_start_matches("-- ");
                serde_json::json!({"status":"found","answer":line,"evidence":[{"doc":doc,"quote":line}]}).to_string()
            };
            let out = serde_json::json!({"response": reply}).to_string();
            let _ = write!(
                s,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{out}",
                out.len()
            );
        }
    });
    port
}

#[test]
fn without_the_config_line_a_failed_watch_asks_no_model_at_all() {
    let lab = Lab::new("off");
    let hits = Arc::new(AtomicUsize::new(0));
    // A model IS listening on the usual kind of port; nothing in the config names it.
    let _model_port = fake_model(hits.clone());
    let cfg = lab.config("");
    for _ in 0..3 {
        let o = lab.run_due(&cfg);
        assert!(text(&o).contains("failed"), "the watch fails: {}", text(&o));
    }
    assert_eq!(
        hits.load(Ordering::SeqCst),
        0,
        "disabled by default: no connection was made"
    );
    assert!(text(&lab.plain(&cfg, &["diagnoses"])).contains("no diagnoses"));
}

#[test]
fn with_a_local_model_named_a_failed_watch_gets_a_verified_advisory_diagnosis() {
    let lab = Lab::new("on");
    let hits = Arc::new(AtomicUsize::new(0));
    let model_port = fake_model(hits.clone());
    let cfg = lab.config(&format!(
        "diagnosis-model ollama 127.0.0.1:{model_port} test-model\n"
    ));
    let o = lab.run_due(&cfg);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    assert!(text(&o).contains("diagnoses: 1 written"), "{}", text(&o));
    assert_eq!(
        hits.load(Ordering::SeqCst),
        2,
        "the proposal and one entailment check: {}",
        text(&lab.plain(&cfg, &["diagnoses"]))
    );
    let listed = text(&lab.plain(&cfg, &["diagnoses"]));
    assert!(
        listed.contains("[verified]") && listed.contains("(advisory)"),
        "{listed}"
    );
    // The decide path is untouched: the item stays failed, the chain verifies, nothing was run.
    let again = lab.run_due(&cfg);
    assert_eq!(
        hits.load(Ordering::SeqCst),
        2 + 2,
        "the next failed item is diagnosed once, the first never again: {}",
        text(&again)
    );
    let status = lab.plain(&cfg, &["status"]);
    assert_eq!(status.status.code(), Some(0), "{}", text(&status));
}

#[test]
fn a_model_that_is_not_on_this_machine_is_refused_at_startup() {
    let lab = Lab::new("remote");
    for host in ["192.0.2.5:11434", "example.com:11434", "8.8.8.8:11434"] {
        let cfg = lab.config(&format!("diagnosis-model ollama {host} m\n"));
        let o = lab.plain(&cfg, &["status"]);
        assert_ne!(o.status.code(), Some(0), "{host}");
        assert!(
            text(&o).contains("this machine") || text(&o).contains("resolve"),
            "{host}: {}",
            text(&o)
        );
    }
}
