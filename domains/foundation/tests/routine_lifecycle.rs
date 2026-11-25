//! A routine's lifecycle is the owner's: pause it, resume it, or retire it for good. Each is an
//! owner verb recorded on the evidence chain; a paused or retired routine fires nothing; a retired
//! one can never be resumed. The schema step that adds the retired flag is additive: a file at
//! schema 8 (the fixture is a real one, written by the 0.12.0 binary) opens and behaves as before.

mod common;
use common::owner;
use std::path::PathBuf;
use std::sync::Arc;

use endora_foundation::application::{Core, NewGoal};
use endora_foundation::domain::{Criterion, GoalId, RoutineId};
use endora_foundation::infrastructure::{ManualClock, SCHEMA_VERSION};
use endora_kernel::{AppError, Tier};
use rusqlite::Connection;

fn goal(core: &Core) -> GoalId {
    core.add_goal(
        &owner(core),
        NewGoal {
            title: "g".into(),
            why: "w".into(),
            criteria: vec![Criterion {
                text: "looks right".into(),
                check: None,
            }],
            scope: vec![],
            tier: Tier::Internal,
            spend_cap_micros: 0,
        },
    )
    .unwrap()
}

fn lab() -> (Core, Arc<ManualClock>, RoutineId) {
    let clock = Arc::new(ManualClock::new(10_000));
    let core = Core::open(":memory:", clock.clone()).unwrap();
    let g = goal(&core);
    let r = core
        .add_routine(&owner(&core), g, "r", 1_000, false, 11_000)
        .unwrap();
    (core, clock, r)
}

fn said(core: &Core, needle: &str) -> bool {
    core.store()
        .evidence_tail(50)
        .unwrap()
        .iter()
        .any(|e| e.output.contains(needle))
}

#[test]
fn a_paused_routine_fires_nothing_until_the_owner_resumes_it() {
    let (core, clock, r) = lab();
    core.pause_routine(&owner(&core), r).unwrap();
    assert!(core.store().routine(r).unwrap().paused);
    clock.advance(5_000);
    assert!(core.fire_due().unwrap().is_empty());
    core.resume_routine(&owner(&core), r).unwrap();
    assert!(!core.store().routine(r).unwrap().paused);
    clock.advance(1_000);
    assert_eq!(core.fire_due().unwrap().len(), 1);
}

#[test]
fn pause_resume_and_retire_are_recorded_on_the_chain_with_the_owner_as_actor() {
    let (core, _clock, r) = lab();
    core.pause_routine(&owner(&core), r).unwrap();
    core.resume_routine(&owner(&core), r).unwrap();
    core.retire_routine(&owner(&core), r).unwrap();
    let rows = core.store().evidence_all().unwrap();
    for (call, text) in [
        ("core.pause_routine", "routine 1 paused"),
        ("core.resume_routine", "routine 1 resumed"),
        ("core.retire_routine", "routine 1 retired"),
    ] {
        let row = rows
            .iter()
            .find(|e| e.call == call)
            .unwrap_or_else(|| panic!("no evidence row for {call}"));
        assert!(row.output.contains(text), "{}", row.output);
        assert_eq!(row.actor, "owner");
    }
    assert!(core.verify_evidence().unwrap().broken.is_none());
}

#[test]
fn a_retired_routine_fires_nothing_and_cannot_be_resumed_or_paused() {
    let (core, clock, r) = lab();
    core.retire_routine(&owner(&core), r).unwrap();
    assert!(core.store().routine(r).unwrap().retired);
    clock.advance(60_000);
    assert!(core.fire_due().unwrap().is_empty());
    let resume = core.resume_routine(&owner(&core), r).unwrap_err();
    assert!(
        matches!(&resume, AppError::BadRequest { message } if message.contains("retired")),
        "{resume:?}"
    );
    assert!(matches!(
        core.pause_routine(&owner(&core), r),
        Err(AppError::BadRequest { .. })
    ));
    // A refusal leaves it retired and writes no resume row.
    assert!(core.store().routine(r).unwrap().retired);
    assert!(!said(&core, "routine 1 resumed"));
    clock.advance(60_000);
    assert!(core.fire_due().unwrap().is_empty());
}

