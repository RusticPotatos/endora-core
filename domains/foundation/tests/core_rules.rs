//! The engine's rules against a FAKE capability defined in this file: nothing here depends on
//! any real capability (T6), and the append-only (T4) and restore (T5) tests live here because
//! they need only the engine and the store.

mod common;
use common::{actor_named, owner};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Instant;

use endora_foundation::application::{
    Acceptance, CallFailure, CallOutput, Capability, Core, NewGoal, Outcome,
};
use endora_foundation::domain::{
    AcceptRefusal, CallSpec, Check, Criterion, Deny, Fact, ItemState, PolicyRow,
};
use endora_foundation::infrastructure::ManualClock;
use endora_kernel::{Reversibility, Tier};

#[derive(Default)]
struct Fake {
    reads: AtomicU32,
    acts: AtomicU32,
    wipes: AtomicU32,
}

fn spec(name: &str, band: Option<Reversibility>) -> CallSpec {
    CallSpec {
        name: name.into(),
        band,
        input: "text".into(),
        output: "text".into(),
        tier: Tier::Internal,
        max_cost_micros: 0,
        max_seconds: endora_foundation::domain::DEFAULT_MAX_SECONDS,
        max_turns: None,
        inputs: vec![endora_foundation::domain::Field::text("input")],
        facts: vec![endora_foundation::domain::Field::word("status", &["ok"])],
        budget: Default::default(),
    }
}

impl Capability for Fake {
    fn offers(&self) -> Vec<CallSpec> {
        vec![
            spec("fake.read", Some(Reversibility::Observe)),
            spec("fake.act", Some(Reversibility::Reversible)),
            spec("fake.wipe", Some(Reversibility::Irreversible)),
        ]
    }
    fn call(&self, call: &str, _input: &str) -> Result<CallOutput, CallFailure> {
        let n = match call {
            "fake.read" => &self.reads,
            "fake.act" => &self.acts,
            _ => &self.wipes,
        };
        n.fetch_add(1, Ordering::SeqCst);
        Ok(CallOutput {
            cost_micros: 0,
            profile: None,
            facts: vec![Fact::word("status", "ok")],
            result: "ok".into(),
            evidence: "fake evidence".into(),
        })
    }
}

fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("core-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn goal(scope: &[&str]) -> NewGoal {
    NewGoal {
        title: "keep service X healthy".into(),
        why: "the house depends on it".into(),
        criteria: vec![Criterion {
            text: "X answers".into(),
            check: Some(Check {
                call: "fake.read".into(),
                input: "x".into(),
                field: "status".into(),
                expect: "ok".into(),
            }),
        }],
        scope: scope.iter().map(|s| (*s).to_owned()).collect(),
        tier: Tier::Internal,
        spend_cap_micros: 0,
    }
}

fn row(actor: &str, call: &str, band: Reversibility) -> PolicyRow {
    PolicyRow {
        actor: actor.into(),
        call: call.into(),
        band,
        tier: Tier::Internal,
        max_uses_per_item: None,
    }
}

fn with_fake(db: &str, clock: Arc<ManualClock>) -> (Core, Arc<Fake>) {
    let mut core = Core::open(db, clock).unwrap();
    let fake = Arc::new(Fake::default());
    core.register(fake.clone()).unwrap();
    (core, fake)
}

#[test]
fn t6_the_engine_holds_decides_and_records_with_no_capability_present() {
    let clock = Arc::new(ManualClock::new(1_000));
    let core = Core::in_memory(clock).unwrap();
    // No capability is present, so a structured check cannot be declared: slice 5a validates
    // checks against the declared facts at goal creation. The goal carries a plain criterion.
    let mut g = goal(&["fake.read"]);
    g.criteria[0].check = None;
    let g = core.add_goal(&owner(&core), g).unwrap();
    let item = core.add_item(g, "one-off").unwrap();
    core.start(&actor_named("runner"), item).unwrap();
    let out = core
        .call(&actor_named("tester"), item, "fake.read", "x")
        .unwrap();
    assert!(matches!(out, Outcome::Refused(Deny::UnknownCall)));
    // Two rows now: `start` records who started the item, then the refusal.
    let ev = core.store().evidence_for(item).unwrap();
    assert_eq!(ev.len(), 2);
    assert!(!ev[1].allowed);
}

