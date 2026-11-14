//! Costs and caps are MICRO-USD (1 USD = 1,000,000). A file written in cents is migrated without
//! rewriting its hashed evidence; a spec declares the seconds and turns it can accept, and Core
//! grants them; the sandbox profile a call ran under is a structured, hashed evidence column.

mod common;
use common::{actor_named, owner};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use endora_foundation::application::{CallFailure, CallOutput, Capability, Core, NewGoal, Outcome};
use endora_foundation::domain::{Budget, BudgetMode, CallSpec, Criterion, ItemId, Kind, PolicyRow};
use endora_foundation::infrastructure::{ManualClock, SCHEMA_VERSION};
use endora_kernel::{Reversibility, Tier};
use rusqlite::Connection;
use sha2::{Digest, Sha256};

/// Records the budget it was granted and reports a fixed cost and profile.
struct Rec {
    seen: Mutex<Vec<Budget>>,
    seconds: u64,
    turns: Option<u32>,
    cost: u64,
}

impl Rec {
    fn new(seconds: u64, turns: Option<u32>, cost: u64) -> Arc<Self> {
        Arc::new(Self {
            seen: Mutex::new(Vec::new()),
            seconds,
            turns,
            cost,
        })
    }
}

impl Capability for Rec {
    fn offers(&self) -> Vec<CallSpec> {
        let mut s = CallSpec::new("x.run", Reversibility::Reversible);
        s.max_cost_micros = 100_000;
        s.budget = BudgetMode::Enforced { units: 1_000_000 };
        s.max_seconds = self.seconds;
        s.max_turns = self.turns;
        vec![s]
    }
    fn call(&self, _call: &str, _input: &str) -> Result<CallOutput, CallFailure> {
        Ok(CallOutput {
            cost_micros: self.cost,
            facts: Vec::new(),
            result: "ok".into(),
            evidence: String::new(),
            profile: Some("test-profile".into()),
        })
    }
    fn call_budgeted(
        &self,
        call: &str,
        input: &str,
        budget: &Budget,
    ) -> Result<CallOutput, CallFailure> {
        self.seen.lock().unwrap().push(budget.clone());
        self.call(call, input)
    }
}

fn engine(path: &str, cap: &Arc<Rec>) -> (Core, ItemId) {
    let mut core = Core::open(path, Arc::new(ManualClock::new(5_000))).unwrap();
    core.register(cap.clone()).unwrap();
    let o = owner(&core);
    core.set_overall_cap(&o, 5_000_000).unwrap();
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
                scope: vec!["x.run".into()],
                tier: Tier::Internal,
                spend_cap_micros: 1_000_000,
            },
        )
        .unwrap();
    core.grant(
        &o,
        PolicyRow {
            actor: "p".into(),
            call: "x.run".into(),
            band: Reversibility::Reversible,
            tier: Tier::Internal,
            max_uses_per_item: None,
        },
    )
    .unwrap();
    let item = core.add_item(g, "i").unwrap();
    core.start(&actor_named("runner"), item).unwrap();
    (core, item)
}

fn run(core: &Core, item: ItemId) -> Outcome {
    core.call(&actor_named("p"), item, "x.run", "").unwrap()
}

