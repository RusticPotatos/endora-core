//! Slice 5a items 4 and 5 (REVIEW2 findings 7 and 8): a failing probe ends its item as failed and
//! the routine keeps firing until a consecutive-failure cap pauses it; a restart recovers an item
//! left running; rhythms are bounded; missed slots are one summary row; `stop` outlasts a lock.

mod common;
use common::{actor_named, owner};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use endora_foundation::application::{
    CallFailure, CallOutput, Capability, Core, NewGoal, WatchEnd,
};
use endora_foundation::domain::{
    CallSpec, Check, Criterion, Fact, Field, GoalId, ItemState, PolicyRow, RoutineId,
};
use endora_foundation::infrastructure::ManualClock;
use endora_kernel::{Reversibility, Tier};

struct Look(AtomicBool);
impl Capability for Look {
    fn offers(&self) -> Vec<CallSpec> {
        vec![CallSpec::new("p.look", Reversibility::Observe).with_facts(vec![Field::bool("up")])]
    }
    fn call(&self, _c: &str, _i: &str) -> Result<CallOutput, CallFailure> {
        let mut o = CallOutput::text("probe");
        o.facts = vec![Fact::bool("up", self.0.load(Ordering::SeqCst))];
        Ok(o)
    }
}

fn setup(path: &str, clock: &Arc<ManualClock>, up: bool) -> (Core, Arc<Look>, GoalId) {
    let look = Arc::new(Look(AtomicBool::new(up)));
    let mut core = Core::open(path, clock.clone()).unwrap();
    core.register(look.clone()).unwrap();
    let goal = core
        .add_goal(
            &owner(&core),
            NewGoal {
                title: "watch".into(),
                why: "w".into(),
                criteria: vec![Criterion {
                    text: "up".into(),
                    check: Some(Check {
                        call: "p.look".into(),
                        input: String::new(),
                        field: "up".into(),
                        expect: "true".into(),
                    }),
                }],
                scope: vec!["p.look".into()],
                tier: Tier::Internal,
                spend_cap_micros: 0,
            },
        )
        .unwrap();
    core.grant(
        &owner(&core),
        PolicyRow {
            actor: "core".into(),
            call: "p.look".into(),
            band: Reversibility::Observe,
            tier: Tier::Internal,
            max_uses_per_item: None,
        },
    )
    .unwrap();
    (core, look, goal)
}

/// One run-due pass over the watches, as the command line does it.
fn pass(core: &Core, clock: &ManualClock) -> Vec<WatchEnd> {
    clock.advance(1_000);
    core.fire_due().unwrap();
    let mut ends = Vec::new();
    for it in core.store().items_in(ItemState::Open).unwrap() {
        core.start(&actor_named("runner"), it.id).unwrap();
        ends.push(core.check_watch(it.id).unwrap().1);
    }
    ends
}

#[test]
fn a_failing_probe_ends_each_item_failed_and_the_routine_keeps_firing_until_the_cap_pauses_it() {
    let clock = Arc::new(ManualClock::new(10_000));
    let (core, look, goal) = setup(":memory:", &clock, false);
    let r = core
        .add_routine_limited(&owner(&core), goal, "look", 1_000, false, 11_000, 5)
        .unwrap();
    for n in 1..=3 {
        let ends = pass(&core, &clock);
        assert_eq!(
            ends,
            vec![WatchEnd::Failed {
                failures: n,
                paused: false
            }]
        );
    }
    let failed = core.store().items_in(ItemState::Failed).unwrap();
    assert_eq!(failed.len(), 3, "three failed items, none left running");
    assert!(
        core.store()
            .items_in(ItemState::Running)
            .unwrap()
            .is_empty()
    );
    assert!(!core.store().routine(r).unwrap().paused);
    // Two more reach the cap of 5: the routine pauses, with an evidence row.
    pass(&core, &clock);
    let last = pass(&core, &clock);
    assert_eq!(
        last,
        vec![WatchEnd::Failed {
            failures: 5,
            paused: true
        }]
    );
    assert!(core.store().routine(r).unwrap().paused);
    assert!(
        core.store()
            .evidence_tail(10)
            .unwrap()
            .iter()
            .any(|e| e.call == "core.watch" && e.output.contains("paused"))
    );
    // Paused: nothing fires, however many slots pass.
    clock.advance(60_000);
    assert!(core.fire_due().unwrap().is_empty());
    // The owner resumes it; it fires again, and a healthy run resets the count.
    core.resume_routine(&owner(&core), r).unwrap();
    look.0.store(true, Ordering::SeqCst);
    assert_eq!(pass(&core, &clock), vec![WatchEnd::Healthy]);
    assert_eq!(core.store().routine(r).unwrap().consecutive_failures, 0);
}

