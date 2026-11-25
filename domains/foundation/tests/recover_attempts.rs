//! Recovery counts only the CURRENT attempt's calls as unsettled (review of the integrated
//! branch, F1), and a lost start race is its own error (F2).
//!
//! An item that was interrupted once and whose re-run then met its criteria used to be recovered
//! AGAIN by every later pass: the attempt-1 call never gets a result, so it stayed "unsettled"
//! for ever, each pass wrote a false `core.recover` row and the third failed an item that was
//! waiting for the owner. The evidence is append-only and hash-chained, so nothing closes the
//! old call; a call counts as unsettled for recovery only when it was recorded after the item's
//! latest `core.start` row.

mod common;
use common::{actor_named, owner};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use endora_foundation::application::{CallFailure, CallOutput, Capability, Core, NewGoal};
use endora_foundation::domain::{CallSpec, Criterion, Field, ItemId, ItemState, PolicyRow};
use endora_foundation::infrastructure::ManualClock;
use endora_kernel::{AppError, Reversibility, Tier};
use rusqlite::Connection;

/// `x.read` observes and never returns: it panics inside the call, which leaves exactly what a
/// process killed inside a call leaves (an allowed intent with no result) in a chain-valid file.
struct Dies;
impl Capability for Dies {
    fn offers(&self) -> Vec<CallSpec> {
        vec![CallSpec {
            name: "x.read".into(),
            band: Some(Reversibility::Observe),
            input: "text".into(),
            output: "text".into(),
            tier: Tier::Internal,
            max_cost_micros: 0,
            max_seconds: endora_foundation::domain::DEFAULT_MAX_SECONDS,
            max_turns: None,
            inputs: vec![Field::text("target")],
            facts: vec![],
            budget: Default::default(),
        }]
    }
    fn call(&self, _c: &str, _i: &str) -> Result<CallOutput, CallFailure> {
        panic!("the process dies inside the call (test)");
    }
}

fn lab(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("attempts-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::canonicalize(&dir).unwrap().join("live.db")
}

fn open(db: &Path) -> Core {
    let mut core = Core::open(db.to_str().unwrap(), Arc::new(ManualClock::new(1))).unwrap();
    core.register(Arc::new(Dies)).unwrap();
    core
}

fn goal_and_item(core: &Core) -> ItemId {
    let g = core
        .add_goal(
            &owner(core),
            NewGoal {
                title: "t".into(),
                why: "w".into(),
                criteria: vec![Criterion {
                    text: "c".into(),
                    check: None,
                }],
                scope: vec!["x.read".into()],
                tier: Tier::Internal,
                spend_cap_micros: 0,
            },
        )
        .unwrap();
    core.grant(
        &owner(core),
        PolicyRow {
            actor: "runner".into(),
            call: "x.read".into(),
            band: Reversibility::Observe,
            tier: Tier::Internal,
            max_uses_per_item: None,
        },
    )
    .unwrap();
    core.add_item(g, "item").unwrap()
}

/// The call dies inside: an allowed intent with no result, as a killed process leaves it.
fn cut_off(core: &Core, item: ItemId) {
    let died = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = core.call(&actor_named("runner"), item, "x.read", "t");
    }));
    assert!(died.is_err(), "the call must have run and died inside");
}

fn state(core: &Core, item: ItemId) -> ItemState {
    core.store().item(item).unwrap().state
}

fn attempts(core: &Core, item: ItemId) -> u32 {
    core.store().item(item).unwrap().attempts
}

fn recover_rows(core: &Core) -> usize {
    core.store()
        .evidence_all()
        .unwrap()
        .iter()
        .filter(|e| e.call == "core.recover")
        .count()
}

/// The stuck state of the review's F1, made by the engine's own steps: attempt 1 is cut off
/// inside a call (a process dies), recovery puts the item back, attempt 2 starts, and its
/// criterion is met (the item waits for the owner). Returns the item; `a` (the dead process) is
/// gone, `b` is the live one.
fn met_after_one_interruption(db: &Path) -> (Core, ItemId) {
    let item = {
        let a = open(db);
        let item = goal_and_item(&a);
        a.start(&actor_named("runner"), item).unwrap();
        cut_off(&a, item);
        item
    };
    let b = open(db);
    let out = b.recover_interrupted().unwrap();
    assert_eq!(out.len(), 1, "the first interruption is recovered");
    assert_eq!(state(&b, item), ItemState::Open);
    b.start(&actor_named("runner"), item).unwrap();
    assert_eq!(attempts(&b, item), 2);
    b.confirm_criterion(&owner(&b), item, 0).unwrap();
    (b, item)
}

#[test]
fn a_recovered_item_that_then_meets_its_criteria_is_left_for_the_owner_by_every_later_pass() {
    let db = lab("met");
    let (b, item) = met_after_one_interruption(&db);
    assert_eq!(recover_rows(&b), 1);
    // The next run-due passes (each a fresh process or a serve tick) find nothing to recover.
    for pass in 0..5 {
        let c = open(&db);
        assert!(
            c.recover_interrupted().unwrap().is_empty(),
            "pass {pass} recovered an item that met its criteria"
        );
        assert!(b.recover_interrupted().unwrap().is_empty());
    }
    assert_eq!(recover_rows(&b), 1, "no false core.recover row");
    assert_eq!(state(&b, item), ItemState::Running);
    assert_eq!(attempts(&b, item), 2);
    // And the owner can still accept it.
    let got = b.accept(&owner(&b), item).unwrap();
    assert!(
        matches!(
            got,
            endora_foundation::application::Acceptance::Accepted { .. }
        ),
        "{got:?}"
    );
    assert_eq!(state(&b, item), ItemState::Done);
    assert!(b.verify_evidence().unwrap().broken.is_none());
}