fn path(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("micro-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("c.db")
}

// ---- seconds and turns are declared by the spec and granted by Core ---------------------------

#[test]
fn the_granted_seconds_and_turns_are_the_specs_maximum_and_reach_the_capability() {
    let cap = Rec::new(90, Some(3), 70_000);
    let (core, item) = engine(":memory:", &cap);
    assert!(matches!(run(&core, item), Outcome::Done(_)));
    assert_eq!(
        cap.seen.lock().unwrap()[0],
        Budget {
            max_cost_micros: 100_000,
            max_seconds: 90,
            max_turns: Some(3)
        }
    );
}

#[test]
fn a_spec_that_declares_nothing_gets_todays_grant_of_600_seconds_and_no_turns() {
    let spec = CallSpec::new("x.y", Reversibility::Observe);
    assert_eq!((spec.max_seconds, spec.max_turns), (600, None));
    let cap = Rec::new(600, None, 0);
    let (core, item) = engine(":memory:", &cap);
    run(&core, item);
    let b = cap.seen.lock().unwrap()[0].clone();
    assert_eq!((b.max_seconds, b.max_turns), (600, None));
}

// ---- the profile is a structured column covered by the chain ----------------------------------

#[test]
fn the_profile_is_a_structured_column_and_editing_it_breaks_the_chain() {
    let p = path("profile");
    let cap = Rec::new(600, None, 70_000);
    let (core, item) = engine(p.to_str().unwrap(), &cap);
    assert!(matches!(run(&core, item), Outcome::Done(_)));
    let ev = core.store().evidence_for(item).unwrap();
    let res = ev.iter().find(|e| e.kind == Kind::Result).unwrap();
    assert_eq!(res.profile.as_deref(), Some("test-profile"));
    assert!(core.verify_evidence().unwrap().broken.is_none());
    {
        let db = core.store().db().lock().unwrap();
        db.execute_batch("DROP TRIGGER evidence_no_change").unwrap();
        db.execute(
            "UPDATE evidence SET profile = 'none' WHERE id = ?1",
            [res.id],
        )
        .unwrap();
    }
    let b = core.verify_evidence().unwrap().broken.expect("detected");
    assert_eq!(b.id, res.id);
}

// ---- a file written in cents is migrated -------------------------------------------------------

fn field(out: &mut String, v: &str) {
    out.push_str(&format!("{}:{v}\n", v.len()));
}

/// The canonical hash a build before the micro-USD unit wrote (cost in cents, no profile).
fn legacy_hash(prev: &str, c: &[String]) -> String {
    let mut s = String::new();
    field(&mut s, prev);
    for v in c {
        field(&mut s, v);
    }
    Sha256::digest(s.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Turns a file the current build wrote into the file the previous build would have written:
/// cents everywhere, chained with the old canonical text, schema version 5. Returns the head.
fn make_cents_file(p: &str) -> String {
    let c = Connection::open(p).unwrap();
    c.execute_batch(
        "DROP TRIGGER evidence_no_change; DROP TRIGGER evidence_no_removal; DROP TRIGGER evidence_no_replace;",
    )
    .unwrap();
    type Row = (
        i64,
        Option<i64>,
        String,
        String,
        String,
        String,
        String,
        String,
        Option<i64>,
        String,
        String,
        i64,
        i64,
    );
    let rows: Vec<Row> = c
        .prepare("SELECT id, item_id, call, actor, actor_kind, band, decision, kind, intent_id, inputs_hash, output, at_ms, cost_micros FROM evidence ORDER BY id")
        .unwrap()
        .query_map([], |r| {
            Ok((
                r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?,
                r.get(7)?, r.get(8)?, r.get(9)?, r.get(10)?, r.get(11)?, r.get(12)?,
            ))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();
    let mut prev = "0".repeat(64);
    for r in rows {
        assert_eq!(r.12 % 10_000, 0, "a whole number of cents");
        let cents = r.12 / 10_000;
        let opt = |v: Option<i64>| v.map_or("-".to_owned(), |n| n.to_string());
        let h = legacy_hash(
            &prev,
            &[
                r.0.to_string(),
                opt(r.1),
                r.2,
                r.3,
                r.4,
                r.5,
                r.6,
                r.7,
                opt(r.8),
                r.9,
                r.10,
                r.11.to_string(),
                cents.to_string(),
            ],
        );
        c.execute(
            "UPDATE evidence SET cost_cents = ?1, prev_hash = ?2, hash = ?3 WHERE id = ?4",
            rusqlite::params![cents, prev, h, r.0],
        )
        .unwrap();
        prev = h;
    }
    c.execute_batch(
        "ALTER TABLE evidence DROP COLUMN cost_micros; ALTER TABLE evidence DROP COLUMN profile;
         UPDATE goals SET spend_cap_micros = spend_cap_micros / 10000;
         ALTER TABLE goals RENAME COLUMN spend_cap_micros TO spend_cap_cents;
         UPDATE control SET key = 'overall_cap_cents', value = value / 10000 WHERE key = 'overall_cap_micros';
         CREATE TRIGGER evidence_no_change BEFORE UPDATE ON evidence BEGIN SELECT RAISE(ABORT, 'evidence is append-only'); END;
         CREATE TRIGGER evidence_no_removal BEFORE DELETE ON evidence BEGIN SELECT RAISE(ABORT, 'evidence is append-only'); END;
         CREATE TRIGGER evidence_no_replace BEFORE INSERT ON evidence WHEN EXISTS (SELECT 1 FROM evidence WHERE id = NEW.id) BEGIN SELECT RAISE(ABORT, 'evidence is append-only'); END;
         PRAGMA user_version = 5;",
    )
    .unwrap();
    prev
}

#[test]
fn a_database_holding_cents_is_migrated_to_micro_usd_without_re_signing_history() {
    let p = path("cents");
    let ps = p.to_str().unwrap();
    let cap = Rec::new(600, None, 70_000);
    {
        let (core, item) = engine(ps, &cap);
        assert!(matches!(run(&core, item), Outcome::Done(_)));
    }
    let head = make_cents_file(ps);
    // The file really is a cents file now: 100 cents goal cap, 500 cents overall, 7 cents spent.
    let c = Connection::open(ps).unwrap();
    let cap_cents: i64 = c
        .query_row("SELECT spend_cap_cents FROM goals", [], |r| r.get(0))
        .unwrap();
    assert_eq!(cap_cents, 100);
    drop(c);

    let core = Core::open(ps, Arc::new(ManualClock::new(6_000))).unwrap();
    let c = Connection::open(ps).unwrap();
    let v: i64 = c
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(v, SCHEMA_VERSION);
    assert_eq!(core.store().goals().unwrap()[0].spend_cap_micros, 1_000_000);
    assert_eq!(
        core.store().control("overall_cap_micros").unwrap(),
        5_000_000
    );
    assert_eq!(core.store().control("overall_cap_cents").unwrap(), 0);
    assert_eq!(core.store().spent_micros(None).unwrap(), 70_000);
    let ev = core.store().evidence_all().unwrap();
    let spent_row = ev.iter().find(|e| e.cost_micros > 0).unwrap();
    assert_eq!(spent_row.cost_micros, 70_000, "7 cents read as micro-USD");
    assert_eq!(spent_row.profile, None, "an old row has no profile");
    // History was not re-signed: the old chain still verifies, with the head it had.
    let r = core.verify_evidence().unwrap();
    assert!(r.broken.is_none(), "{r:?}");
    assert_eq!(r.head, head);
    drop(core);

    // New rows are in micro-USD, carry the profile, and extend the same chain.
    let (core, item) = {
        let mut core = Core::open(ps, Arc::new(ManualClock::new(7_000))).unwrap();
        core.register(Rec::new(600, None, 30_000)).unwrap();
        let item = core.store().items().unwrap()[0].id;
        (core, item)
    };
    assert!(matches!(run(&core, item), Outcome::Done(_)));
    assert_eq!(core.store().spent_micros(None).unwrap(), 100_000);
    let r = core.verify_evidence().unwrap();
    assert!(r.broken.is_none(), "{r:?}");
    assert_ne!(r.head, head);
    let ev = core.store().evidence_all().unwrap();
    assert_eq!(ev.last().unwrap().profile.as_deref(), Some("test-profile"));
}

#[test]
fn a_cap_too_large_to_scale_saturates_and_never_wraps() {
    let p = path("huge");
    let ps = p.to_str().unwrap();
    {
        let _ = engine(ps, &Rec::new(600, None, 0));
    }
    make_cents_file(ps);
    Connection::open(ps)
        .unwrap()
        .execute_batch(&format!(
            "UPDATE goals SET spend_cap_cents = {max}; UPDATE control SET value = {max}",
            max = i64::MAX
        ))
        .unwrap();
    let core = Core::open(ps, Arc::new(ManualClock::new(6_000))).unwrap();
    assert_eq!(
        core.store().goals().unwrap()[0].spend_cap_micros,
        u64::try_from(i64::MAX).unwrap()
    );
    assert_eq!(
        core.store().control("overall_cap_micros").unwrap(),
        i64::MAX
    );
}
