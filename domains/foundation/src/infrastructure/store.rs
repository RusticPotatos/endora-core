//! The store: one SQLite file, one schema, plain reads and writes. Evidence has an append and
//! readers and nothing else; triggers make the database refuse a change or removal too.

use endora_kernel::{RepositoryError, Reversibility, Tier};
use endora_persistence::{Db, backend, corrupt};
use rusqlite::{Connection, OptionalExtension, Row, params, types::ValueRef};
use sha2::{Digest, Sha256};

use super::chain;
use super::holders::{Held, Holders};
use super::legacy_evidence;
use crate::domain::{
    AcceptancePolicy, ActorKind, Check, Criterion, Evidence, Goal, GoalId, ItemId, ItemState, Kind,
    Plan, PolicyRow, Routine, RoutineId, WorkItem, hold::may_move,
};

type Res<T> = Result<T, RepositoryError>;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS goals (
  id INTEGER PRIMARY KEY AUTOINCREMENT, title TEXT NOT NULL, why TEXT NOT NULL,
  tier INTEGER NOT NULL, spend_cap_cents INTEGER NOT NULL, created_ms INTEGER NOT NULL,
  acceptance TEXT NOT NULL DEFAULT 'owner' CHECK (acceptance IN ('owner','auto-observe'))) STRICT;
CREATE TABLE IF NOT EXISTS goal_scope (
  goal_id INTEGER NOT NULL REFERENCES goals(id), call TEXT NOT NULL, PRIMARY KEY (goal_id, call)) STRICT;
CREATE TABLE IF NOT EXISTS routines (
  id INTEGER PRIMARY KEY AUTOINCREMENT, goal_id INTEGER NOT NULL REFERENCES goals(id),
  title TEXT NOT NULL, every_ms INTEGER NOT NULL, next_due_ms INTEGER NOT NULL,
  auto_accept INTEGER NOT NULL, max_failures INTEGER NOT NULL DEFAULT 5,
  consecutive_failures INTEGER NOT NULL DEFAULT 0, paused INTEGER NOT NULL DEFAULT 0) STRICT;
CREATE TABLE IF NOT EXISTS items (
  id INTEGER PRIMARY KEY AUTOINCREMENT, goal_id INTEGER NOT NULL REFERENCES goals(id),
  routine_id INTEGER REFERENCES routines(id), title TEXT NOT NULL,
  state TEXT NOT NULL CHECK (state IN ('open','running','done','blocked','failed')),
  attempts INTEGER NOT NULL DEFAULT 0, due_ms INTEGER, missed INTEGER NOT NULL DEFAULT 0,
  reason TEXT, accepted_by TEXT, accepted_ms INTEGER, created_ms INTEGER NOT NULL, subject TEXT) STRICT;
CREATE TABLE IF NOT EXISTS firings (
  routine_id INTEGER NOT NULL REFERENCES routines(id), due_ms INTEGER NOT NULL,
  outcome TEXT NOT NULL CHECK (outcome IN ('fired','skipped_open','missed_down')),
  item_id INTEGER REFERENCES items(id), missed_count INTEGER NOT NULL DEFAULT 1,
  PRIMARY KEY (routine_id, due_ms)) STRICT;
CREATE TABLE IF NOT EXISTS criteria (
  owner_kind TEXT NOT NULL CHECK (owner_kind IN ('goal','item')), owner_id INTEGER NOT NULL,
  idx INTEGER NOT NULL, text TEXT NOT NULL, check_call TEXT, check_input TEXT, check_expect TEXT, check_field TEXT,
  met_by TEXT, met_evidence_id INTEGER, PRIMARY KEY (owner_kind, owner_id, idx)) STRICT;
