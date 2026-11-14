//! Slice 2 acceptance tests (S1 to S6) for capability B, a git change task. Every agent here is
//! a SCRIPTED stand-in (a shell script): no model, no network, no paid API, no credentials.

mod common;
use common::{actor_named, owner};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use endora_foundation::application::{Acceptance, Core, NewGoal, Outcome};
use endora_foundation::domain::{Check, Criterion, Deny, ItemId, ItemState, Kind, PolicyRow};
use endora_foundation::infrastructure::SystemClock;
use endora_foundation_git::{Config, Confinement, GitChange};
use endora_kernel::{Reversibility, Tier};

const ALL: [&str; 4] = ["git.prepare", "git.change", "git.verify", "git.publish"];

struct Lab {
    dir: PathBuf,
}

impl Lab {
    fn new(name: &str) -> Self {
        let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("git-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("template")).unwrap();
        let dir = dir.canonicalize().unwrap();
        // The repo under change: `add` is wrong, and `test.sh` is the repo's own test.
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
    fn script(&self, name: &str, body: &str) -> PathBuf {
        let p = self.dir.join(name);
        w(&p, body);
        p
    }
    fn cfg(&self, agent: &Path) -> Config {
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
    fn repo(&self, job: &str) -> PathBuf {
        self.dir.join("scratch").join(job).join("repo")
    }
}

fn w(p: &Path, s: &str) {
    std::fs::write(p, s).unwrap();
}

fn row(actor: &str, call: &str, band: Reversibility, max: Option<u32>) -> PolicyRow {
    PolicyRow {
        actor: actor.into(),
        call: call.into(),
        band,
        tier: Tier::Internal,
        max_uses_per_item: max,
    }
}

/// An engine with capability B, an owner goal ("the repo's tests pass") and the runner's grants.
fn engine(db: &str, lab: &Lab, agent: &Path) -> (Core, ItemId) {
    let mut core = Core::open(db, Arc::new(SystemClock)).unwrap();
    core.register(Arc::new(GitChange::new(lab.cfg(agent))))
        .unwrap();
    let goal = core
        .add_goal(
            &owner(&core),
            NewGoal {
                title: "make the repo's tests pass".into(),
                why: "the add function is wrong".into(),
                criteria: vec![Criterion {
                    text: "the repo's own tests pass".into(),
                    check: Some(Check {
                        call: "git.verify".into(),
                        input: "job1".into(),
                        field: "tests".into(),
                        expect: "pass".into(),
                    }),
                }],
                scope: ALL.iter().map(|s| (*s).to_owned()).collect(),
                tier: Tier::Internal,
                spend_cap_micros: 0,
            },
        )
        .unwrap();
    grant_all(&core);
    let item = core.add_item(goal, "fix add").unwrap();
    (core, item)
}

fn grant_all(core: &Core) {
    let g = |actor: &str, call: &str, band, max| {
        core.grant(&owner(core), row(actor, call, band, max))
            .unwrap()
    };
    g("runner", "git.prepare", Reversibility::Reversible, Some(1));
    g("runner", "git.change", Reversibility::Reversible, Some(1));
    g("runner", "git.verify", Reversibility::Observe, None);
    g("core", "git.verify", Reversibility::Observe, None);
    // Even a row that covers the outward-reversible band cannot make Core allow a publish.
    g(
        "runner",
        "git.publish",
        Reversibility::OutwardReversible,
        None,
    );
}

fn done(o: Outcome) -> String {
    match o {
        Outcome::Done(r) => r.output.result,
        other => panic!("expected the call to run, got {other:?}"),
    }
}

#[test]
fn s1_the_scripted_agent_fixes_the_repo_and_the_item_waits_for_the_owner() {
    if !common::confined_or_skip() {
        return;
    }
    let lab = Lab::new("s1");
    let agent = lab.script(
        "fix.sh",
        "printf 'add() { echo $(( $1 + $2 )); }\\n' > lib.sh\necho fixed lib.sh\n",
    );
    let (core, item) = engine(":memory:", &lab, &agent);
    core.start(&actor_named("runner"), item).unwrap();
    done(
        core.call(&actor_named("runner"), item, "git.prepare", "job1")
            .unwrap(),
    );
    assert_eq!(
        done(
            core.call(&actor_named("runner"), item, "git.verify", "job1")
                .unwrap()
        ),
        "tests=fail exit=1"
    );
    let changed = done(
        core.call(&actor_named("runner"), item, "git.change", "job1\nfix add")
            .unwrap(),
    );
    assert!(changed.contains("agent_exit=0"), "{changed}");
    // Verified by the engine's own check (as `core`), not by anything the agent said.
    assert_eq!(core.verify(item).unwrap(), vec![true]);
    assert_eq!(
        core.store().item(item).unwrap().state,
        ItemState::Running,
        "awaiting acceptance"
    );
    // Nobody but the owner closes it: not the policy (reversible work).
    assert!(matches!(
        core.settle_routine_by_policy(item).unwrap(),
        Acceptance::Refused(_)
    ));
    // (A named proposer cannot accept: `accept` takes an OwnerProof, so it cannot compile.)
    assert_eq!(core.store().item(item).unwrap().state, ItemState::Running);
    // The evidence: test run before, the diff, test run after, and who did each.
    let ev = core.store().evidence_for(item).unwrap();
    let text =
        |e: &endora_foundation::domain::Evidence| format!("{} {} {}", e.actor, e.call, e.output);
    let before = ev
        .iter()
        .find(|e| e.call == "git.verify" && e.kind == Kind::Result)
        .unwrap();
    assert!(
        text(before).contains("runner git.verify tests=fail")
            && text(before).contains("FAIL: add 2 3 gave -1")
    );
    let change = ev
        .iter()
        .find(|e| e.call == "git.change" && e.kind == Kind::Result)
        .unwrap();
    assert!(
        change.actor == "runner"
            && change.output.contains("-add() { echo $(( $1 - $2 )); }")
            && change.output.contains("+add() { echo $(( $1 + $2 )); }")
    );
    let after = ev
        .iter()
        .rev()
        .find(|e| e.call == "git.verify" && e.kind == Kind::Result)
        .unwrap();
    assert!(
        after.actor == "core"
            && after.output.starts_with("tests=pass")
            && after.output.contains("PASS")
    );
    // The scripted owner accepts; only then is it done.
    assert!(matches!(
        core.accept(&owner(&core), item).unwrap(),
        Acceptance::Accepted { .. }
    ));
    assert_eq!(core.store().item(item).unwrap().state, ItemState::Done);
}

#[test]
fn s2_an_agent_that_claims_success_but_leaves_the_tests_failing_does_not_finish_the_item() {
    if !common::confined_or_skip() {
        return;
    }
    let lab = Lab::new("s2");
    let agent = lab.script("claim.sh", "echo 'DONE: I fixed it, all tests pass'\n");
    let (core, item) = engine(":memory:", &lab, &agent);
    core.start(&actor_named("runner"), item).unwrap();
    done(
        core.call(&actor_named("runner"), item, "git.prepare", "job1")
            .unwrap(),
    );
    done(
        core.call(&actor_named("runner"), item, "git.change", "job1")
            .unwrap(),
    );
    assert_eq!(core.verify(item).unwrap(), vec![false]);
    assert!(
        matches!(
            core.accept(&owner(&core), item).unwrap(),
            Acceptance::Refused(_)
        ),
        "owner cannot accept unmet criteria"
    );
    assert_eq!(core.store().item(item).unwrap().state, ItemState::Running);
    let ev = core.store().evidence_for(item).unwrap();
    let claim = ev
        .iter()
        .find(|e| e.call == "git.change" && e.kind == Kind::Result)
        .unwrap();
    assert!(
        claim.output.contains("all tests pass"),
        "the claim is kept as the agent's words"
    );
    let check = ev
        .iter()
        .rev()
        .find(|e| e.call == "git.verify" && e.kind == Kind::Result)
        .unwrap();
    assert!(
        check.output.starts_with("tests=fail exit=1")
            && check.output.contains("FAIL: add 2 3 gave -1"),
        "{}",
        check.output
    );
}

#[test]
fn s2b_an_agent_that_rewrites_the_repos_own_test_to_pass_is_caught() {
    if !common::confined_or_skip() {
        return;
    }
    let lab = Lab::new("s2b");
    let agent = lab.script("tamper.sh", "echo 'echo PASS' > test.sh\n");
    let (core, item) = engine(":memory:", &lab, &agent);
    core.start(&actor_named("runner"), item).unwrap();
    done(
        core.call(&actor_named("runner"), item, "git.prepare", "job1")
            .unwrap(),
    );
    done(
        core.call(&actor_named("runner"), item, "git.change", "job1")
            .unwrap(),
    );
    assert_eq!(core.verify(item).unwrap(), vec![false]);
    let ev = core.store().evidence_for(item).unwrap();
    let check = ev
        .iter()
        .rev()
        .find(|e| e.call == "git.verify" && e.kind == Kind::Result)
        .unwrap();
    assert!(
        check.output.starts_with("tests=tampered path=test.sh"),
        "{}",
        check.output
    );
}

#[test]
fn s3_the_agent_cannot_write_outside_or_push_and_publish_is_refused_by_authority() {
    if !common::confined_or_skip() {
        return;
    }
    let lab = Lab::new("s3");
    let outside = lab.dir.join("outside.txt");
    let bare = lab.dir.join("remote.git");
    assert!(
        Command::new("git")
            .args(["init", "-q", "--bare"])
            .arg(&bare)
            .status()
            .unwrap()
            .success()
    );
    let body = format!(
        "echo escaped > {o}; echo write-outside-exit=$?\ngit remote add origin {b}; git push origin HEAD:refs/heads/stolen 2>&1; echo push-exit=$?\necho env-names=$(env | cut -d= -f1 | sort | tr '\\n' ' ')\n",
        o = outside.display(),
        b = bare.display()
    );
    let agent = lab.script("escape.sh", &body);
    let (core, item) = engine(":memory:", &lab, &agent);
    core.start(&actor_named("runner"), item).unwrap();
    done(
        core.call(&actor_named("runner"), item, "git.prepare", "job1")
            .unwrap(),
    );
    let said = match core
        .call(&actor_named("runner"), item, "git.change", "job1")
        .unwrap()
    {
        Outcome::Done(r) => r.output.evidence,
        other => panic!("the agent should have run: {other:?}"),
    };
    assert!(
        !said.contains("write-outside-exit=0") && said.contains("write-outside-exit="),
        "{said}"
    );
    assert!(
        !said.contains("push-exit=0") && said.contains("push-exit="),
        "{said}"
    );
    let names = said
        .lines()
        .find_map(|l| l.strip_prefix("env-names="))
        .unwrap();
    let allowed = [
        "PATH",
        "HOME",
        "TMPDIR",
        "GIT_CONFIG_GLOBAL",
        "GIT_CONFIG_NOSYSTEM",
        "GIT_TERMINAL_PROMPT",
        "AGENT_TASK",
        "PWD",
        "SHLVL",
        "_",
        "OLDPWD",
    ];
    assert!(
        names.split_whitespace().all(|n| allowed.contains(&n)),
        "only a clean environment reaches the agent: {names}"
    );
    assert!(
        !outside.exists(),
        "nothing was written outside the scratch repo"
    );
    let refs = Command::new("git")
        .arg("-C")
        .arg(&bare)
        .args(["for-each-ref"])
        .output()
        .unwrap()
        .stdout;
    assert!(refs.is_empty(), "the remote received nothing");
    // The same push, asked of Core as a call: refused by authority, whatever the policy row says.
    match core
        .call(&actor_named("runner"), item, "git.publish", "job1")
        .unwrap()
    {
        Outcome::Refused(Deny::NeedsOwner) => {}
        other => panic!("publish must be refused, got {other:?}"),
    }
    let ev = core.store().evidence_for(item).unwrap();
    let refusal = ev.iter().find(|e| e.call == "git.publish").unwrap();
    assert!(
        !refusal.allowed
            && refusal.kind == Kind::Refusal
            && refusal.band == Reversibility::OutwardReversible
    );
    assert!(
        ev.iter()
            .any(|e| e.call == "git.change" && e.output.contains("push-exit=")),
        "the failed attempts are in the evidence"
    );
}

fn run_bin(lab: &Lab, db: &Path, item: i64, agent: &Path) -> std::process::Child {
    Command::new(env!("CARGO_BIN_EXE_foundation-git-run"))
        .args([
            "--db",
            db.to_str().unwrap(),
            "--scratch",
            lab.dir.join("scratch").to_str().unwrap(),
        ])
        .args([
            "--template",
            lab.dir.join("template").to_str().unwrap(),
            "--agent-script",
            agent.to_str().unwrap(),
        ])
        .args(["--item", &item.to_string(), "--job", "job1"])
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap()
}

#[test]
fn s4_a_real_process_killed_mid_agent_fails_the_item_explicitly_with_nothing_lost_or_duplicated() {
    if !common::confined_or_skip() {
        return;
    }
    let lab = Lab::new("s4");
    let agent = lab.script(
        "hold.sh",
        "echo $$ > agent.pid\necho started > agent-started\nexec sleep 300\n",
    );
    let db = lab.dir.join("s4.db");
    let (core, item) = engine(db.to_str().unwrap(), &lab, &agent);
    drop(core);
    let mut child = run_bin(&lab, &db, item.0, &agent);
    let began = Instant::now();
    while !lab.repo("job1").join("agent-started").exists() {
        assert!(
            // 60 s, not 20: a container backend starts several containers (git prepare) first,
            // slower when the whole suite runs at once.
            began.elapsed() < Duration::from_secs(60),
            "the agent never started"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    child.kill().unwrap(); // SIGKILL on a real second process, mid-agent
    child.wait().unwrap();
    let mut pid = String::new();
    std::fs::File::open(lab.repo("job1").join("agent.pid"))
        .unwrap()
        .read_to_string(&mut pid)
        .unwrap();
    let _ = Command::new("kill").args(["-KILL", pid.trim()]).status(); // reap the orphaned stand-in

    let (core, _) = engine_reopen(&db, &lab, &agent);
    let before = core.store().evidence_for(item).unwrap();
    assert_eq!(
        core.store().unsettled_intents(item).unwrap().len(),
        1,
        "git.change has an intent and no result"
    );
    let rec = core.recover().unwrap();
    assert_eq!(
        (rec.len(), rec[0].now),
        (1, ItemState::Failed),
        "{}",
        rec[0].why
    );
    assert!(rec[0].why.contains("unknown whether it took effect"));
    let after = core.store().evidence_for(item).unwrap();
    assert!(after.starts_with(&before), "no evidence lost or changed");
    assert_eq!(core.store().items().unwrap().len(), 1, "no duplicate item");
    assert_eq!(
        after
            .iter()
            .filter(|e| e.call == "git.change" && e.kind == Kind::Intent)
            .count(),
        1,
        "no duplicate change"
    );
    assert!(core.fire_due().unwrap().is_empty());
}

/// Reopens the same file with the capability registered (as a restarted process would).
fn engine_reopen(db: &Path, lab: &Lab, agent: &Path) -> (Core, ()) {
    let mut core = Core::open(db.to_str().unwrap(), Arc::new(SystemClock)).unwrap();
    core.register(Arc::new(GitChange::new(lab.cfg(agent))))
        .unwrap();
    (core, ())
}

#[test]
fn s5_the_domain_crate_knows_nothing_of_git_and_depends_on_no_capability() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../foundation");
    let manifest = std::fs::read_to_string(root.join("Cargo.toml")).unwrap();
    assert!(!manifest.contains("foundation-git") && !manifest.contains("foundation-observe"));
    let mut stack = vec![root.join("src"), root.join("tests")];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(d).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "rs") {
                let s = std::fs::read_to_string(&p).unwrap();
                assert!(
                    !s.contains("git.")
                        && !s.contains("foundation_git")
                        && !s.contains("sandbox-exec"),
                    "{p:?}"
                );
            }
        }
    }
}

#[test]
fn s6_with_the_stop_switch_set_the_change_is_refused_and_the_agent_never_runs() {
    if !common::confined_or_skip() {
        return;
    }
    let lab = Lab::new("s6");
    let agent = lab.script("mark.sh", "echo ran > ../agent-ran\n");
    let (core, item) = engine(":memory:", &lab, &agent);
    core.start(&actor_named("runner"), item).unwrap();
    done(
        core.call(&actor_named("runner"), item, "git.prepare", "job1")
            .unwrap(),
    );
    core.stop(&actor_named("tester")).unwrap();
    for call in ["git.change", "git.prepare", "git.verify"] {
        assert!(
            matches!(
                core.call(&actor_named("runner"), item, call, "job1")
                    .unwrap(),
                Outcome::Refused(Deny::Stopped)
            ),
            "{call}"
        );
    }
    assert!(!lab.dir.join("scratch/job1/agent-ran").exists());
}

#[test]
fn without_a_confinement_the_capability_fails_closed() {
    let lab = Lab::new("closed");
    let agent = lab.script("fix.sh", "true\n");
    let mut cfg = lab.cfg(&agent);
    cfg.confinement = Confinement::Unavailable;
    let mut core = Core::open(":memory:", Arc::new(SystemClock)).unwrap();
    core.register(Arc::new(GitChange::new(cfg))).unwrap();
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
                scope: ALL.iter().map(|s| (*s).to_owned()).collect(),
                tier: Tier::Internal,
                spend_cap_micros: 0,
            },
        )
        .unwrap();
    grant_all(&core);
    let item = core.add_item(goal, "i").unwrap();
    core.start(&actor_named("runner"), item).unwrap();
    assert!(
        matches!(core.call(&actor_named("runner"), item, "git.prepare", "job1").unwrap(), Outcome::Failed(m) if m.contains("no confinement"))
    );
    assert!(
        !lab.dir.join("scratch/job1/repo/.git").exists(),
        "nothing was run"
    );
}