#[test]
fn an_item_left_running_by_a_crash_is_found_and_closed_on_restart() {
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("watch-restart-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let db = dir.join("w.db");
    let db = db.to_str().unwrap();
    let clock = Arc::new(ManualClock::new(10_000));
    let stuck = {
        let (core, _, goal) = setup(db, &clock, false);
        core.add_routine(&owner(&core), goal, "look", 1_000, false, 11_000)
            .unwrap();
        clock.advance(1_000);
        core.fire_due().unwrap();
        let it = core.store().items_in(ItemState::Open).unwrap()[0].id;
        core.start(&actor_named("runner"), it).unwrap(); // the process dies here, before any check
        it
    };
    let (core, _, _) = {
        let look = Arc::new(Look(AtomicBool::new(false)));
        let mut core = Core::open(db, clock.clone()).unwrap();
        core.register(look.clone()).unwrap();
        (core, look, ())
    };
    assert_eq!(core.stuck_watch_items().unwrap(), vec![stuck]);
    let (_, end) = core.check_watch(stuck).unwrap();
    assert!(
        matches!(end, WatchEnd::Failed { failures: 1, .. }),
        "{end:?}"
    );
    assert_eq!(core.store().item(stuck).unwrap().state, ItemState::Failed);
    assert!(core.stuck_watch_items().unwrap().is_empty());
    // The routine is not blocked any more: the next slot fires a new item.
    clock.advance(1_000);
    assert!(core.fire_due().unwrap().iter().any(|f| f.item.is_some()));
}

#[test]
fn rhythms_are_bounded_and_never_panic() {
    let clock = Arc::new(ManualClock::new(1_000));
    let (core, _, goal) = setup(":memory:", &clock, true);
    let add = |ms: i64| core.add_routine(&owner(&core), goal, "r", ms, false, 1_000);
    for bad in [0, -1, 999, i64::MAX, 366 * 86_400 * 1_000 + 1] {
        assert!(add(bad).is_err(), "{bad}");
    }
    assert!(add(1_000).is_ok());
    assert!(add(366 * 86_400 * 1_000).is_ok());
}

#[test]
fn a_routine_down_for_eleven_days_writes_one_summary_row_and_runs_once() {
    let clock = Arc::new(ManualClock::new(1_000));
    let (core, _, goal) = setup(":memory:", &clock, true);
    let r: RoutineId = core
        .add_routine(&owner(&core), goal, "r", 1_000, false, 1_000)
        .unwrap();
    clock.advance(11 * 86_400 * 1_000);
    let started = Instant::now();
    let fired = core.fire_due().unwrap();
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "{:?}",
        started.elapsed()
    );
    assert_eq!(fired.len(), 1);
    assert_eq!(fired[0].missed, 950_400, "the count is kept");
    assert!(fired[0].item.is_some(), "it runs once");
    let rows = core.store().firings(r).unwrap();
    assert!(rows.len() <= 3, "{} rows", rows.len());
    assert_eq!(rows.iter().filter(|(_, o)| o == "missed_down").count(), 1);
    assert_eq!(
        core.store().item(fired[0].item.unwrap()).unwrap().missed,
        950_400
    );
}

#[test]
fn stop_succeeds_while_another_connection_holds_the_write_lock_for_two_seconds() {
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("watch-lock-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let db = dir.join("l.db");
    let clock = Arc::new(ManualClock::new(1_000));
    let (core, _, _) = setup(db.to_str().unwrap(), &clock, true);
    let path = db.clone();
    let holder = std::thread::spawn(move || {
        let c = rusqlite::Connection::open(path).unwrap();
        c.execute_batch("BEGIN IMMEDIATE").unwrap();
        std::thread::sleep(Duration::from_secs(2));
        c.execute_batch("COMMIT").unwrap();
    });
    std::thread::sleep(Duration::from_millis(300)); // the lock is held
    let started = Instant::now();
    core.stop(&actor_named("tester"))
        .expect("stop must get through");
    assert!(
        started.elapsed() >= Duration::from_millis(1_000),
        "it did wait"
    );
    holder.join().unwrap();
    assert_eq!(core.store().control("stop").unwrap(), 1);
}