-- How each item's criteria were met. The definitions are the goal's (or the item's own rows).
CREATE TABLE IF NOT EXISTS item_met (
  item_id INTEGER NOT NULL REFERENCES items(id), idx INTEGER NOT NULL, met_by TEXT NOT NULL,
  met_evidence_id INTEGER, PRIMARY KEY (item_id, idx)) STRICT;
CREATE TABLE IF NOT EXISTS policy (
  actor TEXT NOT NULL, call TEXT NOT NULL, band TEXT NOT NULL, tier INTEGER NOT NULL,
  max_uses_per_item INTEGER, PRIMARY KEY (actor, call)) STRICT;
CREATE TABLE IF NOT EXISTS control (key TEXT PRIMARY KEY, value INTEGER NOT NULL) STRICT;
CREATE TABLE IF NOT EXISTS evidence (
  id INTEGER PRIMARY KEY AUTOINCREMENT, item_id INTEGER REFERENCES items(id), call TEXT NOT NULL,
  actor TEXT NOT NULL, band TEXT NOT NULL, decision TEXT NOT NULL CHECK (decision IN ('allow','deny')),
  kind TEXT NOT NULL, intent_id INTEGER, inputs_hash TEXT NOT NULL, output TEXT NOT NULL,
  at_ms INTEGER NOT NULL, cost_cents INTEGER NOT NULL DEFAULT 0) STRICT;
CREATE TRIGGER IF NOT EXISTS evidence_no_change BEFORE UPDATE ON evidence
  BEGIN SELECT RAISE(ABORT, 'evidence is append-only'); END;
CREATE TRIGGER IF NOT EXISTS evidence_no_removal BEFORE DELETE ON evidence
  BEGIN SELECT RAISE(ABORT, 'evidence is append-only'); END;
CREATE TRIGGER IF NOT EXISTS evidence_no_replace BEFORE INSERT ON evidence
  WHEN EXISTS (SELECT 1 FROM evidence WHERE id = NEW.id)
  BEGIN SELECT RAISE(ABORT, 'evidence is append-only'); END;
CREATE TRIGGER IF NOT EXISTS firings_no_change BEFORE UPDATE ON firings
  BEGIN SELECT RAISE(ABORT, 'firings are append-only'); END;
CREATE TRIGGER IF NOT EXISTS firings_no_removal BEFORE DELETE ON firings
  BEGIN SELECT RAISE(ABORT, 'firings are append-only'); END;
CREATE TRIGGER IF NOT EXISTS firings_no_replace BEFORE INSERT ON firings
  WHEN EXISTS (SELECT 1 FROM firings WHERE routine_id = NEW.routine_id AND due_ms = NEW.due_ms)
  BEGIN SELECT RAISE(ABORT, 'firings are append-only'); END;
";

/// One step of the schema's history. Every step is safe on a file that already has what it adds
/// (files made before versions existed are at version 0 with an unknown shape).
type Migration = fn(&Connection) -> Res<()>;

/// The ordered history: step `n` takes a file from version `n-1` to `n`.
const MIGRATIONS: [Migration; 8] = [
    migrate_to_slice4_shape,
    migrate_slice5a_watch_columns,
    migrate_actor_kind,
    migrate_owner_credential,
    migrate_evidence_chain,
    migrate_micro_usd,
    migrate_evidence_profile,
    migrate_item_holder,
];

/// The first schema version that has the hash chain (see [`refuse_laundering`]).
const CHAIN_VERSION: i64 = 5;

fn column_missing(c: &Connection, table: &str, column: &str) -> Res<bool> {
    let n: i64 = c
        .query_row(
            &format!("SELECT COUNT(*) FROM pragma_table_info('{table}') WHERE name='{column}'"),
            [],
            |r| r.get(0),
        )
        .map_err(backend)?;
    Ok(n == 0)
}

fn add_column(c: &Connection, table: &str, column: &str, ddl: &str) -> Res<()> {
    if column_missing(c, table, column)? {
        c.execute_batch(&format!("ALTER TABLE {table} ADD COLUMN {column} {ddl}"))
            .map_err(backend)?;
    }
    Ok(())
}

/// Version 1: the tables, and the columns slices 3 and 4 added one by one.
fn migrate_to_slice4_shape(c: &Connection) -> Res<()> {
    c.execute_batch(SCHEMA).map_err(backend)?;
    // Old rows cost nothing; checks written before slice 3 have no field, so they read back as
    // no check (the owner confirms those criteria); goals before slice 4 keep owner acceptance.
    add_column(c, "evidence", "cost_cents", "INTEGER NOT NULL DEFAULT 0")?;
    add_column(c, "items", "subject", "TEXT")?;
    add_column(c, "criteria", "check_field", "TEXT")?;
    add_column(
        c,
        "goals",
        "acceptance",
        "TEXT NOT NULL DEFAULT 'owner' CHECK (acceptance IN ('owner','auto-observe'))",
    )?;
    // The policy is fixed when the owner creates the goal: the database refuses a change.
    c.execute_batch("CREATE TRIGGER IF NOT EXISTS goals_acceptance_fixed BEFORE UPDATE OF acceptance ON goals BEGIN SELECT RAISE(ABORT, 'acceptance policy is fixed at creation'); END;")
        .map_err(backend)
}

/// Version 2: a routine's failure cap and pause, and the count a summary firing stands for.
fn migrate_slice5a_watch_columns(c: &Connection) -> Res<()> {
    add_column(c, "routines", "max_failures", "INTEGER NOT NULL DEFAULT 5")?;
    add_column(
        c,
        "routines",
        "consecutive_failures",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    add_column(c, "routines", "paused", "INTEGER NOT NULL DEFAULT 0")?;
    add_column(c, "firings", "missed_count", "INTEGER NOT NULL DEFAULT 1")
}

/// Version 3: every evidence row records the actor KIND next to the name. Older rows get the
/// kind their name implies (see `legacy_evidence`).
fn migrate_actor_kind(c: &Connection) -> Res<()> {
    add_column(
        c,
        "evidence",
        "actor_kind",
        "TEXT NOT NULL DEFAULT 'capability'",
    )?;
    legacy_evidence::backfill_actor_kind(c)
}

/// Version 4: the owner credential (a salted hash, never the token), one row, never changed.
fn migrate_owner_credential(c: &Connection) -> Res<()> {
    c.execute_batch(
        "CREATE TABLE IF NOT EXISTS owner_credential (
           id INTEGER PRIMARY KEY CHECK (id = 1), salt TEXT NOT NULL, hash TEXT NOT NULL,
           created_ms INTEGER NOT NULL) STRICT;
         CREATE TRIGGER IF NOT EXISTS owner_credential_no_change BEFORE UPDATE ON owner_credential
           BEGIN SELECT RAISE(ABORT, 'the owner credential is fixed'); END;
         CREATE TRIGGER IF NOT EXISTS owner_credential_no_removal BEFORE DELETE ON owner_credential
           BEGIN SELECT RAISE(ABORT, 'the owner credential is fixed'); END;",
    )
    .map_err(backend)
}

/// Version 5: the evidence hash chain (tamper EVIDENCE: see `chain`). Rows that exist are
/// chained in id order at the upgrade.
fn migrate_evidence_chain(c: &Connection) -> Res<()> {
    add_column(c, "evidence", "prev_hash", "TEXT NOT NULL DEFAULT ''")?;
    add_column(c, "evidence", "hash", "TEXT NOT NULL DEFAULT ''")?;
    legacy_evidence::backfill_chain(c)
}

/// Largest amount of cents that can be multiplied by 10,000 and still fit an `i64`.
const MOST_CENTS: i64 = i64::MAX / 10_000;

/// Version 6: costs and caps are MICRO-USD (1 USD = 1,000,000), not cents. Goal caps and the
/// overall cap are multiplied by 10,000 in place (saturating at `i64::MAX`, an unlimited cap
/// either way) and their columns renamed. Evidence is append-only and hashed, so its rows are NOT
/// rewritten: a new nullable `cost_micros` column marks rows written in the new unit; a row
/// without it keeps its `cost_cents` (and its old hash) and is read as cents times 10,000.
fn migrate_micro_usd(c: &Connection) -> Res<()> {
    add_column(c, "evidence", "cost_micros", "INTEGER")?;
    if !column_missing(c, "goals", "spend_cap_cents")? {
        c.execute_batch(&format!(
            "UPDATE goals SET spend_cap_cents = CASE WHEN spend_cap_cents > {MOST_CENTS} THEN {max} ELSE spend_cap_cents * 10000 END;
             ALTER TABLE goals RENAME COLUMN spend_cap_cents TO spend_cap_micros;",
            max = i64::MAX
        ))
        .map_err(backend)?;
    }
    c.execute_batch(&format!(
        "UPDATE control SET key = 'overall_cap_micros', value = CASE WHEN value > {MOST_CENTS} THEN {max} ELSE value * 10000 END WHERE key = 'overall_cap_cents'",
        max = i64::MAX
    ))
    .map_err(backend)
}

/// Version 7: the confinement profile a call ran under, a structured evidence column (NULL for
/// older rows; hashed for rows written with the micro-USD unit).
fn migrate_evidence_profile(c: &Connection) -> Res<()> {
    add_column(c, "evidence", "profile", "TEXT")
}

/// Version 8: which process runs a started item (the name of its holder file, see `holders`).
/// NULL for every older item and for an item not running: recovery then behaves as it always did.
/// Not part of the evidence chain (items are not).
fn migrate_item_holder(c: &Connection) -> Res<()> {
    add_column(c, "items", "holder", "TEXT")
}

/// The schema version this build writes.
pub const SCHEMA_VERSION: i64 = MIGRATIONS.len() as i64;

/// Brings the file to [`SCHEMA_VERSION`] in one transaction, or refuses to open a newer one.
fn migrate(c: &Connection) -> Res<()> {
    let found: i64 = c
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .map_err(backend)?;
    if found > SCHEMA_VERSION {
        return Err(backend(format!(
            "this database is schema version {found}, newer than this program understands ({SCHEMA_VERSION}): use a newer endora-foundation; nothing was changed"
        )));
    }
    if found == SCHEMA_VERSION {
        return Ok(());
    }
    c.execute_batch("BEGIN IMMEDIATE").map_err(backend)?;
    let done = refuse_laundering(c, found)
        .and_then(|()| {
            MIGRATIONS
                .iter()
                .skip(usize::try_from(found).unwrap_or(0))
                .try_for_each(|step| step(c))
        })
        .and_then(|()| {
            c.execute_batch(&format!("PRAGMA user_version = {SCHEMA_VERSION}"))
                .map_err(backend)
        });
    match done {
        Ok(()) => c.execute_batch("COMMIT").map_err(backend),
        Err(e) => {
            let _ = c.execute_batch("ROLLBACK");
            Err(e)
        }
    }
}

/// Migrations are part of the TRUSTED SETUP context: whoever can run them can also edit the
/// file. They still must not turn a lowered `user_version` into a re-signing of history: when
/// any evidence row already carries a hash (so the chain exists) but the version says it should
/// not, the file is refused and left untouched. (The chain is supplemental tamper detection, not
/// proof of whole-database integrity: see `chain`.)
fn refuse_laundering(c: &Connection, found: i64) -> Res<()> {
    if found >= CHAIN_VERSION || column_missing(c, "evidence", "hash")? {
        return Ok(());
    }
    let hashed: i64 = c
        .query_row("SELECT COUNT(*) FROM evidence WHERE hash <> ''", [], |r| {
            r.get(0)
        })
        .map_err(backend)?;
    if hashed > 0 {
        return Err(backend(format!(
            "{hashed} evidence rows already carry hashes, but the file says schema version {found}, before the hash chain existed: refusing to re-sign history; nothing was changed"
        )));
    }
    Ok(())
}

/// The tables, in the order the canonical dump reads them.
const DUMPED: [&str; 10] = [
    "goals",
    "goal_scope",
    "routines",
    "items",
    "firings",
    "criteria",
    "item_met",
    "policy",
    "control",
    "evidence",
];

/// One criterion of an item as stored, with how it was met.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemCriterion {
    /// The criterion.
    pub criterion: Criterion,
    /// `check` or `owner`, once met.
    pub met_by: Option<String>,
    /// The evidence record that met it, for a check.
    pub met_evidence_id: Option<i64>,
}

