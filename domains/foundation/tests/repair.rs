//! A failing watch may trigger ONE reversible repair, defined by an owner-made policy row and
//! capped at one use per failure episode. The probe re-checks afterwards and the item then waits
//! for the owner. Every decision goes through `decide`; the evidence chain holds the intent and
//! the result of the action.

mod common;
use common::{actor_named, owner};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use endora_foundation::application::{
    Acceptance, CallFailure, CallOutput, Capability, Core, NewGoal, WatchEnd,
};
use endora_foundation::domain::{
    AcceptancePolicy, CallSpec, Check, Criterion, Deny, Fact, Field, GoalId, ItemState, Kind,
    PolicyRow, RoutineId,
};
use endora_foundation::infrastructure::ManualClock;
use endora_kernel::{Reversibility, Tier};

/// A probe and a repair action sharing one pretend service: the action brings it up when `fixes`.
struct World {
    up: AtomicBool,
    fixes: AtomicBool,
    ran: AtomicU32,
    band: Reversibility,
    fails: AtomicBool,
}

impl Capability for World {
    fn offers(&self) -> Vec<CallSpec> {
        vec![
            CallSpec::new("p.look", Reversibility::Observe).with_facts(vec![Field::bool("up")]),
            {
                let mut s = CallSpec::new("fix.run", self.band);
                s.inputs = vec![Field::text("action")];
                s
            },
        ]
    }
    fn call(&self, call: &str, _i: &str) -> Result<CallOutput, CallFailure> {
        if call == "fix.run" {
            self.ran.fetch_add(1, Ordering::SeqCst);
            if self.fails.load(Ordering::SeqCst) {
                return Err(CallFailure::new("the action failed"));
            }
            if self.fixes.load(Ordering::SeqCst) {
                self.up.store(true, Ordering::SeqCst);
            }
            return Ok(CallOutput::text("restarted"));
        }
        let mut o = CallOutput::text("probe");
        o.facts = vec![Fact::bool("up", self.up.load(Ordering::SeqCst))];
        Ok(o)
    }
}

fn world(fixes: bool, band: Reversibility) -> Arc<World> {
    Arc::new(World {
        up: AtomicBool::new(false),
        fixes: AtomicBool::new(fixes),
        ran: AtomicU32::new(0),
        band,
        fails: AtomicBool::new(false),
    })
}

fn setup(
    path: &str,
    w: &Arc<World>,
    acceptance: AcceptancePolicy,
) -> (Core, Arc<ManualClock>, GoalId, RoutineId) {
    let clock = Arc::new(ManualClock::new(10_000));
    let mut core = Core::open(path, clock.clone()).unwrap();
    core.register(w.clone()).unwrap();
    let goal = core
        .add_goal_with_policy(
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
                scope: vec!["p.look".into(), "fix.run".into()],
                tier: Tier::Internal,
                spend_cap_micros: 0,
            },
            acceptance,
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
    let r = core
        .add_routine_limited(&owner(&core), goal, "look", 1_000, false, 11_000, 5)
        .unwrap();
    (core, clock, goal, r)
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

fn fix_rows(core: &Core) -> Vec<(Kind, bool, String)> {
    core.store()
        .evidence_all()
        .unwrap()
        .into_iter()
        .filter(|e| e.call == "fix.run")
        .map(|e| (e.kind, e.allowed, e.actor))
        .collect()
}

#[test]
fn a_failed_watch_runs_the_repair_once_rechecks_and_waits_for_the_owner() {
    let w = world(true, Reversibility::Reversible);
    let (core, clock, goal, r) = setup(":memory:", &w, AcceptancePolicy::AutoObserve);
    core.add_repair(&owner(&core), goal, "fix.run", "restart-service")
        .unwrap();
    assert_eq!(pass(&core, &clock), vec![WatchEnd::Repaired]);
    assert_eq!(w.ran.load(Ordering::SeqCst), 1);
    // One intent and one result for the action, by the repair actor, on the chain.
    assert_eq!(
        fix_rows(&core),
        vec![
            (Kind::Intent, true, "repair".to_owned()),
            (Kind::Result, true, "repair".to_owned())
        ]
    );
    assert!(core.verify_evidence().unwrap().broken.is_none());
    // The item waits for the owner: policy may not accept reversible work.
    let item = core.store().items_in(ItemState::Running).unwrap()[0].id;
    assert!(matches!(
        core.settle_by_policy(item).unwrap(),
        Some(Acceptance::Refused(_))
    ));
    assert_eq!(core.store().item(item).unwrap().state, ItemState::Running);
    // The routine skips the next slot while the repaired item is open.
    clock.advance(1_000);
    let fired = core.fire_due().unwrap();
    assert!(fired.iter().all(|f| f.skipped_open));
    // The owner accepts: done.
    assert!(matches!(
        core.accept(&owner(&core), item).unwrap(),
        Acceptance::Accepted { .. }
    ));
    assert!(!core.store().routine(r).unwrap().paused);
}

#[test]
fn the_stop_switch_on_means_the_action_does_not_run() {
    let w = world(true, Reversibility::Reversible);
    let (core, clock, goal, _r) = setup(":memory:", &w, AcceptancePolicy::Owner);
    core.add_repair(&owner(&core), goal, "fix.run", "restart-service")
        .unwrap();
    core.stop(&actor_named("tester")).unwrap();
    let ends = pass(&core, &clock);
    assert!(matches!(ends[0], WatchEnd::Failed { .. }), "{ends:?}");
    assert_eq!(w.ran.load(Ordering::SeqCst), 0);
    let rows = fix_rows(&core);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0], (Kind::Refusal, false, "repair".to_owned()));
}

