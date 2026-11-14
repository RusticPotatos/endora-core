//! Slice 5a item 3: the executor declares whether it enforces the budget Core passes. A call with
//! a non-zero maximum is allowed only if its spec says `Enforced`; Core passes the granted budget
//! as a typed input and keeps the check-before and the violation-after as the second line.

mod common;
use common::{actor_named, owner};
use std::sync::{Arc, Mutex};

use endora_foundation::application::{CallFailure, CallOutput, Capability, Core, NewGoal, Outcome};
use endora_foundation::domain::Budget;
use endora_foundation::domain::{BudgetMode, CallSpec, Criterion, Deny, ItemId, Kind, PolicyRow};
use endora_foundation::infrastructure::ManualClock;
use endora_kernel::{Reversibility, Tier};

struct Paid {
    spec: CallSpec,
    actual: u64,
    seen: Mutex<Vec<Budget>>,
}
impl Capability for Paid {
    fn offers(&self) -> Vec<CallSpec> {
        vec![self.spec.clone()]
    }
    fn call(&self, _call: &str, _input: &str) -> Result<CallOutput, CallFailure> {
        unreachable!("Core must call the budgeted entry point");
    }
    fn call_budgeted(
        &self,
        _call: &str,
        _input: &str,
        budget: &Budget,
    ) -> Result<CallOutput, CallFailure> {
        self.seen.lock().unwrap().push(budget.clone());
        let mut o = CallOutput::text("ok");
        o.cost_micros = self.actual;
        Ok(o)
    }
}

fn setup(
    mode: BudgetMode,
    max: u64,
    actual: u64,
    goal_cap: u64,
    overall: u64,
) -> (Core, Arc<Paid>, ItemId) {
    let mut spec = CallSpec::new("x.paid", Reversibility::Reversible);
    spec.max_cost_micros = max;
    spec.budget = mode;
    let cap = Arc::new(Paid {
        spec,
        actual,
        seen: Mutex::new(vec![]),
    });
    let mut core = Core::in_memory(Arc::new(ManualClock::new(1))).unwrap();
    core.register(cap.clone()).unwrap();
    core.set_overall_cap(&owner(&core), overall).unwrap();
    let g = core
        .add_goal(
            &owner(&core),
            NewGoal {
                title: "t".into(),
                why: "w".into(),
                criteria: vec![Criterion {
                    text: "c".into(),
                    check: None,
                }],
                scope: vec!["x.paid".into()],
                tier: Tier::Internal,
                spend_cap_micros: goal_cap,
            },
        )
        .unwrap();
    core.grant(
        &owner(&core),
        PolicyRow {
            actor: "p".into(),
            call: "x.paid".into(),
            band: Reversibility::Reversible,
            tier: Tier::Internal,
            max_uses_per_item: None,
        },
    )
    .unwrap();
    let item = core.add_item(g, "i").unwrap();
    core.start(&actor_named("runner"), item).unwrap();
    (core, cap, item)
}

fn last_output(core: &Core, kind: Kind) -> String {
    core.store()
        .evidence_tail(10)
        .unwrap()
        .into_iter()
        .rev()
        .find(|e| e.kind == kind)
        .unwrap()
        .output
}

#[test]
fn an_executor_that_cannot_stop_itself_is_refused_a_non_zero_budget() {
    let (core, cap, item) = setup(BudgetMode::Unenforced, 5, 0, 100, 100);
    let o = core.call(&actor_named("p"), item, "x.paid", "").unwrap();
    assert_eq!(o, Outcome::Refused(Deny::BudgetUnenforceable));
    assert!(cap.seen.lock().unwrap().is_empty(), "it must not run");
    let said = last_output(&core, Kind::Refusal);
    assert!(said.contains("refused-unenforceable"), "{said}");
}

#[test]
fn an_enforcing_executor_receives_the_granted_budget_as_a_typed_input() {
    let (core, cap, item) = setup(BudgetMode::Enforced { units: 10 }, 5, 3, 100, 100);
    let Outcome::Done(_) = core.call(&actor_named("p"), item, "x.paid", "").unwrap() else {
        panic!("an enforced call within its caps runs");
    };
    let seen = cap.seen.lock().unwrap();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].max_cost_micros, 5);
    assert!(seen[0].max_seconds > 0);
    assert_eq!(seen[0].max_turns, None);
    let said = last_output(&core, Kind::Result);
    assert!(said.contains("enforced-by-executor"), "{said}");
}

#[test]
fn a_maximum_above_what_the_executor_can_enforce_is_refused() {
    let (core, cap, item) = setup(BudgetMode::Enforced { units: 4 }, 5, 0, 100, 100);
    assert_eq!(
        core.call(&actor_named("p"), item, "x.paid", "").unwrap(),
        Outcome::Refused(Deny::BudgetUnenforceable)
    );
    assert!(cap.seen.lock().unwrap().is_empty());
}

#[test]
fn actual_above_the_granted_budget_is_still_a_violation() {
    let (core, _, item) = setup(BudgetMode::Enforced { units: 10 }, 5, 6, 100, 100);
    let o = core.call(&actor_named("p"), item, "x.paid", "").unwrap();
    assert_eq!(o, Outcome::Violation { max: 5, actual: 6 });
    let rows = core.store().evidence_tail(10).unwrap();
    assert!(rows.iter().any(|e| e.call == "core.violation"));
}

#[test]
fn a_zero_maximum_is_unaffected_and_still_gets_a_zero_budget() {
    let (core, cap, item) = setup(BudgetMode::Unenforced, 0, 0, 0, 0);
    assert!(matches!(
        core.call(&actor_named("p"), item, "x.paid", "").unwrap(),
        Outcome::Done(_)
    ));
    assert_eq!(cap.seen.lock().unwrap()[0].max_cost_micros, 0);
}

#[test]
fn a_zero_cap_still_refuses_a_non_zero_maximum_even_when_enforced() {
    let (core, cap, item) = setup(BudgetMode::Enforced { units: 10 }, 5, 0, 0, 0);
    assert_eq!(
        core.call(&actor_named("p"), item, "x.paid", "").unwrap(),
        Outcome::Refused(Deny::SpendCap)
    );
    assert!(cap.seen.lock().unwrap().is_empty());
}
