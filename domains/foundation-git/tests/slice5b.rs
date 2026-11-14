//! Slice 5b: attack regression tests for capability B (REVIEW2 findings 1, 2, 9 and parts of 10).
//! Every agent is a scripted shell stand-in; secrets are throwaway dummies in a temp directory.
//! No model, no paid API, no network beyond a loopback listener the sandbox must keep unreachable.

mod common;
use common::owner;
use std::io::Read;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use endora_foundation::application::{CallOutput, Capability};
use endora_foundation_git::{Config, Confinement, GitChange};

const COMMIT: &str = "git -c user.name=a -c user.email=a@b commit -q";

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
            .join(format!("s5b-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("template")).unwrap();
        let dir = dir.canonicalize().unwrap();
        w(
            &dir.join("template/lib.sh"),
            "add() { echo $(( $1 - $2 )); }\n",
        );
        w(
            &dir.join("template/test.sh"),
            ". ./lib.sh\n[ \"$(add 2 3)\" = 5 ] || { echo \"FAIL: add 2 3 gave $(add 2 3)\"; exit 1; }\necho PASS\n",
        );
        Self { dir }
    }
    fn config(&self, agent_body: &str) -> Config {
        let agent = self.dir.join("agent.sh");
        w(&agent, agent_body);
        Config {
            scratch_root: self.dir.join("scratch"),
            template: self.dir.join("template"),
            agent_command: vec!["sh".into(), agent.to_str().unwrap().into()],
            test_command: vec!["sh".into(), "test.sh".into()],
            protected: vec!["test.sh".into()],
            agent_limit: Duration::from_secs(10),
            test_limit: Duration::from_secs(10),
            confinement: Confinement::Auto,
        }
    }
    fn cap(&self, agent_body: &str) -> GitChange {
        GitChange::new(self.config(agent_body))
    }
    fn repo(&self) -> PathBuf {
        self.dir.join("scratch/job/repo")
    }
}

fn prepared_changed(lab: &Lab, agent: &str) -> (GitChange, CallOutput) {
    let cap = lab.cap(agent);
    cap.call("git.prepare", "job").unwrap();
    let ch = cap.call("git.change", "job\nfix add").unwrap();
    (cap, ch)
}

fn verified(cap: &GitChange) -> CallOutput {
    cap.call("git.verify", "job").unwrap()
}

fn not_pass(v: &CallOutput) {
    assert!(
        !v.result.starts_with("tests=pass"),
        "forged pass: {} / {}",
        v.result,
        v.evidence
    );
}

// ---- Item 1: the forged "tests pass" ---------------------------------------------------------

#[test]
fn attack_rewriting_the_base_file_cannot_hide_a_tampered_test() {
    if !common::confined_or_skip() {
        return;
    }
    let lab = Lab::new("base");
    let agent = format!(
        "printf 'echo PASS\\nexit 0\\n' > test.sh\n{COMMIT} -am x\ngit rev-parse HEAD > ../BASE\n"
    );
    let (cap, _) = prepared_changed(&lab, &agent);
    let v = verified(&cap);
    not_pass(&v);
    assert!(
        v.result.starts_with("tests=tampered path=test.sh"),
        "{}",
        v.result
    );
}

#[test]
fn attack_git_replace_cannot_hide_a_tampered_test() {
    if !common::confined_or_skip() {
        return;
    }
    let lab = Lab::new("replace");
    let agent = format!(
        "B=$(cat ../BASE 2>/dev/null || git rev-parse HEAD)\nprintf 'echo PASS\\nexit 0\\n' > test.sh\n{COMMIT} -am x\ngit replace $B HEAD\n"
    );
    let (cap, _) = prepared_changed(&lab, &agent);
    let v = verified(&cap);
    not_pass(&v);
    assert!(
        v.result.starts_with("tests=tampered path=test.sh"),
        "{}",
        v.result
    );
}

#[test]
fn attack_exit_zero_in_a_sourced_library_is_not_a_pass() {
    if !common::confined_or_skip() {
        return;
    }
    let lab = Lab::new("lib");
    let (cap, _) = prepared_changed(&lab, "echo 'exit 0' > lib.sh\n");
    let v = verified(&cap);
    not_pass(&v);
    assert!(
        v.result.contains("exit=0") && v.result.contains("incomplete"),
        "{}",
        v.result
    );
}

#[test]
fn attack_exit_zero_with_a_printed_pass_is_not_a_pass() {
    if !common::confined_or_skip() {
        return;
    }
    let lab = Lab::new("lib2");
    let (cap, _) = prepared_changed(&lab, "printf 'echo PASS\\nexit 0\\n' > lib.sh\n");
    not_pass(&verified(&cap));
}

