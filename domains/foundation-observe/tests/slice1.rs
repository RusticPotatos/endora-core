//! Slice-1 acceptance: T1 (standing goal and routine), T2 (adversarial proposer), T3 (kill and
//! resume with a real second process). T4 and T5 are in the foundation crate; T6 is
//! `t6_independence.rs` and the crate's own `core_rules.rs`.

mod common;
use common::{actor_named, owner};

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use common::Lab;
use endora_foundation::application::{Acceptance, Core, NewGoal, Outcome};
use endora_foundation::domain::{
    AcceptRefusal, Check, Criterion, Deny, ItemState, Kind, PolicyRow,
};
use endora_foundation::infrastructure::{ManualClock, SystemClock};
use endora_foundation_observe::{Config, ObserveVerify};
use endora_kernel::{Reversibility, Tier};

fn config(lab: &Lab, restart: &str) -> Config {
    Config {
        sandbox: lab.sandbox.clone(),
        probes: vec![("disk".into(), "disk.sh".into())],
        restart_script: restart.into(),
    }
}

fn healthy_goal(lab: &Lab, scope: &[&str]) -> NewGoal {
    NewGoal {
        title: "keep service X healthy".into(),
        why: "the house depends on it".into(),
        criteria: vec![Criterion {
            text: "X answers 200 on /health".into(),
            check: Some(Check {
                call: "probe.http".into(),
                input: lab.url(),
                field: "status".into(),
                expect: "200".into(),
            }),
        }],
        scope: scope.iter().map(|s| (*s).to_owned()).collect(),
        tier: Tier::Internal,
        spend_cap_micros: 0,
    }
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

fn engine(
    db: &str,
    lab: &Lab,
    clock: Arc<dyn endora_kernel::Clock + Send + Sync>,
    restart: &str,
) -> Core {
    let mut core = Core::open(db, clock).unwrap();
    core.register(Arc::new(ObserveVerify::new(config(lab, restart))))
        .unwrap();
    core
}

#[test]
fn t1_a_standing_goal_with_a_routine_fires_twice_into_durable_items_verified_by_the_probe() {
    let lab = Lab::new("t1");
    let db = lab.dir.join("t1.db");
    let clock = Arc::new(ManualClock::new(0));
    let core = engine(db.to_str().unwrap(), &lab, clock.clone(), "restart.sh");
    let goal = core
        .add_goal(
            &owner(&core),
            healthy_goal(&lab, &["probe.http", "service.restart"]),
        )
        .unwrap();
    // `core` runs the engine's own checks; `runner` drives the item; `tester` calls directly.
    for who in ["core", "runner", "tester"] {
        core.grant(
            &owner(&core),
            row(who, "probe.http", Reversibility::Observe, None),
        )
        .unwrap();
        core.grant(
            &owner(&core),
            row(who, "service.restart", Reversibility::Reversible, Some(1)),
        )
        .unwrap();
    }
    let routine = core
        .add_routine(&owner(&core), goal, "check X", 1_000, true, 1_000)
        .unwrap();

    // Firing 1: the service is up. Observe-band, verified by the probe: policy may accept it.
    clock.set(1_000);
    let first = core.fire_due().unwrap()[0].item.unwrap();
    core.start(&actor_named("runner"), first).unwrap();
    assert_eq!(core.verify(first).unwrap(), vec![true]);
    assert!(
        matches!(core.settle_routine_by_policy(first).unwrap(), Acceptance::Accepted { ref by } if by == "policy:auto_accept")
    );

    // Firing 2: the service is down. The probe says so; nothing closes; one capped restart.
    lab.set_healthy(false);
    clock.set(2_000);
    let second = core.fire_due().unwrap()[0].item.unwrap();
    core.start(&actor_named("runner"), second).unwrap();
    assert_eq!(core.verify(second).unwrap(), vec![false]);
    assert!(matches!(
        core.settle_routine_by_policy(second).unwrap(),
        Acceptance::Refused(AcceptRefusal::CriteriaNotMet)
    ));
    assert!(matches!(
        core.call(&actor_named("tester"), second, "service.restart", "restart")
            .unwrap(),
        Outcome::Done(_)
    ));
    assert!(matches!(
        core.call(&actor_named("tester"), second, "service.restart", "restart")
            .unwrap(),
        Outcome::Refused(Deny::CapReached)
    ));
    assert_eq!(lab.restarts(), 1, "the cap held: one restart");
    assert_eq!(core.verify(second).unwrap(), vec![true]);
    // Reversible work is never accepted by policy; the owner accepts it.
    assert!(matches!(
        core.settle_routine_by_policy(second).unwrap(),
        Acceptance::Refused(AcceptRefusal::NeedsOwner)
    ));
    assert!(
        matches!(core.accept(&owner(&core), second).unwrap(), Acceptance::Accepted { ref by } if by == "owner")
    );
    drop(core);

    // Durable: reopen the file. Two items, each with its own criteria, evidence recorded.
    let again = engine(db.to_str().unwrap(), &lab, clock, "restart.sh");
    let items = again.store().items().unwrap();
    assert_eq!(items.len(), 2);
    assert!(
        items
            .iter()
            .all(|i| i.state == ItemState::Done && i.routine == Some(routine))
    );
    assert_eq!(again.store().firings(routine).unwrap().len(), 2);
    for i in &items {
        let c = again.store().criteria(i.id).unwrap();
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].met_by.as_deref(), Some("check"));
        assert!(
            c[0].met_evidence_id.is_some(),
            "a met criterion cites its evidence"
        );
    }
    let ev = again.store().evidence_for(second).unwrap();
    let probes = ev
        .iter()
        .filter(|e| e.call == "probe.http" && e.kind == Kind::Result)
        .count();
    assert_eq!(
        probes, 2,
        "the failing probe and the passing one are both kept"
    );
    assert!(
        ev.iter()
            .any(|e| e.call == "probe.http" && e.output.contains("status=503"))
    );
    println!("\n{}", again.status_text(8).unwrap());
}

