//! Slice 3 item 3: a call declares a MAXIMUM cost and returns an ACTUAL cost. Core checks the
//! maximum against the remaining caps before the call, records the actual cost in the evidence,
//! and treats an actual above the maximum as a violation that stops the goal.

mod common;
use common::{actor_named, owner};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use endora_foundation::application::{CallFailure, CallOutput, Capability, Core, NewGoal, Outcome};
use endora_foundation::domain::{CallSpec, Criterion, Deny, ItemId, ItemState, Kind, PolicyRow};
use endora_foundation::infrastructure::ManualClock;
use endora_kernel::{Reversibility, Tier};

struct Paid {
    max: u64,
    actual: u64,
    hits: AtomicU32,
}

impl Capability for Paid {
    fn offers(&self) -> Vec<CallSpec> {
        vec![CallSpec {
            name: "x.paid".into(),
            band: Some(Reversibility::Reversible),
            input: String::new(),
            output: String::new(),
            tier: Tier::Internal,
            max_cost_micros: self.max,
            max_seconds: endora_foundation::domain::DEFAULT_MAX_SECONDS,
            max_turns: None,
            inputs: Vec::new(),
            facts: Vec::new(),
            budget: endora_foundation::domain::BudgetMode::Enforced { units: u64::MAX },
        }]
    }
    fn call(&self, _call: &str, _input: &str) -> Result<CallOutput, CallFailure> {
        self.hits.fetch_add(1, Ordering::SeqCst);
        Ok(CallOutput {
            cost_micros: self.actual,
            profile: None,
            facts: Vec::new(),
            result: "ok".into(),
            evidence: String::new(),
        })
    }
}

fn setup(max: u64, actual: u64, goal_cap: u64, overall: u64) -> (Core, Arc<Paid>, ItemId) {
    let paid = Arc::new(Paid {
        max,
        actual,
        hits: AtomicU32::new(0),
    });
    let mut core = Core::in_memory(Arc::new(ManualClock::new(1))).unwrap();
    core.register(paid.clone()).unwrap();
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
    (core, paid, item)
}

fn run(core: &Core, item: ItemId) -> Outcome {
    core.call(&actor_named("p"), item, "x.paid", "").unwrap()
}

#[test]
fn under_the_cap_is_allowed_and_the_actual_cost_is_recorded_and_drives_the_remainder() {
    // Declared maximum 5, actual 2, caps 10: four calls fit if only the actual counts
    // (2+2+2=6, then 6+5 > 10 refuses the fourth on its maximum).
    let (core, paid, item) = setup(5, 2, 10, 10);
    for _ in 0..3 {
        assert!(matches!(run(&core, item), Outcome::Done(_)));
    }
    assert!(matches!(run(&core, item), Outcome::Refused(Deny::SpendCap)));
    assert_eq!(paid.hits.load(Ordering::SeqCst), 3);
    let costs: Vec<u64> = core
        .store()
        .evidence_for(item)
        .unwrap()
        .iter()
        .filter(|e| e.kind == Kind::Result)
        .map(|e| e.cost_micros)
        .collect();
    assert_eq!(costs, [2, 2, 2]);
}

#[test]
fn a_maximum_that_would_pass_either_cap_is_refused_before_the_call() {
    let (core, paid, item) = setup(5, 1, 4, 100);
    assert!(matches!(run(&core, item), Outcome::Refused(Deny::SpendCap)));
    let (core2, paid2, item2) = setup(5, 1, 100, 4);
    assert!(matches!(
        run(&core2, item2),
        Outcome::Refused(Deny::SpendCap)
    ));
    assert_eq!(
        paid.hits.load(Ordering::SeqCst) + paid2.hits.load(Ordering::SeqCst),
        0
    );
}

