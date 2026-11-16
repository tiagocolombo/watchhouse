CREATE TABLE worker_policies (revision INTEGER PRIMARY KEY, digest TEXT NOT NULL, record TEXT NOT NULL CHECK(json_valid(record)), operator TEXT NOT NULL, installed_at TEXT NOT NULL);
CREATE TRIGGER immutable_worker_policy_update BEFORE UPDATE ON worker_policies BEGIN SELECT RAISE(ABORT,'worker policy revisions are immutable'); END;
CREATE TRIGGER immutable_worker_policy_delete BEFORE DELETE ON worker_policies BEGIN SELECT RAISE(ABORT,'worker policy revisions are immutable'); END;
CREATE TABLE worker_qualifications (id TEXT PRIMARY KEY, case_id TEXT NOT NULL REFERENCES cases(id), state TEXT NOT NULL, deadline TEXT NOT NULL, record TEXT NOT NULL CHECK(json_valid(record)));
CREATE INDEX worker_qualification_due ON worker_qualifications(state,deadline);
CREATE TABLE worker_qualification_late (id TEXT NOT NULL REFERENCES worker_qualifications(id), digest TEXT NOT NULL, PRIMARY KEY(id,digest));
