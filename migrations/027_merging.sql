CREATE TABLE merge_policies (revision INTEGER PRIMARY KEY, digest TEXT NOT NULL, record TEXT NOT NULL, operator TEXT NOT NULL, source TEXT NOT NULL, recorded_at TEXT NOT NULL);
CREATE TRIGGER merge_policy_no_update BEFORE UPDATE ON merge_policies BEGIN SELECT RAISE(ABORT,'merge policies immutable'); END;
CREATE TRIGGER merge_policy_no_delete BEFORE DELETE ON merge_policies BEGIN SELECT RAISE(ABORT,'merge policies immutable'); END;
CREATE TABLE merge_jobs (id TEXT PRIMARY KEY, case_id TEXT NOT NULL REFERENCES cases(id), publication_id TEXT NOT NULL UNIQUE REFERENCES publication_jobs(id), state TEXT NOT NULL, deadline TEXT NOT NULL, record TEXT NOT NULL);
CREATE INDEX merge_due ON merge_jobs(state,deadline);
