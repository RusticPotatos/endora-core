//! Finding 5 (REVIEW2): the actor is a type. A capability cannot take the engine's or the
//! owner's name, the attempt is recorded, and the verifier's grants for `core` stay out of reach.

mod common;
use std::sync::Arc;

use common::{actor_named, owner};
use endora_foundation::application::{
    Acceptance, CallFailure, CallOutput, Capability, Core, NewGoal, Outcome,
};
use endora_foundation::domain::{
    Actor, ActorKind, CallSpec, Criterion, Deny, Fact, Field, ItemState, Kind, PolicyRow,
};
use endora_foundation::infrastructure::ManualClock;
use endora_kernel::{Reversibility, Tier};

struct Reader;

impl Capability for Reader {
    fn offers(&self) -> Vec<CallSpec> {
        vec![CallSpec {
            name: "r.read".into(),
            band: Some(Reversibility::Observe),
            input: "text".into(),
            output: "text".into(),
            tier: Tier::Internal,
            max_cost_micros: 0,
            max_seconds: endora_foundation::domain::DEFAULT_MAX_SECONDS,
            max_turns: None,
            inputs: vec![Field::text("input")],
            facts: vec![Field::word("status", &["ok"])],
            budget: Default::default(),
        }]
    }
    fn call(&self, _call: &str, _input: &str) -> Result<CallOutput, CallFailure> {
        Ok(CallOutput {
            cost_micros: 0,
            profile: None,
            facts: vec![Fact::word("status", "ok")],
            result: "ok".into(),
            evidence: String::new(),
        })
    }
}

fn setup() -> (Core, endora_foundation::domain::ItemId) {
    let mut core = Core::in_memory(Arc::new(ManualClock::new(1_000))).unwrap();
    core.register(Arc::new(Reader)).unwrap();
    let o = owner(&core);
    let g = core
        .add_goal(
            &o,
            NewGoal {
                title: "t".into(),
                why: "w".into(),
                criteria: vec![Criterion {
                    text: "done".into(),
                    check: None,
                }],
                scope: vec!["r.read".into()],
                tier: Tier::Internal,
                spend_cap_micros: 0,
            },
        )
        .unwrap();
    let item = core.add_item(g, "i").unwrap();
    core.start(&actor_named("runner"), item).unwrap();
    (core, item)
}

#[test]
fn a_capability_trying_to_act_as_core_or_owner_is_rejected_and_the_attempt_is_recorded() {
    let (core, _) = setup();
    let tries = [
        "core",
        "owner",
        "policy",
        "Core",
        "OWNER",
        "policy:auto_accept",
        "policy:auto-observe",
        "policy:anything",
        "owner:me",
    ];
    for name in tries {
        assert!(
            core.capability_actor(name).is_err(),
            "{name} must be refused"
        );
        assert!(Actor::capability(name).is_err(), "{name} must be refused");
    }
    let refusals: Vec<_> = core
        .store()
        .evidence_all()
        .unwrap()
        .into_iter()
        .filter(|e| e.call == "core.actor")
        .collect();
    assert_eq!(refusals.len(), tries.len(), "every attempt is a row");
    assert!(
        refusals
            .iter()
            .all(|e| !e.allowed && e.kind == Kind::Refusal)
    );
    assert!(refusals[0].output.contains("reserved"));
    // The engine recorded it as itself, never as the name that was tried.
    assert!(refusals.iter().all(|e| e.actor_kind == ActorKind::Core));
}

#[test]
fn the_verifiers_grants_for_core_cannot_be_reached_by_a_capability_built_actor() {
    let (core, item) = setup();
    // The owner grants the read to `core` only (what a goal's checks need).
    core.grant(
        &owner(&core),
        PolicyRow {
            actor: "core".into(),
            call: "r.read".into(),
            band: Reversibility::Observe,
            tier: Tier::Internal,
            max_uses_per_item: None,
        },
    )
    .unwrap();
    // Every actor a capability can build is a different actor: no policy row.
    for name in ["tester", "core2", "c0re", "core.verifier", "core-"] {
        let a = actor_named(name);
        assert!(matches!(
            core.call(&a, item, "r.read", "x").unwrap(),
            Outcome::Refused(Deny::NoPolicy)
        ));
    }
    // The names that would match are unbuildable.
    assert!(core.capability_actor("core").is_err());
}

#[test]
fn evidence_records_the_actor_kind_and_name_and_an_agent_cannot_have_its_work_accepted_by_itself() {
    let (core, item) = setup();
    core.grant(
        &owner(&core),
        PolicyRow {
            actor: "worker".into(),
            call: "r.read".into(),
            band: Reversibility::Observe,
            tier: Tier::Internal,
            max_uses_per_item: None,
        },
    )
    .unwrap();
    let worker = actor_named("worker");
    assert!(matches!(
        core.call(&worker, item, "r.read", "x").unwrap(),
        Outcome::Done(_)
    ));
    // The criterion needs the owner; nothing the worker has can close the item, and the only
    // acceptance method wants an OwnerProof (see the compile_fail doctests on `Core::accept`).
    assert_eq!(core.store().item(item).unwrap().state, ItemState::Running);
    core.confirm_criterion(&owner(&core), item, 0).unwrap();
    assert!(matches!(
        core.accept(&owner(&core), item).unwrap(),
        Acceptance::Accepted { .. }
    ));
    let ev = core.store().evidence_for(item).unwrap();
    let kind_of = |call: &str| {
        ev.iter()
            .filter(|e| e.call == call)
            .map(|e| (e.actor.clone(), e.actor_kind))
            .next()
            .unwrap()
    };
    assert_eq!(
        kind_of("r.read"),
        ("worker".to_owned(), ActorKind::Capability)
    );
    assert_eq!(
        kind_of("core.start"),
        ("runner".to_owned(), ActorKind::Capability)
    );
    assert_eq!(
        kind_of("core.confirm"),
        ("owner".to_owned(), ActorKind::Owner)
    );
    assert_eq!(
        kind_of("core.accept"),
        ("owner".to_owned(), ActorKind::Owner)
    );
    // Nothing a capability did is recorded as the owner or the engine.
    assert!(
        ev.iter()
            .filter(|e| e.actor == "worker" || e.actor == "runner")
            .all(|e| e.actor_kind == ActorKind::Capability)
    );
}
