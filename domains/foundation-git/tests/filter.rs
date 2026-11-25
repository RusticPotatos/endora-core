//! The read-only filter job behind model-written parser adapters (ADR 0154): one directory
//! mounted read-only, text on stdin, one capped answer on stdout, no network, nothing writable
//! but a small tmpfs. The adapters here are tiny synthetic scripts; the tests run them in
//! whichever backend this host can run (bwrap, else Docker) and print `SKIP: <reason>` when none
//! can (a failure under `ENDORA_STRICT_TESTS=1`). No network, no model, nothing outside the
//! test's own temp directory.

use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::time::Duration;

use endora_foundation_git::{
    Confinement, FilterJob, detect_filter, filter_bwrap_args, filter_docker_args, run_filter,
};

fn image() -> String {
    std::env::var("ENDORA_FOUNDATION_ADAPTER_IMAGE").unwrap_or_else(|_| "python:3.12-slim".into())
}

fn backend() -> Option<Confinement> {
    // The backends are Linux ones (bwrap, Docker); the gate on macOS sets ENDORA_STRICT_TESTS for
    // the macOS sandbox tests, which says nothing about these.
    if !cfg!(target_os = "linux") {
        eprintln!("SKIP: a filter job runs under bwrap or Docker on Linux hosts only");
        return None;
    }
    let (c, note) = detect_filter(&image());
    if c != Confinement::Unavailable {
        return Some(c);
    }
    eprintln!("SKIP: no backend can run a filter job here: {note}");
    assert!(
        std::env::var_os("ENDORA_STRICT_TESTS").is_none(),
        "ENDORA_STRICT_TESTS is set and the test cannot run: {note}"
    );
    None
}