/// The one SQLite file.
pub struct Store {
    db: Db,
    /// Who runs a started item (see `holders`); `None` for a private in-memory file.
    holders: Option<Holders>,
}

fn tier_to(t: Tier) -> i64 {
    t as i64
}
fn tier_from(n: i64) -> Res<Tier> {
    [
        Tier::Public,
        Tier::Internal,
        Tier::Confidential,
        Tier::Restricted,
    ]
    .into_iter()
    .find(|t| *t as i64 == n)
    .ok_or_else(|| corrupt(format!("tier {n}")))
}
fn band_from(s: &str) -> Res<Reversibility> {
    Reversibility::from_name(s).ok_or_else(|| corrupt(format!("band {s}")))
}
fn u32_of(n: i64) -> u32 {
    u32::try_from(n).unwrap_or(0)
}

impl Store {
    /// Opens (creating if needed) the file at `path` and makes the schema.
    ///
    /// # Errors
    /// [`RepositoryError::Backend`].
    pub fn open(path: &str) -> Res<Self> {
        Holders::refuse_uri(path)?;
        let db = Db::open(path)?;
        let store = Self {
            db,
            holders: Holders::for_db(path)?,
        };
        let c = store.db.lock()?;
        migrate(&c)?;
        drop(c);
        Ok(store)
    }

    /// The shared handle (for backups and tests).
    #[must_use]
    pub const fn db(&self) -> &Db {
        &self.db
    }

    // ---- goals ------------------------------------------------------------------------------

    /// Stores a goal with its criteria and scope.
    ///
    /// # Errors
    /// [`RepositoryError::Backend`].
    pub(crate) fn insert_goal(&self, g: &Goal, now: i64) -> Res<GoalId> {
        let c = self.db.lock()?;
        c.execute("INSERT INTO goals (title, why, tier, spend_cap_micros, created_ms, acceptance) VALUES (?1,?2,?3,?4,?5,?6)",
            params![g.title, g.why, tier_to(g.tier), i64::try_from(g.spend_cap_micros).unwrap_or(i64::MAX), now, g.acceptance.name()]).map_err(backend)?;
        let id = c.last_insert_rowid();
        for call in &g.scope {
            c.execute(
                "INSERT OR IGNORE INTO goal_scope (goal_id, call) VALUES (?1,?2)",
                params![id, call],
            )
            .map_err(backend)?;
        }
        put_criteria(&c, "goal", id, &g.criteria)?;
        Ok(GoalId(id))
    }

    /// A goal by id.
    ///
    /// # Errors
    /// [`RepositoryError::Backend`] or `Corrupt` when it is missing.
    pub fn goal(&self, id: GoalId) -> Res<Goal> {
        let c = self.db.lock()?;
        read_goal(&c, id.0)
    }

    /// Every goal, oldest first.
    ///
    /// # Errors
    /// [`RepositoryError::Backend`].
    pub fn goals(&self) -> Res<Vec<Goal>> {
        let c = self.db.lock()?;
        let ids: Vec<i64> = col(&c, "SELECT id FROM goals ORDER BY id", [], |r| r.get(0))?;
        ids.into_iter().map(|i| read_goal(&c, i)).collect()
    }

    // ---- routines and firings ---------------------------------------------------------------

    /// Stores a routine.
    ///
    /// # Errors
    /// [`RepositoryError::Backend`].
    pub(crate) fn insert_routine(
        &self,
        goal: GoalId,
        title: &str,
        every_ms: i64,
        first_due_ms: i64,
        auto_accept: bool,
        max_failures: u32,
    ) -> Res<RoutineId> {
        let c = self.db.lock()?;
        c.execute("INSERT INTO routines (goal_id, title, every_ms, next_due_ms, auto_accept, max_failures) VALUES (?1,?2,?3,?4,?5,?6)",
            params![goal.0, title, every_ms, first_due_ms, i64::from(auto_accept), i64::from(max_failures)]).map_err(backend)?;
        Ok(RoutineId(c.last_insert_rowid()))
    }

    /// A routine by id.
    ///
    /// # Errors
    /// [`RepositoryError::Backend`]; `Corrupt` when a stored flag is neither 0 nor 1.
    pub fn routine(&self, id: RoutineId) -> Res<Routine> {
        let c = self.db.lock()?;
        c.query_row("SELECT id, goal_id, title, every_ms, next_due_ms, auto_accept, max_failures, consecutive_failures, paused FROM routines WHERE id=?1", [id.0], routine_row)
            .map_err(backend)?
    }

    /// Every routine.
    ///
    /// # Errors
    /// [`RepositoryError::Backend`]; `Corrupt` when a stored flag is neither 0 nor 1.
    pub fn routines(&self) -> Res<Vec<Routine>> {
        let c = self.db.lock()?;
        col(
            &c,
            "SELECT id, goal_id, title, every_ms, next_due_ms, auto_accept, max_failures, consecutive_failures, paused FROM routines ORDER BY id",
            [],
            routine_row,
        )?
        .into_iter()
        .collect()
    }

    /// Applies a routine's plan in ONE transaction: records every missed slot, creates the one
    /// item (with its own copy of the goal's criteria) unless the previous one is still open,
    /// records that slot, and moves the routine's next due time. A crash leaves all of it or
    /// none. The plan was made from `r` before the transaction: when another process has moved
    /// the routine since (its `next_due_ms` is no longer the one the plan started from), that
    /// process already fired this slot and the plan is stale, so nothing is written and the
    /// outer `None` says so (the caller skips the routine); never a second item for one slot.
    ///
    /// # Errors
    /// [`RepositoryError::Backend`].
    pub(crate) fn apply_plan(
        &self,
        r: &Routine,
        plan: &Plan,
        now: i64,
    ) -> Res<Option<Option<ItemId>>> {
        let c = self.db.lock()?;
        c.execute_batch("BEGIN IMMEDIATE").map_err(backend)?;
        let done = (|| {
            let due: i64 = c
                .query_row(
                    "SELECT next_due_ms FROM routines WHERE id=?1",
                    [r.id.0],
                    |x| x.get(0),
                )
                .map_err(backend)?;
            if due != r.next_due_ms {
                return Ok(None);
            }
            apply_plan_in(&c, r, plan, now).map(Some)
        })();
        match &done {
            Ok(_) => c.execute_batch("COMMIT").map_err(backend)?,
            Err(_) => {
                let _ = c.execute_batch("ROLLBACK");
            }
        }
        done
    }

    /// Whether the routine's last item is still in flight (open, running or blocked).
    ///
    /// # Errors
    /// [`RepositoryError::Backend`].
    pub fn routine_has_open_item(&self, r: RoutineId) -> Res<bool> {
        let c = self.db.lock()?;
        let n: i64 = c.query_row("SELECT COUNT(*) FROM items WHERE routine_id=?1 AND state IN ('open','running','blocked')", [r.0], |x| x.get(0)).map_err(backend)?;
        Ok(n > 0)
    }

