//! The evidence hash chain is TAMPER EVIDENCE: it detects an edit, a removal or a splice after
//! the append-only triggers were dropped. It does not prevent any of them.

mod common;
use std::path::PathBuf;
use std::sync::Arc;

use common::{actor_named, owner};
use endora_foundation::application::{Core, NewGoal};
use endora_foundation::domain::Criterion;
use endora_foundation::infrastructure::ManualClock;
use endora_kernel::Tier;

fn file_core(name: &str) -> (Core, PathBuf) {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("chain-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("c.db");
    let core = Core::open(path.to_str().unwrap(), Arc::new(ManualClock::new(5_000))).unwrap();
    let o = owner(&core);
    let g = core
        .add_goal(
            &o,
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
    for n in 0..4 {
        let item = core.add_item(g, &format!("i{n}")).unwrap();
        core.start(&actor_named("runner"), item).unwrap();
    }
    (core, path)
}

fn drop_triggers(core: &Core) {
    let db = core.store().db().lock().unwrap();
    db.execute_batch(
        "DROP TRIGGER evidence_no_change; DROP TRIGGER evidence_no_removal; DROP TRIGGER evidence_no_replace;",
    )
    .unwrap();
}

#[test]
fn an_untouched_chain_verifies_and_every_row_links_to_the_one_before() {
    let (core, _) = file_core("ok");
    let r = core.verify_evidence().unwrap();
    assert!(r.broken.is_none(), "{r:?}");
    assert!(r.rows >= 6);
    assert_eq!(r.head.len(), 64);
    // A new row moves the head and the chain still verifies.
    core.stop(&actor_named("anyone")).unwrap();
    let r2 = core.verify_evidence().unwrap();
    assert_eq!(r2.rows, r.rows + 1);
    assert_ne!(r2.head, r.head);
    assert!(r2.broken.is_none());
}

#[test]
fn editing_a_row_after_dropping_the_triggers_is_detected_and_the_row_is_named() {
    let (core, _) = file_core("edit");
    drop_triggers(&core);
    core.store()
        .db()
        .lock()
        .unwrap()
        .execute("UPDATE evidence SET output = 'forged' WHERE id = 3", [])
        .unwrap();
    let r = core.verify_evidence().unwrap();
    let b = r.broken.expect("the edit must be detected");
    assert_eq!(b.id, 3, "{b:?}");
}

#[test]
fn deleting_a_middle_row_is_detected() {
    let (core, _) = file_core("delete");
    drop_triggers(&core);
    core.store()
        .db()
        .lock()
        .unwrap()
        .execute("DELETE FROM evidence WHERE id = 3", [])
        .unwrap();
    let b = core.verify_evidence().unwrap().broken.expect("detected");
    assert_eq!(b.id, 4, "the first row whose predecessor is gone: {b:?}");
}

#[test]
fn rewriting_a_row_and_its_own_hash_still_breaks_the_next_link() {
    let (core, _) = file_core("splice");
    drop_triggers(&core);
    {
        let db = core.store().db().lock().unwrap();
        db.execute(
            "UPDATE evidence SET output = 'forged', hash = 'ab' WHERE id = 2",
            [],
        )
        .unwrap();
    }
    let b = core.verify_evidence().unwrap().broken.expect("detected");
    assert_eq!(b.id, 2);
}

#[test]
fn a_chain_built_by_a_client_that_knows_the_format_is_the_documented_limit() {
    // Detection, not prevention: someone who recomputes every later hash can forge a chain.
    // The head hash is what to keep elsewhere (a printed line, another host) to catch that.
    let (core, _) = file_core("head");
    let head = core.verify_evidence().unwrap().head;
    core.stop(&actor_named("anyone")).unwrap();
    assert_ne!(core.verify_evidence().unwrap().head, head);
}

#[test]
fn rows_written_before_the_chain_existed_are_covered_from_the_upgrade() {
    // A file at schema version 4 (no chain columns) gains the chain over its old rows.
    let (core, path) = file_core("legacy");
    {
        let db = core.store().db().lock().unwrap();
        db.execute_batch(
            "DROP TRIGGER evidence_no_change;
             UPDATE evidence SET prev_hash = '', hash = '';
             PRAGMA user_version = 4;",
        )
        .unwrap();
    }
    drop(core);
    let core = Core::open(path.to_str().unwrap(), Arc::new(ManualClock::new(5_000))).unwrap();
    let r = core.verify_evidence().unwrap();
    assert!(r.broken.is_none(), "{r:?}");
    assert!(r.rows >= 6);
    // The trigger came back with the migration.
    let n = core
        .store()
        .db()
        .lock()
        .unwrap()
        .execute("UPDATE evidence SET output = 'x' WHERE id = 1", [])
        .is_err();
    assert!(n);
}

/// Slice 7 F3 (REVIEW3 finding 3). The review's attack: drop the append-only trigger, edit a
/// row, lower `user_version`, reopen. The migration used to re-hash every row and the edit became
/// part of a clean chain. Migrations are part of the TRUSTED SETUP context; they now refuse a
/// file whose rows already carry hashes while `user_version` says the chain does not exist yet,
/// and they leave the file untouched.
fn lowered_version_attack(name: &str, version: i64) {
    let (core, path) = file_core(name);
    let before = core.verify_evidence().unwrap();
    assert!(before.broken.is_none());
    drop_triggers(&core);
    {
        let db = core.store().db().lock().unwrap();
        db.execute_batch(&format!(
            "UPDATE evidence SET output = 'FORGED' WHERE id = 2; PRAGMA user_version = {version};"
        ))
        .unwrap();
    }
    drop(core);
    let bytes_before = std::fs::read(&path).unwrap();
    let reopened = Core::open(path.to_str().unwrap(), Arc::new(ManualClock::new(5_000)));
    let err = reopened
        .err()
        .expect("the engine must refuse, not re-sign")
        .to_string();
    assert!(err.contains("already carry hashes"), "{err}");
    // Nothing was re-signed: the forged row and the old hashes are exactly as the attacker left them.
    assert_eq!(std::fs::read(&path).unwrap(), bytes_before);
    let c = rusqlite::Connection::open(&path).unwrap();
    let (out, hash): (String, String) = c
        .query_row("SELECT output, hash FROM evidence WHERE id = 2", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!(out, "FORGED");
    assert!(!hash.is_empty());
}

#[test]
fn a_lowered_user_version_cannot_launder_an_edited_row_into_a_clean_chain() {
    lowered_version_attack("launder4", 4);
}

#[test]
fn the_older_actor_kind_migration_cannot_be_replayed_over_forged_rows_either() {
    lowered_version_attack("launder2", 2);
}
