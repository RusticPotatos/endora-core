//! The Docker confinement backend (a plain unprivileged container per executor call), on a host
//! where it can run: attack regressions, the leftover-container clean-up after a SIGKILLed Core,
//! and that no token, database or Docker socket is visible to an executor. Every test SKIPS with
//! a printed reason when Docker or the executor image is missing; none touches the network, a
//! model or anything outside its own temp directory.
//!
//! The executor image must contain `git` and `bash` (`rust:1.87-bookworm` does); set it with
//! `ENDORA_FOUNDATION_EXECUTOR_IMAGE`. `FRX_ATTACK_OUT=<file>` copies the attack transcript there.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use endora_foundation::application::Capability;
use endora_foundation_git::{Config, Confinement, GitChange};

fn image() -> String {
    std::env::var("ENDORA_FOUNDATION_EXECUTOR_IMAGE")
        .unwrap_or_else(|_| "rust:1.87-bookworm".into())
}

/// Docker answers and the image exists locally; otherwise says why and the test returns.
fn docker_or_skip() -> bool {
    // The backend targets Linux hosts. On macOS the `Auto` order picks `sandbox-exec` first, and
    // Docker Desktop's bind mounts report other owners (git then refuses the repo as "dubious
    // ownership"), so these Linux attack tests are not run there.
    if !cfg!(target_os = "linux") {
        eprintln!("SKIPPED: the Docker backend tests run on Linux hosts only");
        return false;
    }
    let ok = |args: &[&str]| {
        Command::new("docker")
            .args(args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    };
    if ok(&["version", "--format", "{{.Server.Version}}"]) && ok(&["image", "inspect", &image()]) {
        return true;
    }
    eprintln!(
        "SKIPPED: Docker is not usable or image `{}` is not present locally",
        image()
    );
    false
}

fn w(p: &Path, s: &str) {
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d).unwrap();
    }
    std::fs::write(p, s).unwrap();
}

struct Lab {
    dir: PathBuf,
}

