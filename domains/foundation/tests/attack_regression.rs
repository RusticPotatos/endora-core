//! Regression tests from the slice 1 verifier's attacks on the policy (see VERIFY-SLICE1.md).
mod common;
use common::{actor_named, owner};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use endora_foundation::application::{CallFailure, CallOutput, Capability, Core, NewGoal, Outcome};
use endora_foundation::domain::{CallSpec, Criterion, Deny, PolicyRow};
use endora_foundation::infrastructure::ManualClock;
use endora_kernel::{Reversibility, Tier};

struct Cap {
    name: &'static str,
    band: Option<Reversibility>,
    cost: u64,
    hits: AtomicU32,
}
impl Capability for Cap {
    fn offers(&self) -> Vec<CallSpec> {
        vec![CallSpec {
            name: self.name.into(),
            band: self.band,
            input: String::new(),
            output: String::new(),
            tier: Tier::Internal,
            max_cost_micros: self.cost,
            max_seconds: endora_foundation::domain::DEFAULT_MAX_SECONDS,
            max_turns: None,
            inputs: Vec::new(),
            facts: Vec::new(),
            budget: endora_foundation::domain::BudgetMode::Enforced { units: u64::MAX },
        }]
    }
    fn call(&self, _c: &str, _i: &str) -> Result<CallOutput, CallFailure> {
        self.hits.fetch_add(1, Ordering::SeqCst);
        Ok(CallOutput {
            cost_micros: self.cost,
            profile: None,
            facts: Vec::new(),
            result: "ok".into(),
            evidence: String::new(),
        })
    }
}
fn cap(name: &'static str, band: Option<Reversibility>, cost: u64) -> Arc<Cap> {
    Arc::new(Cap {
        name,
        band,
        cost,
        hits: AtomicU32::new(0),
    })
}
fn goal(scope: &[&str], cap_cents: u64) -> NewGoal {
    NewGoal {
        title: "t".into(),
        why: "w".into(),
        criteria: vec![Criterion {
            text: "c".into(),
            check: None,
        }],
        scope: scope.iter().map(|s| (*s).to_owned()).collect(),
        tier: Tier::Internal,
        spend_cap_micros: cap_cents,
    }
}
fn row(actor: &str, call: &str, band: Reversibility) -> PolicyRow {
    PolicyRow {
        actor: actor.into(),
        call: call.into(),
        band,
        tier: Tier::Internal,
        max_uses_per_item: None,
    }
}
fn setup(
    c: Arc<Cap>,
    call: &str,
    band: Reversibility,
    goal_cap: u64,
    overall: u64,
) -> (Core, endora_foundation::domain::ItemId) {
    let mut core = Core::in_memory(Arc::new(ManualClock::new(1))).unwrap();
    core.register(c).unwrap();
    core.set_overall_cap(&owner(&core), overall).unwrap();
    let g = core
        .add_goal(&owner(&core), goal(&[call], goal_cap))
        .unwrap();
    core.grant(&owner(&core), row("p", call, band)).unwrap();
    let item = core.add_item(g, "i").unwrap();
    core.start(&actor_named("runner"), item).unwrap();
    (core, item)
}

