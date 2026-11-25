//! Schema 12 is additive (ADR 0153): a real schema-11 file gains the `diagnoses` table, empty,
//! and keeps everything it had (rows, the evidence chain, its notices).

mod common;
use std::sync::Arc;

use endora_foundation::application::Core;
use endora_foundation::infrastructure::{ManualClock, SCHEMA_VERSION};
use rusqlite::Connection;

fn version(p: &std::path::Path) -> i64 {
    Connection::open(p)
        .unwrap()
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap()
}

#[test]
fn a_schema_11_file_gains_the_diagnoses_table_and_keeps_its_state() {
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("diagnosis-v11-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("v11.db");
    let c = Connection::open(&p).unwrap();
    c.execute_batch(include_str!("fixtures/schema_v11.sql"))
        .unwrap();
    c.execute_batch("PRAGMA user_version = 11").unwrap();
    let tables = |c: &Connection| -> i64 {
        c.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='diagnoses'",
            [],
            |r| r.get(0),
        )
        .unwrap()
    };
    assert_eq!(tables(&c), 0);
    let before: i64 = c
        .query_row("SELECT COUNT(*) FROM notices", [], |r| r.get(0))
        .unwrap();
    drop(c);
    assert_eq!(version(&p), 11);
    let core = Core::open(
        p.to_str().unwrap(),
        Arc::new(ManualClock::new(1_795_583_300_000)),
    )
    .unwrap();
    assert_eq!((version(&p), SCHEMA_VERSION), (12, 12));
    assert!(core.verify_evidence().unwrap().broken.is_none());
    drop(core);
    let c = Connection::open(&p).unwrap();
    assert_eq!(tables(&c), 1);
    let n: i64 = c
        .query_row("SELECT COUNT(*) FROM diagnoses", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 0, "starts empty");
    let after: i64 = c
        .query_row("SELECT COUNT(*) FROM notices", [], |r| r.get(0))
        .unwrap();
    assert_eq!(before, after, "no existing row touched");
}