    /// Whether the routine's last item blocks its next slot, for a goal that does not need the
    /// owner: an item not yet started, or cut off inside a call, blocks; one that has finished
    /// its work and merely awaits acceptance does not.
    ///
    /// # Errors
    /// [`RepositoryError::Backend`].
    pub fn routine_has_unfinished_item(&self, r: RoutineId) -> Res<bool> {
        let c = self.db.lock()?;
        let n: i64 = c.query_row(
            "SELECT COUNT(*) FROM items WHERE routine_id=?1 AND (state IN ('open','blocked') OR (state='running' AND EXISTS (SELECT 1 FROM evidence e WHERE e.item_id=items.id AND e.kind='intent' AND e.id NOT IN (SELECT intent_id FROM evidence WHERE intent_id IS NOT NULL))))",
            [r.0],
            |x| x.get(0),
        ).map_err(backend)?;
        Ok(n > 0)
    }

    /// Counts one more failed item for a routine and returns the new run of failures.
    ///
    /// # Errors
    /// [`RepositoryError::Backend`].
    pub(crate) fn add_routine_failure(&self, r: RoutineId) -> Res<u32> {
        let c = self.db.lock()?;
        c.execute(
            "UPDATE routines SET consecutive_failures = consecutive_failures + 1 WHERE id=?1",
            [r.0],
        )
        .map_err(backend)?;
        let n: i64 = c
            .query_row(
                "SELECT consecutive_failures FROM routines WHERE id=?1",
                [r.0],
                |x| x.get(0),
            )
            .map_err(backend)?;
        Ok(u32_of(n))
    }

    /// Clears a routine's run of failures and sets whether it is paused.
    ///
    /// # Errors
    /// [`RepositoryError::Backend`].
    pub(crate) fn reset_routine(&self, r: RoutineId, paused: bool, due_not_before: i64) -> Res<()> {
        let c = self.db.lock()?;
        c.execute(
            "UPDATE routines SET consecutive_failures=0, paused=?2, next_due_ms=MAX(next_due_ms, ?3) WHERE id=?1",
            params![r.0, i64::from(paused), due_not_before],
        )
        .map_err(backend)?;
        Ok(())
    }

    /// Pauses a routine.
    ///
    /// # Errors
    /// [`RepositoryError::Backend`].
    pub(crate) fn pause_routine(&self, r: RoutineId) -> Res<()> {
        let c = self.db.lock()?;
        c.execute("UPDATE routines SET paused=1 WHERE id=?1", [r.0])
            .map_err(backend)?;
        Ok(())
    }

    /// Every recorded slot of a routine, oldest first: `(due_ms, outcome)`.
    ///
    /// # Errors
    /// [`RepositoryError::Backend`].
    pub fn firings(&self, r: RoutineId) -> Res<Vec<(i64, String)>> {
        let c = self.db.lock()?;
        col(
            &c,
            "SELECT due_ms, outcome FROM firings WHERE routine_id=?1 ORDER BY due_ms",
            [r.0],
            |x| Ok((x.get(0)?, x.get(1)?)),
        )
    }

    // ---- items ------------------------------------------------------------------------------

    /// Creates an item for `goal` with its own copy of the goal's criteria.
    ///
    /// # Errors
    /// [`RepositoryError::Backend`].
    pub(crate) fn insert_item(
        &self,
        goal: GoalId,
        title: &str,
        subject: Option<&str>,
        own: Option<&[Criterion]>,
        now: i64,
    ) -> Res<ItemId> {
        let c = self.db.lock()?;
        let id = new_item(&c, goal.0, None, title, None, 0, now)?;
        if let Some(s) = subject {
            c.execute("UPDATE items SET subject=?1 WHERE id=?2", params![s, id.0])
                .map_err(backend)?;
        }
        if let Some(list) = own {
            put_criteria(&c, "item", id.0, list)?;
        }
        Ok(id)
    }

    /// An item by id.
    ///
    /// # Errors
    /// [`RepositoryError::Backend`]; `Corrupt` when the stored state is unreadable.
    pub fn item(&self, id: ItemId) -> Res<WorkItem> {
        let c = self.db.lock()?;
        c.query_row(&format!("{ITEM_SELECT} WHERE id=?1"), [id.0], item_row)
            .map_err(backend)?
    }

    /// Every item, oldest first.
    ///
    /// # Errors
    /// [`RepositoryError::Backend`]; `Corrupt` when a stored state is unreadable.
    pub fn items(&self) -> Res<Vec<WorkItem>> {
        let c = self.db.lock()?;
        col(&c, &format!("{ITEM_SELECT} ORDER BY id"), [], item_row)?
            .into_iter()
            .collect()
    }

    /// Items in `state`.
    ///
    /// # Errors
    /// [`RepositoryError::Backend`].
    pub fn items_in(&self, state: ItemState) -> Res<Vec<WorkItem>> {
        let c = self.db.lock()?;
        col(
            &c,
            &format!("{ITEM_SELECT} WHERE state=?1 ORDER BY id"),
            [state.name()],
            item_row,
        )?
        .into_iter()
        .collect()
    }

    /// Moves an item to `to` (the transition table decides), recording `reason`. Moving to
    /// `running` counts an attempt. `done` is refused here: only [`Store::accept_item`] reaches it.
    ///
    /// # Errors
    /// `Corrupt` for a move the rules refuse, [`RepositoryError::Backend`].
    pub(crate) fn move_item(&self, id: ItemId, to: ItemState, reason: Option<&str>) -> Res<()> {
        if to == ItemState::Done {
            return Err(corrupt("done is reached only through acceptance"));
        }
        // A running item names this process as its holder; any other state names nobody.
        let holder = match (&self.holders, to) {
            (Some(h), ItemState::Running) => {
                let name = h.own_name()?;
                self.sweep_holders(h);
                Some(name)
            }
            _ => None,
        };
        let c = self.db.lock()?;
        let from = read_state(&c, id.0)?;
        may_move(from, to).map_err(corrupt)?;
        let n = c.execute(
            "UPDATE items SET state=?1, reason=?2, attempts = attempts + ?3, holder=?6 WHERE id=?4 AND state=?5",
            params![to.name(), reason, i64::from(to == ItemState::Running), id.0, from.name(), holder],
        ).map_err(backend)?;
        if n != 1 {
            return Err(corrupt("the item changed under the move"));
        }
        Ok(())
    }

    /// Once per process, removes the free holder files no item names (a killed `serve` leaves
    /// one between ticks). Each file is locked first, so its holder is dead and no item can come
    /// to name it afterwards; a file an item still names is left for the recovery of that item.
    fn sweep_holders(&self, h: &Holders) {
        for (name, guard) in h.stale() {
            let named: i64 = self.db.lock().map_or(1, |c| {
                c.query_row("SELECT COUNT(*) FROM items WHERE holder=?1", [&name], |r| {
                    r.get(0)
                })
                .unwrap_or(1)
            });
            if named == 0 {
                guard.remove();
            }
        }
    }

    /// Takes over a RUNNING item for this process, if no other live process runs it: `true` when
    /// this process now holds it (it was held by a dead process, by none, or already by this
    /// process) and so may recover or re-check it; `false` when a live process holds it or it is
    /// not running. The take-over is one compare-and-set on the recorded holder, so two
    /// recovering processes cannot both win, and the winner is itself the live holder others see.
    ///
    /// # Errors
    /// [`RepositoryError::Backend`].
    pub(crate) fn adopt(&self, id: ItemId) -> Res<bool> {
        let Some(h) = &self.holders else {
            return Ok(true);
        };
        let was: Option<Option<String>> = self
            .db
            .lock()?
            .query_row(
                "SELECT holder FROM items WHERE id=?1 AND state='running'",
                [id.0],
                |r| r.get(0),
            )
            .optional()
            .map_err(backend)?;
        let Some(was) = was else {
            return Ok(false);
        };
        let guard = match was.as_deref().map(|name| (name, h.probe(name))) {
            Some((_, Held::Alive)) => return Ok(false),
            Some((_, Held::Dead(g))) => g,
            None => None,
        };
        let me = h.own_name()?;
        if was.as_deref() == Some(me.as_str()) {
            return Ok(true);
        }
        let n = self
            .db
            .lock()?
            .execute(
                "UPDATE items SET holder=?1 WHERE id=?2 AND state='running' AND holder IS ?3",
                params![me, id.0, was],
            )
            .map_err(backend)?;
        if n == 1 {
            if let Some(g) = guard {
                g.remove();
            }
        }
        Ok(n == 1)
    }