#[test]
fn a_patch_touching_a_protected_path_is_recorded_as_a_violation_with_its_diff() {
    if !common::confined_or_skip() {
        return;
    }
    for (name, body, path) in [
        (
            "added",
            "mkdir -p tests; echo 'exit 0' > tests/extra.sh\n",
            "tests/extra.sh",
        ),
        (
            "scripts",
            "mkdir -p scripts; echo x > scripts/run.sh\n",
            "scripts/run.sh",
        ),
        ("deleted", "rm test.sh\n", "test.sh"),
        ("mode", "chmod +x test.sh\n", "test.sh"),
    ] {
        let lab = Lab::new(&format!("prot-{name}"));
        let (cap, ch) = prepared_changed(&lab, body);
        assert!(
            ch.evidence.contains("PROTECTED") && ch.evidence.contains(path),
            "{name}: the change evidence must name the violation: {}",
            ch.evidence
        );
        let v = verified(&cap);
        assert!(
            v.result.starts_with(&format!("tests=tampered path={path}")),
            "{name}: {}",
            v.result
        );
        assert!(
            v.evidence.contains(path) && v.evidence.contains("diff"),
            "{name}: {}",
            v.evidence
        );
    }
}

#[test]
fn an_honest_fix_in_an_unprotected_file_still_passes_on_the_fresh_copy() {
    if !common::confined_or_skip() {
        return;
    }
    let lab = Lab::new("honest");
    let (cap, ch) = prepared_changed(
        &lab,
        "printf 'add() { echo $(( $1 + $2 )); }\\n' > lib.sh\n",
    );
    assert!(
        ch.evidence.contains("+add() { echo $(( $1 + $2 )); }"),
        "{}",
        ch.evidence
    );
    let v = verified(&cap);
    assert!(
        v.result.starts_with("tests=pass"),
        "{} {}",
        v.result,
        v.evidence
    );
    // The live job folder is not what was judged: ruining it afterwards changes nothing.
    w(&lab.repo().join("lib.sh"), "exit 7\n");
    assert!(verified(&cap).result.starts_with("tests=pass"));
}

#[test]
fn a_committed_base_the_agent_rewrites_in_its_own_git_does_not_matter() {
    if !common::confined_or_skip() {
        return;
    }
    let lab = Lab::new("ownrepo");
    // The agent deletes its .git and re-creates it with the tampered test as the "base".
    let agent = format!(
        "printf 'echo PASS\\n' > test.sh\nrm -rf .git\ngit init -q -b main\ngit add -A\n{COMMIT} -m fake\n"
    );
    let (cap, _) = prepared_changed(&lab, &agent);
    let v = verified(&cap);
    not_pass(&v);
    assert!(v.result.starts_with("tests=tampered"), "{}", v.result);
}

// ---- Item 2: sandbox reads, caps and redaction -----------------------------------------------

#[test]
fn attack_the_agent_cannot_read_a_secret_outside_its_job_and_evidence_never_has_it() {
    if !common::confined_or_skip() {
        return;
    }
    let lab = Lab::new("read");
    let outside = lab.dir.join("secret.txt");
    w(&outside, "TOPSECRET-OUTSIDE-TOKEN\n");
    let fake_home = lab.dir.join("fakehome");
    w(&fake_home.join(".ssh/id_rsa"), "TOPSECRET-SSH-KEY\n");
    w(
        &fake_home.join(".config/tool/token"),
        "TOPSECRET-CONFIG-TOKEN\n",
    );
    w(
        &fake_home.join("Library/Keychains/login.keychain"),
        "TOPSECRET-KEYCHAIN\n",
    );
    let body = format!(
        "for f in {o} {h}/.ssh/id_rsa {h}/.config/tool/token {h}/Library/Keychains/login.keychain; do\n  cat \"$f\" 2>&1; echo \"read-exit=$?\"\ndone\nls {h} 2>&1; ls /Users 2>&1; ls /private/tmp 2>&1\n",
        o = outside.display(),
        h = fake_home.display()
    );
    let (_, ch) = prepared_changed(&lab, &body);
    assert!(
        !ch.evidence.contains("TOPSECRET"),
        "secret reached evidence: {}",
        ch.evidence
    );
    assert!(!ch.result.contains("TOPSECRET"));
    assert!(
        !ch.evidence.contains("read-exit=0"),
        "a read succeeded: {}",
        ch.evidence
    );
    assert_eq!(
        ch.evidence.matches("read-exit=").count(),
        4,
        "{}",
        ch.evidence
    );
}

