//! The evidence hash chain: TAMPER EVIDENCE, not tamper prevention.
//!
//! Every evidence row stores `prev_hash` (the previous row's `hash`, or [`GENESIS`] for the
//! first) and `hash = sha256(prev_hash || canonical row content)`. Anyone can recompute the
//! chain, so an edited row, a removed row or a spliced row shows up as a broken link and
//! [`verify_chain`] names the first one. What it does NOT do: stop a client from writing the
//! file (the append-only triggers only protect connections that carry them), and it cannot see
//! the removal of the LAST rows or a chain re-forged end to end by someone who knows this
//! format. Keep the head hash somewhere else (a printed line, another host) to catch those.

use endora_kernel::RepositoryError;
use endora_persistence::backend;
use rusqlite::Connection;
use sha2::{Digest, Sha256};

use super::store::hex;

type Res<T> = Result<T, RepositoryError>;

/// The `prev_hash` of the first row.
pub const GENESIS: &str = concat!(
    "0000000000000000",
    "0000000000000000",
    "0000000000000000",
    "0000000000000000"
);

/// One evidence row as stored, every column as written.
#[derive(Debug, Clone)]
pub(super) struct RawRow {
    pub id: i64,
    pub item: Option<i64>,
    pub call: String,
    pub actor: String,
    pub actor_kind: String,
    pub band: String,
    pub decision: String,
    pub kind: String,
    pub intent: Option<i64>,
    pub inputs_hash: String,
    pub output: String,
    pub at_ms: i64,
    /// The legacy cost column, in CENTS (rows written before the micro-USD unit).
    pub cost_cents: i64,
    /// The cost in micro-USD; `None` marks a row written before that unit existed (its cost is
    /// then `cost_cents` and its canonical text is the old one, so its hash stays valid).
    pub cost_micros: Option<i64>,
    /// The confinement profile (hashed for rows that carry `cost_micros`).
    pub profile: Option<String>,
    pub prev_hash: String,
    pub hash: String,
}

fn field(out: &mut String, v: &str) {
    // Length-prefixed, so no two different rows share one canonical text.
    out.push_str(&v.len().to_string());
    out.push(':');
    out.push_str(v);
    out.push('\n');
}

fn opt(v: Option<i64>) -> String {
    v.map_or_else(|| "-".to_owned(), |n| n.to_string())
}

/// `sha256(prev_hash || canonical content)`, in hex.
///
/// A row with `cost_micros` hashes that amount and its profile; an older row hashes exactly what
/// it always did (its cents), so history written before the unit change is never re-signed.
pub(super) fn row_hash(prev: &str, r: &RawRow) -> String {
    let mut s = String::new();
    field(&mut s, prev);
    let cost = r.cost_micros.unwrap_or(r.cost_cents);
    let mut parts = vec![
        r.id.to_string(),
        opt(r.item),
        r.call.clone(),
        r.actor.clone(),
        r.actor_kind.clone(),
        r.band.clone(),
        r.decision.clone(),
        r.kind.clone(),
        opt(r.intent),
        r.inputs_hash.clone(),
        r.output.clone(),
        r.at_ms.to_string(),
        cost.to_string(),
    ];
    if r.cost_micros.is_some() {
        parts.push(r.profile.clone().unwrap_or_else(|| "-".to_owned()));
    }
    for v in parts {
        field(&mut s, &v);
    }
    hex(&Sha256::digest(s.as_bytes()))
}

fn has_column(c: &Connection, column: &str) -> Res<bool> {
    let n: i64 = c
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('evidence') WHERE name=?1",
            [column],
            |r| r.get(0),
        )
        .map_err(backend)?;
    Ok(n > 0)
}

/// Every row as stored. The migration that chains old rows runs before the micro-USD and profile
/// columns exist, so a missing column reads as NULL (a legacy row).
pub(super) fn read_rows(c: &Connection) -> Res<Vec<RawRow>> {
    let col = |name: &str| -> Res<String> {
        Ok(if has_column(c, name)? {
            name.to_owned()
        } else {
            format!("NULL AS {name}")
        })
    };
    let columns = format!(
        "id, item_id, call, actor, actor_kind, band, decision, kind, intent_id, inputs_hash, output, at_ms, cost_cents, {}, {}, prev_hash, hash",
        col("cost_micros")?,
        col("profile")?
    );
    let mut stmt = c
        .prepare(&format!("SELECT {columns} FROM evidence ORDER BY id"))
        .map_err(backend)?;
    let rows = stmt
        .query_map([], |r| {
            Ok(RawRow {
                id: r.get(0)?,
                item: r.get(1)?,
                call: r.get(2)?,
                actor: r.get(3)?,
                actor_kind: r.get(4)?,
                band: r.get(5)?,
                decision: r.get(6)?,
                kind: r.get(7)?,
                intent: r.get(8)?,
                inputs_hash: r.get(9)?,
                output: r.get(10)?,
                at_ms: r.get(11)?,
                cost_cents: r.get(12)?,
                cost_micros: r.get(13)?,
                profile: r.get(14)?,
                prev_hash: r.get(15)?,
                hash: r.get(16)?,
            })
        })
        .map_err(backend)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(backend)?;
    Ok(rows)
}

/// The first row the chain cannot vouch for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Broken {
    /// The row id.
    pub id: i64,
    /// What did not match.
    pub why: String,
}

/// The result of recomputing the chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChainReport {
    /// How many rows were read.
    pub rows: usize,
    /// The last row's hash ([`GENESIS`] when there are no rows): keep it elsewhere.
    pub head: String,
    /// The first broken row, if any.
    pub broken: Option<Broken>,
}

/// Recomputes the whole chain from the rows as they are now.
pub(super) fn verify_chain(c: &Connection) -> Res<ChainReport> {
    let rows = read_rows(c)?;
    let mut prev = GENESIS.to_owned();
    for r in &rows {
        if r.prev_hash != prev {
            return Ok(ChainReport {
                rows: rows.len(),
                head: prev,
                broken: Some(Broken {
                    id: r.id,
                    why: "its link to the previous row does not match (a row before it was removed, replaced or changed)".to_owned(),
                }),
            });
        }
        if row_hash(&r.prev_hash, r) != r.hash {
            return Ok(ChainReport {
                rows: rows.len(),
                head: prev,
                broken: Some(Broken {
                    id: r.id,
                    why: "its content does not match its hash (the row was changed)".to_owned(),
                }),
            });
        }
        prev.clone_from(&r.hash);
    }
    Ok(ChainReport {
        rows: rows.len(),
        head: prev,
        broken: None,
    })
}
