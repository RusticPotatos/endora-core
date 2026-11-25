-- A real schema-9 file: made by the build at origin/main 20836a0 (init, goal add, two routine add, run-due), dumped
-- as SQL with the owner credential row left out (the test enrols its own). Foreign keys are off because a dump lists
-- tables in name order. Each 64-digit hash is written as four 16-digit parts joined with || so the personal-values
-- check does not take a chain hash for a secret. PRAGMA user_version = 9 is set by the test.
PRAGMA foreign_keys = OFF;
BEGIN TRANSACTION;
CREATE TABLE control (key TEXT PRIMARY KEY, value INTEGER NOT NULL) STRICT;
CREATE TABLE criteria (
  owner_kind TEXT NOT NULL CHECK (owner_kind IN ('goal','item')), owner_id INTEGER NOT NULL,
  idx INTEGER NOT NULL, text TEXT NOT NULL, check_call TEXT, check_input TEXT, check_expect TEXT, check_field TEXT,
  met_by TEXT, met_evidence_id INTEGER, PRIMARY KEY (owner_kind, owner_id, idx)) STRICT;
INSERT INTO "criteria" VALUES('goal',1,0,'looks right',NULL,NULL,NULL,NULL,NULL,NULL);
CREATE TABLE evidence (
  id INTEGER PRIMARY KEY AUTOINCREMENT, item_id INTEGER REFERENCES items(id), call TEXT NOT NULL,
  actor TEXT NOT NULL, band TEXT NOT NULL, decision TEXT NOT NULL CHECK (decision IN ('allow','deny')),
  kind TEXT NOT NULL, intent_id INTEGER, inputs_hash TEXT NOT NULL, output TEXT NOT NULL,
  at_ms INTEGER NOT NULL, cost_cents INTEGER NOT NULL DEFAULT 0, actor_kind TEXT NOT NULL DEFAULT 'capability', prev_hash TEXT NOT NULL DEFAULT '', hash TEXT NOT NULL DEFAULT '', cost_micros INTEGER, profile TEXT) STRICT;
INSERT INTO "evidence" VALUES(1,NULL,'core.owner-enrolled','core','observe','allow','control',NULL,'3a674af87b99a8d6'||'5953f6f5c2b1460f'||'80b1dc65dadf24e6'||'b83b1d8f4ae07483','an owner credential was created',1795580138949,0,'core','0000000000000000'||'0000000000000000'||'0000000000000000'||'0000000000000000','21dde4677ad8775d'||'86d8ff88b8ad6d81'||'ff2bbb6b51b81486'||'5ac4e7d300b6634f',0,NULL);
INSERT INTO "evidence" VALUES(2,NULL,'core.goal','owner','observe','allow','control',NULL,'e4066ae39815923e'||'6c70aec58fb8798a'||'1f7fba259ad78b3a'||'afcbf48025950bc7','goal 1 created (acceptance: owner)',1795580138984,0,'owner','21dde4677ad8775d'||'86d8ff88b8ad6d81'||'ff2bbb6b51b81486'||'5ac4e7d300b6634f','4ce69709cc820ade'||'4778687f24058323'||'f9a5848b8f9d7eab'||'4c5e189830487a44',0,NULL);
INSERT INTO "evidence" VALUES(3,NULL,'core.routine','owner','observe','allow','control',NULL,'2574608072d4d7f2'||'02218bcf4ee9c413'||'f56f7b9706f1d99b'||'ca01092515730085','routine 1 declared (pauses after 5 failures)',1795580139010,0,'owner','4ce69709cc820ade'||'4778687f24058323'||'f9a5848b8f9d7eab'||'4c5e189830487a44','6f4c2177c156e0cf'||'e2c89332551aa69d'||'62247f019ed08e0e'||'786e76c8b60d6331',0,NULL);
INSERT INTO "evidence" VALUES(4,NULL,'core.routine','owner','observe','allow','control',NULL,'497d3c7c98ec4c7d'||'21987c4fca18bec4'||'d2dcc164c7949da7'||'f489b105f43ddeb7','routine 2 declared (pauses after 5 failures)',1795580139036,0,'owner','6f4c2177c156e0cf'||'e2c89332551aa69d'||'62247f019ed08e0e'||'786e76c8b60d6331','c8ec6714c847c691'||'ddf227fb33255c50'||'5badeb09843f29eb'||'47280ef006173234',0,NULL);
INSERT INTO "evidence" VALUES(5,1,'core.start','runner','observe','allow','control',NULL,'2b86660329aa2bc0'||'fe1fd3aa9949d624'||'798311dbdde40e57'||'d47925043fdf90b0','item started',1795580139114,0,'capability','c8ec6714c847c691'||'ddf227fb33255c50'||'5badeb09843f29eb'||'47280ef006173234','baf584372b44b1e9'||'cead41179ccf189b'||'68825f0a97e6deed'||'460aa47842581422',0,NULL);
INSERT INTO "evidence" VALUES(6,2,'core.start','runner','observe','allow','control',NULL,'2b86660329aa2bc0'||'fe1fd3aa9949d624'||'798311dbdde40e57'||'d47925043fdf90b0','item started',1795580139134,0,'capability','baf584372b44b1e9'||'cead41179ccf189b'||'68825f0a97e6deed'||'460aa47842581422','1dffd37fb63ffcb8'||'a463857e98466ed0'||'c4e3c18ee24bfa89'||'14eb0e8859393c37',0,NULL);
CREATE TABLE firings (
  routine_id INTEGER NOT NULL REFERENCES routines(id), due_ms INTEGER NOT NULL,
  outcome TEXT NOT NULL CHECK (outcome IN ('fired','skipped_open','missed_down')),
  item_id INTEGER REFERENCES items(id), missed_count INTEGER NOT NULL DEFAULT 1,
  PRIMARY KEY (routine_id, due_ms)) STRICT;