#[test]
fn default_deny_then_a_grant_allows_and_the_declared_band_cannot_be_talked_up() {
    let clock = Arc::new(ManualClock::new(1_000));
    let (core, fake) = with_fake(":memory:", clock);
    let g = core
        .add_goal(&owner(&core), goal(&["fake.read", "fake.act", "fake.wipe"]))
        .unwrap();
    let item = core.add_item(g, "work").unwrap();
    core.start(&actor_named("runner"), item).unwrap();
    assert!(matches!(
        core.call(&actor_named("tester"), item, "fake.read", "x")
            .unwrap(),
        Outcome::Refused(Deny::NoPolicy)
    ));
    core.grant(
        &owner(&core),
        row("tester", "fake.read", Reversibility::Observe),
    )
    .unwrap();
    assert!(matches!(
        core.call(&actor_named("tester"), item, "fake.read", "x")
            .unwrap(),
        Outcome::Done(_)
    ));
    // A row that says observe does not cover a reversible call, whatever the actor claims.
    core.grant(
        &owner(&core),
        row("tester", "fake.act", Reversibility::Observe),
    )
    .unwrap();
    assert!(matches!(
        core.call(&actor_named("tester"), item, "fake.act", "x")
            .unwrap(),
        Outcome::Refused(Deny::BandAboveRow)
    ));
    // Irreversible is blocked (an unclassified call cannot be registered: tests/registration.rs), and no row for them can even be granted.
    assert!(
        core.grant(
            &owner(&core),
            row("tester", "fake.wipe", Reversibility::Irreversible)
        )
        .is_err()
    );
    assert!(matches!(
        core.call(&actor_named("tester"), item, "fake.wipe", "x")
            .unwrap(),
        Outcome::Refused(Deny::Irreversible)
    ));
    // (Only the owner grants: `grant` takes an OwnerProof, so a proposer cannot compile it.)
    assert_eq!(
        (
            fake.reads.load(Ordering::SeqCst),
            fake.acts.load(Ordering::SeqCst),
            fake.wipes.load(Ordering::SeqCst)
        ),
        (1, 0, 0)
    );
}

#[test]
fn the_stop_switch_blocks_all_calls_anyone_may_stop_and_only_the_owner_clears_it() {
    let clock = Arc::new(ManualClock::new(1_000));
    let (core, fake) = with_fake(":memory:", clock);
    let g = core.add_goal(&owner(&core), goal(&["fake.read"])).unwrap();
    core.grant(
        &owner(&core),
        row("tester", "fake.read", Reversibility::Observe),
    )
    .unwrap();
    let item = core.add_item(g, "work").unwrap();
    core.start(&actor_named("runner"), item).unwrap();
    core.stop(&actor_named("proposer")).unwrap();
    assert!(matches!(
        core.call(&actor_named("tester"), item, "fake.read", "x")
            .unwrap(),
        Outcome::Refused(Deny::Stopped)
    ));
    assert_eq!(fake.reads.load(Ordering::SeqCst), 0);
    core.clear_stop(&owner(&core)).unwrap();
    assert!(matches!(
        core.call(&actor_named("tester"), item, "fake.read", "x")
            .unwrap(),
        Outcome::Done(_)
    ));
}

#[test]
fn nothing_closes_itself_criteria_met_waits_for_an_authorised_acceptance() {
    let clock = Arc::new(ManualClock::new(1_000));
    let (core, _) = with_fake(":memory:", clock);
    let g = core.add_goal(&owner(&core), goal(&["fake.read"])).unwrap();
    core.grant(
        &owner(&core),
        row("tester", "fake.read", Reversibility::Observe),
    )
    .unwrap();
    // The engine's own checks run as `core`: the owner grants it the read as well.
    core.grant(
        &owner(&core),
        row("core", "fake.read", Reversibility::Observe),
    )
    .unwrap();
    let item = core.add_item(g, "work").unwrap();
    // Not running: not acceptable.
    assert!(matches!(
        core.accept(&owner(&core), item).unwrap(),
        Acceptance::Refused(AcceptRefusal::NotRunning(ItemState::Open))
    ));
    core.start(&actor_named("runner"), item).unwrap();
    // Running, criteria not yet checked: refused, and recorded as a refusal.
    assert!(matches!(
        core.accept(&owner(&core), item).unwrap(),
        Acceptance::Refused(AcceptRefusal::CriteriaNotMet)
    ));
    assert_eq!(core.verify(item).unwrap(), vec![true]);
    // Criteria met, still running: it has not closed itself.
    assert_eq!(core.store().item(item).unwrap().state, ItemState::Running);
    // (A proposer cannot accept: `accept` takes an OwnerProof, so it cannot compile.)
    assert_eq!(core.store().item(item).unwrap().state, ItemState::Running);
    assert!(matches!(
        core.accept(&owner(&core), item).unwrap(),
        Acceptance::Accepted { .. }
    ));
    let done = core.store().item(item).unwrap();
    assert_eq!(
        (done.state, done.accepted_by.as_deref()),
        (ItemState::Done, Some("owner"))
    );
    assert!(matches!(
        core.accept(&owner(&core), item).unwrap(),
        Acceptance::Refused(AcceptRefusal::NotRunning(ItemState::Done))
    ));
    let refusals = core
        .store()
        .evidence_for(item)
        .unwrap()
        .iter()
        .filter(|e| !e.allowed)
        .count();
    assert_eq!(refusals, 3, "each refused acceptance is evidence");
}