#[test]
fn the_job_folder_stays_readable_and_the_agent_command_files_still_run() {
    if !common::confined_or_skip() {
        return;
    }
    let lab = Lab::new("readok");
    let (_, ch) = prepared_changed(&lab, "cat lib.sh; echo job-read-ok\n");
    assert!(
        ch.evidence.contains("job-read-ok") && ch.evidence.contains("add()"),
        "{}",
        ch.evidence
    );
}

#[test]
fn evidence_output_is_capped_with_the_full_length_and_secrets_are_redacted() {
    if !common::confined_or_skip() {
        return;
    }
    let lab = Lab::new("cap");
    let body = "echo 'API_KEY=hunter2-value'\necho 'export GITHUB_TOKEN=ghp_abcdefghijklmnop'\n\
                echo 'key: sk-ant-api03-ABCDEFGHIJKLMNOPQRSTUV'\n\
                printf -- '-----BEGIN OPENSSH PRIVATE KEY-----\\nAAAAB3NzaC1yc2EAAAADAQABAAABAQ\\n-----END OPENSSH PRIVATE KEY-----\\n'\n\
                echo 'harmless=value'\n\
                i=0; while [ $i -lt 4000 ]; do echo 'line-of-padding-to-exceed-the-sixteen-kilobyte-cap'; i=$((i+1)); done\n";
    let (_, ch) = prepared_changed(&lab, body);
    for secret in [
        "hunter2-value",
        "ghp_abcdefghijklmnop",
        "sk-ant-api03",
        "AAAAB3NzaC1yc2E",
        "BEGIN OPENSSH",
    ] {
        assert!(
            !ch.evidence.contains(secret),
            "{secret} leaked: {}",
            &ch.evidence[..400.min(ch.evidence.len())]
        );
    }
    assert!(ch.evidence.contains("harmless=value"));
    assert!(ch.evidence.contains("[redacted]"));
    assert!(ch.evidence.contains("[truncated"), "no truncation marker");
    assert!(
        ch.evidence.len() < 40_000,
        "evidence is {} bytes",
        ch.evidence.len()
    );
    // The marker records the full length (4000 lines of 53 bytes plus the first lines).
    assert!(
        ch.evidence.contains("of 2"),
        "full length missing from the marker"
    );
}

#[test]
fn attack_a_symlink_to_a_secret_is_recorded_as_a_link_never_followed() {
    if !common::confined_or_skip() {
        return;
    }
    let lab = Lab::new("link");
    let secret = lab.dir.join("link-secret.txt");
    w(&secret, "TOPSECRET-VIA-SYMLINK\n");
    let body = format!("ln -s {} lib.sh\n", secret.display());
    let (cap, ch) = prepared_changed(&lab, &body);
    assert!(!ch.evidence.contains("TOPSECRET"), "{}", ch.evidence);
    let v = verified(&cap);
    not_pass(&v);
    assert!(!v.evidence.contains("TOPSECRET"), "{}", v.evidence);
}

// ---- Item 3: process cleanup and the external diff -------------------------------------------

#[test]
fn attack_a_background_child_cannot_write_after_the_call_returned() {
    if !common::confined_or_skip() {
        return;
    }
    let lab = Lab::new("orphan");
    let (_, _) = prepared_changed(
        &lab,
        "( sleep 2; echo late > late.txt; echo late > ../late-outside.txt ) >/dev/null 2>&1 &\nexit 0\n",
    );
    std::thread::sleep(Duration::from_secs(4));
    assert!(
        !lab.repo().join("late.txt").exists(),
        "the orphan wrote after the call"
    );
}

#[test]
fn a_background_child_of_a_failing_agent_is_also_stopped() {
    if !common::confined_or_skip() {
        return;
    }
    let lab = Lab::new("orphan2");
    let (_, _) = prepared_changed(
        &lab,
        "( sleep 2; echo late > late.txt ) >/dev/null 2>&1 &\nexit 3\n",
    );
    std::thread::sleep(Duration::from_secs(4));
    assert!(!lab.repo().join("late.txt").exists());
}

#[test]
fn attack_diff_external_cannot_forge_the_recorded_diff() {
    if !common::confined_or_skip() {
        return;
    }
    let lab = Lab::new("ext");
    let body = "echo 'add() { echo $(( $1 + $2 )); }' > lib.sh\n\
        printf '#!/bin/sh\\necho NO-CHANGES\\n' > ../tmp/fake\nchmod +x ../tmp/fake\n\
        git config diff.external \"$PWD/../tmp/fake\"\n\
        git config diff.evil.textconv \"$PWD/../tmp/fake\"\n\
        git config core.pager \"$PWD/../tmp/fake\"\n\
        printf '* diff=evil\\n* -diff\\n' > .gitattributes\n";
    let (cap, ch) = prepared_changed(&lab, body);
    assert!(
        ch.evidence.contains("+add() { echo $(( $1 + $2 )); }"),
        "forged: {}",
        ch.evidence
    );
    assert!(!ch.evidence.contains("NO-CHANGES"), "{}", ch.evidence);
    assert!(!ch.evidence.contains("Binary files"), "{}", ch.evidence);
    let _ = cap;
}

