CREATE TABLE hypothesis_approvals(id TEXT NOT NULL,revision INTEGER NOT NULL,record TEXT NOT NULL CHECK(json_valid(record)),PRIMARY KEY(id,revision),FOREIGN KEY(id,revision) REFERENCES incident_artifacts(id,revision));
CREATE TRIGGER immutable_hypothesis_approval BEFORE UPDATE ON hypothesis_approvals BEGIN SELECT RAISE(ABORT,'hypothesis acceptance is immutable'); END;
CREATE TRIGGER retained_hypothesis_approval BEFORE DELETE ON hypothesis_approvals BEGIN SELECT RAISE(ABORT,'hypothesis acceptance is retained'); END;