    /// Closes a running item as done, once an acceptance has been authorised.
    ///
    /// # Errors
    /// `Corrupt` when the item is not running, [`RepositoryError::Backend`].
    pub(crate) fn accept_item(&self, id: ItemId, by: &str, now: i64) -> Res<()> {
        let c = self.db.lock()?;
        let n = c.execute(
            "UPDATE items SET state='done', accepted_by=?1, accepted_ms=?2, holder=NULL WHERE id=?3 AND state='running'",
            params![by, now, id.0],
        ).map_err(backend)?;
        if n != 1 {
            return Err(corrupt("only a running item is accepted"));
        }
        Ok(())
    }

    /// An item's criteria in order, with how each was met.
    ///
    /// # Errors
    /// [`RepositoryError::Backend`].
    pub fn criteria(&self, id: ItemId) -> Res<Vec<ItemCriterion>> {
        let c = self.db.lock()?;
        let own = read_criteria(&c, "item", id.0, id.0)?;
        if !own.is_empty() {
            return Ok(own);
        }
        let goal: i64 = c
            .query_row("SELECT goal_id FROM items WHERE id=?1", [id.0], |r| {
                r.get(0)
            })
            .map_err(backend)?;
        read_criteria(&c, "goal", goal, id.0)
    }

    /// Marks criterion `idx` of an item met, by a `check` (citing its evidence) or the `owner`.
    ///
    /// # Errors
    /// [`RepositoryError::Backend`].
    pub(crate) fn mark_met(
        &self,
        id: ItemId,
        idx: usize,
        by: &str,
        evidence_id: Option<i64>,
    ) -> Res<()> {
        let c = self.db.lock()?;
        c.execute("INSERT OR REPLACE INTO item_met (item_id, idx, met_by, met_evidence_id) VALUES (?3,?4,?1,?2)",
            params![by, evidence_id, id.0, i64::try_from(idx).unwrap_or(i64::MAX)]).map_err(backend)?;
        Ok(())
    }

    /// The owner marks a criterion met, but only while the item is `open`, `running` or `blocked`
    /// (one statement, so an item finishing meanwhile cannot slip through). `false`, and nothing
    /// written, when it is finished, unreadable or absent.
    ///
    /// # Errors
    /// [`RepositoryError::Backend`].
    pub(crate) fn confirm_unfinished(&self, id: ItemId, idx: usize) -> Res<bool> {
        let n = self.db.lock()?.execute(
            "INSERT OR REPLACE INTO item_met (item_id, idx, met_by, met_evidence_id) SELECT ?1, ?2, 'owner', NULL WHERE EXISTS (SELECT 1 FROM items WHERE id=?1 AND state IN ('open','running','blocked'))",
            params![id.0, i64::try_from(idx).unwrap_or(i64::MAX)]).map_err(backend)?;
        Ok(n == 1)
    }

    // ---- policy and control -----------------------------------------------------------------

    /// Stores (or replaces) a policy row.
    ///
    /// # Errors
    /// [`RepositoryError::Backend`].
    pub(crate) fn put_policy(&self, r: &PolicyRow) -> Res<()> {
        let c = self.db.lock()?;
        c.execute("INSERT OR REPLACE INTO policy (actor, call, band, tier, max_uses_per_item) VALUES (?1,?2,?3,?4,?5)",
            params![r.actor, r.call, r.band.name(), tier_to(r.tier), r.max_uses_per_item.map(i64::from)]).map_err(backend)?;
        Ok(())
    }

    /// Every policy row.
    ///
    /// # Errors
    /// [`RepositoryError::Backend`] or `Corrupt`.
    pub fn policy(&self) -> Res<Vec<PolicyRow>> {
        let c = self.db.lock()?;
        let raw: Vec<(String, String, String, i64, Option<i64>)> = col(
            &c,
            "SELECT actor, call, band, tier, max_uses_per_item FROM policy ORDER BY actor, call",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )?;
        raw.into_iter()
            .map(|(actor, call, band, tier, max)| {
                Ok(PolicyRow {
                    actor,
                    call,
                    band: band_from(&band)?,
                    tier: tier_from(tier)?,
                    max_uses_per_item: max.map(u32_of),
                })
            })
            .collect()
    }

    /// A control value (`stop`, `overall_cap_micros`), zero when never set.
    ///
    /// # Errors
    /// [`RepositoryError::Backend`].
    pub fn control(&self, key: &str) -> Res<i64> {
        let c = self.db.lock()?;
        Ok(
            c.query_row("SELECT value FROM control WHERE key=?1", [key], |r| {
                r.get(0)
            })
            .optional()
            .map_err(backend)?
            .unwrap_or(0),
        )
    }

    /// Sets a control value.
    ///
    /// # Errors
    /// [`RepositoryError::Backend`].
    pub(crate) fn set_control(&self, key: &str, value: i64) -> Res<()> {
        let c = self.db.lock()?;
        c.execute(
            "INSERT OR REPLACE INTO control (key, value) VALUES (?1,?2)",
            params![key, value],
        )
        .map_err(backend)?;
        Ok(())
    }