#[test]
fn attack_git_config_code_hooks_never_run() {
    if !common::confined_or_skip() {
        return;
    }
    let lab = Lab::new("cfg");
    let body = "printf '#!/bin/sh\\necho ran >> %s\\n' \"$PWD/../tmp/hook-ran\" > ../tmp/hook\nchmod +x ../tmp/hook\n\
        git config core.fsmonitor \"$PWD/../tmp/hook\"\n\
        git config core.sshCommand \"$PWD/../tmp/hook\"\n\
        git config protocol.ext.allow always\n\
        mkdir -p .git/hooks; cp ../tmp/hook .git/hooks/pre-commit\n\
        echo 'add() { echo $(( $1 + $2 )); }' > lib.sh\n";
    let (cap, ch) = prepared_changed(&lab, body);
    let v = verified(&cap);
    assert!(
        v.result.starts_with("tests=pass"),
        "{} {}",
        v.result,
        ch.evidence
    );
    assert!(
        !lab.dir.join("scratch/job/tmp/hook-ran").exists(),
        "agent-written config ran code"
    );
}

// ---- Item 5: verify keeps its band only if it cannot write outside or reach the network ------

#[test]
fn verify_runs_agent_code_without_writing_outside_its_copy_or_reaching_the_network() {
    if !common::confined_or_skip() {
        return;
    }
    let lab = Lab::new("band");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let port = listener.local_addr().unwrap().port();
    let outside = lab.dir.join("outside-verify.txt");
    let secret = lab.dir.join("band-secret.txt");
    w(&secret, "TOPSECRET-BAND\n");
    // The fix is real, so the tests do run `lib.sh`: its top-level lines run during verify.
    let body = format!(
        "cat > lib.sh <<'EOF'\n/usr/bin/nc -z -w1 127.0.0.1 {port} >/dev/null 2>&1\necho x > {o} 2>/dev/null\necho x > ../../escape.txt 2>/dev/null\ncat {s} > leaked.txt 2>/dev/null\nadd() {{ echo $(( $1 + $2 )); }}\nEOF\n",
        o = outside.display(),
        s = secret.display()
    );
    let (cap, _) = prepared_changed(&lab, &body);
    let v = verified(&cap);
    assert!(
        v.result.starts_with("tests=pass"),
        "{} {}",
        v.result,
        v.evidence
    );
    assert!(!outside.exists(), "verify wrote outside its copy");
    assert!(!lab.dir.join("scratch/escape.txt").exists());
    assert!(!lab.dir.join("scratch/.meta/escape.txt").exists());
    assert!(listener.accept().is_err(), "verify reached the network");
    assert!(!v.evidence.contains("TOPSECRET"));
    // The verify copy is temporary: nothing from it stays behind.
    let left: Vec<_> = std::fs::read_dir(lab.dir.join("scratch/.meta/job"))
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("fresh"))
        .collect();
    assert!(left.is_empty(), "leftover verify copies: {left:?}");
}

// ---- Item 4: a clean environment -------------------------------------------------------------

fn run_bin(lab: &Lab, extra_env: &[(&str, &str)], extra_args: &[&str]) {
    let agent = lab.dir.join("envdump.sh");
    w(&agent, "env | sort > env.dump\n");
    let mut c = Command::new(env!("CARGO_BIN_EXE_foundation-git-run"));
    c.args(["--db", lab.dir.join("e.db").to_str().unwrap()])
        .args(["--scratch", lab.dir.join("scratch").to_str().unwrap()])
        .args(["--template", lab.dir.join("template").to_str().unwrap()])
        .args(["--agent-script", agent.to_str().unwrap()])
        .args(["--item", "1", "--job", "job"])
        .args(extra_args)
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    for (k, v) in extra_env {
        c.env(k, v);
    }
    let mut child = c.spawn().unwrap();
    let mut err = String::new();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut err)
        .unwrap();
    child.wait().unwrap();
}

