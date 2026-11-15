//! CORE-2: stored item state is read strictly, and a finished item's criteria are not changed.
//! (1) A row whose state cannot be read is an error naming the damage, never a valid state.
//! (2) `confirm_criterion` on a done or failed item is refused and writes nothing.

mod common;
use std::path::PathBuf;
use std::sync::Arc;

use common::{actor_named, owner};
use endora_foundation::application::{Core, NewGoal};
use endora_foundation::domain::{Criterion, ItemId, ItemState};
use endora_foundation::infrastructure::ManualClock;
use endora_kernel::Tier;

fn file_core(name: &str) -> (Core, PathBuf, PathBuf) {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("stored-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("c.db");
    let core = Core::open(path.to_str().unwrap(), Arc::new(ManualClock::new(5_000))).unwrap();
    (core, path, dir)
}

/// Two items on one goal with two owner-confirmed criteria each: `running` (item 1) and a second.
fn two_running(core: &Core) -> (ItemId, ItemId) {
    let o = owner(core);
    let goal = core
        .add_goal(
            &o,
            NewGoal {
                title: "t".into(),
                why: "w".into(),
                criteria: vec![
                    Criterion {
                        text: "one".into(),
                        check: None,
                    },
                    Criterion {
                        text: "two".into(),
                        check: None,
                    },
                ],
                scope: vec![],
                tier: Tier::Internal,
                spend_cap_micros: 0,
            },
        )
        .unwrap();
    let a = core.add_item(goal, "a").unwrap();
    let b = core.add_item(goal, "b").unwrap();
    for i in [a, b] {
        core.start(&actor_named("runner"), i).unwrap();
    }
    (a, b)
}

#[test]
fn an_unreadable_item_state_is_an_error_naming_the_damage_never_failed() {
    let (core, path, dir) = file_core("corrupt");
    let (a, _) = two_running(&core);
    let o = owner(&core);
    let head = core.verify_evidence().unwrap();
    drop(core);
    {
        let c = rusqlite::Connection::open(&path).unwrap();
        c.execute_batch(
            "PRAGMA ignore_check_constraints=1; UPDATE items SET state='zzz' WHERE id=1;",
        )
        .unwrap();
    }
    let core = Core::open(path.to_str().unwrap(), Arc::new(ManualClock::new(6_000))).unwrap();
    let rows_before = core.store().evidence_all().unwrap().len();

    let says = |e: String| {
        assert!(e.contains("corrupt stored data"), "{e}");
        assert!(e.contains("item 1") && e.contains("zzz"), "{e}");
        assert!(!e.contains("is failed"), "{e}");
    };
    says(core.store().item(a).unwrap_err().to_string());
    says(core.store().items().unwrap_err().to_string());
    says(core.status_text(5).unwrap_err().to_string());
    says(core.accept(&o, a).map(|_| ()).unwrap_err().to_string());
    says(core.decline(&o, a, "no").unwrap_err().to_string());
    says(core.confirm_criterion(&o, a, 0).unwrap_err().to_string());
    // Nothing was written by any of them: the row, the criteria and the evidence are as they were.
    let raw = rusqlite::Connection::open(&path).unwrap();
    let (state, reason): (String, Option<String>) = raw
        .query_row("SELECT state, reason FROM items WHERE id=1", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!((state.as_str(), reason), ("zzz", None));
    let met: i64 = raw
        .query_row("SELECT COUNT(*) FROM item_met WHERE item_id=1", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(met, 0);
    assert_eq!(core.store().evidence_all().unwrap().len(), rows_before);
    // verify-evidence is the integrity check and keeps working on this file.
    let after = core.verify_evidence().unwrap();
    assert!(after.broken.is_none());
    assert_eq!((after.rows, after.head), (head.rows, head.head));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn confirming_a_criterion_of_a_done_or_failed_item_is_refused_and_writes_nothing() {
    let (core, _path, dir) = file_core("finished");
    let (a, b) = two_running(&core);
    let o = owner(&core);
    // `a` ends done (both criteria confirmed, then accepted); `b` ends failed with both unmet.
    core.confirm_criterion(&o, a, 0).unwrap();
    core.confirm_criterion(&o, a, 1).unwrap();
    core.accept(&o, a).unwrap();
    core.decline(&o, b, "no").unwrap();
    for (item, want) in [(a, ItemState::Done), (b, ItemState::Failed)] {
        let rows = core.store().evidence_all().unwrap().len();
        let item_before = core.store().item(item).unwrap();
        let met_before = core.store().criteria(item).unwrap();
        for idx in [0, 1] {
            let e = core.confirm_criterion(&o, item, idx).unwrap_err();
            assert!(
                e.to_string()
                    .contains(&format!("item {} is already {}", item.0, want.name())),
                "{e}"
            );
            assert!(!e.to_string().contains("corrupt"), "{e}");
        }
        assert_eq!(core.store().evidence_all().unwrap().len(), rows);
        assert_eq!(core.store().item(item).unwrap(), item_before);
        assert_eq!(core.store().criteria(item).unwrap(), met_before);
        assert_eq!(item_before.state, want);
    }
    assert!(core.verify_evidence().unwrap().broken.is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_unreadable_evidence_decision_or_routine_flag_is_an_error_never_a_valid_value() {
    let (core, path, dir) = file_core("decision");
    let (a, _) = two_running(&core);
    let o = owner(&core);
    core.add_routine(
        &o,
        core.store().item(a).unwrap().goal,
        "r",
        60_000,
        false,
        1_000,
    )
    .unwrap();
    drop(core);
    let flip = |sql: &str| {
        let c = rusqlite::Connection::open(&path).unwrap();
        c.execute_batch(&format!(
            "PRAGMA ignore_check_constraints=1; DROP TRIGGER evidence_no_change; {sql}"
        ))
        .unwrap();
    };
    let row: i64 = rusqlite::Connection::open(&path)
        .unwrap()
        .query_row("SELECT MIN(id) FROM evidence WHERE item_id=1", [], |r| {
            r.get(0)
        })
        .unwrap();
    flip(&format!(
        "UPDATE evidence SET decision='maybe' WHERE id={row}; UPDATE routines SET paused=2, auto_accept=2;"
    ));
    let core = Core::open(path.to_str().unwrap(), Arc::new(ManualClock::new(6_000))).unwrap();
    let says = |e: String, what: &str| {
        assert!(e.contains("corrupt stored data"), "{e}");
        assert!(e.contains(what), "{e}");
    };
    says(
        core.store().evidence_all().unwrap_err().to_string(),
        &format!("evidence row {row} has an unreadable decision \"maybe\""),
    );
    says(
        core.store()
            .evidence_for(a)
            .map(|_| ())
            .unwrap_err()
            .to_string(),
        "decision",
    );
    says(
        core.store().evidence_tail(10).unwrap_err().to_string(),
        "decision",
    );
    says(core.status_text(5).unwrap_err().to_string(), "decision");
    says(
        core.store().routines().unwrap_err().to_string(),
        "routine 1 has an unreadable",
    );
    says(
        core.store()
            .routine(endora_foundation::domain::RoutineId(1))
            .unwrap_err()
            .to_string(),
        "routine 1 has an unreadable",
    );
    // The integrity check still runs, and it is the one that names the edited row.
    assert!(core.verify_evidence().unwrap().broken.is_some());
    let _ = std::fs::remove_dir_all(&dir);
}
