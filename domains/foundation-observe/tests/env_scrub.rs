//! Slice 7 F5 (REVIEW3 finding 8): the observe capability starts script probes and the owner's
//! command probes. Neither may hand the Core process's environment on: the owner token path, the
//! database path and anything token-like must be invisible to the child. `unsafe` is forbidden in
//! this workspace, so the test runs itself again as a child with the variables set.

use std::process::Command;

use endora_foundation::application::Capability;
use endora_foundation_observe::{Config, ObserveVerify, owner};

const MARK: &str = "SCRUB_CHILD";
const LEAK: [(&str, &str); 3] = [
    ("ENDORA_OWNER_TOKEN_FILE", "/nowhere/owner.token"),
    ("ENDORA_FOUNDATION_DB", "/nowhere/core.db"),
    ("DUMMY_SECRET_TOKEN", "hunter2-not-a-real-token"),
];

#[test]
fn script_probes_and_owner_commands_do_not_see_core_secrets() {
    if std::env::var_os(MARK).is_some() {
        return;
    }
    let mut cmd = Command::new(std::env::current_exe().unwrap());
    cmd.args(["--exact", "child_checks_the_environment", "--nocapture"])
        .env(MARK, "1")
        .env("ALLOWED_DUMMY", "kept");
    for (k, v) in LEAK {
        cmd.env(k, v);
    }
    let out = cmd.output().unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.status.success(), "{text}");
    assert!(
        text.contains("1 passed"),
        "the child test did not run: {text}"
    );
}

#[test]
fn child_checks_the_environment() {
    if std::env::var_os(MARK).is_none() {
        return;
    }
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("scrub-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("env.sh"), "env\n").unwrap();
    let cap = |allow: &[&str]| {
        ObserveVerify::new(Config {
            sandbox: dir.clone(),
            probes: vec![("env".into(), "env.sh".into())],
            restart_script: "env.sh".into(),
        })
        .with_env_allow(allow.iter().map(|s| (*s).to_owned()).collect())
    };
    let seen_script = |allow: &[&str]| cap(allow).call("probe.command", "env").unwrap().result;
    let seen_command = |allow: &[&str]| {
        let allow: Vec<String> = allow.iter().map(|s| (*s).to_owned()).collect();
        owner::command_with_env("env", &allow).unwrap().evidence
    };
    for seen in [seen_script(&[]), seen_command(&[])] {
        for (k, v) in LEAK {
            assert!(
                !seen.contains(k) && !seen.contains(v),
                "{k} leaked:\n{seen}"
            );
        }
        assert!(seen.contains("PATH="), "a minimal PATH is set:\n{seen}");
        assert!(seen.contains("HOME="), "a minimal HOME is set:\n{seen}");
        assert!(
            !seen.contains("ALLOWED_DUMMY"),
            "not allowed, not seen:\n{seen}"
        );
    }
    // The allow-list is the only way through, and it cannot be used to name the secrets by accident.
    for seen in [
        seen_script(&["ALLOWED_DUMMY"]),
        seen_command(&["ALLOWED_DUMMY"]),
    ] {
        assert!(seen.contains("ALLOWED_DUMMY=kept"), "{seen}");
        assert!(!seen.contains("ENDORA_OWNER_TOKEN_FILE"), "{seen}");
    }
    // The restart script gets the same environment.
    let restart = cap(&[]).call("service.restart", "restart").unwrap().result;
    assert!(
        !restart.contains("ENDORA_") && !restart.contains("DUMMY_SECRET"),
        "{restart}"
    );
}
