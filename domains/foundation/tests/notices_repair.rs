//! Owner notices (ADR 0152) and the repair policy (ADR 0151) on one watch. A repair that heals a
//! failing watch before the grace leaves no "failing" notice; a watch whose repair does not help,
//! or fails, still says "failing" once; the owner's pause does not unsay a run already failing.

mod common;
use common::{actor_named, owner};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use endora_foundation::application::{
    CallFailure, CallOutput, Capability, Core, NewGoal, WatchEnd,
};
use endora_foundation::domain::{
    CallSpec, Check, Criterion, Fact, Field, GRACE_MS, ItemState, NoticeState, PolicyRow, RoutineId,
};
use endora_foundation::infrastructure::ManualClock;
use endora_kernel::{Reversibility, Tier};

const MIN: i64 = 60_000;

/// A probe and a repair action over one pretend service: the action brings it up when `fixes`, and
/// errors when `fails`.
struct World {
    up: AtomicBool,
    fixes: AtomicBool,
    fails: AtomicBool,
    ran: AtomicU32,
}

impl Capability for World {
    fn offers(&self) -> Vec<CallSpec> {
        vec![
            CallSpec::new("p.look", Reversibility::Observe).with_facts(vec![Field::bool("up")]),
            {
                let mut s = CallSpec::new("fix.run", Reversibility::Reversible);
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

fn lab(fixes: bool, fails: bool) -> (Core, Arc<ManualClock>, Arc<World>, RoutineId) {
    let w = Arc::new(World {
        up: AtomicBool::new(false),
        fixes: AtomicBool::new(fixes),
        fails: AtomicBool::new(fails),
        ran: AtomicU32::new(0),
    });
    let clock = Arc::new(ManualClock::new(1_000_000));
    let mut core = Core::open(":memory:", clock.clone()).unwrap();
    core.register(w.clone()).unwrap();
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
                scope: vec!["p.look".into(), "fix.run".into()],
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
    core.add_repair(&owner(&core), goal, "fix.run", "restart-service")
        .unwrap();
    let r = core
        .add_routine_limited(&owner(&core), goal, "web front", MIN, false, 0, 50)
        .unwrap();
    (core, clock, w, r)
}

/// One minute passes, due slots fire, each open item is checked.
fn pass(core: &Core, clock: &ManualClock) -> Vec<WatchEnd> {
    clock.advance(MIN);
    core.fire_due().unwrap();
    let mut ends = Vec::new();
    for it in core.store().items_in(ItemState::Open).unwrap() {
        core.start(&actor_named("runner"), it.id).unwrap();
        ends.push(core.check_watch(it.id).unwrap().1);
    }
    ends
}

fn states(core: &Core) -> Vec<NoticeState> {
    core.store()
        .notices(true)
        .unwrap()
        .iter()
        .map(|n| n.state)
        .collect()
}

#[test]
fn a_repair_that_fixes_a_failing_watch_before_the_grace_makes_no_failing_notice() {
    let (core, clock, w, _r) = lab(true, false);
    assert_eq!(pass(&core, &clock), vec![WatchEnd::Repaired]);
    assert_eq!(w.ran.load(Ordering::SeqCst), 1);
    // Long past the grace, with every sweep the loop makes: nothing was ever failing.
    clock.advance(GRACE_MS * 3);
    assert_eq!(core.sweep_notices().unwrap(), 0);
    assert!(states(&core).is_empty());
    assert!(core.verify_evidence().unwrap().broken.is_none());
}

#[test]
fn a_failing_watch_whose_repair_does_not_help_or_fails_still_says_failing_once() {
    for (fixes, fails) in [(false, false), (false, true)] {
        let (core, clock, w, _r) = lab(fixes, fails);
        // The grace is 10 minutes and three failures: run well past both, one check a minute.
        for _ in 0..16 {
            pass(&core, &clock);
        }
        assert_eq!(
            states(&core),
            vec![NoticeState::Failing],
            "fixes={fixes} fails={fails}"
        );
        // The repair ran once for the whole failure episode; the notice is not repeated.
        assert_eq!(
            w.ran.load(Ordering::SeqCst),
            1,
            "fixes={fixes} fails={fails}"
        );
        assert_eq!(core.sweep_notices().unwrap(), 0);
        assert_eq!(states(&core).len(), 1);
    }
}

#[test]
fn pausing_a_routine_already_failing_still_says_failing_after_the_grace() {
    let (core, clock, _w, r) = lab(false, false);
    // Three failures in a row, inside the grace: nothing said yet.
    for _ in 0..3 {
        pass(&core, &clock);
    }
    assert!(states(&core).is_empty());
    core.pause_routine(&owner(&core), r).unwrap();
    // Paused, it checks no more, but it is still down: the grace ends and the owner is told.
    clock.advance(GRACE_MS);
    assert_eq!(core.sweep_notices().unwrap(), 1);
    assert_eq!(states(&core), vec![NoticeState::Failing]);
}

#[test]
fn a_repair_that_heals_a_run_already_said_makes_its_one_recovered_notice() {
    let (core, clock, w, r) = lab(false, false);
    for _ in 0..16 {
        pass(&core, &clock);
    }
    assert_eq!(states(&core), vec![NoticeState::Failing]);
    // The owner fixes the cause of the repair, and resumes: the next failure is a new episode, its
    // repair now works, and the healed check ends the said run.
    w.fixes.store(true, Ordering::SeqCst);
    core.resume_routine(&owner(&core), r).unwrap();
    let ends = pass(&core, &clock);
    assert_eq!(ends, vec![WatchEnd::Repaired]);
    assert_eq!(w.ran.load(Ordering::SeqCst), 2);
    assert_eq!(
        states(&core),
        vec![NoticeState::Failing, NoticeState::Recovered]
    );
}
