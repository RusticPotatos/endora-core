//! Slice 6: the Claude Code executor's adapter, tested with a FAKE `claude` (a shell script that
//! prints the CLI's JSON) under the real-agent sandbox profile. The real CLI is run by the
//! acceptance runs in the slice 6 report, not here: these tests spend nothing.
#![cfg(target_os = "macos")]

mod common;
use common::{actor_named, owner};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use endora_foundation::application::{Core, NewGoal, Outcome};
use endora_foundation::domain::{Check, Criterion, Deny, ItemId, ItemState, Kind, PolicyRow};
use endora_foundation::infrastructure::SystemClock;
use endora_foundation_git::{
    AgentExecutor, ClaudeCode, ClaudeLimits, Config, Confinement, GitChange, Options, Profile,
    Reads, mac_profile_real_agent,
};
use endora_kernel::{Reversibility, Tier};

const TOKEN: &str = concat!("fake-credential-", "0a1b2c3d4e5f6a7b8c9d0e1f");

struct Lab {
    dir: PathBuf,
    fake: PathBuf,
}

fn w(p: &Path, s: &str) {
    std::fs::write(p, s).unwrap();
}

impl Lab {
    fn new(name: &str, mode: &str) -> Self {
        let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("s6-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("template")).unwrap();
        std::fs::create_dir_all(dir.join("outside")).unwrap();
        let dir = dir.canonicalize().unwrap();
        w(
            &dir.join("template/lib.sh"),
            "add() { echo $(( $1 - $2 )); }\n",
        );
        w(
            &dir.join("template/test.sh"),
            ". ./lib.sh\n[ \"$(add 2 3)\" = 5 ] || { echo \"FAIL\"; exit 1; }\necho PASS\n",
        );
        w(&dir.join("template/mode"), mode);
        w(
            &dir.join("template/probe.py"),
            "import ctypes, os\nl = ctypes.CDLL(None)\nm = (ctypes.c_int * 3)(1, 49, os.getppid())\nn = ctypes.c_size_t(0)\nl.sysctl(m, 3, None, ctypes.byref(n), None, 0)\nb = ctypes.create_string_buffer(n.value)\nl.sysctl(m, 3, b, ctypes.byref(n), None, 0)\nprint(b.raw.decode('latin1'))\n",
        );
        w(&dir.join("outside/canary"), "DUMMY-SECRET-CANARY\n");
        w(&dir.join("cred"), TOKEN);
        let fake = dir.join("fake-claude");
        w(
            &fake,
            r#"#!/bin/sh
printf '%s\n' "$@" > "$TMPDIR/argv.txt"
env > "$TMPDIR/env.txt"
res() { echo "{\"type\":\"result\",\"subtype\":\"$1\",\"is_error\":$2,\"num_turns\":$3,\"total_cost_usd\":$4,\"result\":\"$5\"}"; }
case "$(cat mode)" in
 ok) printf 'add() { echo $(( $1 + $2 )); }\n' > lib.sh; res success false 3 0.12 fixed;;
 over) printf 'add() { echo $(( $1 + $2 )); }\n' > lib.sh; res success false 3 0.50 fixed;;
 turns) res error_max_turns false 1 0.02 stopped; exit 1;;
 hang) sleep 30;;
 garbage) echo nope;;
 psenv) ps eww -p $$ > psout.txt 2>&1
        /usr/bin/python3 probe.py > sysctl.txt 2>&1
        res success false 1 0.01 ps;;
 signal) kill -0 $PPID 2> /dev/null; echo $? > ppid.txt
        sleep 60 & c=$!; kill $c; echo $? > child_kill.txt
        res success false 1 0.01 signalled;;
 snoop) cat "$(cat canary_path)" > leaked.txt 2> snoop.err; nc -z 127.0.0.1 "$(cat port)" > loop.txt 2>&1 && echo OPEN > loop.txt || echo CLOSED > loop.txt
        res success false 1 0.01 snooped;;
