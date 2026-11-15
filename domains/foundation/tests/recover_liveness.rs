//! Recovery must not settle an item a LIVE process is running (weekly review B, WF-02).
//!
//! A process that starts an item holds an exclusive `flock` on a file in `<db>.holders/` for as
//! long as it lives and records that file's name on the item. Recovery settles a running item
//! only when it can take that lock (the holder is dead); an item without a holder (written by an
//! older build) is recovered as before. Several `Core`s on one file in one process stand in for
//! several processes here: each has its own holder, and `flock` locks conflict between open file
//! descriptions even inside one process. The two-process runs are in `app/foundation/tests`.

mod common;
use common::{actor_named, owner};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use endora_foundation::application::{CallFailure, CallOutput, Capability, Core, NewGoal};
use endora_foundation::domain::{CallSpec, Criterion, ItemId, ItemState};
use endora_foundation::infrastructure::{ManualClock, SCHEMA_VERSION};
use endora_kernel::Tier;
use rusqlite::Connection;

struct NoCalls;
impl Capability for NoCalls {
    fn offers(&self) -> Vec<CallSpec> {
        Vec::new()
    }
    fn call(&self, _c: &str, _i: &str) -> Result<CallOutput, CallFailure> {
        Err(CallFailure::new("none"))
    }
}

fn lab(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("liveness-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::canonicalize(&dir).unwrap().join("live.db")
}

fn open(db: &Path) -> Core {
    let mut core = Core::open(db.to_str().unwrap(), Arc::new(ManualClock::new(1))).unwrap();
    core.register(Arc::new(NoCalls)).unwrap();
    core
}

fn holders(db: &Path) -> PathBuf {
    let mut s = db.as_os_str().to_owned();
    s.push(".holders");
    PathBuf::from(s)
}

/// A goal and one running item that is cut off inside an observe-band call (an allowed intent
/// with no result, written the way an older build or a killed process leaves it).
fn running_cut_off(core: &Core) -> ItemId {
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
                scope: vec![],
                tier: Tier::Internal,
                spend_cap_micros: 0,
            },
        )
        .unwrap();
    let item = core.add_item(g, "cut off").unwrap();
    core.start(&actor_named("runner"), item).unwrap();
    cut_off(core, item);
    item
}

fn cut_off(core: &Core, item: ItemId) {
    core.store()
        .db()
        .lock()
        .unwrap()
        .execute(
            "INSERT INTO evidence (item_id, call, actor, band, decision, kind, inputs_hash, output, at_ms) VALUES (?1,'x.read','core','observe','allow','intent','h','',1)",
            [item.0],
        )
        .unwrap();
}

fn state(core: &Core, item: ItemId) -> ItemState {
    core.store().item(item).unwrap().state
}

fn recover_rows(core: &Core) -> usize {
    core.store()
        .evidence_all()
        .unwrap()
        .iter()
        .filter(|e| e.call == "core.recover")
        .count()
}

fn set_holder(core: &Core, item: ItemId, holder: Option<&str>) {
    core.store()
        .db()
        .lock()
        .unwrap()
        .execute(
            "UPDATE items SET holder = ?1 WHERE id = ?2",
            rusqlite::params![holder, item.0],
        )
        .unwrap();
}

#[test]
fn a_live_holder_keeps_its_item_from_recovery_and_a_dead_one_does_not() {
    let db = lab("live-dead");
    let a = open(&db);
    let item = running_cut_off(&a);
    let b = open(&db);
    // A (a live process) holds the item: B leaves it alone, writes nothing, settles nothing.
    assert!(b.recover_interrupted().unwrap().is_empty());
    assert!(b.recover().unwrap().is_empty());
    assert_eq!(state(&b, item), ItemState::Running);
    assert_eq!(recover_rows(&b), 0, "no false core.recover row");
    // A dies (its descriptors close, which is what the OS does for a kill -9): B recovers it.
    drop(a);
    let out = b.recover_interrupted().unwrap();
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].item, item);
    assert_eq!(state(&b, item), ItemState::Open);
    assert_eq!(recover_rows(&b), 1);
}

