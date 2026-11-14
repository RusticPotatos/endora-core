//! Upgrading evidence rows written by older builds. This is the ONLY code in the crate that
//! changes existing evidence rows: it runs once, inside the migration's transaction, with the
//! update trigger dropped and put back before the transaction ends. No engine path calls it
//! after the upgrade, and it only fills columns that did not exist when the row was written.

use endora_kernel::RepositoryError;
use endora_persistence::backend;
use rusqlite::Connection;

type Res<T> = Result<T, RepositoryError>;

/// Runs `f` with the evidence update trigger off, then puts it back.
pub(super) fn with_evidence_update_allowed(
    c: &Connection,
    f: impl FnOnce(&Connection) -> Res<()>,
) -> Res<()> {
    c.execute_batch("DROP TRIGGER IF EXISTS evidence_no_change")
        .map_err(backend)?;
    f(c)?;
    c.execute_batch("CREATE TRIGGER IF NOT EXISTS evidence_no_change BEFORE UPDATE ON evidence BEGIN SELECT RAISE(ABORT, 'evidence is append-only'); END;")
        .map_err(backend)
}

/// Older rows get the kind their name implies. Their `owner` rows cannot be told from a
/// forged `owner`: that is the very weakness version 3 closes for NEW rows.
pub(super) fn backfill_actor_kind(c: &Connection) -> Res<()> {
    with_evidence_update_allowed(c, |c| {
        c.execute_batch(
            "UPDATE evidence SET actor_kind = CASE
               WHEN actor = 'core' THEN 'core' WHEN actor = 'owner' THEN 'owner'
               WHEN actor LIKE 'policy:%' THEN 'policy' ELSE 'capability' END",
        )
        .map_err(backend)
    })
}

/// Gives every row written before the chain existed its link, in id order. The chain therefore
/// vouches for old rows only from the moment of the upgrade: it cannot know what they said
/// before. Only rows whose hash is EMPTY are filled; a row that already has a hash is never
/// touched (the store refuses to migrate a file that has any, see `refuse_laundering`).
/// Migrations belong to the trusted setup context.
pub(super) fn backfill_chain(c: &Connection) -> Res<()> {
    with_evidence_update_allowed(c, |c| {
        let mut prev = super::chain::GENESIS.to_owned();
        for mut r in super::chain::read_rows(c)?
            .into_iter()
            .filter(|r| r.hash.is_empty())
        {
            r.prev_hash.clone_from(&prev);
            let h = super::chain::row_hash(&prev, &r);
            c.execute(
                "UPDATE evidence SET prev_hash = ?1, hash = ?2 WHERE id = ?3 AND hash = ''",
                rusqlite::params![prev, h, r.id],
            )
            .map_err(backend)?;
            prev = h;
        }
        Ok(())
    })
}