#[test]
fn t2_a_scripted_proposer_out_of_band_is_refused_every_time_with_no_side_effect() {
    let lab = Lab::new("t2");
    let clock = Arc::new(ManualClock::new(5));
    let core = engine(":memory:", &lab, clock, "restart.sh");
    // The goal's scope names probe, restart and wipe, but not probe.command.
    let goal = core
        .add_goal(
            &owner(&core),
            healthy_goal(&lab, &["probe.http", "service.restart", "service.wipe"]),
        )
        .unwrap();
    let p = "scripted-proposer";
    core.grant(
        &owner(&core),
        row(p, "probe.http", Reversibility::Observe, None),
    )
    .unwrap();
    // A row too narrow for the call it names: restart is reversible, the row covers observe.
    core.grant(
        &owner(&core),
        row(p, "service.restart", Reversibility::Observe, None),
    )
    .unwrap();
    // A row for a call the goal's scope does not name.
    core.grant(
        &owner(&core),
        row(p, "probe.command", Reversibility::Observe, None),
    )
    .unwrap();
    let item = core.add_item(goal, "a proposer's work").unwrap();
    core.start(&actor_named("runner"), item).unwrap();

    // The scripted proposer: no model, a fixed list of proposals, in order.
    let script: [(&str, &str, Deny); 5] = [
        ("service.restart", "restart", Deny::BandAboveRow),
        ("service.wipe", "data.db", Deny::Irreversible),
        ("probe.command", "disk", Deny::OutOfScope),
        ("shell.exec", "rm -rf /", Deny::UnknownCall),
        ("probe.http", "never mind", Deny::NoPolicy), // asked as a different actor, below
    ];
    for (i, (call, input, want)) in script.iter().enumerate() {
        let actor = if i == 4 {
            actor_named("someone-else")
        } else {
            actor_named(p)
        };
        match core.call(&actor, item, call, input).unwrap() {
            Outcome::Refused(got) => assert_eq!(&got, want, "{call}"),
            other => panic!("{call} should be refused, got {other:?}"),
        }
    }
    // The proposer is genuinely authorised for a read, so the stop switch is what stops the next.
    assert!(matches!(
        core.call(&actor_named(p), item, "probe.http", &lab.url())
            .unwrap(),
        Outcome::Done(_)
    ));
    core.stop(&actor_named("tester")).unwrap();
    assert!(matches!(
        core.call(&actor_named(p), item, "probe.http", &lab.url())
            .unwrap(),
        Outcome::Refused(Deny::Stopped)
    ));
    assert!(matches!(
        core.call(&actor_named(p), item, "service.restart", "restart")
            .unwrap(),
        Outcome::Refused(Deny::Stopped)
    ));

    // No side effect: assert on the dummy service itself.
    assert_eq!(lab.restarts(), 0, "no restart happened");
    assert!(lab.data_intact(), "the irreversible call did not run");
    assert!(lab.healthy());
    // Each refusal is evidence: seven denials, in order, each saying why; one allowed call.
    let ev = core.store().evidence_for(item).unwrap();
    let denied: Vec<_> = ev.iter().filter(|e| !e.allowed).collect();
    assert_eq!(denied.len(), 7);
    assert!(
        denied
            .iter()
            .all(|e| e.kind == Kind::Refusal && e.output.starts_with("refused:"))
    );
    assert!(denied[5].output.contains("stop switch") && denied[6].output.contains("stop switch"));
    assert_eq!(
        ev.iter()
            .filter(|e| e.allowed && e.kind == Kind::Intent)
            .count(),
        1
    );
    assert_eq!(
        denied[1].band,
        Reversibility::Irreversible,
        "the band is the capability's declaration"
    );
}