#[test]
fn firings_missed_while_down_are_recorded_and_run_once() {
    let clock = Arc::new(ManualClock::new(10_000));
    let (core, _) = with_fake(":memory:", clock.clone());
    let g = core.add_goal(&owner(&core), goal(&["fake.read"])).unwrap();
    let r = core
        .add_routine(&owner(&core), g, "check X", 1_000, false, 10_000)
        .unwrap();
    // Down for five slots: 10_000..=14_000 are due at 14_500.
    clock.set(14_500);
    let fired = core.fire_due().unwrap();
    assert_eq!(fired.len(), 1);
    assert_eq!((fired[0].missed, fired[0].item.is_some()), (4, true));
    let items = core.store().items().unwrap();
    assert_eq!(items.len(), 1, "run ONCE, not five times");
    assert_eq!((items[0].missed, items[0].due_ms), (4, Some(14_000)));
    // Slice 5a: the four missed slots are ONE summary row (with the count), then the fired one.
    assert_eq!(
        core.store().firings(r).unwrap().len(),
        2,
        "one summary row for the missed slots, one for the slot that ran"
    );
    // Asking again at the same time fires nothing: no duplicate.
    assert!(
        core.fire_due()
            .unwrap()
            .iter()
            .all(|f| f.item.is_none() && f.missed == 0)
    );
    assert_eq!(core.store().items().unwrap().len(), 1);
    // The item is still open at the next slot: that firing is skipped, recorded, not stacked.
    clock.set(15_000);
    let again = core.fire_due().unwrap();
    assert!(again[0].skipped_open && again[0].item.is_none());
    assert_eq!(core.store().items().unwrap().len(), 1);
}

#[test]
fn t4_evidence_is_append_only_by_the_api_and_by_the_database() {
    let clock = Arc::new(ManualClock::new(1_000));
    let (core, _) = with_fake(":memory:", clock);
    let g = core.add_goal(&owner(&core), goal(&["fake.read"])).unwrap();
    core.grant(
        &owner(&core),
        row("tester", "fake.read", Reversibility::Observe),
    )
    .unwrap();
    let item = core.add_item(g, "work").unwrap();
    core.start(&actor_named("runner"), item).unwrap();
    core.call(&actor_named("tester"), item, "fake.read", "x")
        .unwrap();
    let before = core.store().evidence_all().unwrap();
    assert!(before.len() >= 3);
    // The database itself refuses a change or a removal.
    let db = core.store().db();
    let conn = db.lock().unwrap();
    assert!(
        conn.execute("UPDATE evidence SET output = 'forged'", [])
            .is_err()
    );
    assert!(conn.execute("DELETE FROM evidence", []).is_err());
    assert!(
        conn.execute("UPDATE evidence SET decision = 'allow' WHERE id = 1", [])
            .is_err()
    );
    drop(conn);
    assert_eq!(core.store().evidence_all().unwrap(), before);
    // And no code path in the crate offers one: the only statements naming the table append or read.
    for file in ["src/infrastructure/store.rs", "src/application/core.rs"] {
        let text =
            std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(file)).unwrap();
        for forbidden in [
            "UPDATE evidence",
            "DELETE FROM evidence",
            "DROP TABLE evidence",
            "evidence SET",
        ] {
            assert!(!text.contains(forbidden), "{file} contains `{forbidden}`");
        }
    }
}

