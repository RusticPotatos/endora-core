//! Slice 3 item 5: a criterion's check names a FIELD of the call's structured result and the
//! exact value it must have. Text (the result string, the evidence) is for people and is never
//! checked, so an agent that prints the expected words cannot satisfy a check.

mod common;
use common::{actor_named, owner};
use std::sync::Arc;

use endora_foundation::application::{CallFailure, CallOutput, Capability, Core, NewGoal};
use endora_foundation::domain::{CallSpec, Check, Criterion, Fact, ItemId, PolicyRow};
use endora_foundation::infrastructure::ManualClock;
use endora_kernel::{Reversibility, Tier};

/// Prints whatever it was given as free text (as an agent would) and reports one real fact.
struct Runner;

impl Capability for Runner {
    fn offers(&self) -> Vec<CallSpec> {
        vec![CallSpec {
            name: "r.run".into(),
            band: Some(Reversibility::Observe),
            input: "what the agent says".into(),
            output: "exit code".into(),
            tier: Tier::Internal,
            max_cost_micros: 0,
            max_seconds: endora_foundation::domain::DEFAULT_MAX_SECONDS,
            max_turns: None,
            inputs: vec![endora_foundation::domain::Field::text("agent")],
            facts: vec![
                endora_foundation::domain::Field::int("exit"),
                endora_foundation::domain::Field::word("tests", &["pass", "fail"]),
            ],
            budget: Default::default(),
        }]
    }
    fn call(&self, _call: &str, input: &str) -> Result<CallOutput, CallFailure> {
        // The agent's words go into result and evidence; the exit code (here: non-zero unless the
        // input is "really") is a fact observed by the capability.
        let exit = i64::from(input != "really");
        Ok(CallOutput {
            cost_micros: 0,
            profile: None,
            facts: vec![Fact::int("exit", exit), Fact::word("tests", "fail")],
            result: format!("agent said: {input}"),
            evidence: format!("agent said: {input}"),
        })
    }
}

fn run_check(input: &str, field: &str, expect: &str) -> Vec<bool> {
    let mut core = Core::in_memory(Arc::new(ManualClock::new(1))).unwrap();
    core.register(Arc::new(Runner)).unwrap();
    let g = core
        .add_goal(
            &owner(&core),
            NewGoal {
                title: "t".into(),
                why: "w".into(),
                criteria: vec![Criterion {
                    text: "it exits cleanly".into(),
                    check: Some(Check {
                        call: "r.run".into(),
                        input: input.into(),
                        field: field.into(),
                        expect: expect.into(),
                    }),
                }],
                scope: vec!["r.run".into()],
                tier: Tier::Internal,
                spend_cap_micros: 0,
            },
        )
        .unwrap();
    core.grant(
        &owner(&core),
        PolicyRow {
            actor: "core".into(),
            call: "r.run".into(),
            band: Reversibility::Observe,
            tier: Tier::Internal,
            max_uses_per_item: None,
        },
    )
    .unwrap();
    let item: ItemId = core.add_item(g, "i").unwrap();
    core.start(&actor_named("runner"), item).unwrap();
    core.verify(item).unwrap()
}

#[test]
fn an_agent_printing_the_expected_text_does_not_satisfy_the_check() {
    // The agent prints exactly what the check wants; the observed exit code is 1.
    assert_eq!(run_check("exit=0 tests=pass exit 0", "exit", "0"), [false]);
    assert_eq!(run_check("tests=pass", "tests", "pass"), [false]);
}

#[test]
fn the_observed_fact_satisfies_it() {
    assert_eq!(run_check("really", "exit", "0"), [true]);
}

#[test]
fn the_match_is_exact_not_a_substring() {
    // (slice 5a: an empty expectation for a number and an undeclared field are now rejected when
    // the goal is added; contract_shapes.rs covers both, so only the exact-match cases remain)
    assert_eq!(run_check("x", "exit", "10"), [false]); // 1 is not 10
    assert_eq!(run_check("x", "exit", "1"), [true]);
}
