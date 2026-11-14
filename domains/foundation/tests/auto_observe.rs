//! Slice 4 item 1: the per-goal acceptance policy. `owner` (default) keeps today's behaviour;
//! `auto-observe` lets Core accept an item whose every call was observe-band and whose criteria
//! were all settled by structured checks, recorded as the actor `policy:auto-observe`. Anything
//! else falls back to the owner, and no capability or agent output can set the policy.

mod common;
use common::{actor_named, owner};
use std::sync::Arc;

use endora_foundation::application::{
    Acceptance, CallFailure, CallOutput, Capability, Core, NewGoal, Outcome,
};
use endora_foundation::domain::{
    AcceptRefusal, AcceptancePolicy, CallSpec, Check, Criterion, Fact, ItemState, PolicyRow,
};
use endora_foundation::infrastructure::ManualClock;
use endora_kernel::{Reversibility, Tier};

/// `w.look` observes (fact `up` = true unless the input is `down`); `w.fix` is reversible.
struct Watch;

fn spec(name: &str, band: Reversibility) -> CallSpec {
    CallSpec {
        name: name.into(),
        band: Some(band),
        input: "text".into(),
        output: "text".into(),
        tier: Tier::Internal,
        max_cost_micros: 0,
        max_seconds: endora_foundation::domain::DEFAULT_MAX_SECONDS,
        max_turns: None,
        inputs: vec![endora_foundation::domain::Field::text("target")],
        facts: vec![endora_foundation::domain::Field::bool("up")],
        budget: Default::default(),
    }
}

impl Capability for Watch {
    fn offers(&self) -> Vec<CallSpec> {
        vec![
            spec("w.look", Reversibility::Observe),
            spec("w.fix", Reversibility::Reversible),
        ]
    }
    fn call(&self, call: &str, input: &str) -> Result<CallOutput, CallFailure> {
        Ok(CallOutput {
            cost_micros: 0,
            profile: None,
            facts: vec![Fact::bool("up", call == "w.look" && input != "down")],
            result: "ok".into(),
            evidence: String::new(),
        })
    }
}

fn setup(policy: AcceptancePolicy, target: &str, clock: &Arc<ManualClock>) -> (Core, i64) {
    let mut core = Core::in_memory(clock.clone()).unwrap();
    core.register(Arc::new(Watch)).unwrap();
    let goal = core
        .add_goal_with_policy(
            &owner(&core),
            NewGoal {
                title: "watch the service".into(),
                why: "standing watch".into(),
                criteria: vec![Criterion {
                    text: "the service is up".into(),
                    check: Some(Check {
                        call: "w.look".into(),
                        input: target.into(),
                        field: "up".into(),
                        expect: "true".into(),
                    }),
                }],
                scope: vec!["w.look".into(), "w.fix".into()],
                tier: Tier::Internal,
                spend_cap_micros: 0,
            },
            policy,
        )
        .unwrap();
    for (call, band, who) in [
        ("w.look", Reversibility::Observe, "core"),
        ("w.fix", Reversibility::Reversible, "core"),
        ("w.fix", Reversibility::Reversible, "tester"),
    ] {
        core.grant(
            &owner(&core),
            PolicyRow {
                actor: who.into(),
                call: call.into(),
                band,
                tier: Tier::Internal,
                max_uses_per_item: None,
            },
        )
        .unwrap();
    }
    core.add_routine(&owner(&core), goal, "look", 1_000, false, 1_000)
        .unwrap();
    (core, goal.0)
}

/// Fire the due slot, start the new item and verify it; returns its id.
fn fire_and_verify(core: &Core) -> endora_foundation::domain::ItemId {
    let fired = core.fire_due().unwrap();
    let item = fired[0].item.expect("an item was created");
    core.start(&actor_named("runner"), item).unwrap();
    core.verify(item).unwrap();
    item
}

#[test]
fn a_verified_observe_item_is_accepted_by_the_policy_and_the_next_firing_runs() {
    let clock = Arc::new(ManualClock::new(1_000));
    let (core, _) = setup(AcceptancePolicy::AutoObserve, "svc", &clock);
    let first = fire_and_verify(&core);
    let got = core.settle_by_policy(first).unwrap();
    assert_eq!(
        got,
        Some(Acceptance::Accepted {
            by: "policy:auto-observe".into()
        })
    );
    assert_eq!(core.store().item(first).unwrap().state, ItemState::Done);
    clock.set(2_000);
    let second = core.fire_due().unwrap();
    assert!(second[0].item.is_some() && !second[0].skipped_open);
}