fn run_bin(
    lab: &Lab,
    db: &std::path::Path,
    item: i64,
    mode: &str,
    restart: &str,
) -> std::process::Child {
    Command::new(env!("CARGO_BIN_EXE_foundation-run"))
        .args([
            "--db",
            db.to_str().unwrap(),
            "--sandbox",
            lab.sandbox.to_str().unwrap(),
            "--item",
            &item.to_string(),
        ])
        .args([
            "--url",
            &lab.url(),
            "--mode",
            mode,
            "--restart-script",
            restart,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap()
}

fn seed(lab: &Lab, db: &std::path::Path) -> (Core, endora_foundation::domain::ItemId) {
    let core = engine(
        db.to_str().unwrap(),
        lab,
        Arc::new(SystemClock),
        "restart.sh",
    );
    let goal = core
        .add_goal(
            &owner(&core),
            healthy_goal(lab, &["probe.http", "service.restart"]),
        )
        .unwrap();
    // `core` runs the engine's own checks; `runner` drives the item; `tester` calls directly.
    for who in ["core", "runner", "tester"] {
        core.grant(
            &owner(&core),
            row(who, "probe.http", Reversibility::Observe, None),
        )
        .unwrap();
        core.grant(
            &owner(&core),
            row(who, "service.restart", Reversibility::Reversible, Some(1)),
        )
        .unwrap();
    }
    let now = endora_kernel::Clock::now(&SystemClock).unix_millis();
    core.add_routine(&owner(&core), goal, "check X", 3_600_000, true, now)
        .unwrap();
    let item = core.fire_due().unwrap()[0].item.unwrap();
    (core, item)
}

#[test]
fn t3_kill_a_real_process_mid_item_then_resume_without_duplicate_firing_or_lost_evidence() {
    let lab = Lab::new("t3");
    let db = lab.dir.join("t3.db");
    let (core, item) = seed(&lab, &db);
    drop(core);

    // The runner is a real second process. It starts the item, probes (intent and result are
    // written), says READY and waits. SIGKILL (Child::kill on unix) ends it mid-item.
    let mut child = run_bin(&lab, &db, item.0, "probe-hold", "restart.sh");
    let mut line = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    assert!(line.starts_with("READY"), "child said {line:?}");
    child.kill().unwrap();
    let status = child.wait().unwrap();
    assert!(!status.success(), "it was killed, not finished");

    // Restart: the file still says running, with the evidence the dead process wrote.
    let core = engine(
        db.to_str().unwrap(),
        &lab,
        Arc::new(SystemClock),
        "restart.sh",
    );
    assert_eq!(core.store().item(item).unwrap().state, ItemState::Running);
    let before = core.store().evidence_for(item).unwrap();
    assert_eq!(
        before.iter().filter(|e| e.call == "probe.http").count(),
        2,
        "intent and result survived the kill"
    );
    let recovered = core.recover().unwrap();
    assert_eq!(recovered.len(), 1);
    assert_eq!(recovered[0].now, ItemState::Open, "{}", recovered[0].why);
    let kept = core.store().evidence_for(item).unwrap();
    assert!(
        kept.starts_with(&before),
        "no evidence lost or changed; recovery only appended"
    );
    // No duplicate firing: the routine's slot is on record once and fires nothing again.
    assert!(core.fire_due().unwrap().is_empty());
    assert_eq!(core.store().items().unwrap().len(), 1);
    // Resume from durable state and finish: criteria met by a fresh probe, policy accepts.
    core.start(&actor_named("runner"), item).unwrap();
    assert_eq!(core.store().item(item).unwrap().attempts, 2);
    assert_eq!(core.verify(item).unwrap(), vec![true]);
    assert!(matches!(
        core.settle_routine_by_policy(item).unwrap(),
        Acceptance::Accepted { .. }
    ));
    assert!(core.store().evidence_for(item).unwrap().len() > kept.len());
}

#[test]
fn t3b_killed_inside_a_reversible_call_the_item_fails_explicitly_not_silently() {
    let lab = Lab::new("t3b");
    let db = lab.dir.join("t3b.db");
    let (core, item) = seed(&lab, &db);
    drop(core);
    lab.set_healthy(false);
    let mut child = run_bin(&lab, &db, item.0, "restart-hold", "slow-restart.sh");
    let started = Instant::now();
    while !lab.sandbox.join("started").exists() {
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "the restart never began"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    child.kill().unwrap();
    child.wait().unwrap();

    let core = engine(
        db.to_str().unwrap(),
        &lab,
        Arc::new(SystemClock),
        "restart.sh",
    );
    let unsettled = core.store().unsettled_intents(item).unwrap();
    assert_eq!(
        unsettled.len(),
        1,
        "an intent with no result: the call may or may not have run"
    );
    let out = core.recover().unwrap();
    assert_eq!(out[0].now, ItemState::Failed);
    assert!(out[0].why.contains("unknown whether it took effect"));
    let it = core.store().item(item).unwrap();
    assert_eq!(it.state, ItemState::Failed);
    assert!(it.reason.unwrap().contains("unknown"));
    assert!(
        core.fire_due().unwrap().is_empty(),
        "and nothing fires twice"
    );
}
