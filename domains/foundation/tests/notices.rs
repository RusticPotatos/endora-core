//! Owner notices on a watch's change (ADR 0152). A watch that fails for the grace, three times in
//! a row, makes ONE "failing" notice; its next success makes ONE "recovered" notice. Notices go to
//! an outbox and are delivered to a LOCAL sink, retried without duplicates. A notice holds the
//! routine's name, the state and the time, never a probe's text. Nothing here sends anywhere.

mod common;
use common::{actor_named, owner};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use endora_foundation::application::{
    CallFailure, CallOutput, Capability, Core, NewGoal, NoticeSink,
};
use endora_foundation::domain::{
    CallSpec, Check, Criterion, FAILING_AFTER, Fact, Field, GRACE_MS, GoalId, ItemState, Notice,
    NoticeState, PolicyRow, RoutineId,
};
use endora_foundation::infrastructure::{FileSink, ManualClock};
use endora_kernel::{Reversibility, Tier};

const MIN: i64 = 60_000;
/// Text a probe prints; it must never reach a notice.
const LEAK: &str = "SECRET-PROBE-OUTPUT-xyzzy";

struct Look(AtomicBool);
impl Capability for Look {
    fn offers(&self) -> Vec<CallSpec> {
        vec![CallSpec::new("p.look", Reversibility::Observe).with_facts(vec![Field::bool("up")])]
    }
    fn call(&self, _c: &str, _i: &str) -> Result<CallOutput, CallFailure> {
        let mut o = CallOutput::text(LEAK);
        o.evidence = LEAK.into();
        o.facts = vec![Fact::bool("up", self.0.load(Ordering::SeqCst))];
        Ok(o)
    }
}

struct Lab {
    core: Core,
    clock: Arc<ManualClock>,
    look: Arc<Look>,
    routine: RoutineId,
}

fn lab() -> Lab {
    let clock = Arc::new(ManualClock::new(1_000_000));
    let look = Arc::new(Look(AtomicBool::new(false)));
    let mut core = Core::open(":memory:", clock.clone()).unwrap();
    core.register(look.clone()).unwrap();
    let goal: GoalId = core
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
    let routine = core
        .add_routine_limited(
            &owner(&core),
            goal,
            "web front",
            1_000,
            false,
            1_000_000,
            50,
        )
        .unwrap();
    Lab {
        core,
        clock,
        look,
        routine,
    }
}

impl Lab {
    /// One pass: time moves on by `by`, due slots fire, each open item is checked.
    fn pass(&self, by: i64, up: bool) {
        self.look.0.store(up, Ordering::SeqCst);
        self.clock.advance(by);
        self.core.fire_due().unwrap();
        for it in self.core.store().items_in(ItemState::Open).unwrap() {
            self.core.start(&actor_named("runner"), it.id).unwrap();
            self.core.check_watch(it.id).unwrap();
        }
    }
    fn notices(&self) -> Vec<Notice> {
        self.core.store().notices(true).unwrap()
    }
    fn states(&self) -> Vec<NoticeState> {
        self.notices().iter().map(|n| n.state).collect()
    }
}

/// A sink that records what it was given and can be made to fail.
#[derive(Default)]
struct Recording {
    got: Mutex<Vec<String>>,
    down: AtomicBool,
}
impl NoticeSink for Recording {
    fn deliver(&self, n: &Notice) -> Result<(), String> {
        if self.down.load(Ordering::SeqCst) {
            return Err("sink is down".into());
        }
        self.got.lock().unwrap().push(n.key.clone());
        Ok(())
    }
}

#[test]
fn three_failures_across_the_grace_make_exactly_one_failing_notice() {
    let l = lab();
    assert_eq!((FAILING_AFTER, GRACE_MS), (3, 10 * MIN));
    l.pass(5 * MIN, false); // failure 1, the run starts
    l.pass(5 * MIN, false); // failure 2
    assert!(l.notices().is_empty(), "two failures say nothing");
    l.pass(5 * MIN, false); // failure 3, ten minutes into the run
    let n = l.notices();
    assert_eq!(n.len(), 1, "{n:?}");
    assert_eq!(n[0].state, NoticeState::Failing);
    assert_eq!(n[0].routine, l.routine);
    assert_eq!(n[0].routine_title, "web front");
    assert_eq!(
        n[0].at_ms,
        endora_kernel::Clock::now(&*l.clock).unix_millis()
    );
    // Failures 4 and 5 add nothing; sweeping again adds nothing.
    l.pass(5 * MIN, false);
    l.pass(5 * MIN, false);
    l.core.sweep_notices().unwrap();
    l.core.sweep_notices().unwrap();
    assert_eq!(l.states(), vec![NoticeState::Failing]);
}