#[test]
fn the_evidence_names_the_policy_actor() {
    let clock = Arc::new(ManualClock::new(1_000));
    let (core, _) = setup(AcceptancePolicy::AutoObserve, "svc", &clock);
    let item = fire_and_verify(&core);
    core.settle_by_policy(item).unwrap();
    let ev = core.store().evidence_for(item).unwrap();
    let acc = ev.iter().find(|e| e.call == "core.accept").unwrap();
    assert_eq!(acc.actor, "policy:auto-observe");
    assert!(acc.allowed);
    assert!(acc.output.contains("policy:auto-observe"), "{}", acc.output);
    assert_eq!(
        core.store().item(item).unwrap().accepted_by.as_deref(),
        Some("policy:auto-observe")
    );
}

#[test]
fn a_reversible_call_is_not_auto_accepted() {
    let clock = Arc::new(ManualClock::new(1_000));
    let (core, _) = setup(AcceptancePolicy::AutoObserve, "svc", &clock);
    let item = fire_and_verify(&core);
    // The item also made a reversible call (allowed by the owner's row for core).
    assert!(matches!(
        core.call(&actor_named("tester"), item, "w.fix", "x")
            .unwrap(),
        Outcome::Done(_)
    ));
    let got = core.settle_by_policy(item).unwrap();
    assert_eq!(got, Some(Acceptance::Refused(AcceptRefusal::NeedsOwner)));
    assert_eq!(core.store().item(item).unwrap().state, ItemState::Running);
    // The owner still can.
    assert!(matches!(
        core.accept(&owner(&core), item).unwrap(),
        Acceptance::Accepted { .. }
    ));
}

#[test]
fn an_unmet_criterion_is_not_auto_accepted() {
    let clock = Arc::new(ManualClock::new(1_000));
    let (core, _) = setup(AcceptancePolicy::AutoObserve, "down", &clock);
    let item = fire_and_verify(&core);
    let got = core.settle_by_policy(item).unwrap();
    assert_eq!(
        got,
        Some(Acceptance::Refused(AcceptRefusal::CriteriaNotMet))
    );
    assert_eq!(core.store().item(item).unwrap().state, ItemState::Running);
}

#[test]
fn a_criterion_the_owner_confirmed_is_not_auto_accepted() {
    let clock = Arc::new(ManualClock::new(1_000));
    let mut core = Core::in_memory(clock.clone()).unwrap();
    core.register(Arc::new(Watch)).unwrap();
    let goal = core
        .add_goal_with_policy(
            &owner(&core),
            NewGoal {
                title: "t".into(),
                why: "w".into(),
                criteria: vec![Criterion {
                    text: "looks fine to a person".into(),
                    check: None,
                }],
                scope: vec![],
                tier: Tier::Internal,
                spend_cap_micros: 0,
            },
            AcceptancePolicy::AutoObserve,
        )
        .unwrap();
    let item = core.add_item(goal, "one").unwrap();
    core.start(&actor_named("runner"), item).unwrap();
    core.confirm_criterion(&owner(&core), item, 0).unwrap();
    assert_eq!(
        core.settle_by_policy(item).unwrap(),
        Some(Acceptance::Refused(AcceptRefusal::NotVerifiedByCheck))
    );
}

#[test]
fn the_owner_policy_is_the_default_and_never_auto_accepts_or_unblocks() {
    let clock = Arc::new(ManualClock::new(1_000));
    let (core, goal) = setup(AcceptancePolicy::Owner, "svc", &clock);
    assert_eq!(
        core.store()
            .goal(endora_foundation::domain::GoalId(goal))
            .unwrap()
            .acceptance,
        AcceptancePolicy::Owner
    );
    let item = fire_and_verify(&core);
    assert_eq!(core.settle_by_policy(item).unwrap(), None, "no attempt");
    assert_eq!(core.store().item(item).unwrap().state, ItemState::Running);
    clock.set(2_000);
    let again = core.fire_due().unwrap();
    assert!(
        again[0].skipped_open && again[0].item.is_none(),
        "still blocks"
    );
}

#[test]
fn an_auto_observe_goal_does_not_block_on_an_item_awaiting_the_owner() {
    let clock = Arc::new(ManualClock::new(1_000));
    let (core, _) = setup(AcceptancePolicy::AutoObserve, "down", &clock);
    let first = fire_and_verify(&core); // unmet: awaits the owner
    assert!(core.settle_by_policy(first).unwrap().is_some());
    clock.set(2_000);
    let again = core.fire_due().unwrap();
    assert!(again[0].item.is_some() && !again[0].skipped_open);
}

#[test]
fn an_item_not_yet_started_blocks_an_auto_observe_routine() {
    let clock = Arc::new(ManualClock::new(1_000));
    let (core, _) = setup(AcceptancePolicy::AutoObserve, "svc", &clock);
    core.fire_due().unwrap(); // the item stays open: nobody started it
    clock.set(2_000);
    assert!(core.fire_due().unwrap()[0].skipped_open);
}

