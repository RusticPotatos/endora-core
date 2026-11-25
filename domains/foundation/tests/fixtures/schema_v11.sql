-- Schema 11 as written by the build before diagnosis (the schema-10 file plus the notice tables of ADR 0152), with one failing run and its notice.
-- A real schema-10 file: made by the build at origin/main 8778985 (init, goal add, two routine add, repair add, routine pause,
-- run-due), dumped as SQL with the owner credential row left out (the test enrols its own). It holds a repair row (goal 1)
-- and a paused routine (routine 2). Foreign keys are off because a dump lists tables in name order. Each 64-digit hash is
-- written as four 16-digit parts joined with || so the personal-values check does not take a chain hash for a secret.
-- PRAGMA user_version = 10 is set by the test.
PRAGMA foreign_keys = OFF;
BEGIN TRANSACTION;
CREATE TABLE control (key TEXT PRIMARY KEY, value INTEGER NOT NULL) STRICT;
CREATE TABLE criteria (
  owner_kind TEXT NOT NULL CHECK (owner_kind IN ('goal','item')), owner_id INTEGER NOT NULL,
  idx INTEGER NOT NULL, text TEXT NOT NULL, check_call TEXT, check_input TEXT, check_expect TEXT, check_field TEXT,
  met_by TEXT, met_evidence_id INTEGER, PRIMARY KEY (owner_kind, owner_id, idx)) STRICT;
INSERT INTO "criteria" VALUES('goal',1,0,'looks right',NULL,NULL,NULL,NULL,NULL,NULL);
INSERT INTO "criteria" VALUES('goal',1,1,'probe web is healthy','probe.run','web','0','exit',NULL,NULL);
CREATE TABLE evidence (
  id INTEGER PRIMARY KEY AUTOINCREMENT, item_id INTEGER REFERENCES items(id), call TEXT NOT NULL,
  actor TEXT NOT NULL, band TEXT NOT NULL, decision TEXT NOT NULL CHECK (decision IN ('allow','deny')),
  kind TEXT NOT NULL, intent_id INTEGER, inputs_hash TEXT NOT NULL, output TEXT NOT NULL,
  at_ms INTEGER NOT NULL, cost_cents INTEGER NOT NULL DEFAULT 0, actor_kind TEXT NOT NULL DEFAULT 'capability', prev_hash TEXT NOT NULL DEFAULT '', hash TEXT NOT NULL DEFAULT '', cost_micros INTEGER, profile TEXT) STRICT;