#[test]
fn the_process_that_started_the_item_may_recover_its_own_leftovers() {
    // `serve` calls recover on every tick; an item its own earlier tick left behind is its own.
    let db = lab("own");
    let a = open(&db);
    let item = running_cut_off(&a);
    assert_eq!(a.recover_interrupted().unwrap().len(), 1);
    assert_eq!(state(&a, item), ItemState::Open);
}

#[test]
fn an_item_without_a_holder_is_recovered_as_before() {
    let db = lab("legacy");
    let a = open(&db);
    let item = running_cut_off(&a);
    set_holder(&a, item, None);
    let b = open(&db); // a is still alive, but the row names no holder: today's behaviour
    assert_eq!(b.recover_interrupted().unwrap().len(), 1);
    assert_eq!(state(&b, item), ItemState::Open);
}

#[test]
fn a_reused_pid_does_not_keep_a_dead_holders_item_alive() {
    // The holder file is named `<pid>-<time>`, but only the LOCK says whether it lives. A new
    // process that has the dead holder's pid owns a different file; the dead file stays free.
    let db = lab("pid-reuse");
    let a = open(&db);
    let item = running_cut_off(&a);
    let dead = format!("{}-1", std::process::id());
    std::fs::write(holders(&db).join(&dead), b"").unwrap();
    set_holder(&a, item, Some(&dead));
    let b = open(&db);
    assert_eq!(b.recover_interrupted().unwrap().len(), 1);
    assert_eq!(state(&b, item), ItemState::Open);
    assert!(
        !holders(&db).join(&dead).exists(),
        "the dead holder's file is cleaned up"
    );
}

#[test]
fn a_missing_holder_file_means_a_dead_holder() {
    let db = lab("missing");
    let a = open(&db);
    let item = running_cut_off(&a);
    set_holder(&a, item, Some("99999-2"));
    let b = open(&db);
    assert_eq!(b.recover_interrupted().unwrap().len(), 1);
}

#[test]
fn a_holder_name_that_is_not_a_plain_file_name_is_never_opened_and_leaves_the_item_alone() {
    let db = lab("badname");
    let a = open(&db);
    let item = running_cut_off(&a);
    let outside = db.parent().unwrap().join("outside");
    std::fs::write(&outside, b"x").unwrap();
    for bad in ["../outside", "/etc/passwd", "a/b", "", ".."] {
        set_holder(&a, item, Some(bad));
        let b = open(&db);
        assert!(
            b.recover_interrupted().unwrap().is_empty(),
            "an unreadable holder is treated as alive: {bad:?}"
        );
        assert_eq!(state(&b, item), ItemState::Running);
    }
    assert!(
        outside.exists(),
        "nothing outside the holders directory was touched"
    );
}

#[test]
fn the_stuck_watch_check_also_leaves_a_live_holders_item_alone() {
    let db = lab("watch");
    let a = open(&db);
    // A routine item whose check is not met yet and which a (live) process is running. The
    // check is written straight into the criteria row (no capability declares `x.read`).
    let g = a
        .add_goal(
            &owner(&a),
            NewGoal {
                title: "w".into(),
                why: "w".into(),
                criteria: vec![Criterion {
                    text: "up".into(),
                    check: None,
                }],
                scope: vec![],
                tier: Tier::Internal,
                spend_cap_micros: 0,
            },
        )
        .unwrap();
    a.add_routine(&owner(&a), g, "r", 1_000, false, 1).unwrap();
    let item = a.fire_due().unwrap()[0].item.unwrap();
    a.start(&actor_named("runner"), item).unwrap();
    a.store()
        .db()
        .lock()
        .unwrap()
        .execute(
            "UPDATE criteria SET check_call='x.read', check_input='t', check_field='up', check_expect='true' WHERE owner_kind='goal' AND owner_id=?1",
            [g.0],
        )
        .unwrap();
    let b = open(&db);
    assert!(b.stuck_watch_items().unwrap().is_empty(), "A is alive");
    drop(a);
    assert_eq!(b.stuck_watch_items().unwrap(), vec![item], "A is dead");
}

