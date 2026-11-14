//! Slice 5a item 6: the file carries `PRAGMA user_version`; older files are migrated forward in a
//! transaction, a newer file is refused untouched. The fixtures are the real `SCHEMA` text of
//! slice 2 (77042cb) and slice 4 (12cf4d1), taken from git, not a current file with a column dropped.

mod common;
use common::owner;
use std::path::PathBuf;
use std::sync::Arc;

use endora_foundation::application::Core;
use endora_foundation::domain::AcceptancePolicy;
use endora_foundation::infrastructure::{ManualClock, SCHEMA_VERSION};
use rusqlite::Connection;

fn file(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("schema-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("old.db")
}

fn version(p: &PathBuf) -> i64 {
    Connection::open(p)
        .unwrap()
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap()
}

/// A file as an older slice wrote it: its schema, a goal, an item and an evidence row.
fn old_file(name: &str, schema: &str, acceptance: Option<&str>) -> PathBuf {
    let p = file(name);
    let c = Connection::open(&p).unwrap();
    c.execute_batch(schema).unwrap();
    match acceptance {
        None => c.execute_batch("INSERT INTO goals (title, why, tier, spend_cap_cents, created_ms) VALUES ('old','w',1,0,1)").unwrap(),
        Some(a) => c.execute_batch(&format!("INSERT INTO goals (title, why, tier, spend_cap_cents, created_ms, acceptance) VALUES ('old','w',1,0,1,'{a}')")).unwrap(),
    }
    c.execute_batch(
        "INSERT INTO items (goal_id, title, state, created_ms) VALUES (1,'i','open',1);
         INSERT INTO routines (goal_id, title, every_ms, next_due_ms, auto_accept) VALUES (1,'r',60000,1,0);
         INSERT INTO firings (routine_id, due_ms, outcome) VALUES (1,1,'missed_down');
         INSERT INTO evidence (item_id, call, actor, band, decision, kind, inputs_hash, output, at_ms) VALUES (1,'x.y','a','observe','allow','result','h','old row',1)",
    )
    .unwrap();
    assert_eq!(version(&p), 0, "files from before versions are at 0");
    p
}

fn check_migrated(p: &PathBuf) {
    let core = Core::open(p.to_str().unwrap(), Arc::new(ManualClock::new(5_000))).unwrap();
    assert_eq!(version(p), SCHEMA_VERSION);
    let goals = core.store().goals().unwrap();
    assert_eq!(goals.len(), 1);
    let ev = core.store().evidence_all().unwrap();
    assert!(
        ev.iter()
            .any(|e| e.output == "old row" && e.cost_micros == 0)
    );
    let r = core
        .store()
        .routine(endora_foundation::domain::RoutineId(1))
        .unwrap();
    assert_eq!(
        (r.max_failures, r.consecutive_failures, r.paused),
        (5, 0, false)
    );
    // The new columns work: a routine can be added and the old firing row is still there.
    core.add_routine(&owner(&core), goals[0].id, "new", 1_000, false, 5_000)
        .unwrap();
    assert_eq!(core.store().firings(r.id).unwrap().len(), 1);
    assert_eq!(core.store().items().unwrap().len(), 1);
}

#[test]
fn a_slice_2_shaped_file_is_migrated_forward() {
    let p = old_file("s2", include_str!("fixtures/schema_slice2.sql"), None);
    check_migrated(&p);
    let core = Core::open(p.to_str().unwrap(), Arc::new(ManualClock::new(1))).unwrap();
    assert_eq!(
        core.store().goals().unwrap()[0].acceptance,
        AcceptancePolicy::Owner
    );
}

#[test]
fn a_slice_4_shaped_file_is_migrated_forward_and_keeps_its_policy() {
    let p = old_file(
        "s4",
        include_str!("fixtures/schema_slice4.sql"),
        Some("auto-observe"),
    );
    check_migrated(&p);
    let core = Core::open(p.to_str().unwrap(), Arc::new(ManualClock::new(1))).unwrap();
    assert_eq!(
        core.store().goals().unwrap()[0].acceptance,
        AcceptancePolicy::AutoObserve
    );
    // The policy is still fixed by the database after migration.
    let raw = Connection::open(&p).unwrap();
    assert!(
        raw.execute("UPDATE goals SET acceptance='owner'", [])
            .is_err()
    );
}

#[test]
fn a_newer_file_is_refused_and_left_untouched() {
    let p = file("newer");
    drop(Core::open(p.to_str().unwrap(), Arc::new(ManualClock::new(1))).unwrap());
    let newer = SCHEMA_VERSION + 1;
    Connection::open(&p)
        .unwrap()
        .execute_batch(&format!("PRAGMA user_version = {newer}"))
        .unwrap();
    let before = std::fs::read(&p).unwrap();
    let e = match Core::open(p.to_str().unwrap(), Arc::new(ManualClock::new(1))) {
        Ok(_) => panic!("must refuse"),
        Err(e) => e.to_string(),
    };
    assert!(e.contains("newer") && e.contains(&newer.to_string()), "{e}");
    assert_eq!(version(&p), newer);
    assert_eq!(std::fs::read(&p).unwrap(), before, "not one byte changed");
}

#[test]
fn a_current_file_opens_again_unchanged() {
    let p = file("again");
    drop(Core::open(p.to_str().unwrap(), Arc::new(ManualClock::new(1))).unwrap());
    assert_eq!(version(&p), SCHEMA_VERSION);
    drop(Core::open(p.to_str().unwrap(), Arc::new(ManualClock::new(1))).unwrap());
    assert_eq!(version(&p), SCHEMA_VERSION);
}