#[test]
fn both_capabilities_share_one_engine_one_file_and_one_status_with_no_change_to_either() {
    if !common::confined_or_skip() {
        return;
    }
    use endora_foundation_observe::{Config as ObserveConfig, ObserveVerify};
    let lab = Lab::new("both");
    let agent = lab.script(
        "fix.sh",
        "printf 'add() { echo $(( $1 + $2 )); }\\n' > lib.sh\n",
    );
    let sandbox = lab.dir.join("observe");
    std::fs::create_dir_all(&sandbox).unwrap();
    w(&sandbox.join("disk.sh"), "echo disk-ok\n");
    let (mut core, item) = engine(":memory:", &lab, &agent);
    core.register(Arc::new(ObserveVerify::new(ObserveConfig {
        sandbox,
        probes: vec![("disk".into(), "disk.sh".into())],
        restart_script: "restart.sh".into(),
    })))
    .unwrap();
    let goal = core
        .add_goal(
            &owner(&core),
            NewGoal {
                title: "keep the disk probe answering".into(),
                why: "capability A beside B".into(),
                criteria: vec![Criterion {
                    text: "disk answers".into(),
                    check: None,
                }],
                scope: vec!["probe.command".into()],
                tier: Tier::Internal,
                spend_cap_micros: 0,
            },
        )
        .unwrap();
    core.grant(
        &owner(&core),
        row("runner", "probe.command", Reversibility::Observe, None),
    )
    .unwrap();
    let watch = core.add_item(goal, "watch the disk").unwrap();
    core.start(&actor_named("runner"), watch).unwrap();
    core.start(&actor_named("runner"), item).unwrap();
    assert_eq!(
        done(
            core.call(&actor_named("runner"), watch, "probe.command", "disk")
                .unwrap()
        ),
        "disk-ok"
    );
    done(
        core.call(&actor_named("runner"), item, "git.prepare", "job1")
            .unwrap(),
    );
    let status = core.status_text(10).unwrap();
    for want in [
        "keep the disk probe answering",
        "make the repo's tests pass",
        "runner probe.command observe allow",
        "runner git.prepare reversible allow",
    ] {
        assert!(status.contains(want), "{want}\n{status}");
    }
}