#[test]
fn retiring_a_paused_routine_works_and_an_unknown_routine_is_refused() {
    let (core, _clock, r) = lab();
    core.pause_routine(&owner(&core), r).unwrap();
    core.retire_routine(&owner(&core), r).unwrap();
    assert!(core.store().routine(r).unwrap().retired);
    for e in [
        core.pause_routine(&owner(&core), RoutineId(99)),
        core.retire_routine(&owner(&core), RoutineId(99)),
    ] {
        assert!(e.is_err());
    }
}

#[test]
fn retiring_one_routine_leaves_its_siblings_firing() {
    let (core, clock, r) = lab();
    let g = core.store().goals().unwrap()[0].id;
    let other = core
        .add_routine(&owner(&core), g, "other", 1_000, false, 11_000)
        .unwrap();
    core.retire_routine(&owner(&core), r).unwrap();
    clock.advance(1_000);
    let fired = core.fire_due().unwrap();
    assert_eq!(fired.len(), 1);
    assert_eq!(fired[0].routine, other);
}

// ---- the additive schema step, against a real schema-8 file --------------------------------

fn schema8_file(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("lifecycle-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("v8.db");
    let c = Connection::open(&p).unwrap();
    c.execute_batch(include_str!("fixtures/schema_v8.sql"))
        .unwrap();
    c.execute_batch("PRAGMA user_version = 8").unwrap();
    p
}

fn version(p: &PathBuf) -> i64 {
    Connection::open(p)
        .unwrap()
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap()
}

#[test]
fn a_schema_8_file_opens_unchanged_in_behaviour_and_gains_only_the_retired_flag() {
    let p = schema8_file("open");
    assert_eq!(version(&p), 8);
    let before = {
        let c = Connection::open(&p).unwrap();
        let rows: i64 = c
            .query_row("SELECT COUNT(*) FROM evidence", [], |r| r.get(0))
            .unwrap();
        rows
    };
    // A fixture clock just after the file was written.
    let clock = Arc::new(ManualClock::new(1_795_577_400_000));
    let core = Core::open(p.to_str().unwrap(), clock.clone()).unwrap();
    assert_eq!(version(&p), SCHEMA_VERSION);
    assert_eq!(SCHEMA_VERSION, 12);
    // Nothing that was there changed: same rows, same chain, every routine live.
    let rows = core.store().evidence_all().unwrap();
    assert_eq!(i64::try_from(rows.len()).unwrap(), before);
    assert!(core.verify_evidence().unwrap().broken.is_none());
    let routines = core.store().routines().unwrap();
    assert_eq!(routines.len(), 2);
    assert!(routines.iter().all(|r| !r.retired && !r.paused));
    assert_eq!(core.store().items().unwrap().len(), 2);
    // It behaves as before: routine 1 (every 60 s) fires again, routine 2 (hourly) does not.
    clock.advance(120_000);
    let fired = core.fire_due().unwrap();
    assert!(fired.iter().any(|f| f.routine == RoutineId(1)));
    // The new verbs work on it, and the pre-existing pause (a flag set before) is kept.
    core.pause_routine(&owner(&core), RoutineId(2)).unwrap();
    core.retire_routine(&owner(&core), RoutineId(1)).unwrap();
    drop(core);
    let again = Core::open(p.to_str().unwrap(), clock).unwrap();
    assert!(again.store().routine(RoutineId(2)).unwrap().paused);
    assert!(again.store().routine(RoutineId(1)).unwrap().retired);
    assert!(again.verify_evidence().unwrap().broken.is_none());
}

#[test]
fn a_schema_8_file_with_a_paused_routine_keeps_it_paused() {
    let p = schema8_file("paused");
    Connection::open(&p)
        .unwrap()
        .execute_batch("UPDATE routines SET paused=1, consecutive_failures=5 WHERE id=1")
        .unwrap();
    let clock = Arc::new(ManualClock::new(1_795_577_400_000 + 7_200_000));
    let core = Core::open(p.to_str().unwrap(), clock).unwrap();
    let r = core.store().routine(RoutineId(1)).unwrap();
    assert!(r.paused && !r.retired);
    assert!(
        core.fire_due()
            .unwrap()
            .iter()
            .all(|f| f.routine != RoutineId(1))
    );
    core.resume_routine(&owner(&core), RoutineId(1)).unwrap();
}