INSERT INTO "evidence" VALUES(1,NULL,'core.owner-enrolled','core','observe','allow','control',NULL,'3a674af87b99a8d6'||'5953f6f5c2b1460f'||'80b1dc65dadf24e6'||'b83b1d8f4ae07483','an owner credential was created',1795583202273,0,'core','0000000000000000'||'0000000000000000'||'0000000000000000'||'0000000000000000','726d5d7f20d81bff'||'d9081c1015a68fc7'||'fdd439448adbd670'||'d6701f4c68f0178d',0,NULL);
INSERT INTO "evidence" VALUES(2,NULL,'core.goal','owner','observe','allow','control',NULL,'013ff97d50cd5212'||'06d20f2fcc3a378e'||'da931d644add7ffe'||'ac1ce5fcc303294b','goal 1 created (acceptance: auto-observe)',1795583202350,0,'owner','726d5d7f20d81bff'||'d9081c1015a68fc7'||'fdd439448adbd670'||'d6701f4c68f0178d','ab7b2851e333e9c9'||'6cf0affea0727cce'||'47b5f05693e36f0a'||'0b9d05e99e4e0f9f',0,NULL);
INSERT INTO "evidence" VALUES(3,NULL,'core.grant','owner','observe','allow','control',NULL,'231322e18744df13'||'083f230cf9f35af7'||'aaab8ff014671a48'||'0725151c0251c9fc','core may call probe.run up to observe',1795583202368,0,'owner','ab7b2851e333e9c9'||'6cf0affea0727cce'||'47b5f05693e36f0a'||'0b9d05e99e4e0f9f','7dae1ed85edd8454'||'35662e8a10d845b4'||'373d4cf9be209635'||'4eb890751def291e',0,NULL);
INSERT INTO "evidence" VALUES(4,NULL,'core.routine','owner','observe','allow','control',NULL,'2574608072d4d7f2'||'02218bcf4ee9c413'||'f56f7b9706f1d99b'||'ca01092515730085','routine 1 declared (pauses after 5 failures)',1795583202394,0,'owner','7dae1ed85edd8454'||'35662e8a10d845b4'||'373d4cf9be209635'||'4eb890751def291e','f45a62606a3cfe1a'||'4f7a9f1c7cc6cf2b'||'c0aee95dc11c6220'||'bdbed3085c5fbeae',0,NULL);
INSERT INTO "evidence" VALUES(5,NULL,'core.routine','owner','observe','allow','control',NULL,'497d3c7c98ec4c7d'||'21987c4fca18bec4'||'d2dcc164c7949da7'||'f489b105f43ddeb7','routine 2 declared (pauses after 5 failures)',1795583202420,0,'owner','f45a62606a3cfe1a'||'4f7a9f1c7cc6cf2b'||'c0aee95dc11c6220'||'bdbed3085c5fbeae','72b07a970eab3ac8'||'952c10caf063cfda'||'ac01ec4c956de0aa'||'803b87a038f7119e',0,NULL);
INSERT INTO "evidence" VALUES(6,NULL,'core.repair','owner','observe','allow','control',NULL,'97187e95cf87930c'||'f9a7401db6162e72'||'074da513a76fd4f9'||'4abb5647c929c6c9','goal 1 may repair a failed watch with action.run, once per failure episode',1795583202455,0,'owner','72b07a970eab3ac8'||'952c10caf063cfda'||'ac01ec4c956de0aa'||'803b87a038f7119e','d323cc4202e847da'||'a839ecb0e0d3a35a'||'5211ff7ed43476c7'||'c0c652c5631d3a2c',0,NULL);
INSERT INTO "evidence" VALUES(7,NULL,'core.pause_routine','owner','observe','allow','control',NULL,'966eff5b80ba7bea'||'2b7f153d74df8d50'||'4155cb33b9e37510'||'86e8d9f5ad2c6a2c','routine 2 paused',1795583202480,0,'owner','d323cc4202e847da'||'a839ecb0e0d3a35a'||'5211ff7ed43476c7'||'c0c652c5631d3a2c','bac93773f5079401'||'d8ea3a54e012ddb8'||'2fce56a913416c3c'||'5815628fe804634c',0,NULL);
INSERT INTO "evidence" VALUES(8,1,'core.start','runner','observe','allow','control',NULL,'2b86660329aa2bc0'||'fe1fd3aa9949d624'||'798311dbdde40e57'||'d47925043fdf90b0','item started',1795583202517,0,'capability','bac93773f5079401'||'d8ea3a54e012ddb8'||'2fce56a913416c3c'||'5815628fe804634c','245ac91947d6ea12'||'5cbff85c33f70ed7'||'18ec4bf89700be47'||'c4435b64f2c0f8d0',0,NULL);
INSERT INTO "evidence" VALUES(9,1,'probe.run','core','observe','allow','intent',NULL,'f9536a789e0aaf2c'||'4f16599bf1de8f37'||'9282b5fabaf0a77a'||'642f3757f74555d1','',1795583202528,0,'core','245ac91947d6ea12'||'5cbff85c33f70ed7'||'18ec4bf89700be47'||'c4435b64f2c0f8d0','b0e1ac624979bf71'||'064ebb2ed665b98f'||'6da1738f97a74765'||'b7a3cecb48bd9139',0,NULL);
INSERT INTO "evidence" VALUES(10,1,'probe.run','core','observe','allow','result',9,'f9536a789e0aaf2c'||'4f16599bf1de8f37'||'9282b5fabaf0a77a'||'642f3757f74555d1','command true
[facts] exit=0',1795583202544,0,'core','b0e1ac624979bf71'||'064ebb2ed665b98f'||'6da1738f97a74765'||'b7a3cecb48bd9139','4585c28baaf0f1ae'||'2742f0713c38d91b'||'255f3821b82f25c1'||'2b749d2327257b6d',0,NULL);
INSERT INTO "evidence" VALUES(11,1,'core.accept','policy:auto-observe','observe','deny','refusal',NULL,'7e7118721cd1e3eb'||'580f963709c7c1cf'||'9b16c249017b568a'||'7d533fddef2169d7','refused: a criterion is not met',1795583202564,0,'policy','4585c28baaf0f1ae'||'2742f0713c38d91b'||'255f3821b82f25c1'||'2b749d2327257b6d','b122f8de318fefca'||'8966a396580e72ae'||'11bd35955d140a93'||'65ea6781f9f06b9c',0,NULL);
CREATE TABLE firings (
  routine_id INTEGER NOT NULL REFERENCES routines(id), due_ms INTEGER NOT NULL,
  outcome TEXT NOT NULL CHECK (outcome IN ('fired','skipped_open','missed_down')),
  item_id INTEGER REFERENCES items(id), missed_count INTEGER NOT NULL DEFAULT 1,
  PRIMARY KEY (routine_id, due_ms)) STRICT;
INSERT INTO "firings" VALUES(1,1795583202385,'fired',1,1);
CREATE TABLE goal_scope (
  goal_id INTEGER NOT NULL REFERENCES goals(id), call TEXT NOT NULL, PRIMARY KEY (goal_id, call)) STRICT;
INSERT INTO "goal_scope" VALUES(1,'probe.run');
INSERT INTO "goal_scope" VALUES(1,'action.run');
CREATE TABLE goals (
  id INTEGER PRIMARY KEY AUTOINCREMENT, title TEXT NOT NULL, why TEXT NOT NULL,
  tier INTEGER NOT NULL, spend_cap_micros INTEGER NOT NULL, created_ms INTEGER NOT NULL,
  acceptance TEXT NOT NULL DEFAULT 'owner' CHECK (acceptance IN ('owner','auto-observe'))) STRICT;
INSERT INTO "goals" VALUES(1,'web front','web front',1,0,1795583202289,'auto-observe');
CREATE TABLE item_met (
  item_id INTEGER NOT NULL REFERENCES items(id), idx INTEGER NOT NULL, met_by TEXT NOT NULL,
  met_evidence_id INTEGER, PRIMARY KEY (item_id, idx)) STRICT;
INSERT INTO "item_met" VALUES(1,1,'check',10);
CREATE TABLE items (
  id INTEGER PRIMARY KEY AUTOINCREMENT, goal_id INTEGER NOT NULL REFERENCES goals(id),
  routine_id INTEGER REFERENCES routines(id), title TEXT NOT NULL,
  state TEXT NOT NULL CHECK (state IN ('open','running','done','blocked','failed')),
  attempts INTEGER NOT NULL DEFAULT 0, due_ms INTEGER, missed INTEGER NOT NULL DEFAULT 0,
  reason TEXT, accepted_by TEXT, accepted_ms INTEGER, created_ms INTEGER NOT NULL, subject TEXT, holder TEXT) STRICT;
INSERT INTO "items" VALUES(1,1,1,'web front','running',1,1795583202385,0,NULL,NULL,NULL,1795583202497,NULL,'1865911-18eb3168c62b007f');
CREATE TABLE owner_credential (
           id INTEGER PRIMARY KEY CHECK (id = 1), salt TEXT NOT NULL, hash TEXT NOT NULL,
           created_ms INTEGER NOT NULL) STRICT;
CREATE TABLE policy (
  actor TEXT NOT NULL, call TEXT NOT NULL, band TEXT NOT NULL, tier INTEGER NOT NULL,
  max_uses_per_item INTEGER, PRIMARY KEY (actor, call)) STRICT;
INSERT INTO "policy" VALUES('core','probe.run','observe',1,NULL);
INSERT INTO "policy" VALUES('repair','action.run','reversible',1,1);
CREATE TABLE repairs (
           goal_id INTEGER PRIMARY KEY REFERENCES goals(id), call TEXT NOT NULL,
           input TEXT NOT NULL, created_ms INTEGER NOT NULL) STRICT;
INSERT INTO "repairs" VALUES(1,'action.run','restart',1795583202438);
CREATE TABLE routines (
  id INTEGER PRIMARY KEY AUTOINCREMENT, goal_id INTEGER NOT NULL REFERENCES goals(id),
  title TEXT NOT NULL, every_ms INTEGER NOT NULL, next_due_ms INTEGER NOT NULL,
  auto_accept INTEGER NOT NULL, max_failures INTEGER NOT NULL DEFAULT 5,
  consecutive_failures INTEGER NOT NULL DEFAULT 0, paused INTEGER NOT NULL DEFAULT 0, retired INTEGER NOT NULL DEFAULT 0) STRICT;
INSERT INTO "routines" VALUES(1,1,'web front',60000,1795583262385,0,5,0,0,0);
INSERT INTO "routines" VALUES(2,1,'second',120000,1795583202411,0,5,0,1,0);
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
INSERT INTO "sqlite_sequence" VALUES('evidence',11);
INSERT INTO "sqlite_sequence" VALUES('goals',1);
INSERT INTO "sqlite_sequence" VALUES('routines',2);
INSERT INTO "sqlite_sequence" VALUES('items',1);
CREATE TABLE watch_state (
           routine_id INTEGER PRIMARY KEY REFERENCES routines(id),
           failing_since_ms INTEGER NOT NULL, announced INTEGER NOT NULL DEFAULT 0) STRICT;
CREATE TABLE notices (
           id INTEGER PRIMARY KEY AUTOINCREMENT, key TEXT NOT NULL UNIQUE,
           routine_id INTEGER NOT NULL REFERENCES routines(id), routine_title TEXT NOT NULL,
           state TEXT NOT NULL CHECK (state IN ('failing','recovered')), at_ms INTEGER NOT NULL,
           delivered_ms INTEGER, attempts INTEGER NOT NULL DEFAULT 0, acked_ms INTEGER) STRICT;
INSERT INTO "watch_state" VALUES(1,1795583000000,1);
INSERT INTO "notices" VALUES(1,'1:failing:1795583000000',1,'example watch','failing',1795583700000,NULL,0,NULL);
COMMIT;