#[test]
fn an_actual_cost_above_the_maximum_is_a_violation_recorded_and_it_stops_the_goal() {
    let (core, paid, item) = setup(5, 9, 100, 100);
    let out = run(&core, item);
    assert!(
        matches!(out, Outcome::Violation { max: 5, actual: 9 }),
        "{out:?}"
    );
    let ev = core.store().evidence_for(item).unwrap();
    let result = ev.iter().find(|e| e.kind == Kind::Result).unwrap();
    // Slice 7 F2: the charged cost is clamped to the declared maximum; the raw report is kept in
    // the text and in the violation row.
    assert_eq!(
        result.cost_micros, 5,
        "the charge never exceeds the maximum"
    );
    assert!(result.output.contains("reported 9"), "{}", result.output);
    let v = ev
        .iter()
        .find(|e| e.call == "core.violation")
        .expect("a violation record");
    assert!(
        v.output.contains("9") && v.output.contains("5"),
        "{}",
        v.output
    );
    // The item failed, and the goal takes no further call, even from a fresh item.
    assert_eq!(core.store().item(item).unwrap().state, ItemState::Failed);
    let again = core
        .add_item(core.store().item(item).unwrap().goal, "j")
        .unwrap();
    core.start(&actor_named("runner"), again).unwrap();
    assert!(matches!(
        run(&core, again),
        Outcome::Refused(Deny::GoalStopped)
    ));
    assert_eq!(paid.hits.load(Ordering::SeqCst), 1);
    // Only the owner lifts it.
    let goal = core.store().item(item).unwrap().goal;
    // (A non-owner cannot resume: `resume_goal` takes an OwnerProof, so it cannot compile.)
    core.resume_goal(&owner(&core), goal).unwrap();
    assert!(matches!(run(&core, again), Outcome::Violation { .. }));
}

#[test]
fn a_zero_cap_refuses_every_call_with_a_nonzero_maximum_and_allows_a_free_one() {
    let (core, paid, item) = setup(1, 0, 0, 0);
    assert!(matches!(run(&core, item), Outcome::Refused(Deny::SpendCap)));
    assert_eq!(paid.hits.load(Ordering::SeqCst), 0);
    let (core, paid, item) = setup(0, 0, 0, 0);
    assert!(matches!(run(&core, item), Outcome::Done(_)));
    assert_eq!(paid.hits.load(Ordering::SeqCst), 1);
}

#[test]
fn a_store_made_before_the_cost_column_opens_and_gains_it() {
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("spend-mig-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("old.db");
    let p = path.to_str().unwrap();
    {
        let core = Core::open(p, Arc::new(ManualClock::new(1))).unwrap();
        core.add_goal(
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
        core.store()
            .db()
            .lock()
            .unwrap()
            // A real pre-cost file has no hash columns either (slice 7 F3).
            .execute_batch(
                "ALTER TABLE evidence DROP COLUMN hash; ALTER TABLE evidence DROP COLUMN prev_hash;
                 ALTER TABLE evidence DROP COLUMN cost_micros; ALTER TABLE evidence DROP COLUMN profile;
                 ALTER TABLE evidence DROP COLUMN cost_cents;
                 ALTER TABLE goals RENAME COLUMN spend_cap_micros TO spend_cap_cents; PRAGMA user_version = 0;",
            )
            .unwrap();
    }
    let core = Core::open(p, Arc::new(ManualClock::new(1))).unwrap();
    let ev = core.store().evidence_all().unwrap();
    // Two rows: the owner enrolment (the test helper enrols) and the goal.
    assert_eq!(ev.len(), 2);
    assert!(ev.iter().all(|e| e.cost_micros == 0));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_reported_cost_of_u64_max_is_charged_the_maximum_and_blocks_nothing_else() {
    // REVIEW3 finding 5: u64::MAX used to saturate to i64::MAX and refuse every paid call on
    // every goal for ever. Now: the maximum is charged, a violation row names the raw report,
    // and a call on another goal still runs.
    let (core, _paid, item) = setup(5, u64::MAX, 100, 100);
    let out = run(&core, item);
    assert!(
        matches!(
            out,
            Outcome::Violation {
                max: 5,
                actual: u64::MAX
            }
        ),
        "{out:?}"
    );
    let ev = core.store().evidence_for(item).unwrap();
    let result = ev.iter().find(|e| e.kind == Kind::Result).unwrap();
    assert_eq!(result.cost_micros, 5);
    let v = ev.iter().find(|e| e.call == "core.violation").unwrap();
    assert!(v.output.contains(&u64::MAX.to_string()), "{}", v.output);
    assert_eq!(core.store().spent_micros(None).unwrap(), 5);
    // Another goal is not blocked (the first goal stays stopped until the owner resumes it).
    let g2 = core
        .add_goal(
            &owner(&core),
            NewGoal {
                title: "other".into(),
                why: "w".into(),
                criteria: vec![Criterion {
                    text: "c".into(),
                    check: None,
                }],
                scope: vec!["x.paid".into()],
                tier: Tier::Internal,
                spend_cap_micros: 100,
            },
        )
        .unwrap();
    let item2 = core.add_item(g2, "i").unwrap();
    core.start(&actor_named("runner"), item2).unwrap();
    // The second call also reports u64::MAX (the stand-in always does): it is a violation again,
    // not a spend-cap refusal: the cap arithmetic still works.
    assert!(matches!(run(&core, item2), Outcome::Violation { .. }));
    assert_eq!(core.store().spent_micros(None).unwrap(), 10);
}