#[test]
fn an_item_leaving_running_gives_up_its_holder() {
    let db = lab("cleared");
    let a = open(&db);
    let item = running_cut_off(&a);
    let held = |c: &Core| -> Option<String> {
        c.store()
            .db()
            .lock()
            .unwrap()
            .query_row("SELECT holder FROM items WHERE id=?1", [item.0], |r| {
                r.get(0)
            })
            .unwrap()
    };
    assert!(held(&a).is_some(), "a started item names its holder");
    a.recover_interrupted().unwrap();
    assert_eq!(held(&a), None);
}

/// A file as the build before this change wrote it (schema version 7): made by the current
/// engine (a goal, a started item, evidence rows chained by the unchanged evidence code), then
/// returned to the version 7 shape. Step 8 only appends `items.holder`, so dropping that column
/// gives the version 7 `items` table exactly; every other table is untouched by the step.
fn v7_file(name: &str) -> (PathBuf, ItemId) {
    let db = lab(name);
    let item = {
        let core = open(&db);
        let g = core
            .add_goal(
                &owner(&core),
                NewGoal {
                    title: "old".into(),
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
        let item = core.add_item(g, "old item").unwrap();
        core.start(&actor_named("runner"), item).unwrap();
        item
    };
    let c = Connection::open(&db).unwrap();
    c.execute_batch("ALTER TABLE items DROP COLUMN holder; PRAGMA user_version = 7;")
        .unwrap();
    let v: i64 = c
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(v, 7);
    let has_holder: i64 = c
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('items') WHERE name='holder'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        has_holder, 0,
        "a version 7 items table has no holder column"
    );
    (db, item)
}

#[test]
fn a_version_7_file_migrates_by_one_nullable_column_and_its_chain_is_untouched() {
    let (db, item) = v7_file("v7");
    let (rows, head): (i64, String) = Connection::open(&db)
        .unwrap()
        .query_row(
            "SELECT COUNT(*), (SELECT hash FROM evidence ORDER BY id DESC LIMIT 1) FROM evidence",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert!(rows >= 3, "the old file has chained rows: {rows}");
    let core = open(&db);
    let c = Connection::open(&db).unwrap();
    let v: i64 = c
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(v, SCHEMA_VERSION);
    assert_eq!(SCHEMA_VERSION, 8);
    let col: (String, i64) = c
        .query_row(
            "SELECT type, \"notnull\" FROM pragma_table_info('items') WHERE name='holder'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(col, ("TEXT".to_owned(), 0), "a nullable TEXT column");
    let holder: Option<String> = c
        .query_row("SELECT holder FROM items WHERE id=?1", [item.0], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(holder, None, "an old item names no holder");
    // The evidence chain is exactly what the old build wrote: same rows, same head, intact.
    let report = core.verify_evidence().unwrap();
    assert!(report.broken.is_none(), "{report:?}");
    assert_eq!(report.rows, usize::try_from(rows).unwrap());
    assert_eq!(report.head, head);
    // The old item, running with an unsettled call and no holder, is recovered as before.
    cut_off(&core, item);
    let out = core.recover_interrupted().unwrap();
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].item, item);
    assert_eq!(state(&core, item), ItemState::Open);
}

// ---- round 2: URI paths, stale holder files, a broken holders directory --------------------------

#[test]
fn a_sqlite_uri_path_is_refused_not_opened_with_the_lease_off() {
    let db = lab("uri");
    for uri in [
        format!("file:{}", db.display()),
        format!("file://{}?mode=rwc", db.display()),
        "file::memory:".to_owned(),
        "file:x.db?cache=shared".to_owned(),
    ] {
        let err = Core::open(&uri, Arc::new(ManualClock::new(1)))
            .err()
            .unwrap_or_else(|| panic!("{uri} was opened"));
        let said = err.to_string();
        assert!(said.contains("URI"), "{uri}: {said}");
    }
    assert!(!db.exists(), "a refused path created nothing");
    // The plain path and the in-memory name still open.
    drop(open(&db));
    assert!(Core::in_memory(Arc::new(ManualClock::new(1))).is_ok());
}

#[test]
fn two_spellings_of_one_plain_path_share_one_holder_directory() {
    let db = lab("spellings");
    let a = open(&db);
    let item = running_cut_off(&a);
    let dotted = db.parent().unwrap().join(".").join("live.db");
    let b = Core::open(dotted.to_str().unwrap(), Arc::new(ManualClock::new(1))).unwrap();
    assert!(
        b.recover_interrupted().unwrap().is_empty(),
        "a live holder is seen through another spelling"
    );
    assert_eq!(state(&b, item), ItemState::Running);
}

#[test]
fn a_new_holder_sweeps_free_files_no_item_names_and_keeps_the_rest() {
    let db = lab("sweep");
    let a = open(&db);
    let named = running_cut_off(&a);
    let dir = holders(&db);
    let stale = "111-1";
    let still_named = "222-2";
    std::fs::write(dir.join(stale), b"").unwrap();
    std::fs::write(dir.join(still_named), b"").unwrap();
    set_holder(&a, named, Some(still_named));
    // A second live process: its file is locked, so it is never swept.
    let live = open(&db);
    let g2 = live
        .add_goal(
            &owner(&live),
            NewGoal {
                title: "t2".into(),
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
    let i2 = live.add_item(g2, "second").unwrap();
    live.start(&actor_named("runner"), i2).unwrap();
    let live_file = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n != stale && n != still_named)
        .count();
    assert!(live_file >= 2, "two live holders have files");
    // A third process starts an item: the sweep runs once, as it makes its own file.
    let c = open(&db);
    let g3 = c
        .add_goal(
            &owner(&c),
            NewGoal {
                title: "t3".into(),
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
    let i3 = c.add_item(g3, "third").unwrap();
    c.start(&actor_named("runner"), i3).unwrap();
    assert!(!dir.join(stale).exists(), "a free, unnamed file is removed");
    assert!(
        dir.join(still_named).exists(),
        "a free file a running item still names is left for recovery"
    );
    let after = std::fs::read_dir(&dir).unwrap().flatten().count();
    assert_eq!(
        after,
        live_file + 2,
        "the two live holders' files, the new one's own and the still-named one remain"
    );
    // Recovery still works through the named file, and removes it.
    drop(a);
    assert_eq!(c.recover_interrupted().unwrap().len(), 1);
    assert!(!dir.join(still_named).exists());
}

#[test]
fn an_unusable_holders_directory_fails_closed_and_the_error_names_the_path() {
    let db = lab("broken-dir");
    let a = open(&db);
    let g = a
        .add_goal(
            &owner(&a),
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
    let item = a.add_item(g, "i").unwrap();
    std::fs::write(holders(&db), b"a file where the directory should be").unwrap();
    let err = a
        .start(&actor_named("runner"), item)
        .unwrap_err()
        .to_string();
    assert!(err.contains(".holders"), "{err}");
    assert_eq!(
        state(&a, item),
        ItemState::Open,
        "fail closed: nothing started"
    );
}

#[test]
fn a_database_in_a_non_ascii_directory_opens_and_its_lease_works() {
    let db = lab("日本ÄÖé").parent().unwrap().join("fileé.db");
    let a = open(&db);
    let item = running_cut_off(&a);
    let b = open(&db);
    assert!(b.recover_interrupted().unwrap().is_empty());
    assert_eq!(state(&b, item), ItemState::Running);
    drop(a);
    assert_eq!(b.recover_interrupted().unwrap().len(), 1);
}
