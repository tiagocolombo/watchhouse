CREATE TABLE model_policies(revision INTEGER PRIMARY KEY,digest TEXT NOT NULL,record TEXT NOT NULL CHECK(json_valid(record)),operator TEXT NOT NULL,source TEXT NOT NULL,installed_at TEXT NOT NULL);
CREATE TRIGGER model_policy_immutable BEFORE UPDATE ON model_policies BEGIN SELECT RAISE(ABORT,'model policy is immutable'); END;
CREATE TRIGGER model_policy_retained BEFORE DELETE ON model_policies BEGIN SELECT RAISE(ABORT,'model policy is retained'); END;
CREATE TABLE model_jobs(id TEXT PRIMARY KEY,case_id TEXT NOT NULL REFERENCES cases(id),policy_revision INTEGER NOT NULL REFERENCES model_policies(revision),state TEXT NOT NULL,deadline TEXT NOT NULL,record TEXT NOT NULL CHECK(json_valid(record)));
CREATE INDEX model_job_dispatch ON model_jobs(state,deadline);
CREATE TABLE model_admissions(revision INTEGER PRIMARY KEY REFERENCES model_policies(revision),record TEXT NOT NULL CHECK(json_valid(record)));