impl Lab {
    fn new(name: &str) -> Self {
        let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("dk-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("template")).unwrap();
        let dir = dir.canonicalize().unwrap();
        w(&dir.join("template/test.sh"), "echo PASS\n");
        Self { dir }
    }
    fn config(&self, agent_body: &str, limit: u64) -> Config {
        let agent = self.dir.join("agent.sh");
        w(&agent, agent_body);
        Config {
            scratch_root: self.dir.join("scratch"),
            template: self.dir.join("template"),
            agent_command: vec!["bash".into(), agent.to_str().unwrap().into()],
            test_command: vec!["sh".into(), "test.sh".into()],
            protected: vec!["test.sh".into()],
            agent_limit: Duration::from_secs(limit),
            test_limit: Duration::from_secs(10),
            confinement: Confinement::Docker,
        }
    }
    fn cap(&self, agent_body: &str, limit: u64) -> GitChange {
        let o = endora_foundation_git::Options {
            executor_image: Some(image()),
            ..Default::default()
        };
        GitChange::with_options(self.config(agent_body, limit), o)
    }
}

fn docker(args: &[&str]) -> String {
    let o = Command::new("docker").args(args).output().unwrap();
    String::from_utf8_lossy(&o.stdout).trim().to_owned()
}

fn running(name: &str) -> bool {
    !docker(&["ps", "-q", "--filter", &format!("name=^{name}$")]).is_empty()
}

fn exists(name: &str) -> bool {
    !docker(&["ps", "-aq", "--filter", &format!("name=^{name}$")]).is_empty()
}

fn records(lab: &Lab) -> Vec<String> {
    std::fs::read_dir(lab.dir.join("scratch/.meta/.containers"))
        .map(|rd| {
            rd.flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default()
}

fn bridge_gateway() -> String {
    docker(&[
        "network",
        "inspect",
        "bridge",
        "--format",
        "{{(index .IPAM.Config 0).Gateway}}",
    ])
}

const ATTACK: &str = r#"
r() { echo "ATTACK $1: $2"; }
held_if_fails() { if "${@:2}" >/dev/null 2>&1; then r "$1" "NOT HELD"; else r "$1" "held"; fi; }
for p in $SECRETS; do held_if_fails "read $p" cat "$p"; done
held_if_fails "write /etc" bash -c 'echo x > /etc/frx-w'
held_if_fails "write /usr" bash -c 'echo x > /usr/frx-w'
held_if_fails "write rootfs /frx-w" bash -c 'echo x > /frx-w'
held_if_fails "write outside job (parent dir)" bash -c "echo x > $OUTSIDE/frx-w"
held_if_fails "exec from /tmp (noexec)" bash -c 'cp /bin/true /tmp/t && /tmp/t'
held_if_fails "tcp 127.0.0.1:$PORT (host listener)" bash -c "exec 3<>/dev/tcp/127.0.0.1/$PORT"
held_if_fails "tcp docker gateway $GW:$PORT" bash -c "exec 3<>/dev/tcp/$GW/$PORT"
held_if_fails "tcp external 1.1.1.1:53" timeout 5 bash -c "exec 3<>/dev/tcp/1.1.1.1/53"
r "interfaces" "$(ls /sys/class/net | tr '\n' ' ')"
for p in $NEVER; do if [ -e "$p" ]; then r "path $p" "NOT HELD (exists)"; else r "path $p" "held (absent)"; fi; done
r "docker" "$(command -v docker || echo none) sock: $(ls /var/run/docker.sock /run/docker.sock 2>&1 | tr '\n' ' ')"
held_if_fails "mount tmpfs" mount -t tmpfs none /mnt
held_if_fails "ptrace pid 1" bash -c 'cat /proc/1/mem'
held_if_fails "read /host-like paths" bash -c 'ls /host /hostfs /rootfs'
r "pid 1" "$(tr '\0' ' ' < /proc/1/cmdline)"
r "processes visible" "$(ls /proc | grep -c '^[0-9]')"
r "pid 1 environ names" "$(tr '\0' '\n' < /proc/1/environ | cut -d= -f1 | tr '\n' ' ')"
r "own environ names" "$(env | cut -d= -f1 | sort | tr '\n' ' ')"
r "id" "$(id 2>&1)"
r "mounts" "$(awk '{print $2}' /proc/self/mounts | tr '\n' ' ')"
r "hostname" "$(cat /proc/sys/kernel/hostname)"
r "dev" "$(ls /dev | tr '\n' ' ')"
r "sys" "$(ls /sys | tr '\n' ' ')"
r "cap" "$(grep -E 'Cap(Eff|Bnd)|NoNewPrivs|Seccomp:' /proc/self/status | tr '\n\t' '  ')"
# setsid escape: a session leader must stay inside the container (checked after the call).
setsid sleep 300 &
# fork bomb: 300 sleepers in a subshell using only builtins (a full pid table must not stop it
# from counting or cleaning up); the pids limit must keep the number alive at or under 128.
f=$PWD/forkcount
setsid -w bash -c 'trap "kill -9 0" EXIT; for ((i=1;i<=250;i++)); do sleep 30 2>/dev/null & echo $i > '"$f"'.started; done; set -- /proc/[0-9]*; echo $# > '"$f"'.alive' 2>/dev/null
alive=$(cat "$f.alive" 2>/dev/null || cat "$f.started" 2>/dev/null || echo unknown)
if [ "$alive" != unknown ] && [ "$alive" -le 128 ]; then r "fork bomb (250 tries): peak count" "held ($alive <= 128)"; else r "fork bomb" "NOT HELD ($alive)"; fi
# LAST: the kernel may end the whole container rather than one process (still the limit holding).
# memory: `tail -c 1G` of a pipe buffers a gigabyte; the 512m limit must kill that process.
r "memory 1GB" "allocating"
head -c 1000000000 /dev/zero | tail -c 1000000000 >/dev/null 2>&1
rc=${PIPESTATUS[1]}
if [ "$rc" = 0 ]; then r "memory 1GB" "NOT HELD"; else r "memory 1GB" "held (exit $rc)"; fi
echo done
"#;

#[test]
fn the_docker_executor_is_isolated_in_every_way_the_boundary_promises() {
    if !docker_or_skip() {
        return;
    }
    let lab = Lab::new("attack");
    // Dummy secrets: outside the job dir, in a fake HOME, a token file and a database file.
    let home = lab.dir.join("fakehome");
    w(&home.join(".ssh/id_rsa"), "DUMMY-SECRET-KEY-MATERIAL\n");
    w(&home.join("secret.txt"), "DUMMY-SECRET-TEXT\n");
    let token = lab.dir.join("owner.token");
    w(&token, "DUMMY-OWNER-TOKEN\n");
    let db = lab.dir.join("core.db");
    w(&db, "SQLite format 3\0 DUMMY-DATABASE\n");
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    let mut secrets = vec![
        home.join(".ssh/id_rsa").display().to_string(),
        home.join("secret.txt").display().to_string(),
        token.display().to_string(),
        db.display().to_string(),
        "/etc/shadow".to_string(),
    ];
    if let Ok(extra) = std::env::var("FRX_ATTACK_EXTRA_SECRET") {
        secrets.push(extra); // a mode-600 file in the real home, made by the operator
    }
    let never = [token.display().to_string(), db.display().to_string()];
    let script = format!(
        "SECRETS='{}'\nNEVER='{}'\nOUTSIDE='{}'\nPORT={port}\nGW='{}'\n{ATTACK}",
        secrets.join(" "),
        never.join(" "),
        lab.dir.join("scratch").display(),
        bridge_gateway(),
    );
    let cap = lab.cap(&script, 60);
    assert_eq!(cap.confinement(), Confinement::Docker);
    cap.call("git.prepare", "job").unwrap();
    let started = Instant::now();
    let ch = cap.call("git.change", "job\nattack").unwrap();
    assert_eq!(
        ch.profile.as_deref(),
        Some("docker"),
        "the profile is evidence"
    );
    let took = started.elapsed();
    let said = ch.evidence;
    let report = format!("took {took:?}; result: {}\n{said}", ch.result);
    if let Ok(out) = std::env::var("FRX_ATTACK_OUT") {
        std::fs::write(out, &report).unwrap();
    }
    assert!(
        !said.contains("NOT HELD"),
        "an isolation promise broke:\n{report}"
    );
    for secret in ["DUMMY-SECRET", "DUMMY-OWNER-TOKEN", "DUMMY-DATABASE"] {
        assert!(
            !said.contains(secret),
            "{secret} reached evidence:\n{report}"
        );
    }
    // The memory limit: the shell reports it held, or the kernel ended the whole container
    // (exit 137) right after "allocating": either way 1 GB was not granted.
    assert!(
        said.contains("memory 1GB: held")
            || (ch.result.contains("agent_exit=137") && said.contains("memory 1GB: allocating")),
        "memory limit: {report}"
    );
    for needed in [
        "held (absent)",
        "interfaces: lo",
        "fork bomb (250 tries): peak count: held",
    ] {
        assert!(said.contains(needed), "missing `{needed}`:\n{report}");
    }
    // No docker, no socket, a pid space of its own.
    assert!(said.contains("docker: none"), "{report}");
    // Nothing of it survives the call.
    for r in records(&lab) {
        assert!(!exists(&r), "container {r} outlived its call");
    }
}

#[test]
fn a_call_that_times_out_has_its_container_killed() {
    if !docker_or_skip() {
        return;
    }
    let lab = Lab::new("timeout");
    let cap = lab.cap("sleep 300 &\nsleep 300\n", 3);
    cap.call("git.prepare", "job").unwrap();
    let t = Instant::now();
    let failed = cap.call("git.change", "job\nhang").unwrap_err();
    let took = t.elapsed();
    eprintln!("timeout call took {took:?}: {failed:?}");
    assert!(took < Duration::from_secs(20), "{took:?}");
    assert!(failed.reason.contains("stopped after 3s"), "{failed:?}");
    // Only this test's own record matters; the host may run other `frx-` containers.
    let left = records(&lab);
    assert!(left.is_empty(), "records left: {left:?}");
}

/// The child half of the SIGKILL test: a Core that starts a long call and is killed mid-way.
#[test]
fn docker_child_helper() {
    let Ok(dir) = std::env::var("FRX_CHILD_LAB") else {
        return; // not the child: nothing to do
    };
    let limit: u64 = std::env::var("FRX_CHILD_LIMIT").unwrap().parse().unwrap();
    let lab = Lab {
        dir: PathBuf::from(dir),
    };
    let cap = lab.cap("touch ../started\nsleep 1000\n", limit);
    cap.call("git.prepare", "job").unwrap();
    let _ = cap.call("git.change", "job\nlong");
}

fn spawn_child(lab: &Lab, limit: u64) -> (std::process::Child, String) {
    // The child half re-runs this very test binary.
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "docker_child_helper",
            "--exact",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("FRX_CHILD_LAB", &lab.dir)
        .env("FRX_CHILD_LIMIT", limit.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let t = Instant::now();
    loop {
        // `git.prepare` runs short containers of its own: wait for the agent's marker, which
        // only the long call writes, then take the container that is running now.
        if lab.dir.join("scratch/job/started").exists() {
            if let Some(name) = records(lab).iter().find(|n| running(n)) {
                return (child, name.clone());
            }
        }
        assert!(
            t.elapsed() < Duration::from_secs(60),
            "the child never started a container"
        );
        if let Some(s) = child.try_wait().unwrap() {
            panic!("the child ended early: {s}");
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[test]
fn a_killed_core_leaves_a_container_that_the_next_start_removes() {
    if !docker_or_skip() {
        return;
    }
    let lab = Lab::new("sigkill");
    let (mut child, name) = spawn_child(&lab, 120);
    let killed_at = Instant::now();
    child.kill().unwrap(); // SIGKILL: no destructor, no clean-up of any kind
    child.wait().unwrap();
    std::thread::sleep(Duration::from_secs(3));
    assert!(
        running(&name),
        "the container should outlive its killed parent until recovery"
    );
    // The next start: constructing the capability removes what the last one recorded.
    let cap = lab.cap("true\n", 5);
    let lived = killed_at.elapsed();
    assert!(!exists(&name), "container {name} survived the next start");
    assert!(
        records(&lab).is_empty(),
        "the record is removed with the container"
    );
    eprintln!(
        "orphan container {name} lived {lived:?} after the SIGKILL, until the next start removed it"
    );
    drop(cap);
}

#[test]
fn a_killed_core_with_no_next_start_still_ends_the_container_at_the_limit() {
    if !docker_or_skip() {
        return;
    }
    let lab = Lab::new("sigkill-bounded");
    let limit = 6;
    let (mut child, name) = spawn_child(&lab, limit);
    let killed_at = Instant::now();
    child.kill().unwrap();
    child.wait().unwrap();
    let t = Instant::now();
    while exists(&name) {
        assert!(
            t.elapsed() < Duration::from_secs(30),
            "container {name} still there after 30 s"
        );
        std::thread::sleep(Duration::from_millis(200));
    }
    let lived = killed_at.elapsed();
    eprintln!(
        "orphan container {name} (limit {limit} s) ended by its own timeout {lived:?} after the SIGKILL"
    );
    assert!(lived < Duration::from_secs(limit + 10), "{lived:?}");
    // The stale record is cleared by the next start.
    let _ = lab.cap("true\n", 5);
    assert!(records(&lab).is_empty());
}