#[test]
fn a_second_interruption_in_the_next_attempt_is_still_recovered_and_the_third_fails_the_item() {
    let db = lab("three");
    let item = {
        let a = open(&db);
        let item = goal_and_item(&a);
        a.start(&actor_named("runner"), item).unwrap();
        cut_off(&a, item);
        item
    };
    // Attempts 2 and 3 are each cut off inside a call by a process that then dies.
    for expected in [2, 3] {
        let c = open(&db);
        let out = c.recover_interrupted().unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].now, ItemState::Open, "{out:?}");
        c.start(&actor_named("runner"), item).unwrap();
        assert_eq!(attempts(&c, item), expected);
        cut_off(&c, item);
    }
    let d = open(&db);
    let out = d.recover_interrupted().unwrap();
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].now, ItemState::Failed, "{out:?}");
    assert!(
        out[0].why.contains("interrupted again after 3 attempts"),
        "{out:?}"
    );
    assert_eq!(recover_rows(&d), 3, "one row per real interruption");
    assert!(d.verify_evidence().unwrap().broken.is_none());
}

#[test]
fn a_call_cut_off_in_the_current_attempt_is_recovered_even_when_an_older_one_was_too() {
    let db = lab("both");
    let (b, item) = met_after_one_interruption(&db);
    cut_off(&b, item); // attempt 2 is cut off as well
    drop(b);
    let c = open(&db);
    let out = c.recover_interrupted().unwrap();
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].item, item);
    assert_eq!(recover_rows(&c), 2);
}

/// Old files: the stuck state is the rows the engine wrote before this change (nothing about the
/// rows differs after it, only what recovery makes of them). A schema-7 file is the same file
/// with the `holder` column dropped and `user_version` 7 (step 8 only appends that column).
#[test]
fn old_files_with_a_stuck_item_are_handled_sanely_in_schema_7_and_8() {
    for version in [8, 7] {
        let db = lab(&format!("old-v{version}"));
        let (b, item) = met_after_one_interruption(&db);
        let (rows, head) = (
            b.verify_evidence().unwrap().rows,
            b.verify_evidence().unwrap().head,
        );
        drop(b);
        if version == 7 {
            let c = Connection::open(&db).unwrap();
            c.execute_batch("ALTER TABLE items DROP COLUMN holder; PRAGMA user_version = 7;")
                .unwrap();
        }
        let core = open(&db);
        let v: i64 = Connection::open(&db)
            .unwrap()
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(v, 9);
        // The old unsettled call of attempt 1 is ignored: the item that met its criteria waits.
        for _ in 0..3 {
            assert!(core.recover_interrupted().unwrap().is_empty(), "v{version}");
        }
        assert_eq!(state(&core, item), ItemState::Running, "v{version}");
        assert_eq!(attempts(&core, item), 2);
        assert_eq!(recover_rows(&core), 1);
        // No row was added or changed by opening and recovering: same rows, same head.
        let report = core.verify_evidence().unwrap();
        assert!(report.broken.is_none(), "{report:?}");
        assert_eq!((report.rows, report.head), (rows, head), "v{version}");
        // A call cut off now is still recovered, and the owner can accept what was waiting.
        let waiting = goal_and_item(&core);
        core.start(&actor_named("runner"), waiting).unwrap();
        cut_off(&core, waiting);
        drop(core);
        let core = open(&db);
        let out = core.recover_interrupted().unwrap();
        assert_eq!(out.len(), 1, "v{version}");
        assert_eq!(out[0].item, waiting);
        core.accept(&owner(&core), item).unwrap();
        assert_eq!(state(&core, item), ItemState::Done);
    }
}

#[test]
fn a_lost_start_race_is_a_conflict_not_corrupt_data_and_writes_nothing() {
    let db = lab("race");
    let a = open(&db);
    let item = goal_and_item(&a);
    // B listed the item as open; A starts it first; then B's start loses.
    let b = open(&db);
    assert_eq!(b.store().item(item).unwrap().state, ItemState::Open);
    a.start(&actor_named("runner"), item).unwrap();
    let rows = a.verify_evidence().unwrap().rows;
    let err = b.start(&actor_named("runner"), item).unwrap_err();
    match &err {
        AppError::Conflict { code, message } => {
            assert_eq!(*code, "item-taken");
            assert!(message.contains("another process"), "{message}");
            assert!(!message.contains("corrupt"), "{message}");
        }
        other => panic!("not a Conflict: {other:?}"),
    }
    assert_eq!(
        a.verify_evidence().unwrap().rows,
        rows,
        "the loser wrote nothing"
    );
    assert_eq!(attempts(&a, item), 1, "the item ran once");
    assert_eq!(state(&a, item), ItemState::Running);
}

#[test]
fn starting_a_finished_item_is_still_refused() {
    let db = lab("finished");
    let a = open(&db);
    let item = goal_and_item(&a);
    a.start(&actor_named("runner"), item).unwrap();
    a.confirm_criterion(&owner(&a), item, 0).unwrap();
    a.accept(&owner(&a), item).unwrap();
    assert!(a.start(&actor_named("runner"), item).is_err());
}
