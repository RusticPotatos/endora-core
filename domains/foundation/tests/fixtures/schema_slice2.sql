
CREATE TABLE IF NOT EXISTS goals (
  id INTEGER PRIMARY KEY AUTOINCREMENT, title TEXT NOT NULL, why TEXT NOT NULL,
  tier INTEGER NOT NULL, spend_cap_cents INTEGER NOT NULL, created_ms INTEGER NOT NULL) STRICT;
CREATE TABLE IF NOT EXISTS goal_scope (
  goal_id INTEGER NOT NULL REFERENCES goals(id), call TEXT NOT NULL, PRIMARY KEY (goal_id, call)) STRICT;
CREATE TABLE IF NOT EXISTS routines (
  id INTEGER PRIMARY KEY AUTOINCREMENT, goal_id INTEGER NOT NULL REFERENCES goals(id),
  title TEXT NOT NULL, every_ms INTEGER NOT NULL, next_due_ms INTEGER NOT NULL,
  auto_accept INTEGER NOT NULL) STRICT;
CREATE TABLE IF NOT EXISTS items (
  id INTEGER PRIMARY KEY AUTOINCREMENT, goal_id INTEGER NOT NULL REFERENCES goals(id),
  routine_id INTEGER REFERENCES routines(id), title TEXT NOT NULL,
  state TEXT NOT NULL CHECK (state IN ('open','running','done','blocked','failed')),
  attempts INTEGER NOT NULL DEFAULT 0, due_ms INTEGER, missed INTEGER NOT NULL DEFAULT 0,
  reason TEXT, accepted_by TEXT, accepted_ms INTEGER, created_ms INTEGER NOT NULL) STRICT;
CREATE TABLE IF NOT EXISTS firings (
  routine_id INTEGER NOT NULL REFERENCES routines(id), due_ms INTEGER NOT NULL,
  outcome TEXT NOT NULL CHECK (outcome IN ('fired','skipped_open','missed_down')),
  item_id INTEGER REFERENCES items(id), PRIMARY KEY (routine_id, due_ms)) STRICT;
CREATE TABLE IF NOT EXISTS criteria (
  owner_kind TEXT NOT NULL CHECK (owner_kind IN ('goal','item')), owner_id INTEGER NOT NULL,
  idx INTEGER NOT NULL, text TEXT NOT NULL, check_call TEXT, check_input TEXT, check_expect TEXT,
  met_by TEXT, met_evidence_id INTEGER, PRIMARY KEY (owner_kind, owner_id, idx)) STRICT;
CREATE TABLE IF NOT EXISTS policy (
  actor TEXT NOT NULL, call TEXT NOT NULL, band TEXT NOT NULL, tier INTEGER NOT NULL,
  max_uses_per_item INTEGER, PRIMARY KEY (actor, call)) STRICT;
CREATE TABLE IF NOT EXISTS control (key TEXT PRIMARY KEY, value INTEGER NOT NULL) STRICT;
CREATE TABLE IF NOT EXISTS evidence (
  id INTEGER PRIMARY KEY AUTOINCREMENT, item_id INTEGER REFERENCES items(id), call TEXT NOT NULL,
  actor TEXT NOT NULL, band TEXT NOT NULL, decision TEXT NOT NULL CHECK (decision IN ('allow','deny')),
  kind TEXT NOT NULL, intent_id INTEGER, inputs_hash TEXT NOT NULL, output TEXT NOT NULL,
  at_ms INTEGER NOT NULL) STRICT;
CREATE TRIGGER IF NOT EXISTS evidence_no_change BEFORE UPDATE ON evidence
  BEGIN SELECT RAISE(ABORT, 'evidence is append-only'); END;
CREATE TRIGGER IF NOT EXISTS evidence_no_removal BEFORE DELETE ON evidence
  BEGIN SELECT RAISE(ABORT, 'evidence is append-only'); END;