#[test]
fn t5_a_copy_taken_while_idle_restores_to_identical_state() {
    let dir = scratch("restore");
    let live = dir.join("live.db");
    let clock = Arc::new(ManualClock::new(1_000));
    let (core, _) = with_fake(live.to_str().unwrap(), clock);
    let g = core.add_goal(&owner(&core), goal(&["fake.read"])).unwrap();
    core.add_routine(&owner(&core), g, "check X", 1_000, true, 1_000)
        .unwrap();
    core.grant(
        &owner(&core),
        row("tester", "fake.read", Reversibility::Observe),
    )
    .unwrap();
    // The engine's own checks run as `core`: the owner grants it the read as well.
    core.grant(
        &owner(&core),
        row("core", "fake.read", Reversibility::Observe),
    )
    .unwrap();
    core.fire_due().unwrap();
    let item = core.store().items().unwrap()[0].id;
    core.start(&actor_named("runner"), item).unwrap();
    core.verify(item).unwrap();
    core.settle_routine_by_policy(item).unwrap();
    let want = core.store().dump_hash().unwrap();
    drop(core); // idle: nothing has the file open

    let t = Instant::now();
    let copy = dir.join("elsewhere").join("restored.db");
    std::fs::create_dir_all(copy.parent().unwrap()).unwrap();
    std::fs::copy(&live, &copy).unwrap();
    let backup = t.elapsed();
    let t = Instant::now();
    let restored = Core::open(copy.to_str().unwrap(), Arc::new(ManualClock::new(1_000))).unwrap();
    let got = restored.store().dump_hash().unwrap();
    let restore = t.elapsed();
    assert_eq!(got, want, "hash of the canonical dump");
    assert_eq!(restored.store().item(item).unwrap().state, ItemState::Done);
    println!("T5 backup (file copy) {backup:?}, restore (open + dump hash) {restore:?}");

    // The persistence crate's own snapshot (VACUUM INTO), the safe way while running, agrees.
    let snap = dir.join("snap.db");
    endora_persistence::snapshot(&live, &snap).unwrap();
    let from_snap = Core::open(snap.to_str().unwrap(), Arc::new(ManualClock::new(1_000))).unwrap();
    assert_eq!(from_snap.store().dump_hash().unwrap(), want);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Slice 3 item 1. SQLite's REPLACE conflict handling deletes the old row without firing the
/// DELETE trigger, so `INSERT OR REPLACE` / `REPLACE INTO` overwrote evidence (found by the
/// slice 1 verifier). Tried on a copy of a real file store.
#[test]
fn t4b_replace_cannot_overwrite_evidence_or_firings() {
    let dir = scratch("replace");
    let live = dir.join("live.db");
    let (core, _) = with_fake(live.to_str().unwrap(), Arc::new(ManualClock::new(1_000)));
    let g = core.add_goal(&owner(&core), goal(&["fake.read"])).unwrap();
    core.add_routine(&owner(&core), g, "check X", 1_000, true, 1_000)
        .unwrap();
    core.fire_due().unwrap();
    drop(core);
    let copy = dir.join("attacked.db");
    std::fs::copy(&live, &copy).unwrap();
    let attacked = Core::open(copy.to_str().unwrap(), Arc::new(ManualClock::new(1_000))).unwrap();
    let before = attacked.store().dump_hash().unwrap();
    {
        let conn = attacked.store().db().lock().unwrap();
        for sql in [
            "INSERT OR REPLACE INTO evidence (id, call, actor, band, decision, kind, inputs_hash, output, at_ms) VALUES (1,'forged','x','observe','allow','result','h','forged',1)",
            "REPLACE INTO evidence (id, call, actor, band, decision, kind, inputs_hash, output, at_ms) VALUES (1,'forged','x','observe','allow','result','h','forged',1)",
            "INSERT OR REPLACE INTO firings (routine_id, due_ms, outcome) VALUES (1,1000,'missed_down')",
            "REPLACE INTO firings (routine_id, due_ms, outcome) VALUES (1,1000,'missed_down')",
            "UPDATE firings SET outcome = 'missed_down'",
            "DELETE FROM firings",
        ] {
            assert!(conn.execute(sql, []).is_err(), "must be refused: {sql}");
        }
        // A fresh id still appends: the guard is only against overwriting.
        assert!(conn.execute("INSERT INTO evidence (id, call, actor, band, decision, kind, inputs_hash, output, at_ms) VALUES (9999,'new','x','observe','allow','result','h','fine',1)", []).is_ok());
    }
    let after = attacked.store().evidence_all().unwrap();
    assert!(after.iter().all(|e| e.call != "forged"));
    assert_eq!(after.last().unwrap().id, 9999);
    assert_ne!(before, attacked.store().dump_hash().unwrap()); // only the one legitimate append
    let _ = std::fs::remove_dir_all(&dir);
}