esac
"#,
        );
        std::process::Command::new("chmod")
            .arg("+x")
            .arg(&fake)
            .status()
            .unwrap();
        Self { dir, fake }
    }

    fn engine(&self, limits: ClaudeLimits, cap: u64, limit_secs: u64) -> (Core, ItemId) {
        let mut core = Core::open(":memory:", Arc::new(SystemClock)).unwrap();
        core.register(Arc::new(GitChange::with_options(
            Config {
                scratch_root: self.dir.join("scratch"),
                template: self.dir.join("template"),
                agent_command: vec!["unused".into()],
                test_command: vec!["sh".into(), "test.sh".into()],
                protected: vec!["test.sh".into()],
                agent_limit: Duration::from_secs(limit_secs),
                test_limit: Duration::from_secs(10),
                confinement: Confinement::Auto,
            },
            Options {
                executor: AgentExecutor::ClaudeCode(ClaudeCode {
                    binary: self.fake.clone(),
                    model: "claude-haiku-5-5".into(),
                    extra_args: vec![],
                }),
                claude_limits: limits,
                credential_file: Some(self.dir.join("cred")),
                ..Options::default()
            },
        )))
        .unwrap();
        // The overall cap starts at zero and no command line sets it (a finding of this slice).
        core.set_overall_cap(&owner(&core), 100_000_000).unwrap();
        let goal = core
            .add_goal(
                &owner(&core),
                NewGoal {
                    title: "fix".into(),
                    why: "add is wrong".into(),
                    criteria: vec![Criterion {
                        text: "tests pass".into(),
                        check: Some(Check {
                            call: "git.verify".into(),
                            input: "job1".into(),
                            field: "tests".into(),
                            expect: "pass".into(),
                        }),
                    }],
                    scope: ["git.prepare", "git.change", "git.verify"]
                        .map(String::from)
                        .to_vec(),
                    tier: Tier::Internal,
                    spend_cap_micros: cap,
                },
            )
            .unwrap();
        for (a, c, b, m) in [
            ("runner", "git.prepare", Reversibility::Reversible, Some(1)),
            ("runner", "git.change", Reversibility::Reversible, Some(1)),
            ("runner", "git.verify", Reversibility::Observe, None),
            ("core", "git.verify", Reversibility::Observe, None),
        ] {
            core.grant(
                &owner(&core),
                PolicyRow {
                    actor: a.into(),
                    call: c.into(),
                    band: b,
                    tier: Tier::Internal,
                    max_uses_per_item: m,
                },
            )
            .unwrap();
        }
        let item = core.add_item(goal, "fix add").unwrap();
        core.start(&actor_named("runner"), item).unwrap();
        core.call(&actor_named("runner"), item, "git.prepare", "job1")
            .unwrap();
        (core, item)
    }
}

fn change(core: &Core, item: ItemId) -> Outcome {
    core.call(&actor_named("runner"), item, "git.change", "job1\nfix add")
        .unwrap()
}

fn tmp_file(lab: &Lab, name: &str) -> String {
    std::fs::read_to_string(lab.dir.join("scratch/job1/tmp").join(name)).unwrap()
}

#[test]
fn an_honest_run_records_the_clis_cost_the_profile_and_passes_fresh_verification() {
    let lab = Lab::new("ok", "ok");
    let (core, item) = lab.engine(ClaudeLimits::default(), 1_000_000, 20);
    let Outcome::Done(r) = change(&core, item) else {
        panic!("expected a run")
    };
    // 0.12 USD is 120,000 micro-USD; the granted budget and the model are on the command line.
    assert_eq!(r.output.cost_micros, 120_000);
    assert!(
        r.output
            .result
            .contains("agent_stop=finished cost_micros=120000"),
        "{}",
        r.output.result
    );
    let argv = tmp_file(&lab, "argv.txt");
    assert!(
        argv.contains("--max-budget-usd\n0.300000\n") && argv.contains("--max-turns\n8\n"),
        "{argv}"
    );
    assert!(argv.contains("claude-haiku-5-5") && !argv.contains(TOKEN));
    // The token reached the child's environment (and only there).
    assert!(tmp_file(&lab, "env.txt").contains(&format!("CLAUDE_CODE_OAUTH_TOKEN={TOKEN}")));
    let all = format!("{:?}", core.store().evidence_for(item).unwrap());
    assert!(!all.contains(TOKEN), "the token is in the evidence");
    assert!(all.contains("cost_usd=0.12"), "{all}");
    // The profile is a structured column of the result row, not a text line.
    let ev = core.store().evidence_for(item).unwrap();
    let res = ev
        .iter()
        .find(|e| e.call == "git.change" && e.kind == Kind::Result)
        .unwrap();
    assert_eq!(res.profile.as_deref(), Some("real-agent"));
    assert!(
        !all.contains("profile: real-agent"),
        "no text line any more"
    );
    assert_eq!(core.verify(item).unwrap(), vec![true]);
    assert_eq!(core.store().item(item).unwrap().state, ItemState::Running);
}