#[test]
fn the_cap_is_one_use_per_failure_episode_and_a_new_episode_starts_after_a_healthy_check() {
    // The action does not fix it: the first failure spends the episode's one use.
    let w = world(false, Reversibility::Reversible);
    let (core, clock, goal, _r) = setup(":memory:", &w, AcceptancePolicy::Owner);
    core.add_repair(&owner(&core), goal, "fix.run", "restart-service")
        .unwrap();
    assert!(matches!(
        pass(&core, &clock)[0],
        WatchEnd::Failed { failures: 1, .. }
    ));
    assert_eq!(w.ran.load(Ordering::SeqCst), 1);
    // The second failure in the same episode: refused by decide (CapReached), the item fails.
    assert!(matches!(
        pass(&core, &clock)[0],
        WatchEnd::Failed { failures: 2, .. }
    ));
    assert_eq!(
        w.ran.load(Ordering::SeqCst),
        1,
        "no second use in one episode"
    );
    let refused: Vec<_> = core
        .store()
        .evidence_all()
        .unwrap()
        .into_iter()
        .filter(|e| e.call == "fix.run" && e.kind == Kind::Refusal)
        .map(|e| e.output)
        .collect();
    assert_eq!(refused, vec![Deny::CapReached.said().to_owned()]);
    // The service comes up by itself: healthy check, the episode ends.
    w.up.store(true, Ordering::SeqCst);
    assert_eq!(pass(&core, &clock), vec![WatchEnd::Healthy]);
    let item = core.store().items_in(ItemState::Running).unwrap()[0].id;
    core.accept(&owner(&core), item).unwrap();
    // It goes down again: a NEW episode, so the repair is allowed once more.
    w.up.store(false, Ordering::SeqCst);
    w.fixes.store(true, Ordering::SeqCst);
    assert_eq!(pass(&core, &clock), vec![WatchEnd::Repaired]);
    assert_eq!(w.ran.load(Ordering::SeqCst), 2);
}

