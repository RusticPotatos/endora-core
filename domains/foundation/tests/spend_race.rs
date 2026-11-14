//! Slice 7 F1 (REVIEW3 finding 1, owner requirement R2): several processes (here: connections,
//! each with its own `Core`) sharing one database file must not overspend a cap. The spend check
//! and the call's intent are one immediate transaction, so a second connection sees the first's
//! reservation. Double firing of a routine slot stays impossible under the same contention.

mod common;
use common::{actor_named, owner};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Barrier};

use endora_foundation::application::{CallFailure, CallOutput, Capability, Core, NewGoal, Outcome};
use endora_foundation::domain::{BudgetMode, CallSpec, Criterion, Deny, ItemState, PolicyRow};
use endora_foundation::infrastructure::SystemClock;
use endora_kernel::{Reversibility, Tier};

const PROCS: usize = 6;
const TRIALS: usize = 12;

struct Paid {
    max: u64,
    hits: Arc<AtomicU32>,
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
            budget: BudgetMode::Enforced { units: u64::MAX },
        }]
    }
    fn call(&self, _call: &str, _input: &str) -> Result<CallOutput, CallFailure> {
        self.hits.fetch_add(1, Ordering::SeqCst);
        // Stay inside the call long enough for the others to pass their own checks.
        std::thread::sleep(std::time::Duration::from_millis(30));
        Ok(CallOutput {
            cost_micros: self.max,
            profile: None,
            facts: Vec::new(),
            result: "ok".into(),
            evidence: String::new(),
        })
    }
}

fn open_core(path: &str, hits: &Arc<AtomicU32>) -> Core {
    let mut core = Core::open(path, Arc::new(SystemClock)).unwrap();
    core.register(Arc::new(Paid {
        max: 60,
        hits: hits.clone(),
    }))
    .unwrap();
    core
}

/// One database file with `goals` goals (each capped at `goal_cap`), the overall cap, one item per
/// goal. Returns the item ids.
fn prepare(
    path: &str,
    goal_cap: u64,
    overall: u64,
    goals: usize,
) -> Vec<endora_foundation::domain::ItemId> {
    let hits = Arc::new(AtomicU32::new(0));
    let core = open_core(path, &hits);
    core.set_overall_cap(&owner(&core), overall).unwrap();
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
    (0..goals)
        .map(|n| {
            let g = core
                .add_goal(
                    &owner(&core),
                    NewGoal {
                        title: format!("t{n}"),
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
            let item = core.add_item(g, "i").unwrap();
            core.start(&actor_named("runner"), item).unwrap();
            item
        })
        .collect()
}

/// `PROCS` connections each try one call (max 60c) at the same moment; item `n % items.len()`.
fn storm(path: &str, items: &[endora_foundation::domain::ItemId]) -> (u32, usize) {
    let hits = Arc::new(AtomicU32::new(0));
    let barrier = Arc::new(Barrier::new(PROCS));
    let handles: Vec<_> = (0..PROCS)
        .map(|n| {
            let (path, hits, barrier) = (path.to_owned(), hits.clone(), barrier.clone());
            let item = items[n % items.len()];
            std::thread::spawn(move || {
                let core = open_core(&path, &hits);
                barrier.wait();
                core.call(&actor_named("p"), item, "x.paid", "").unwrap()
            })
        })
        .collect();
    let outcomes: Vec<Outcome> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    let refused = outcomes
        .iter()
        .filter(|o| matches!(o, Outcome::Refused(Deny::SpendCap)))
        .count();
    (hits.load(Ordering::SeqCst), refused)
}

fn tmp(tag: &str, n: usize) -> String {
    let dir = std::env::temp_dir().join(format!("spend-race-{}-{tag}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("core.db").to_string_lossy().into_owned()
}

#[test]
fn concurrent_calls_cannot_overspend_a_goal_cap() {
    for n in 0..TRIALS {
        let path = tmp("goal", n);
        let items = prepare(&path, 100, 10_000, 1);
        let (ran, refused) = storm(&path, &items);
        assert_eq!(
            ran, 1,
            "trial {n}: {ran} calls ran against a 100c cap at 60c each"
        );
        assert_eq!(refused, PROCS - 1, "trial {n}");
    }
}

#[test]
fn concurrent_calls_cannot_overspend_the_overall_cap() {
    for n in 0..TRIALS {
        let path = tmp("overall", n);
        // Every call is on its own goal (each goal's cap alone would allow it): only the
        // overall cap of 100c stands in the way.
        let items = prepare(&path, 10_000, 100, PROCS);
        let (ran, refused) = storm(&path, &items);
        assert_eq!(
            ran, 1,
            "trial {n}: {ran} calls ran against a 100c overall cap"
        );
        assert_eq!(refused, PROCS - 1, "trial {n}");
    }
}

#[test]
fn the_reserved_maximum_counts_until_the_actual_cost_replaces_it() {
    // One call at a time through two connections: the first's actual cost (60) replaces its
    // reservation, so a second call of max 60 is refused (60 + 60 > 100) and one of the
    // remaining 40c would be too; the cap is never passed.
    let path = tmp("seq", 0);
    let items = prepare(&path, 100, 10_000, 1);
    let hits = Arc::new(AtomicU32::new(0));
    let a = open_core(&path, &hits);
    let b = open_core(&path, &hits);
    assert!(matches!(
        a.call(&actor_named("p"), items[0], "x.paid", "").unwrap(),
        Outcome::Done(_)
    ));
    assert!(matches!(
        b.call(&actor_named("p"), items[0], "x.paid", "").unwrap(),
        Outcome::Refused(Deny::SpendCap)
    ));
    assert_eq!(hits.load(Ordering::SeqCst), 1);
}

#[test]
fn concurrent_fire_due_never_fires_one_slot_twice() {
    for n in 0..TRIALS {
        let path = tmp("fire", n);
        let items = prepare(&path, 100, 10_000, 1);
        let hits = Arc::new(AtomicU32::new(0));
        let core = open_core(&path, &hits);
        let goal = core.store().item(items[0]).unwrap().goal;
        // Due immediately, far in the past of the system clock: one slot due (the ones before it
        // are missed and summarised).
        let r = core
            .add_routine(&owner(&core), goal, "r", 86_400_000, false, core_now_ms())
            .unwrap();
        let barrier = Arc::new(Barrier::new(PROCS));
        let handles: Vec<_> = (0..PROCS)
            .map(|_| {
                let (path, hits, barrier) = (path.clone(), hits.clone(), barrier.clone());
                std::thread::spawn(move || {
                    let core = open_core(&path, &hits);
                    barrier.wait();
                    // The loser may get an error (the slot is taken); it must never get a second item.
                    core.fire_due().unwrap_or_default()
                })
            })
            .collect();
        let made: usize = handles
            .into_iter()
            .map(|h| h.join().unwrap())
            .flat_map(|f| f.into_iter().filter_map(|x| x.item))
            .count();
        assert_eq!(made, 1, "trial {n}: {made} items for one slot");
        let fired = core
            .store()
            .firings(r)
            .unwrap()
            .iter()
            .filter(|(_, o)| o == "fired")
            .count();
        assert_eq!(fired, 1, "trial {n}");
        let open = core.store().items_in(ItemState::Open).unwrap().len();
        assert_eq!(open, 1, "trial {n}");
    }
}

fn core_now_ms() -> i64 {
    let d = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap();
    i64::try_from(d.as_millis()).unwrap()
}