#[test]
fn attack_outward_reversible_with_a_row_that_covers_it_is_still_refused() {
    let c = cap("x.out", Some(Reversibility::OutwardReversible), 0);
    let (core, item) = setup(c.clone(), "x.out", Reversibility::OutwardReversible, 0, 0);
    assert!(matches!(
        core.call(&actor_named("p"), item, "x.out", "").unwrap(),
        Outcome::Refused(Deny::NeedsOwner)
    ));
    assert_eq!(c.hits.load(Ordering::SeqCst), 0);
}
#[test]
fn attack_paid_call_with_zero_goal_cap_or_zero_overall_cap() {
    let c = cap("x.paid", Some(Reversibility::Reversible), 5);
    let (core, item) = setup(c.clone(), "x.paid", Reversibility::Reversible, 0, 100);
    assert!(matches!(
        core.call(&actor_named("p"), item, "x.paid", "").unwrap(),
        Outcome::Refused(Deny::SpendCap)
    ));
    let c2 = cap("x.paid", Some(Reversibility::Reversible), 5);
    let (core2, item2) = setup(c2.clone(), "x.paid", Reversibility::Reversible, 100, 0);
    assert!(matches!(
        core2.call(&actor_named("p"), item2, "x.paid", "").unwrap(),
        Outcome::Refused(Deny::SpendCap)
    ));
    assert_eq!(
        c.hits.load(Ordering::SeqCst) + c2.hits.load(Ordering::SeqCst),
        0
    );
}
#[test]
fn attack_paid_call_runs_until_the_cap_then_stops() {
    let c = cap("x.paid", Some(Reversibility::Reversible), 5);
    let (core, item) = setup(c.clone(), "x.paid", Reversibility::Reversible, 10, 10);
    assert!(matches!(
        core.call(&actor_named("p"), item, "x.paid", "").unwrap(),
        Outcome::Done(_)
    ));
    // 5 spent, cap 10: the second would reach 10 -> allowed (not over), the third is over.
    assert!(matches!(
        core.call(&actor_named("p"), item, "x.paid", "").unwrap(),
        Outcome::Done(_)
    ));
    assert!(matches!(
        core.call(&actor_named("p"), item, "x.paid", "").unwrap(),
        Outcome::Refused(Deny::SpendCap)
    ));
    assert_eq!(c.hits.load(Ordering::SeqCst), 2);
}
#[test]
fn attack_a_second_capability_cannot_shadow_a_declared_band() {
    let a = cap("dup.call", Some(Reversibility::Irreversible), 0);
    let b = cap("dup.call", Some(Reversibility::Observe), 0);
    let mut core = Core::in_memory(Arc::new(ManualClock::new(1))).unwrap();
    core.register(a.clone()).unwrap();
    assert!(
        core.register(b.clone()).is_err(),
        "slice 3: a duplicate call name is rejected, not shadowed"
    );
    let g = core
        .add_goal(&owner(&core), goal(&["dup.call"], 0))
        .unwrap();
    core.grant(&owner(&core), row("p", "dup.call", Reversibility::Observe))
        .unwrap();
    let item = core.add_item(g, "i").unwrap();
    core.start(&actor_named("runner"), item).unwrap();
    assert!(matches!(
        core.call(&actor_named("p"), item, "dup.call", "").unwrap(),
        Outcome::Refused(Deny::Irreversible)
    ));
    assert_eq!(
        a.hits.load(Ordering::SeqCst) + b.hits.load(Ordering::SeqCst),
        0
    );
}
#[test]
fn attack_reverse_order_shadowing_the_lower_band_registered_first_wins() {
    // Registered FIRST, the benign declaration used to silently shadow a later stricter one.
    // Now the later registration is rejected, so the stricter declaration is not lost silently.
    let a = cap("dup.call", Some(Reversibility::Observe), 0);
    let b = cap("dup.call", Some(Reversibility::Irreversible), 0);
    let mut core = Core::in_memory(Arc::new(ManualClock::new(1))).unwrap();
    core.register(a.clone()).unwrap();
    assert!(
        core.register(b.clone()).is_err(),
        "slice 3: a duplicate call name is rejected, not shadowed"
    );
    let g = core
        .add_goal(&owner(&core), goal(&["dup.call"], 0))
        .unwrap();
    core.grant(&owner(&core), row("p", "dup.call", Reversibility::Observe))
        .unwrap();
    let item = core.add_item(g, "i").unwrap();
    core.start(&actor_named("runner"), item).unwrap();
    let out = core.call(&actor_named("p"), item, "dup.call", "").unwrap();
    eprintln!(
        "REVERSE-ORDER: {out:?} a={} b={}",
        a.hits.load(Ordering::SeqCst),
        b.hits.load(Ordering::SeqCst)
    );
    assert!(matches!(out, Outcome::Done(_)));
    assert_eq!(b.hits.load(Ordering::SeqCst), 0);
}
#[test]
fn attack_a_liar_capability_declaring_observe_for_a_real_effect_is_allowed() {
    // Core cannot see inside a capability: the declaration is the trust boundary.
    let c = cap("liar.delete", Some(Reversibility::Observe), 0);
    let (core, item) = setup(c.clone(), "liar.delete", Reversibility::Observe, 0, 0);
    assert!(matches!(
        core.call(&actor_named("p"), item, "liar.delete", "")
            .unwrap(),
        Outcome::Done(_)
    ));
    assert_eq!(c.hits.load(Ordering::SeqCst), 1);
}
#[test]
fn attack_a_proposer_cannot_take_the_engines_or_the_owners_name() {
    // Was: the actor was a caller-asserted string, so a proposer could call as "core". Now
    // the name cannot be built (details in typed_actor.rs); under its own name it can call.
    let c = cap("x.read", Some(Reversibility::Observe), 0);
    let mut core = Core::in_memory(Arc::new(ManualClock::new(1))).unwrap();
    core.register(c.clone()).unwrap();
    let g = core.add_goal(&owner(&core), goal(&["x.read"], 0)).unwrap();
    core.grant(
        &owner(&core),
        row("tester", "x.read", Reversibility::Observe),
    )
    .unwrap();
    let item = core.add_item(g, "i").unwrap();
    core.start(&actor_named("runner"), item).unwrap();
    assert!(core.capability_actor("core").is_err());
    assert!(core.capability_actor("owner").is_err());
    assert!(matches!(
        core.call(&actor_named("tester"), item, "x.read", "")
            .unwrap(),
        Outcome::Done(_)
    ));
}
#[test]
fn attack_call_after_stop_for_every_band() {
    for band in [Reversibility::Observe, Reversibility::Reversible] {
        let c = cap("x.c", Some(band), 0);
        let (core, item) = setup(c.clone(), "x.c", band, 0, 0);
        core.stop(&actor_named("tester")).unwrap();
        assert!(matches!(
            core.call(&actor_named("p"), item, "x.c", "").unwrap(),
            Outcome::Refused(Deny::Stopped)
        ));
        assert_eq!(c.hits.load(Ordering::SeqCst), 0);
    }
}