/// The turns on the command line are the ones the SPEC declares (Core grants them), and the
/// dollar flag is the granted micro-USD with six decimals.
#[test]
fn the_flags_come_from_the_turns_and_cost_the_spec_declares_and_core_grants() {
    let lab = Lab::new("flags", "ok");
    let limits = ClaudeLimits {
        max_micros: 123_456,
        max_turns: 3,
    };
    let (core, item) = lab.engine(limits, 1_000_000, 20);
    let spec = core.spec("git.change").unwrap();
    assert_eq!((spec.max_cost_micros, spec.max_turns), (123_456, Some(3)));
    let Outcome::Done(_) = change(&core, item) else {
        panic!("expected a run")
    };
    let argv = tmp_file(&lab, "argv.txt");
    assert!(
        argv.contains("--max-turns\n3\n") && argv.contains("--max-budget-usd\n0.123456\n"),
        "{argv}"
    );
}

#[test]
fn a_cost_above_the_grant_is_recorded_as_a_violation_and_stops_the_goal() {
    let lab = Lab::new("over", "over");
    let (core, item) = lab.engine(ClaudeLimits::default(), 1_000_000, 20);
    assert!(matches!(
        change(&core, item),
        Outcome::Violation {
            max: 300_000,
            actual: 500_000
        }
    ));
    assert_eq!(core.store().item(item).unwrap().state, ItemState::Failed);
}

#[test]
fn a_run_the_executor_stopped_fails_with_its_actual_cost_and_no_violation() {
    let lab = Lab::new("turns", "turns");
    let (core, item) = lab.engine(ClaudeLimits::default(), 1_000_000, 20);
    let Outcome::Failed(why) = change(&core, item) else {
        panic!("expected a failure")
    };
    assert!(why.contains("stop=max_turns"), "{why}");
    let ev = core.store().evidence_for(item).unwrap();
    let last = ev
        .iter()
        .rev()
        .find(|e| e.call == "git.change" && e.kind == Kind::Result)
        .unwrap();
    assert_eq!(last.cost_micros, 20_000);
    assert!(ev.iter().all(|e| e.call != "core.violation"));
    assert_ne!(core.store().item(item).unwrap().state, ItemState::Done);
}

#[test]
fn a_hang_is_killed_at_the_deadline_and_charged_the_maximum() {
    let lab = Lab::new("hang", "hang");
    let (core, item) = lab.engine(ClaudeLimits::default(), 1_000_000, 2);
    let Outcome::Failed(why) = change(&core, item) else {
        panic!("expected a failure")
    };
    assert!(why.contains("killed at 2 s"), "{why}");
    let ev = core.store().evidence_for(item).unwrap();
    let last = ev
        .iter()
        .rev()
        .find(|e| e.call == "git.change" && e.kind == Kind::Result)
        .unwrap();
    assert_eq!(
        last.cost_micros, 300_000,
        "an unknown cost is charged the declared maximum"
    );
}

#[test]
fn an_unreadable_report_is_a_failure_charged_the_maximum() {
    let lab = Lab::new("garbage", "garbage");
    let (core, item) = lab.engine(ClaudeLimits::default(), 1_000_000, 20);
    assert!(matches!(change(&core, item), Outcome::Failed(_)));
    let ev = core.store().evidence_for(item).unwrap();
    let last = ev
        .iter()
        .rev()
        .find(|e| e.call == "git.change" && e.kind == Kind::Result)
        .unwrap();
    assert_eq!(last.cost_micros, 300_000);
}

#[test]
fn a_maximum_above_what_the_executor_can_enforce_is_refused_and_a_zero_cap_refuses() {
    let lab = Lab::new("refuse", "ok");
    let big = ClaudeLimits {
        max_micros: 600_000,
        max_turns: 8,
    };
    let (core, item) = lab.engine(big, 10_000_000, 20);
    assert!(matches!(
        change(&core, item),
        Outcome::Refused(Deny::BudgetUnenforceable)
    ));
    let lab = Lab::new("zerocap", "ok");
    let (core, item) = lab.engine(ClaudeLimits::default(), 0, 20);
    assert!(matches!(change(&core, item), Outcome::Refused(_)));
}