#[test]
fn an_item_cut_off_inside_a_call_blocks_an_auto_observe_routine_until_recovered() {
    let dir = std::env::temp_dir().join(format!("fr-s4-cutoff-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("c.db");
    let _ = std::fs::remove_file(&file);
    let clock = Arc::new(ManualClock::new(1_000));
    let mut core = Core::open(file.to_str().unwrap(), clock.clone()).unwrap();
    core.register(Arc::new(Watch)).unwrap();
    let goal = core
        .add_goal_with_policy(
            &owner(&core),
            NewGoal {
                title: "t".into(),
                why: "w".into(),
                criteria: vec![Criterion {
                    text: "c".into(),
                    check: None,
                }],
                scope: vec!["w.look".into()],
                tier: Tier::Internal,
                spend_cap_micros: 0,
            },
            AcceptancePolicy::AutoObserve,
        )
        .unwrap();
    core.add_routine(&owner(&core), goal, "look", 1_000, false, 1_000)
        .unwrap();
    let item = core.fire_due().unwrap()[0].item.unwrap();
    core.start(&actor_named("runner"), item).unwrap();
    // A process died after recording an intent and before its result.
    let raw = rusqlite::Connection::open(&file).unwrap();
    raw.execute(
        "INSERT INTO evidence (item_id, call, actor, band, decision, kind, inputs_hash, output, at_ms) VALUES (?1,'w.look','core','observe','allow','intent','h','',1000)",
        [item.0],
    )
    .unwrap();
    clock.set(2_000);
    assert!(core.fire_due().unwrap()[0].skipped_open, "mid-call blocks");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn nothing_a_capability_returns_or_a_named_actor_does_can_set_or_change_the_policy() {
    let clock = Arc::new(ManualClock::new(1_000));
    let (core, goal) = setup(AcceptancePolicy::Owner, "svc", &clock);
    let goal = endora_foundation::domain::GoalId(goal);
    // (Only the owner creates a goal with a policy: `add_goal_with_policy` takes an OwnerProof.)
    // The policy actor cannot be asked to accept on an owner goal: `settle_by_policy` attempts
    // nothing there (the refusal rule itself is covered by the unit test in domain/accept.rs).
    let item = fire_and_verify(&core);
    assert!(core.settle_by_policy(item).unwrap().is_none());
    // The stored policy cannot be changed after creation, not even by raw SQL on the file.
    let dir = std::env::temp_dir().join(format!("fr-s4-policy-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("p.db");
    let _ = std::fs::remove_file(&file);
    {
        let c2 = Core::open(file.to_str().unwrap(), clock.clone()).unwrap();
        let g2 = c2
            .add_goal(
                &owner(&core),
                NewGoal {
                    title: "t".into(),
                    why: "w".into(),
                    criteria: vec![Criterion {
                        text: "c".into(),
                        check: None,
                    }],
                    scope: vec![],
                    tier: Tier::Internal,
                    spend_cap_micros: 0,
                },
            )
            .unwrap();
        assert_eq!(
            c2.store().goal(g2).unwrap().acceptance,
            AcceptancePolicy::Owner
        );
    }
    let raw = rusqlite::Connection::open(&file).unwrap();
    let e = raw
        .execute("UPDATE goals SET acceptance='auto-observe'", [])
        .unwrap_err();
    assert!(e.to_string().contains("fixed at creation"), "{e}");
    let _ = goal;
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_file_from_before_slice_4_gains_the_column_and_every_goal_stays_owner() {
    let dir = std::env::temp_dir().join(format!("fr-s4-migrate-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("m.db");
    let _ = std::fs::remove_file(&file);
    let clock = Arc::new(ManualClock::new(1));
    {
        let core = Core::open(file.to_str().unwrap(), clock.clone()).unwrap();
        core.add_goal(
            &owner(&core),
            NewGoal {
                title: "old".into(),
                why: "w".into(),
                criteria: vec![Criterion {
                    text: "c".into(),
                    check: None,
                }],
                scope: vec![],
                tier: Tier::Internal,
                spend_cap_micros: 0,
            },
        )
        .unwrap();
    }
    let raw = rusqlite::Connection::open(&file).unwrap();
    raw.execute_batch(
        // A real pre-chain file has no hash columns (slice 7 F3: a file with hashes and a lowered
        // version is refused, not migrated).
        "DROP TRIGGER goals_acceptance_fixed; ALTER TABLE goals DROP COLUMN acceptance;
         ALTER TABLE evidence DROP COLUMN hash; ALTER TABLE evidence DROP COLUMN prev_hash;
         PRAGMA user_version = 0;",
    )
    .unwrap();
    drop(raw);
    let core = Core::open(file.to_str().unwrap(), clock).unwrap();
    let g = core.store().goals().unwrap();
    assert_eq!(g[0].acceptance, AcceptancePolicy::Owner);
    let _ = std::fs::remove_dir_all(&dir);
}
