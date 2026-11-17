CREATE TABLE impact_assessments(id TEXT PRIMARY KEY,case_id TEXT NOT NULL REFERENCES cases(id),revision INTEGER NOT NULL,review_at TEXT NOT NULL,due INTEGER NOT NULL DEFAULT 0,record TEXT NOT NULL CHECK(json_valid(record)),UNIQUE(case_id,revision));
CREATE INDEX impact_review ON impact_assessments(due,review_at);
CREATE TRIGGER immutable_impact_record BEFORE UPDATE OF record,case_id,revision,review_at ON impact_assessments BEGIN SELECT RAISE(ABORT,'impact revisions are immutable'); END;
CREATE TRIGGER retained_impact BEFORE DELETE ON impact_assessments BEGIN SELECT RAISE(ABORT,'impact revisions are retained'); END;
