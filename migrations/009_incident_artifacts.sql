CREATE TABLE incident_artifacts (id TEXT NOT NULL, revision INTEGER NOT NULL, case_id TEXT NOT NULL REFERENCES cases(id), record TEXT NOT NULL CHECK(json_valid(record)), PRIMARY KEY(id,revision));
CREATE INDEX incident_artifact_case ON incident_artifacts(case_id,id,revision);
CREATE TRIGGER immutable_incident_artifact BEFORE UPDATE ON incident_artifacts BEGIN SELECT RAISE(ABORT,'artifact versions are immutable'); END;
CREATE TABLE communication_approvals (id TEXT NOT NULL, revision INTEGER NOT NULL, record TEXT NOT NULL, PRIMARY KEY(id,revision), FOREIGN KEY(id,revision) REFERENCES incident_artifacts(id,revision));
CREATE TABLE artifact_obligations (id TEXT PRIMARY KEY, case_id TEXT NOT NULL REFERENCES cases(id), owner TEXT NOT NULL, due_at TEXT NOT NULL, state TEXT NOT NULL);
CREATE INDEX artifact_obligation_due ON artifact_obligations(state,due_at);