    /// Sets a control value, getting through a busy database: an immediate transaction, a short
    /// busy timeout, and up to 10 tries 200 ms apart. Says plainly why when it still cannot.
    ///
    /// # Errors
    /// [`RepositoryError::Backend`].
    pub(crate) fn set_control_retrying(&self, key: &str, value: i64) -> Res<()> {
        const TRIES: u32 = 10;
        let mut last = String::new();
        for attempt in 1..=TRIES {
            {
                let c = self.db.lock()?;
                c.busy_timeout(std::time::Duration::from_millis(500))
                    .map_err(backend)?;
                let tried = c
                    .execute_batch("BEGIN IMMEDIATE")
                    .and_then(|()| {
                        c.execute(
                            "INSERT OR REPLACE INTO control (key, value) VALUES (?1,?2)",
                            params![key, value],
                        )
                    })
                    .and_then(|_| c.execute_batch("COMMIT"));
                let _ = c.busy_timeout(std::time::Duration::from_millis(5_000));
                match tried {
                    Ok(()) => return Ok(()),
                    Err(e) => {
                        let _ = c.execute_batch("ROLLBACK");
                        last = e.to_string();
                    }
                }
            }
            if attempt < TRIES {
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
        }
        Err(backend(format!(
            "the database stayed busy for {TRIES} tries ({last}); nothing was changed"
        )))
    }

    // ---- evidence: append and read, nothing else -------------------------------------------

    /// Appends one record and returns its id.
    ///
    /// # Errors
    /// [`RepositoryError::Backend`].
    pub(crate) fn append_evidence(&self, e: &Evidence) -> Res<i64> {
        let c = self.db.lock()?;
        // One immediate transaction: read the last link, take the next id, write the row.
        c.execute_batch("BEGIN IMMEDIATE").map_err(backend)?;
        let done = append_in(&c, e);
        match done {
            Ok(id) => c.execute_batch("COMMIT").map(|()| id).map_err(backend),
            Err(e) => {
                let _ = c.execute_batch("ROLLBACK");
                Err(e)
            }
        }
    }

    /// Records the intent of a call ONLY IF its declared maximum still fits under the goal's cap
    /// and the overall cap, as one immediate transaction: the check and the write cannot be
    /// separated by another process. The spend counted is the cost on record plus the maximum
    /// (`max_of`) of every allowed call that has no result yet, so a second process sees the
    /// first one's reservation until its actual cost replaces it. `None` means the cap would be
    /// passed and nothing was written (the caller records the refusal).
    ///
    /// # Errors
    /// [`RepositoryError::Backend`].
    pub(crate) fn append_intent_reserving(
        &self,
        e: &Evidence,
        goal: GoalId,
        goal_cap: u64,
        max_cost: u64,
        max_of: &dyn Fn(&str) -> u64,
    ) -> Res<Option<i64>> {
        let c = self.db.lock()?;
        c.execute_batch("BEGIN IMMEDIATE").map_err(backend)?;
        let done = (|| -> Res<Option<i64>> {
            if max_cost > 0 {
                let total_cap: i64 = c
                    .query_row(
                        "SELECT value FROM control WHERE key='overall_cap_micros'",
                        [],
                        |r| r.get(0),
                    )
                    .optional()
                    .map_err(backend)?
                    .unwrap_or(0);
                let total_cap = u64::try_from(total_cap).unwrap_or(0);
                for (scope, cap) in [(Some(goal), goal_cap), (None, total_cap)] {
                    let open: u64 = unsettled_in(&c, scope)?.iter().map(|n| max_of(n)).sum();
                    let held = spent_in(&c, scope)?.saturating_add(open);
                    if held.saturating_add(max_cost) > cap {
                        return Ok(None);
                    }
                }
            }
            append_in(&c, e).map(Some)
        })();
        match done {
            Ok(Some(id)) => c
                .execute_batch("COMMIT")
                .map(|()| Some(id))
                .map_err(backend),
            Ok(None) => c.execute_batch("ROLLBACK").map(|()| None).map_err(backend),
            Err(e) => {
                let _ = c.execute_batch("ROLLBACK");
                Err(e)
            }
        }
    }

    /// Recomputes the evidence hash chain and reports the first broken row, if any. Detection
    /// only: see the `chain` module for what it can and cannot see.
    ///
    /// # Errors
    /// [`RepositoryError::Backend`].
    pub fn verify_evidence_chain(&self) -> Res<chain::ChainReport> {
        {
            let c = self.db.lock()?;
            chain::verify_chain(&c)
        }
    }

    /// Every record, oldest first.
    ///
    /// # Errors
    /// [`RepositoryError::Backend`] or `Corrupt`.
    pub fn evidence_all(&self) -> Res<Vec<Evidence>> {
        self.evidence_where("1=1", [])
    }

    /// Every record about one item, oldest first.
    ///
    /// # Errors
    /// [`RepositoryError::Backend`] or `Corrupt`.
    pub fn evidence_for(&self, item: ItemId) -> Res<Vec<Evidence>> {
        self.evidence_where("item_id=?1", [item.0])
    }

    /// The last `n` records, oldest first.
    ///
    /// # Errors
    /// [`RepositoryError::Backend`] or `Corrupt`.
    pub fn evidence_tail(&self, n: usize) -> Res<Vec<Evidence>> {
        let all = self.evidence_all()?;
        let skip = all.len().saturating_sub(n);
        Ok(all.into_iter().skip(skip).collect())
    }

    /// Allowed intents recorded for `actor` making `call` for `item`.
    ///
    /// # Errors
    /// [`RepositoryError::Backend`].
    pub fn uses(&self, actor: &str, call: &str, item: ItemId) -> Res<u32> {
        let c = self.db.lock()?;
        let n: i64 = c.query_row("SELECT COUNT(*) FROM evidence WHERE kind='intent' AND actor=?1 AND call=?2 AND item_id=?3",
            params![actor, call, item.0], |r| r.get(0)).map_err(backend)?;
        Ok(u32_of(n))
    }

    /// Micro-USD recorded as spent by results, for one goal or (with `None`) overall.
    ///
    /// # Errors
    /// [`RepositoryError::Backend`].
    pub fn spent_micros(&self, goal: Option<GoalId>) -> Res<u64> {
        spent_in(&*self.db.lock()?, goal)
    }

    /// The call names of allowed intents that have no result record (a crash may have cut
    /// them), for one goal or (with `None`) overall.
    ///
    /// # Errors
    /// [`RepositoryError::Backend`].
    pub fn unsettled_calls(&self, goal: Option<GoalId>) -> Res<Vec<String>> {
        unsettled_in(&*self.db.lock()?, goal)
    }

    /// Allowed intents of `item` with no result record: calls that may or may not have run.
    ///
    /// # Errors
    /// [`RepositoryError::Backend`] or `Corrupt`.
    pub fn unsettled_intents(&self, item: ItemId) -> Res<Vec<Evidence>> {
        self.evidence_where("item_id=?1 AND kind='intent' AND id NOT IN (SELECT intent_id FROM evidence WHERE intent_id IS NOT NULL)", [item.0])
    }

    /// Allowed intents of `item` with no result record that were recorded since the item's latest
    /// `core.start` row: the calls of the CURRENT attempt that may or may not have run. An
    /// earlier attempt's call that never got a result was already recovered (that is what put
    /// the item back to `open` for this attempt) and the append-only evidence cannot close it, so
    /// it must not make every later pass recover the item again. An item with no `core.start`
    /// row counts every unsettled call. [`Store::unsettled_intents`] (all attempts) and
    /// [`Store::unsettled_calls`] (spend: assume the worst for every call without a result) are
    /// unchanged.
    ///
    /// # Errors
    /// [`RepositoryError::Backend`] or `Corrupt`.
    pub fn unsettled_intents_this_attempt(&self, item: ItemId) -> Res<Vec<Evidence>> {
        self.evidence_where("item_id=?1 AND kind='intent' AND id NOT IN (SELECT intent_id FROM evidence WHERE intent_id IS NOT NULL) AND id > COALESCE((SELECT MAX(id) FROM evidence WHERE item_id=?1 AND kind='control' AND call='core.start'), 0)", [item.0])
    }

    /// Bands of the item's allowed calls.
    ///
    /// # Errors
    /// [`RepositoryError::Backend`] or `Corrupt`.
    pub fn bands_used(&self, item: ItemId) -> Res<Vec<Reversibility>> {
        let c = self.db.lock()?;
        let raw: Vec<String> = col(
            &c,
            "SELECT band FROM evidence WHERE item_id=?1 AND kind='intent'",
            [item.0],
            |r| r.get(0),
        )?;
        raw.iter().map(|b| band_from(b)).collect()
    }

    fn evidence_where<P: rusqlite::Params>(&self, clause: &str, p: P) -> Res<Vec<Evidence>> {
        let c = self.db.lock()?;
        let raw = col(
            &c,
            &format!(
                "SELECT id, item_id, call, actor, band, decision, kind, intent_id, inputs_hash, output, at_ms, cost_cents, cost_micros, actor_kind, profile FROM evidence WHERE {clause} ORDER BY id"
            ),
            p,
            |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, Option<i64>>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, String>(5)?,
                    r.get::<_, String>(6)?,
                    r.get::<_, Option<i64>>(7)?,
                    r.get::<_, String>(8)?,
                    r.get::<_, String>(9)?,
                    r.get::<_, i64>(10)?,
                    r.get::<_, i64>(11)?,
                    r.get::<_, Option<i64>>(12)?,
                    r.get::<_, String>(13)?,
                    r.get::<_, Option<String>>(14)?,
                ))
            },
        )?;
        raw.into_iter()
            .map(
                |(
                    id,
                    item,
                    call,
                    actor,
                    band,
                    dec,
                    kind,
                    intent_id,
                    inputs_hash,
                    output,
                    at_ms,
                    cents,
                    micros,
                    actor_kind,
                    profile,
                )| {
                    // A row without `cost_micros` was written in cents.
                    let cost = match micros {
                        Some(m) => u64::try_from(m).map_err(|_| corrupt(format!("cost {m}")))?,
                        None => u64::try_from(cents)
                            .ok()
                            .and_then(|c| c.checked_mul(10_000))
                            .ok_or_else(|| corrupt(format!("legacy cost {cents} cents")))?,
                    };
                    Ok(Evidence {
                        id,
                        item: item.map(ItemId),
                        call,
                        actor,
                        actor_kind: ActorKind::from_name(&actor_kind)
                            .ok_or_else(|| corrupt(format!("actor kind {actor_kind}")))?,
                        band: band_from(&band)?,
                        allowed: match dec.as_str() {
                            "allow" => true,
                            "deny" => false,
                            _ => {
                                return Err(corrupt(format!(
                                    "evidence row {id} has an unreadable decision {dec:?}"
                                )));
                            }
                        },
                        kind: Kind::from_name(&kind)
                            .ok_or_else(|| corrupt(format!("kind {kind}")))?,
                        intent_id,
                        inputs_hash,
                        output,
                        at_ms,
                        cost_micros: cost,
                        profile,
                    })
                },
            )
            .collect()
    }

    // ---- the owner credential ---------------------------------------------------------------

    /// The stored `(salt, hash)` of the owner token, as hex, if one was made.
    ///
    /// # Errors
    /// [`RepositoryError::Backend`].
    pub fn owner_credential(&self) -> Res<Option<(String, String)>> {
        let c = self.db.lock()?;
        c.query_row(
            "SELECT salt, hash FROM owner_credential WHERE id = 1",
            [],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
        )
        .optional()
        .map_err(backend)
    }

    /// Stores the credential once; a second call fails on the primary key.
    pub(crate) fn insert_owner_credential(&self, salt: &str, hash: &str, now: i64) -> Res<()> {
        let c = self.db.lock()?;
        c.execute(
            "INSERT INTO owner_credential (id, salt, hash, created_ms) VALUES (1, ?1, ?2, ?3)",
            params![salt, hash, now],
        )
        .map_err(backend)?;
        Ok(())
    }

    // ---- the canonical dump -----------------------------------------------------------------

    /// Every table, in a fixed order and row order, as text: what a restore must reproduce.
    ///
    /// # Errors
    /// [`RepositoryError::Backend`].
    pub fn canonical_dump(&self) -> Res<String> {
        let c = self.db.lock()?;
        let mut out = String::new();
        for table in DUMPED {
            out.push_str(&format!("# {table}\n"));
            let mut stmt = c
                .prepare(&format!("SELECT * FROM {table} ORDER BY rowid"))
                .map_err(backend)?;
            let n = stmt.column_count();
            let mut rows = stmt.query([]).map_err(backend)?;
            while let Some(r) = rows.next().map_err(backend)? {
                for i in 0..n {
                    match r.get_ref(i).map_err(backend)? {
                        ValueRef::Null => out.push_str("NULL"),
                        ValueRef::Integer(v) => out.push_str(&v.to_string()),
                        ValueRef::Real(v) => out.push_str(&v.to_string()),
                        ValueRef::Text(t) => {
                            out.push_str(&format!("{:?}", String::from_utf8_lossy(t)))
                        }
                        ValueRef::Blob(b) => out.push_str(&format!("blob{}", b.len())),
                    }
                    out.push('|');
                }
                out.push('\n');
            }
        }
        Ok(out)
    }

    /// SHA-256 of [`Store::canonical_dump`], in hex.
    ///
    /// # Errors
    /// [`RepositoryError::Backend`].
    pub fn dump_hash(&self) -> Res<String> {
        Ok(hex(&Sha256::digest(self.canonical_dump()?.as_bytes())))
    }
}

