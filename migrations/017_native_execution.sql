CREATE TABLE container_policies (revision INTEGER PRIMARY KEY,digest TEXT NOT NULL,record TEXT NOT NULL CHECK(json_valid(record)),operator TEXT NOT NULL,installed_at TEXT NOT NULL);
CREATE TRIGGER container_policy_no_update BEFORE UPDATE ON container_policies BEGIN SELECT RAISE(ABORT,'container policy is immutable'); END;
CREATE TRIGGER container_policy_no_delete BEFORE DELETE ON container_policies BEGIN SELECT RAISE(ABORT,'container policy is immutable'); END;
CREATE TABLE container_jobs(id TEXT PRIMARY KEY,case_id TEXT NOT NULL REFERENCES cases(id),policy_revision INTEGER NOT NULL REFERENCES container_policies(revision),state TEXT NOT NULL,deadline TEXT NOT NULL,record TEXT NOT NULL CHECK(json_valid(record)));
CREATE INDEX container_job_dispatch ON container_jobs(state,deadline);
CREATE TABLE container_admissions(revision INTEGER PRIMARY KEY REFERENCES container_policies(revision),record TEXT NOT NULL CHECK(json_valid(record)));
CREATE TABLE repair_candidates(id TEXT PRIMARY KEY,case_id TEXT NOT NULL REFERENCES cases(id),repair_id TEXT NOT NULL REFERENCES repair_requests(id),record TEXT NOT NULL CHECK(json_valid(record)));
CREATE TRIGGER candidate_no_update BEFORE UPDATE ON repair_candidates BEGIN SELECT RAISE(ABORT,'candidate is immutable'); END;
CREATE TRIGGER candidate_no_delete BEFORE DELETE ON repair_candidates BEGIN SELECT RAISE(ABORT,'candidate is immutable'); END;
CREATE TABLE candidate_reviews(id TEXT PRIMARY KEY,candidate_id TEXT NOT NULL REFERENCES repair_candidates(id),case_id TEXT NOT NULL REFERENCES cases(id),record TEXT NOT NULL CHECK(json_valid(record)));
CREATE TRIGGER candidate_review_no_update BEFORE UPDATE ON candidate_reviews BEGIN SELECT RAISE(ABORT,'review is immutable'); END;
CREATE TRIGGER candidate_review_no_delete BEFORE DELETE ON candidate_reviews BEGIN SELECT RAISE(ABORT,'review is immutable'); END;

CREATE TABLE repair_execution_budgets(repair_id TEXT PRIMARY KEY REFERENCES repair_requests(id),started_at TEXT NOT NULL,deadline TEXT NOT NULL);