struct Dirs(PathBuf);
impl Dirs {
    fn new(name: &str) -> Self {
        let d = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("filter-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("adapter")).unwrap();
        std::fs::create_dir_all(d.join("records")).unwrap();
        Self(d)
    }
    fn adapter(&self, source: &str) -> PathBuf {
        let p = self.0.join("adapter");
        std::fs::write(p.join("adapter.py"), source).unwrap();
        p
    }
}
impl Drop for Dirs {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn run(
    c: Confinement,
    d: &Dirs,
    adapter: &Path,
    stdin: &[u8],
    secs: u64,
    most: usize,
) -> endora_foundation_git::Filtered {
    let image = image();
    let records = d.0.join("records");
    run_filter(&FilterJob {
        confinement: c,
        dir: adapter,
        entry: "adapter.py",
        stdin,
        limit: Duration::from_secs(secs),
        most_out: most,
        image: &image,
        records: &records,
    })
    .unwrap()
}

const ECHO: &str = r#"import json, sys
t = json.loads(sys.stdin.read())["text"]
print(json.dumps({"status": "ok", "reason": "", "facts": [{"name": "n", "value": len(t)}]}))
"#;

#[test]
fn bwrap_vector_has_no_network_no_writable_bind_and_the_directory_read_only() {
    let a = filter_bwrap_args(Path::new("/srv/adapters/abc"));
    let has = |x: &str| a.iter().any(|s| s == x);
    assert!(has("--unshare-net") && has("--unshare-pid") && has("--die-with-parent"));
    assert!(
        !has("--bind") && !has("--bind-try"),
        "nothing is bound writable: {a:?}"
    );
    assert!(
        a.windows(3)
            .any(|w| w[0] == "--ro-bind" && w[1] == "/srv/adapters/abc" && w[2] == "/adapter"),
        "{a:?}"
    );
    assert!(a.windows(2).any(|w| w[0] == "--tmpfs" && w[1] == "/tmp"));
    assert!(
        a.windows(2)
            .any(|w| w[0] == "--chdir" && w[1] == "/adapter")
    );
    assert_eq!(a.last().map(String::as_str), Some("--"));
}

#[test]
fn docker_vector_has_no_network_a_read_only_root_and_a_read_only_mount() {
    let a = filter_docker_args(
        "frx-00",
        "img:1",
        Path::new("/srv/adapters/abc"),
        "adapter.py",
    );
    let pair = |x: &str, y: &str| a.windows(2).any(|w| w[0] == x && w[1] == y);
    assert!(pair("--network", "none") && pair("--cap-drop", "ALL") && pair("--pull", "never"));
    assert!(a.iter().any(|s| s == "--read-only") && a.iter().any(|s| s == "-i"));
    assert!(pair("-v", "/srv/adapters/abc:/adapter:ro"), "{a:?}");
    assert!(
        !a.iter().any(|s| s.ends_with(":rw")),
        "no writable mount: {a:?}"
    );
    let img = a.iter().position(|s| s == "img:1").unwrap();
    assert_eq!(
        a[img + 1..].join(" "),
        "-I -S -B /adapter/adapter.py",
        "the interpreter's own flags follow the image: {a:?}"
    );
}

#[test]
fn an_adapter_reads_stdin_and_answers_on_stdout() {
    let Some(c) = backend() else { return };
    let d = Dirs::new("echo");
    let dir = d.adapter(ECHO);
    let r = run(c, &d, &dir, br#"{"text": "abcd"}"#, 20, 4096);
    assert_eq!(r.code, Some(0), "{r:?}");
    let out = String::from_utf8(r.stdout).unwrap();
    assert!(out.contains(r#""value": 4"#), "{out}");
}

#[test]
fn an_adapter_cannot_reach_a_network_even_on_loopback() {
    let Some(c) = backend() else { return };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let d = Dirs::new("net");
    let dir = d.adapter(&format!(
        r#"import socket, sys
sys.stdin.read()
seen = []
for host, port in (("127.0.0.1", {port}), ("192.0.2.1", 80)):
    try:
        socket.create_connection((host, port), timeout=2).close()
        seen.append("connected")
    except OSError:
        seen.append("blocked")
print(",".join(seen))
"#
    ));
    let r = run(c, &d, &dir, b"{}", 20, 4096);
    assert_eq!(
        String::from_utf8(r.stdout).unwrap().trim(),
        "blocked,blocked"
    );
    listener.set_nonblocking(true).unwrap();
    assert!(
        listener.accept().is_err(),
        "the host's listener never saw a connection"
    );
}

#[test]
fn an_adapter_cannot_write_its_own_directory_or_the_host() {
    let Some(c) = backend() else { return };
    let d = Dirs::new("write");
    let host_file = d.0.join("host-side");
    let dir = d.adapter(&format!(
        r#"import sys
sys.stdin.read()
seen = []
for path in ("/adapter/new.txt", "/adapter/adapter.py", "{}", "/usr/new.txt"):
    try:
        open(path, "w").write("x")
        seen.append("wrote")
    except OSError:
        seen.append("refused")
print(",".join(seen))
"#,
        host_file.display()
    ));
    let r = run(c, &d, &dir, b"{}", 20, 4096);
    assert_eq!(
        String::from_utf8(r.stdout).unwrap().trim(),
        "refused,refused,refused,refused"
    );
    assert!(!host_file.exists());
    assert!(!dir.join("new.txt").exists());
    assert_eq!(
        std::fs::read_to_string(dir.join("adapter.py")).unwrap()[..6],
        *"import"
    );
}

#[test]
fn an_adapter_that_never_ends_is_stopped_at_the_limit() {
    let Some(c) = backend() else { return };
    let d = Dirs::new("loop");
    let dir = d.adapter("while True:\n    pass\n");
    let t = std::time::Instant::now();
    let r = run(c, &d, &dir, b"{}", 2, 4096);
    assert!(r.timed_out, "{r:?}");
    assert!(t.elapsed() < Duration::from_secs(15), "{:?}", t.elapsed());
}

#[test]
fn an_answer_over_the_cap_is_flagged_and_cut() {
    let Some(c) = backend() else { return };
    let d = Dirs::new("big");
    let dir = d.adapter("import sys\nsys.stdin.read()\nsys.stdout.write('x' * 300000)\n");
    let r = run(c, &d, &dir, b"{}", 20, 1000);
    assert!(
        r.over && r.stdout.len() <= 1000,
        "{} {}",
        r.over,
        r.stdout.len()
    );
}

#[test]
fn a_large_input_does_not_stall_the_job() {
    let Some(c) = backend() else { return };
    let d = Dirs::new("large");
    let dir = d.adapter(ECHO);
    let text = "a".repeat(250_000);
    let input = format!(r#"{{"text": "{text}"}}"#);
    let r = run(c, &d, &dir, input.as_bytes(), 20, 4096);
    assert!(
        String::from_utf8(r.stdout).unwrap().contains("250000"),
        "{:?}",
        r.code
    );
}