/// Lowercase hex of bytes.
#[must_use]
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Writes one evidence row inside the caller's open transaction and returns its id. A cost that
/// does not fit the integer column is an error, never a saturated value.
fn append_in(c: &Connection, e: &Evidence) -> Res<i64> {
    let cost = i64::try_from(e.cost_micros).map_err(|_| {
        backend(format!(
            "cost {} does not fit the evidence record",
            e.cost_micros
        ))
    })?;
    let prev: String = c
        .query_row(
            "SELECT hash FROM evidence ORDER BY id DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .optional()
        .map_err(backend)?
        .unwrap_or_else(|| chain::GENESIS.to_owned());
    let id: i64 = c
        .query_row(
            "SELECT MAX(COALESCE((SELECT seq FROM sqlite_sequence WHERE name='evidence'),0), COALESCE((SELECT MAX(id) FROM evidence),0)) + 1",
            [],
            |r| r.get(0),
        )
        .map_err(backend)?;
    let raw = chain::RawRow {
        id,
        item: e.item.map(|i| i.0),
        call: e.call.clone(),
        actor: e.actor.clone(),
        actor_kind: e.actor_kind.name().to_owned(),
        band: e.band.name().to_owned(),
        decision: if e.allowed { "allow" } else { "deny" }.to_owned(),
        kind: e.kind.name().to_owned(),
        intent: e.intent_id,
        inputs_hash: e.inputs_hash.clone(),
        output: e.output.clone(),
        at_ms: e.at_ms,
        cost_cents: 0,
        cost_micros: Some(cost),
        profile: e.profile.clone(),
        prev_hash: prev.clone(),
        hash: String::new(),
    };
    let hash = chain::row_hash(&prev, &raw);
    c.execute(
        "INSERT INTO evidence (id, item_id, call, actor, actor_kind, band, decision, kind, intent_id, inputs_hash, output, at_ms, cost_cents, cost_micros, profile, prev_hash, hash) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)",
        params![
            raw.id, raw.item, raw.call, raw.actor, raw.actor_kind, raw.band,
            raw.decision, raw.kind, raw.intent, raw.inputs_hash, raw.output,
            raw.at_ms, raw.cost_cents, raw.cost_micros, raw.profile, raw.prev_hash, hash
        ],
    )
    .map_err(backend)?;
    Ok(id)
}

/// Micro-USD recorded by results for a goal or overall. A negative or overflowing sum is an error,
/// never a smaller number.
fn spent_in(c: &Connection, goal: Option<GoalId>) -> Res<u64> {
    // Rows in micro-USD and legacy rows in cents are summed apart (SQLite refuses an integer
    // overflow), then combined with checked arithmetic.
    let (micros, cents): (i64, i64) = c.query_row(
        "SELECT COALESCE(SUM(CASE WHEN e.cost_micros IS NULL THEN 0 ELSE e.cost_micros END),0), COALESCE(SUM(CASE WHEN e.cost_micros IS NULL THEN e.cost_cents ELSE 0 END),0) FROM evidence e JOIN items i ON i.id = e.item_id WHERE e.kind='result' AND (?1 IS NULL OR i.goal_id = ?1)",
        [goal.map(|g| g.0)], |r| Ok((r.get(0)?, r.get(1)?))).map_err(backend)?;
    let negative = |n: i64| {
        backend(format!(
            "the recorded spend is negative ({n}): refusing to count it"
        ))
    };
    let micros = u64::try_from(micros).map_err(|_| negative(micros))?;
    let cents = u64::try_from(cents).map_err(|_| negative(cents))?;
    cents
        .checked_mul(10_000)
        .and_then(|m| m.checked_add(micros))
        .ok_or_else(|| backend("the recorded spend overflows: refusing to count it"))
}

/// Call names of allowed intents with no result record, for a goal or overall.
fn unsettled_in(c: &Connection, goal: Option<GoalId>) -> Res<Vec<String>> {
    col(
        c,
        "SELECT e.call FROM evidence e JOIN items i ON i.id = e.item_id WHERE e.kind='intent' AND (?1 IS NULL OR i.goal_id = ?1) AND e.id NOT IN (SELECT intent_id FROM evidence WHERE intent_id IS NOT NULL)",
        [goal.map(|g| g.0)],
        |r| r.get(0),
    )
}

const ITEM_SELECT: &str = "SELECT id, goal_id, routine_id, title, state, attempts, due_ms, missed, reason, accepted_by, subject FROM items";

fn col<T, P: rusqlite::Params>(
    c: &Connection,
    sql: &str,
    p: P,
    f: impl FnMut(&Row<'_>) -> rusqlite::Result<T>,
) -> Res<Vec<T>> {
    let mut stmt = c.prepare(sql).map_err(backend)?;
    let rows = stmt.query_map(p, f).map_err(backend)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(backend)
}

/// A stored flag is 0 or 1; anything else is damage, not `true`.
fn flag(id: i64, what: &str, n: i64) -> Res<bool> {
    match n {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(corrupt(format!(
            "routine {id} has an unreadable {what} flag {n}"
        ))),
    }
}

fn routine_row(r: &Row<'_>) -> rusqlite::Result<Res<Routine>> {
    let id: i64 = r.get(0)?;
    let (goal, title, every_ms, next_due_ms) = (GoalId(r.get(1)?), r.get(2)?, r.get(3)?, r.get(4)?);
    let (auto, max, run, paused): (i64, i64, i64, i64) =
        (r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?);
    let built = || -> Res<Routine> {
        Ok(Routine {
            id: RoutineId(id),
            goal,
            title,
            every_ms,
            next_due_ms,
            auto_accept: flag(id, "auto_accept", auto)?,
            max_failures: u32_of(max),
            consecutive_failures: u32_of(run),
            paused: flag(id, "paused", paused)?,
        })
    };
    Ok(built())
}

/// An unreadable stored state names the item and the value; it is never read as a valid state.
fn bad_state(id: i64, state: &str) -> RepositoryError {
    corrupt(format!("item {id} has an unreadable state {state:?}"))
}

fn item_row(r: &Row<'_>) -> rusqlite::Result<Res<WorkItem>> {
    let state: String = r.get(4)?;
    let Some(state) = ItemState::from_name(&state) else {
        return Ok(Err(bad_state(r.get(0)?, &state)));
    };
    Ok(Ok(WorkItem {
        id: ItemId(r.get(0)?),
        goal: GoalId(r.get(1)?),
        routine: r.get::<_, Option<i64>>(2)?.map(RoutineId),
        title: r.get(3)?,
        state,
        attempts: u32_of(r.get(5)?),
        due_ms: r.get(6)?,
        missed: u32_of(r.get(7)?),
        reason: r.get(8)?,
        accepted_by: r.get(9)?,
        subject: r.get(10)?,
    }))
}

fn read_state(c: &Connection, id: i64) -> Res<ItemState> {
    let s: String = c
        .query_row("SELECT state FROM items WHERE id=?1", [id], |r| r.get(0))
        .map_err(backend)?;
    ItemState::from_name(&s).ok_or_else(|| bad_state(id, &s))
}

fn put_criteria(c: &Connection, kind: &str, id: i64, list: &[Criterion]) -> Res<()> {
    for (i, k) in list.iter().enumerate() {
        let (call, input, field, expect) =
            k.check.as_ref().map_or((None, None, None, None), |ch| {
                (
                    Some(&ch.call),
                    Some(&ch.input),
                    Some(&ch.field),
                    Some(&ch.expect),
                )
            });
        c.execute("INSERT INTO criteria (owner_kind, owner_id, idx, text, check_call, check_input, check_field, check_expect) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
            params![kind, id, i64::try_from(i).unwrap_or(i64::MAX), k.text, call, input, field, expect]).map_err(backend)?;
    }
    Ok(())
}

/// The criteria defined by `kind` (`goal` or `item`) `id`, with how `item` met each (an item's
/// state lives in `item_met`; the legacy `criteria.met_by` of a pre-slice-3 file still reads).
fn read_criteria(c: &Connection, kind: &str, id: i64, item: i64) -> Res<Vec<ItemCriterion>> {
    col(
        c,
        "SELECT k.text, k.check_call, k.check_input, k.check_expect, k.check_field, COALESCE(m.met_by, k.met_by), COALESCE(m.met_evidence_id, k.met_evidence_id) FROM criteria k LEFT JOIN item_met m ON m.item_id=?3 AND m.idx=k.idx WHERE k.owner_kind=?1 AND k.owner_id=?2 ORDER BY k.idx",
        params![kind, id, item],
        |r| {
            let check = match (
                r.get::<_, Option<String>>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, Option<String>>(4)?,
            ) {
                (Some(call), Some(input), Some(expect), Some(field)) => Some(Check {
                    call,
                    input,
                    field,
                    expect,
                }),
                _ => None,
            };
            Ok(ItemCriterion {
                criterion: Criterion {
                    text: r.get(0)?,
                    check,
                },
                met_by: r.get(5)?,
                met_evidence_id: r.get(6)?,
            })
        },
    )
}

fn read_goal(c: &Connection, id: i64) -> Res<Goal> {
    let (title, why, tier, cap, acceptance): (String, String, i64, i64, String) = c
        .query_row(
            "SELECT title, why, tier, spend_cap_micros, acceptance FROM goals WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .map_err(backend)?;
    Ok(Goal {
        id: GoalId(id),
        title,
        why,
        tier: tier_from(tier)?,
        spend_cap_micros: u64::try_from(cap).unwrap_or(0),
        acceptance: AcceptancePolicy::from_name(&acceptance)
            .ok_or_else(|| corrupt("goals.acceptance"))?,
        criteria: read_criteria(c, "goal", id, -1)?
            .into_iter()
            .map(|k| k.criterion)
            .collect(),
        scope: col(
            c,
            "SELECT call FROM goal_scope WHERE goal_id=?1 ORDER BY call",
            [id],
            |r| r.get(0),
        )?,
    })
}

fn new_item(
    c: &Connection,
    goal: i64,
    routine: Option<i64>,
    title: &str,
    due: Option<i64>,
    missed: u32,
    now: i64,
) -> Result<ItemId, RepositoryError> {
    c.execute("INSERT INTO items (goal_id, routine_id, title, state, due_ms, missed, created_ms) VALUES (?1,?2,?3,'open',?4,?5,?6)",
        params![goal, routine, title, due, i64::from(missed), now]).map_err(backend)?;
    Ok(ItemId(c.last_insert_rowid()))
}

fn apply_plan_in(c: &Connection, r: &Routine, plan: &Plan, now: i64) -> Res<Option<ItemId>> {
    // Slots missed while down are ONE summary row (the first missed slot, with the count).
    if let Some(first) = plan.first_missed {
        c.execute(
            "INSERT INTO firings (routine_id, due_ms, outcome, missed_count) VALUES (?1,?2,'missed_down',?3)",
            params![r.id.0, first, i64::try_from(plan.missed).unwrap_or(i64::MAX)],
        )
        .map_err(backend)?;
    }
    let mut item = None;
    if let Some(slot) = plan.latest {
        if plan.skipped_open {
            c.execute(
                "INSERT INTO firings (routine_id, due_ms, outcome) VALUES (?1,?2,'skipped_open')",
                params![r.id.0, slot],
            )
            .map_err(backend)?;
        } else {
            let id = new_item(
                c,
                r.goal.0,
                Some(r.id.0),
                &r.title,
                Some(slot),
                u32::try_from(plan.missed).unwrap_or(u32::MAX),
                now,
            )?;
            c.execute("INSERT INTO firings (routine_id, due_ms, outcome, item_id) VALUES (?1,?2,'fired',?3)", params![r.id.0, slot, id.0]).map_err(backend)?;
            item = Some(id);
        }
    }
    c.execute(
        "UPDATE routines SET next_due_ms=?1 WHERE id=?2",
        params![plan.next_due_ms, r.id.0],
    )
    .map_err(backend)?;
    Ok(item)
}
