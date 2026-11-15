CREATE TABLE policy_versions (
 revision INTEGER PRIMARY KEY CHECK(revision>0), name TEXT NOT NULL UNIQUE,
 digest TEXT NOT NULL UNIQUE, record TEXT NOT NULL CHECK(json_valid(record)),
 installed_by TEXT NOT NULL, source TEXT NOT NULL, installed_at TEXT NOT NULL
);
CREATE TABLE policy_active (singleton INTEGER PRIMARY KEY CHECK(singleton=1), revision INTEGER NOT NULL REFERENCES policy_versions(revision));
CREATE TABLE case_policy (case_id TEXT PRIMARY KEY REFERENCES cases(id), revision INTEGER NOT NULL REFERENCES policy_versions(revision));
CREATE TRIGGER immutable_policy BEFORE UPDATE ON policy_versions BEGIN SELECT RAISE(ABORT,'policy versions are immutable'); END;
CREATE TRIGGER retain_policy BEFORE DELETE ON policy_versions BEGIN SELECT RAISE(ABORT,'policy versions are retained'); END;
CREATE TRIGGER pinned_case_policy BEFORE UPDATE ON case_policy BEGIN SELECT RAISE(ABORT,'governing policy cannot change for an active case'); END;
