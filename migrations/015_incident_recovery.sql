CREATE TABLE recovery_state (case_id TEXT PRIMARY KEY REFERENCES cases(id), record TEXT NOT NULL CHECK(json_valid(record)));
CREATE TABLE recovery_records (id TEXT PRIMARY KEY, case_id TEXT NOT NULL REFERENCES cases(id), kind TEXT NOT NULL, record TEXT NOT NULL CHECK(json_valid(record)));
CREATE TRIGGER recovery_records_no_update BEFORE UPDATE ON recovery_records BEGIN SELECT RAISE(ABORT,'recovery records are immutable'); END;
CREATE TRIGGER recovery_records_no_delete BEFORE DELETE ON recovery_records BEGIN SELECT RAISE(ABORT,'recovery records are immutable'); END;
CREATE TABLE corrective_actions (id TEXT PRIMARY KEY, case_id TEXT NOT NULL REFERENCES cases(id), state TEXT NOT NULL, due_at TEXT NOT NULL, escalated INTEGER NOT NULL DEFAULT 0, record TEXT NOT NULL CHECK(json_valid(record)));
CREATE INDEX corrective_action_due ON corrective_actions(state,due_at,escalated);