#[test]
fn three_quick_failures_wait_out_the_grace_and_a_sweep_says_it_once() {
    let l = lab();
    for _ in 0..3 {
        l.pass(1_000, false);
    }
    assert!(
        l.notices().is_empty(),
        "three failures inside the grace are not yet said"
    );
    // The watch fails no more (the routine was paused, say), but the grace runs out: the sweep
    // that every run-due pass makes says it, once.
    l.clock.advance(GRACE_MS);
    assert_eq!(l.core.sweep_notices().unwrap(), 1);
    assert_eq!(l.core.sweep_notices().unwrap(), 0);
    assert_eq!(l.states(), vec![NoticeState::Failing]);
}

#[test]
fn a_paused_routine_that_is_still_failing_is_still_said() {
    let l = lab();
    // Three quick failures, then the owner pauses it: nothing fires, but it is still failing.
    for _ in 0..3 {
        l.pass(1_000, false);
    }
    l.core.pause_routine(&owner(&l.core), l.routine).unwrap();
    l.clock.advance(2 * GRACE_MS);
    l.core.sweep_notices().unwrap();
    assert_eq!(l.states(), vec![NoticeState::Failing]);
}

#[test]
fn the_next_success_makes_exactly_one_recovered_notice() {
    let l = lab();
    for _ in 0..3 {
        l.pass(5 * MIN, false);
    }
    l.pass(5 * MIN, true);
    assert_eq!(
        l.states(),
        vec![NoticeState::Failing, NoticeState::Recovered]
    );
    l.pass(5 * MIN, true);
    l.pass(5 * MIN, true);
    l.core.sweep_notices().unwrap();
    assert_eq!(
        l.states(),
        vec![NoticeState::Failing, NoticeState::Recovered]
    );
}

#[test]
fn a_blip_that_was_never_said_is_not_said_when_it_ends() {
    let l = lab();
    l.pass(5 * MIN, false);
    l.pass(5 * MIN, false);
    l.pass(5 * MIN, true);
    l.pass(5 * MIN, true);
    l.core.sweep_notices().unwrap();
    assert!(
        l.notices().is_empty(),
        "no failing notice, so no recovered one"
    );
}

#[test]
fn a_second_outage_after_a_recovery_is_a_new_notice_with_its_own_id() {
    let l = lab();
    for _ in 0..3 {
        l.pass(5 * MIN, false);
    }
    l.pass(5 * MIN, true);
    // The healthy item waits for the owner (the goal's policy), which holds the routine's next
    // slot: accepting it lets the routine fire again.
    for it in l.core.store().items_in(ItemState::Running).unwrap() {
        l.core.accept(&owner(&l.core), it.id).unwrap();
    }
    for _ in 0..3 {
        l.pass(5 * MIN, false);
    }
    l.pass(5 * MIN, true);
    assert_eq!(
        l.states(),
        vec![
            NoticeState::Failing,
            NoticeState::Recovered,
            NoticeState::Failing,
            NoticeState::Recovered
        ]
    );
    let mut keys: Vec<String> = l.notices().into_iter().map(|n| n.key).collect();
    keys.sort();
    keys.dedup();
    assert_eq!(keys.len(), 4, "every notice has its own id");
}

#[test]
fn a_notice_holds_the_name_the_state_and_the_time_and_never_the_probes_text() {
    let l = lab();
    for _ in 0..3 {
        l.pass(5 * MIN, false);
    }
    l.pass(5 * MIN, true);
    // The probe text really is in the evidence (so the test can fail)...
    let tail = l.core.store().evidence_tail(200).unwrap();
    assert!(tail.iter().any(|e| e.output.contains(LEAK)));
    // ...and in no notice field, line or delivered file.
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("notice-leak");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("notices.log");
    l.core.deliver_notices(&FileSink::new(&file)).unwrap();
    let written = std::fs::read_to_string(&file).unwrap();
    assert!(written.contains("web front") && written.contains("failing"));
    assert!(written.contains("recovered"));
    assert!(!written.contains(LEAK));
    for n in l.notices() {
        assert!(
            !format!("{n:?}{}", n.line()).contains(LEAK),
            "a notice leaked probe text: {n:?}"
        );
    }
}

#[test]
fn a_notice_line_has_no_control_characters_even_when_the_routine_name_does() {
    let l = lab();
    let bad = l
        .core
        .add_routine_limited(
            &owner(&l.core),
            GoalId(1),
            "x\nfake line\u{1b}[31m",
            5 * MIN,
            false,
            1,
            5,
        )
        .unwrap();
    // Straight into the store: the line is made from whatever the title is.
    let id = l
        .core
        .store()
        .raise_notice(bad, NoticeState::Failing, 5, 7)
        .unwrap();
    let n = l
        .core
        .store()
        .notices(true)
        .unwrap()
        .into_iter()
        .find(|n| Some(n.id) == id)
        .unwrap();
    assert!(!n.line().chars().any(char::is_control), "{:?}", n.line());
    assert_eq!(n.line().lines().count(), 1);
}