#[test]
fn no_policy_row_means_nothing_runs_and_the_item_fails_as_before() {
    let dir = std::env::temp_dir().join(format!("endora-repair-nopolicy-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let db = dir.join("f.db");
    let w = world(true, Reversibility::Reversible);
    let (core, clock, goal, _r) = setup(db.to_str().unwrap(), &w, AcceptancePolicy::Owner);
    core.add_repair(&owner(&core), goal, "fix.run", "restart-service")
        .unwrap();
    // The policy row is removed behind Core's back (the owner's setup context).
    let c = rusqlite::Connection::open(&db).unwrap();
    let n = c
        .execute("DELETE FROM policy WHERE actor='repair'", [])
        .unwrap();
    assert_eq!(n, 1, "add_repair made exactly one policy row");
    let ends = pass(&core, &clock);
    assert!(matches!(ends[0], WatchEnd::Failed { .. }), "{ends:?}");
    assert_eq!(w.ran.load(Ordering::SeqCst), 0);
    let refused: Vec<_> = core
        .store()
        .evidence_all()
        .unwrap()
        .into_iter()
        .filter(|e| e.call == "fix.run")
        .map(|e| e.output)
        .collect();
    assert_eq!(refused, vec![Deny::NoPolicy.said().to_owned()]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_repair_row_is_one_reversible_use_per_item_and_owner_made() {
    let w = world(true, Reversibility::Reversible);
    let (core, _clock, goal, _r) = setup(":memory:", &w, AcceptancePolicy::Owner);
    core.add_repair(&owner(&core), goal, "fix.run", "restart-service")
        .unwrap();
    let row = core
        .store()
        .policy()
        .unwrap()
        .into_iter()
        .find(|p| p.actor == "repair")
        .unwrap();
    assert_eq!(row.call, "fix.run");
    assert_eq!(row.band, Reversibility::Reversible);
    assert_eq!(row.max_uses_per_item, Some(1));
    // The goal has one repair: a second is refused, and nothing changes.
    let again = core.add_repair(&owner(&core), goal, "fix.run", "other");
    assert!(again.is_err());
}

#[test]
fn a_repair_that_is_irreversible_outward_observe_unknown_or_out_of_scope_is_refused() {
    for band in [
        Reversibility::Irreversible,
        Reversibility::OutwardReversible,
        Reversibility::Observe,
    ] {
        let w = world(true, band);
        let (core, _clock, goal, _r) = setup(":memory:", &w, AcceptancePolicy::Owner);
        let got = core.add_repair(&owner(&core), goal, "fix.run", "restart-service");
        assert!(got.is_err(), "{band:?} must be refused at registration");
        assert!(
            core.store()
                .policy()
                .unwrap()
                .iter()
                .all(|p| p.actor != "repair"),
            "a refused repair leaves no policy row"
        );
    }
    let w = world(true, Reversibility::Reversible);
    let (core, _clock, goal, _r) = setup(":memory:", &w, AcceptancePolicy::Owner);
    assert!(
        core.add_repair(&owner(&core), goal, "no.such", "x")
            .is_err(),
        "an unknown call"
    );
    assert!(
        core.add_repair(&owner(&core), goal, "p.look", "x").is_err(),
        "an observe call is not a repair"
    );
    // A call the goal's scope does not name is refused too.
    let other = core
        .add_goal(
            &owner(&core),
            NewGoal {
                title: "other".into(),
                why: "w".into(),
                criteria: vec![Criterion {
                    text: "c".into(),
                    check: None,
                }],
                scope: vec!["p.look".into()],
                tier: Tier::Internal,
                spend_cap_micros: 0,
            },
        )
        .unwrap();
    assert!(
        core.add_repair(&owner(&core), other, "fix.run", "restart-service")
            .is_err(),
        "out of the goal's scope"
    );
}

#[test]
fn a_repair_that_fails_to_run_is_recorded_and_the_item_fails() {
    let w = world(true, Reversibility::Reversible);
    w.fails.store(true, Ordering::SeqCst);
    let (core, clock, goal, _r) = setup(":memory:", &w, AcceptancePolicy::Owner);
    core.add_repair(&owner(&core), goal, "fix.run", "restart-service")
        .unwrap();
    let ends = pass(&core, &clock);
    assert!(
        matches!(ends[0], WatchEnd::Failed { failures: 1, .. }),
        "{ends:?}"
    );
    assert_eq!(
        fix_rows(&core).iter().map(|r| r.0).collect::<Vec<_>>(),
        vec![Kind::Intent, Kind::Result]
    );
}

#[test]
fn a_goal_without_a_repair_behaves_as_before() {
    let w = world(true, Reversibility::Reversible);
    let (core, clock, _goal, _r) = setup(":memory:", &w, AcceptancePolicy::Owner);
    let ends = pass(&core, &clock);
    assert!(
        matches!(ends[0], WatchEnd::Failed { failures: 1, .. }),
        "{ends:?}"
    );
    assert_eq!(w.ran.load(Ordering::SeqCst), 0);
    assert!(fix_rows(&core).is_empty());
}

/// A routine's item opened before the owner's word, now checked with a failing probe.
fn failed_check_after(word: &str, core: &Core, clock: &ManualClock, r: RoutineId) -> WatchEnd {
    clock.advance(1_000);
    core.fire_due().unwrap();
    match word {
        "pause" => core.pause_routine(&owner(core), r).unwrap(),
        _ => core.retire_routine(&owner(core), r).unwrap(),
    }
    let it = core.store().items_in(ItemState::Open).unwrap()[0].id;
    core.start(&actor_named("runner"), it).unwrap();
    core.check_watch(it).unwrap().1
}

#[test]
fn a_paused_or_retired_routine_never_triggers_a_repair() {
    for word in ["pause", "retire"] {
        let w = world(true, Reversibility::Reversible);
        let (core, clock, goal, r) = setup(":memory:", &w, AcceptancePolicy::AutoObserve);
        core.add_repair(&owner(&core), goal, "fix.run", "restart-service")
            .unwrap();
        let end = failed_check_after(word, &core, &clock, r);
        // The item fails as it always did: no action ran and no row names the repair actor.
        assert!(
            matches!(end, WatchEnd::Failed { failures: 1, .. }),
            "{word}: {end:?}"
        );
        assert_eq!(w.ran.load(Ordering::SeqCst), 0, "{word}");
        assert!(fix_rows(&core).is_empty(), "{word}");
        assert!(core.verify_evidence().unwrap().broken.is_none());
    }
}

#[test]
fn a_repair_held_routine_still_respects_pause_and_resume() {
    let w = world(true, Reversibility::Reversible);
    let (core, clock, goal, r) = setup(":memory:", &w, AcceptancePolicy::AutoObserve);
    core.add_repair(&owner(&core), goal, "fix.run", "restart-service")
        .unwrap();
    assert_eq!(pass(&core, &clock), vec![WatchEnd::Repaired]);
    let item = core.store().items_in(ItemState::Running).unwrap()[0].id;
    // The owner pauses the routine while its repaired item waits: nothing fires, not even a skip.
    core.pause_routine(&owner(&core), r).unwrap();
    clock.advance(5_000);
    assert!(core.fire_due().unwrap().is_empty());
    // Accepting the held item does not lift the pause; the routine stays quiet until resumed.
    assert!(matches!(
        core.accept(&owner(&core), item).unwrap(),
        Acceptance::Accepted { .. }
    ));
    clock.advance(5_000);
    assert!(core.fire_due().unwrap().is_empty());
    assert!(core.store().routine(r).unwrap().paused);
    core.resume_routine(&owner(&core), r).unwrap();
    clock.advance(5_000);
    assert!(!core.fire_due().unwrap().is_empty());
    assert_eq!(w.ran.load(Ordering::SeqCst), 1);
}

#[test]
fn a_schema_9_file_gains_the_repairs_table_and_keeps_its_paused_and_retired_routines() {
    use endora_foundation::infrastructure::SCHEMA_VERSION;
    let dir = std::env::temp_dir().join(format!("endora-repair-migrate-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let db = dir.join("f.db");
    let w = world(true, Reversibility::Reversible);
    let (core, clock, goal, paused) = setup(db.to_str().unwrap(), &w, AcceptancePolicy::Owner);
    let retired = core
        .add_routine(&owner(&core), goal, "second", 1_000, false, 11_000)
        .unwrap();
    core.pause_routine(&owner(&core), paused).unwrap();
    core.retire_routine(&owner(&core), retired).unwrap();
    let goals_before = core.store().goals().unwrap().len();
    drop(core);
    // The shape PR 20's build wrote: the retired column, and no repairs table.
    let c = rusqlite::Connection::open(&db).unwrap();
    c.execute_batch("DROP TABLE repairs; PRAGMA user_version = 9;")
        .unwrap();
    drop(c);
    let mut core = Core::open(db.to_str().unwrap(), clock.clone()).unwrap();
    core.register(w.clone()).unwrap();
    let v: i64 = rusqlite::Connection::open(&db)
        .unwrap()
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(v, SCHEMA_VERSION);
    assert_eq!(SCHEMA_VERSION, 12);
    assert_eq!(core.store().goals().unwrap().len(), goals_before);
    assert!(core.store().repair(goal).unwrap().is_none());
    // The paused routine is still paused and the retired one retired; neither fires.
    let p = core.store().routine(paused).unwrap();
    assert!(p.paused && !p.retired);
    assert!(core.store().routine(retired).unwrap().retired);
    clock.advance(5_000);
    assert!(core.fire_due().unwrap().is_empty());
    // The new table works on the migrated file.
    core.add_repair(&owner(&core), goal, "fix.run", "restart-service")
        .unwrap();
    assert!(core.store().repair(goal).unwrap().is_some());
    let _ = std::fs::remove_dir_all(&dir);
}