fn lab_with_goal(name: &str) -> Lab {
    // The runner needs an item to call against: build the file with the engine, then drop it.
    use endora_foundation::application::{Core, NewGoal};
    use endora_foundation::domain::{Criterion, PolicyRow};
    use endora_foundation::infrastructure::SystemClock;
    use endora_kernel::{Reversibility, Tier};
    use std::sync::Arc;
    let lab = Lab::new(name);
    let core = Core::open(
        lab.dir.join("e.db").to_str().unwrap(),
        Arc::new(SystemClock),
    )
    .unwrap();
    let goal = core
        .add_goal(
            &owner(&core),
            NewGoal {
                title: "t".into(),
                why: "w".into(),
                criteria: vec![Criterion {
                    text: "c".into(),
                    check: None,
                }],
                scope: vec!["git.prepare".into(), "git.change".into()],
                tier: Tier::Internal,
                spend_cap_micros: 0,
            },
        )
        .unwrap();
    for call in ["git.prepare", "git.change"] {
        core.grant(
            &owner(&core),
            PolicyRow {
                actor: "runner".into(),
                call: call.into(),
                band: Reversibility::Reversible,
                tier: Tier::Internal,
                max_uses_per_item: Some(1),
            },
        )
        .unwrap();
    }
    core.add_item(goal, "i").unwrap();
    lab
}

#[test]
fn attack_parent_secrets_are_invisible_to_the_child() {
    if !common::confined_or_skip() {
        return;
    }
    let lab = lab_with_goal("env");
    run_bin(
        &lab,
        &[
            ("ENDORA_FOUNDATION_DB", "/dummy/path/e.db"),
            ("TOKEN", "dummy-token-value"),
            ("MY_API_KEY", "dummy-key"),
        ],
        &[],
    );
    let dump = std::fs::read_to_string(lab.repo().join("env.dump")).expect("the agent ran");
    for bad in [
        "ENDORA_FOUNDATION_DB",
        "TOKEN",
        "MY_API_KEY",
        "dummy-token-value",
    ] {
        assert!(!dump.contains(bad), "{bad} reached the child:\n{dump}");
    }
    let home = dump.lines().find_map(|l| l.strip_prefix("HOME=")).unwrap();
    assert!(
        home.starts_with(lab.dir.join("scratch/job").to_str().unwrap()),
        "HOME must be a temp dir inside the job folder: {home}"
    );
}

#[test]
fn only_allow_listed_variables_reach_the_child() {
    if !common::confined_or_skip() {
        return;
    }
    let lab = lab_with_goal("envallow");
    run_bin(
        &lab,
        &[
            ("ENDORA_ALLOWED_VAR", "visible"),
            ("TOKEN", "dummy-token-value"),
        ],
        &["--env-allow", "ENDORA_ALLOWED_VAR"],
    );
    let dump = std::fs::read_to_string(lab.repo().join("env.dump")).expect("the agent ran");
    assert!(dump.contains("ENDORA_ALLOWED_VAR=visible"), "{dump}");
    assert!(!dump.contains("TOKEN"), "{dump}");
}

fn gone(pid: &str) -> bool {
    for _ in 0..40 {
        let alive = Command::new("kill")
            .args(["-0", pid])
            .stderr(Stdio::null())
            .status()
            .unwrap()
            .success();
        if !alive {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

/// REVIEW3 finding 9: the profile started from `(allow default)`, so a confined agent could signal
/// the engine. It may still signal its own children, and the call's process-group kill at the end
/// still removes what the agent left running.
#[cfg(target_os = "macos")]
#[test]
fn a_confined_agent_cannot_signal_the_engine_but_keeps_its_own_children() {
    let lab = Lab::new("signal");
    let agent = "\
kill -0 $PPID 2>/dev/null; echo $? > ppid.txt
kill -0 1 2>/dev/null; echo $? > init.txt
sleep 60 & c=$!
kill -0 $c; echo $? > child_alive.txt
sleep 61 & echo $! > leftover.pid
kill $c; echo $? > child_kill.txt
";
    let cap = lab.cap(agent);
    cap.call("git.prepare", "job").unwrap();
    cap.call("git.change", "job\nfix add").unwrap();
    let repo = lab.repo();
    let n = |f: &str| {
        std::fs::read_to_string(repo.join(f))
            .unwrap_or_else(|_| panic!("{f} missing"))
            .trim()
            .to_owned()
    };
    assert_ne!(n("ppid.txt"), "0", "the agent signalled the engine");
    assert_ne!(n("init.txt"), "0", "the agent signalled pid 1");
    assert_eq!(n("child_alive.txt"), "0", "own child must stay reachable");
    assert_eq!(n("child_kill.txt"), "0", "own child must be killable");
    assert!(
        gone(&n("leftover.pid")),
        "the end-of-call group kill must remove what the agent left running"
    );
}