#[test]
fn a_failed_delivery_retries_without_a_duplicate() {
    let l = lab();
    for _ in 0..3 {
        l.pass(5 * MIN, false);
    }
    let sink = Recording::default();
    sink.down.store(true, Ordering::SeqCst);
    let first = l.core.deliver_notices(&sink).unwrap();
    assert_eq!((first.delivered, first.failed), (0, 1));
    assert_eq!(l.notices()[0].attempts, 1);
    assert!(l.notices()[0].delivered_ms.is_none());
    // The sink comes back: the next tick delivers it, once, and a third tick sends nothing.
    sink.down.store(false, Ordering::SeqCst);
    let second = l.core.deliver_notices(&sink).unwrap();
    assert_eq!((second.delivered, second.failed), (1, 0));
    let third = l.core.deliver_notices(&sink).unwrap();
    assert_eq!((third.delivered, third.failed), (0, 0));
    assert_eq!(sink.got.lock().unwrap().len(), 1);
    assert!(l.notices()[0].delivered_ms.is_some());
}

#[test]
fn a_crash_between_the_write_and_the_mark_does_not_duplicate_in_the_file() {
    let l = lab();
    for _ in 0..3 {
        l.pass(5 * MIN, false);
    }
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("notice-crash");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("notices.log");
    let sink = FileSink::new(&file);
    // The sink wrote it, then the process died before the mark: the sink is asked again.
    let n = l.notices().remove(0);
    sink.deliver(&n).unwrap();
    sink.deliver(&n).unwrap();
    l.core.deliver_notices(&sink).unwrap();
    let text = std::fs::read_to_string(&file).unwrap();
    assert_eq!(text.lines().count(), 1, "one line per notice id:\n{text}");
    assert!(text.contains(&n.key));
}

#[test]
fn the_stop_switch_does_not_silence_notices() {
    let l = lab();
    for _ in 0..3 {
        l.pass(5 * MIN, false);
    }
    l.core.stop(&actor_named("anyone")).unwrap();
    l.clock.advance(MIN);
    let sink = Recording::default();
    let d = l.core.deliver_notices(&sink).unwrap();
    assert_eq!(d.delivered, 1);
    l.core.sweep_notices().unwrap();
    assert_eq!(l.states(), vec![NoticeState::Failing]);
}

#[test]
fn only_the_owner_acknowledges_and_an_acknowledged_notice_leaves_the_list() {
    let l = lab();
    for _ in 0..3 {
        l.pass(5 * MIN, false);
    }
    let id = l.notices()[0].id;
    l.core.acknowledge_notice(&owner(&l.core), id).unwrap();
    assert!(l.core.store().notices(false).unwrap().is_empty());
    assert_eq!(l.core.store().notices(true).unwrap().len(), 1);
    assert!(l.notices()[0].acked_ms.is_some());
    // Acknowledging again, or a notice that is not there, is an error, not a silent no-op.
    assert!(l.core.acknowledge_notice(&owner(&l.core), id).is_err());
    assert!(l.core.acknowledge_notice(&owner(&l.core), 999).is_err());
    // An acknowledged notice is not delivered again either.
    let sink = Recording::default();
    assert_eq!(l.core.deliver_notices(&sink).unwrap().delivered, 0);
}

// ---- the schema step: additive, from a real schema-9 file ----