INSERT INTO "firings" VALUES(1,1795580139001,'fired',1,1);
INSERT INTO "firings" VALUES(2,1795580139027,'fired',2,1);
CREATE TABLE goal_scope (
  goal_id INTEGER NOT NULL REFERENCES goals(id), call TEXT NOT NULL, PRIMARY KEY (goal_id, call)) STRICT;
CREATE TABLE goals (
  id INTEGER PRIMARY KEY AUTOINCREMENT, title TEXT NOT NULL, why TEXT NOT NULL,
  tier INTEGER NOT NULL, spend_cap_micros INTEGER NOT NULL, created_ms INTEGER NOT NULL,
  acceptance TEXT NOT NULL DEFAULT 'owner' CHECK (acceptance IN ('owner','auto-observe'))) STRICT;
INSERT INTO "goals" VALUES(1,'t','w',1,0,1795580138964,'owner');
CREATE TABLE item_met (
  item_id INTEGER NOT NULL REFERENCES items(id), idx INTEGER NOT NULL, met_by TEXT NOT NULL,
  met_evidence_id INTEGER, PRIMARY KEY (item_id, idx)) STRICT;
CREATE TABLE items (
  id INTEGER PRIMARY KEY AUTOINCREMENT, goal_id INTEGER NOT NULL REFERENCES goals(id),
  routine_id INTEGER REFERENCES routines(id), title TEXT NOT NULL,
  state TEXT NOT NULL CHECK (state IN ('open','running','done','blocked','failed')),
  attempts INTEGER NOT NULL DEFAULT 0, due_ms INTEGER, missed INTEGER NOT NULL DEFAULT 0,
  reason TEXT, accepted_by TEXT, accepted_ms INTEGER, created_ms INTEGER NOT NULL, subject TEXT, holder TEXT) STRICT;
INSERT INTO "items" VALUES(1,1,1,'disk free','running',1,1795580139001,0,NULL,NULL,NULL,1795580139058,NULL,'1700612-18eb2e9f8522f664');
INSERT INTO "items" VALUES(2,1,2,'disk free','running',1,1795580139027,0,NULL,NULL,NULL,1795580139058,NULL,'1700612-18eb2e9f8522f664');
CREATE TABLE owner_credential (
           id INTEGER PRIMARY KEY CHECK (id = 1), salt TEXT NOT NULL, hash TEXT NOT NULL,
           created_ms INTEGER NOT NULL) STRICT;
CREATE TABLE policy (
  actor TEXT NOT NULL, call TEXT NOT NULL, band TEXT NOT NULL, tier INTEGER NOT NULL,
  max_uses_per_item INTEGER, PRIMARY KEY (actor, call)) STRICT;
CREATE TABLE routines (
  id INTEGER PRIMARY KEY AUTOINCREMENT, goal_id INTEGER NOT NULL REFERENCES goals(id),
  title TEXT NOT NULL, every_ms INTEGER NOT NULL, next_due_ms INTEGER NOT NULL,
  auto_accept INTEGER NOT NULL, max_failures INTEGER NOT NULL DEFAULT 5,
  consecutive_failures INTEGER NOT NULL DEFAULT 0, paused INTEGER NOT NULL DEFAULT 0, retired INTEGER NOT NULL DEFAULT 0) STRICT;
INSERT INTO "routines" VALUES(1,1,'disk free',60000,1795580199001,0,5,0,0,0);
INSERT INTO "routines" VALUES(2,1,'disk free',3600000,1795583739027,0,5,0,0,0);
CREATE TRIGGER evidence_no_removal BEFORE DELETE ON evidence
  BEGIN SELECT RAISE(ABORT, 'evidence is append-only'); END;
CREATE TRIGGER evidence_no_replace BEFORE INSERT ON evidence
  WHEN EXISTS (SELECT 1 FROM evidence WHERE id = NEW.id)
  BEGIN SELECT RAISE(ABORT, 'evidence is append-only'); END;
CREATE TRIGGER firings_no_change BEFORE UPDATE ON firings
  BEGIN SELECT RAISE(ABORT, 'firings are append-only'); END;
CREATE TRIGGER firings_no_removal BEFORE DELETE ON firings
  BEGIN SELECT RAISE(ABORT, 'firings are append-only'); END;
CREATE TRIGGER firings_no_replace BEFORE INSERT ON firings
  WHEN EXISTS (SELECT 1 FROM firings WHERE routine_id = NEW.routine_id AND due_ms = NEW.due_ms)
  BEGIN SELECT RAISE(ABORT, 'firings are append-only'); END;
CREATE TRIGGER goals_acceptance_fixed BEFORE UPDATE OF acceptance ON goals BEGIN SELECT RAISE(ABORT, 'acceptance policy is fixed at creation'); END;
CREATE TRIGGER owner_credential_no_change BEFORE UPDATE ON owner_credential
           BEGIN SELECT RAISE(ABORT, 'the owner credential is fixed'); END;
CREATE TRIGGER owner_credential_no_removal BEFORE DELETE ON owner_credential
           BEGIN SELECT RAISE(ABORT, 'the owner credential is fixed'); END;
CREATE TRIGGER evidence_no_change BEFORE UPDATE ON evidence BEGIN SELECT RAISE(ABORT, 'evidence is append-only'); END;
DELETE FROM "sqlite_sequence";
INSERT INTO "sqlite_sequence" VALUES('evidence',6);
INSERT INTO "sqlite_sequence" VALUES('goals',1);
INSERT INTO "sqlite_sequence" VALUES('routines',2);
INSERT INTO "sqlite_sequence" VALUES('items',2);
COMMIT;
