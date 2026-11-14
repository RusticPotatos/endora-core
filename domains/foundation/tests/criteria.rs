//! Slice 3 item 4: a goal holds its done criteria once. Items reference them; an item has its
//! own subject (substituted for `{subject}` in a check's input) or, rarely, its own criteria.
//! Still two levels: goal and work item.

mod common;
use common::{actor_named, owner};
use std::sync::{Arc, Mutex};

use endora_foundation::application::{CallFailure, CallOutput, Capability, Core, NewGoal, NewItem};
use endora_foundation::domain::{CallSpec, Check, Criterion, Fact, ItemId, PolicyRow};
use endora_foundation::infrastructure::ManualClock;
use endora_kernel::{Reversibility, Tier};

#[derive(Default)]
struct Echo {
    seen: Mutex<Vec<String>>,
}

impl Capability for Echo {
    fn offers(&self) -> Vec<CallSpec> {
        vec![CallSpec {
            name: "j.check".into(),
            band: Some(Reversibility::Observe),
            input: "job name".into(),
            output: "status".into(),
            tier: Tier::Internal,
            max_cost_micros: 0,
            max_seconds: endora_foundation::domain::DEFAULT_MAX_SECONDS,
            max_turns: None,
            inputs: vec![endora_foundation::domain::Field::text("job")],
            facts: vec![endora_foundation::domain::Field::word(
                "status",
                &["ok", "bad"],
            )],
            budget: Default::default(),
        }]
    }
    fn call(&self, _call: &str, input: &str) -> Result<CallOutput, CallFailure> {
        self.seen.lock().unwrap().push(input.to_owned());
        Ok(CallOutput {
            cost_micros: 0,
            profile: None,
            facts: vec![Fact::word(
                "status",
                if input == "job1" { "ok" } else { "bad" },
            )],
            result: if input == "job1" {
                "status=ok"
            } else {
                "status=bad"
            }
            .into(),
            evidence: String::new(),
        })
    }
}

fn check(input: &str) -> Criterion {
    Criterion {
        text: "the job is healthy".into(),
        check: Some(Check {
            call: "j.check".into(),
            input: input.into(),
            field: "status".into(),
            expect: "ok".into(),
        }),
    }
}

fn engine(criteria: Vec<Criterion>) -> (Core, Arc<Echo>, endora_foundation::domain::GoalId) {
    let echo = Arc::new(Echo::default());
    let mut core = Core::in_memory(Arc::new(ManualClock::new(1))).unwrap();
    core.register(echo.clone()).unwrap();
    let g = core
        .add_goal(
            &owner(&core),
            NewGoal {
                title: "keep jobs healthy".into(),
                why: "w".into(),
                criteria,
                scope: vec!["j.check".into()],
                tier: Tier::Internal,
                spend_cap_micros: 0,
            },
        )
        .unwrap();
    core.grant(
        &owner(&core),
        PolicyRow {
            actor: "core".into(),
            call: "j.check".into(),
            band: Reversibility::Observe,
            tier: Tier::Internal,
            max_uses_per_item: None,
        },
    )
    .unwrap();
    (core, echo, g)
}

fn job(core: &Core, g: endora_foundation::domain::GoalId, subject: &str) -> ItemId {
    let item = core
        .add_item_with(NewItem {
            goal: g,
            title: format!("check {subject}"),
            subject: Some(subject.into()),
            criteria: None,
        })
        .unwrap();
    core.start(&actor_named("runner"), item).unwrap();
    item
}

#[test]
fn one_goal_two_items_one_criteria_definition() {
    let (core, echo, g) = engine(vec![check("{subject}")]);
    let (a, b) = (job(&core, g, "job1"), job(&core, g, "job2"));
    // One definition is stored, however many items there are.
    let n: i64 = core
        .store()
        .db()
        .lock()
        .unwrap()
        .query_row("SELECT COUNT(*) FROM criteria", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 1);
    assert_eq!(core.verify(a).unwrap(), [true]);
    assert_eq!(
        core.verify(b).unwrap(),
        [false],
        "job2's own subject was checked, and is bad"
    );
    assert_eq!(*echo.seen.lock().unwrap(), ["job1", "job2"]);
    // The stored definition keeps the placeholder; each item's met state is its own.
    assert_eq!(
        core.store().goal(g).unwrap().criteria[0]
            .check
            .as_ref()
            .unwrap()
            .input,
        "{subject}"
    );
    assert!(core.store().criteria(a).unwrap()[0].met_by.is_some());
    assert!(core.store().criteria(b).unwrap()[0].met_by.is_none());
}

#[test]
fn an_item_may_carry_its_own_criteria_instead() {
    let (core, echo, g) = engine(vec![check("{subject}")]);
    let own = core
        .add_item_with(NewItem {
            goal: g,
            title: "special".into(),
            subject: None,
            criteria: Some(vec![
                check("job1"),
                Criterion {
                    text: "owner looks".into(),
                    check: None,
                },
            ]),
        })
        .unwrap();
    core.start(&actor_named("runner"), own).unwrap();
    assert_eq!(core.verify(own).unwrap(), [true, false]);
    assert_eq!(*echo.seen.lock().unwrap(), ["job1"]);
    // A blank or over-long override is refused like a goal's.
    assert!(
        core.add_item_with(NewItem {
            goal: g,
            title: "x".into(),
            subject: None,
            criteria: Some(vec![])
        })
        .is_err()
    );
}

#[test]
fn a_plain_item_still_inherits_the_goal_criteria_unchanged() {
    let (core, _echo, g) = engine(vec![check("job1")]);
    let item = core.add_item(g, "plain").unwrap();
    core.start(&actor_named("runner"), item).unwrap();
    assert_eq!(core.verify(item).unwrap(), [true]);
}