#[test]
fn the_real_agent_profile_allows_only_outbound_443_and_denies_secrets_by_name() {
    let reads = Reads {
        dirs: vec![],
        files: vec!["/Users/x/bin/claude".into()],
        deny: vec![],
    };
    let p = mac_profile_real_agent(
        Path::new("/private/tmp/job"),
        &reads,
        &[
            PathBuf::from("/Users/x/.ssh"),
            PathBuf::from("/Users/x/token"),
        ],
    );
    assert!(p.contains("(deny network*)") && p.contains("(remote tcp \"*:443\")"));
    assert!(
        !p.contains("network-inbound") && !p.contains("network-bind") && !p.contains("localhost")
    );
    let at = |s: &str| p.find(s).unwrap();
    assert!(
        at("(subpath \"/Users/x/.ssh\")")
            < at("(allow file-read-data (subpath \"/private/tmp/job\")")
    );
    assert!(p.ends_with("(literal \"/Users/x/bin/claude\"))"));
    assert_eq!(Profile::RealAgent { deny: vec![] }.name(), "real-agent");
}

/// REVIEW3 finding 9, real-agent profile: it cannot signal the engine, can signal its own child.
#[test]
fn the_real_agent_profile_cannot_signal_the_engine_but_can_signal_its_own_child() {
    let lab = Lab::new("signal", "signal");
    let (core, item) = lab.engine(ClaudeLimits::default(), 1_000_000, 20);
    let Outcome::Done(_) = change(&core, item) else {
        panic!("expected a run")
    };
    let repo = lab.dir.join("scratch/job1/repo");
    let n = |f: &str| {
        std::fs::read_to_string(repo.join(f))
            .unwrap()
            .trim()
            .to_owned()
    };
    assert_ne!(n("ppid.txt"), "0", "the agent signalled the engine");
    assert_eq!(n("child_kill.txt"), "0");
}

#[test]
fn a_snooping_agent_cannot_read_outside_the_job_or_reach_loopback() {
    let lab = Lab::new("snoop", "snoop");
    let canary = lab.dir.join("outside/canary");
    w(
        &lab.dir.join("template/canary_path"),
        canary.to_str().unwrap(),
    );
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    w(
        &lab.dir.join("template/port"),
        &listener.local_addr().unwrap().port().to_string(),
    );
    let (core, item) = lab.engine(ClaudeLimits::default(), 1_000_000, 20);
    let Outcome::Done(_) = change(&core, item) else {
        panic!("expected a run")
    };
    let repo = lab.dir.join("scratch/job1/repo");
    assert_eq!(
        std::fs::read_to_string(repo.join("leaked.txt")).unwrap_or_default(),
        ""
    );
    assert!(
        std::fs::read_to_string(repo.join("snoop.err"))
            .unwrap()
            .contains("not permitted")
    );
    assert!(
        std::fs::read_to_string(repo.join("loop.txt"))
            .unwrap()
            .contains("CLOSED")
    );
    let all = format!("{:?}", core.store().evidence_for(item).unwrap());
    assert!(!all.contains("DUMMY-SECRET-CANARY"));
}

/// RESIDUAL RISK, measured: the CLI's environment holds the access token. Inside the profile `ps`
/// cannot be executed (setuid) and the system `python3` shim fails; that does not prove no route
/// to the parent's environment exists (the README says so).
#[test]
fn residual_risk_the_obvious_routes_to_the_parents_token_are_closed_not_proven_all() {
    let lab = Lab::new("psenv", "psenv");
    let (core, item) = lab.engine(ClaudeLimits::default(), 1_000_000, 20);
    let Outcome::Done(_) = change(&core, item) else {
        panic!("expected a run")
    };
    let repo = lab.dir.join("scratch/job1/repo");
    let ps = std::fs::read_to_string(repo.join("psout.txt")).unwrap();
    // `ps` is a setuid binary: the profile cannot exec it.
    assert!(ps.contains("Operation not permitted"), "{ps}");
    // The system `python3` is a shim that needs the Xcode tree, which the profile hides, so a
    // sysctl read of the parent's environment from it fails too. Not proof that no route exists.
    let py = std::fs::read_to_string(repo.join("sysctl.txt")).unwrap();
    assert!(py.contains("xcrun: error"), "{py}");
    assert!(!py.contains(TOKEN));
    // And it never reached the evidence (redaction by name, and the agent's text is not env).
    let all = format!("{:?}", core.store().evidence_for(item).unwrap());
    assert!(!all.contains(TOKEN));
}