fn schema9_file(name: &str) -> std::path::PathBuf {
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("notices-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("v9.db");
    let c = rusqlite::Connection::open(&p).unwrap();
    c.execute_batch(include_str!("fixtures/schema_v9.sql"))
        .unwrap();
    c.execute_batch("PRAGMA user_version = 9").unwrap();
    p
}

fn version(p: &std::path::PathBuf) -> i64 {
    rusqlite::Connection::open(p)
        .unwrap()
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap()
}

#[test]
fn a_schema_9_file_gains_only_the_notice_tables_and_keeps_every_row() {
    use endora_foundation::infrastructure::SCHEMA_VERSION;
    let p = schema9_file("open");
    assert_eq!(version(&p), 9);
    let rows = |c: &rusqlite::Connection, t: &str| -> i64 {
        c.query_row(&format!("SELECT COUNT(*) FROM {t}"), [], |r| r.get(0))
            .unwrap()
    };
    let before: Vec<i64> = {
        let c = rusqlite::Connection::open(&p).unwrap();
        [
            "goals", "routines", "items", "evidence", "firings", "criteria",
        ]
        .iter()
        .map(|t| rows(&c, t))
        .collect()
    };
    let clock = Arc::new(ManualClock::new(1_795_580_200_000));
    let core = Core::open(p.to_str().unwrap(), clock.clone()).unwrap();
    assert_eq!((version(&p), SCHEMA_VERSION), (11, 11));
    assert!(core.verify_evidence().unwrap().broken.is_none());
    assert!(core.store().notices(true).unwrap().is_empty());
    drop(core);
    let c = rusqlite::Connection::open(&p).unwrap();
    let after: Vec<i64> = [
        "goals", "routines", "items", "evidence", "firings", "criteria",
    ]
    .iter()
    .map(|t| rows(&c, t))
    .collect();
    assert_eq!(
        before, after,
        "nothing that was there was changed or removed"
    );
    // The new tables exist, and no existing table gained or lost a column.
    for t in ["notices", "watch_state"] {
        assert_eq!(rows(&c, t), 0, "{t} starts empty");
    }
    let routine_columns: i64 = c
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('routines')",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(routine_columns, 10, "routines is as schema 9 left it");
}

#[test]
fn a_migrated_schema_9_file_notices_a_failing_watch() {
    let p = schema9_file("works");
    let clock = Arc::new(ManualClock::new(1_795_580_200_000));
    let look = Arc::new(Look(AtomicBool::new(false)));
    let mut core = Core::open(p.to_str().unwrap(), clock.clone()).unwrap();
    core.register(look.clone()).unwrap();
    let goal = core
        .add_goal(
            &owner(&core),
            NewGoal {
                title: "w".into(),
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
    core.add_routine_limited(&owner(&core), goal, "migrated watch", 5 * MIN, false, 0, 50)
        .unwrap();
    for _ in 0..3 {
        clock.advance(5 * MIN);
        core.fire_due().unwrap();
        for it in core.store().items_in(ItemState::Open).unwrap() {
            if it.title != "migrated watch" {
                continue;
            }
            core.start(&actor_named("runner"), it.id).unwrap();
            core.check_watch(it.id).unwrap();
        }
    }
    let n = core.store().notices(true).unwrap();
    assert_eq!(n.len(), 1, "{n:?}");
    assert_eq!(n[0].routine_title, "migrated watch");
}

// ---- the schema step from a real schema-10 file (the repair policy's) ----

#[test]
fn a_schema_10_file_gains_the_notice_tables_and_keeps_its_repair_and_paused_routine() {
    use endora_foundation::infrastructure::SCHEMA_VERSION;
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("notices-v10-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("v10.db");
    let c = rusqlite::Connection::open(&p).unwrap();
    c.execute_batch(include_str!("fixtures/schema_v10.sql"))
        .unwrap();
    c.execute_batch("PRAGMA user_version = 10").unwrap();
    let has_table = |c: &rusqlite::Connection, t: &str| -> bool {
        c.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
            [t],
            |r| r.get::<_, i64>(0),
        )
        .unwrap()
            == 1
    };
    assert!(!has_table(&c, "notices") && !has_table(&c, "watch_state"));
    drop(c);
    assert_eq!(version(&p), 10);
    let clock = Arc::new(ManualClock::new(1_795_583_300_000));
    let core = Core::open(p.to_str().unwrap(), clock.clone()).unwrap();
    assert_eq!((version(&p), SCHEMA_VERSION), (11, 11));
    assert!(core.verify_evidence().unwrap().broken.is_none());
    // The schema-10 state is intact: the goal's repair, and the paused (not retired) routine.
    let repair = core
        .store()
        .repair(GoalId(1))
        .unwrap()
        .expect("the repair row");
    assert_eq!(
        (repair.call.as_str(), repair.input.as_str()),
        ("action.run", "restart")
    );
    let paused = core.store().routine(RoutineId(2)).unwrap();
    assert!(paused.paused && !paused.retired);
    assert!(!core.store().routine(RoutineId(1)).unwrap().paused);
    // The notice tables are new and empty.
    drop(core);
    let c = rusqlite::Connection::open(&p).unwrap();
    for t in ["notices", "watch_state"] {
        assert!(has_table(&c, t), "{t} was created");
        let n: i64 = c
            .query_row(&format!("SELECT COUNT(*) FROM {t}"), [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0, "{t} starts empty");
    }
    let repairs: i64 = c
        .query_row("SELECT COUNT(*) FROM repairs", [], |r| r.get(0))
        .unwrap();
    assert_eq!(repairs, 1, "the repair row is untouched");
}
