-- Do not rewrite migration 001 or historical v1 events/command receipts.
CREATE TABLE case_evidence (
    case_id TEXT NOT NULL REFERENCES cases(id),
    digest TEXT NOT NULL REFERENCES evidence(digest),
    PRIMARY KEY(case_id, digest)
);
INSERT INTO case_evidence(case_id,digest)
SELECT DISTINCT a.case_id,r.evidence_digest FROM runs r
JOIN assignments a ON a.id=r.assignment_id WHERE r.evidence_digest IS NOT NULL;
CREATE TABLE dispositions (
    id TEXT PRIMARY KEY,
    case_id TEXT NOT NULL REFERENCES cases(id),
    recorded_at TEXT NOT NULL,
    due_at TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('active','reopened','superseded')),
    occurrence_count INTEGER NOT NULL DEFAULT 0 CHECK (occurrence_count >= 0),
    record TEXT NOT NULL CHECK (json_valid(record))
);
CREATE UNIQUE INDEX one_current_disposition ON dispositions(case_id) WHERE state != 'superseded';
CREATE TABLE obligations (
    id TEXT PRIMARY KEY,
    disposition_id TEXT NOT NULL UNIQUE REFERENCES dispositions(id),
    owner TEXT NOT NULL,
    task TEXT NOT NULL,
    due_at TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('pending','due','superseded')),
    triggered_at TEXT
);
CREATE INDEX obligations_due ON obligations(state,due_at);
CREATE TABLE reconsideration_evidence (
    disposition_id TEXT NOT NULL REFERENCES dispositions(id),
    digest TEXT NOT NULL REFERENCES evidence(digest),
    PRIMARY KEY(disposition_id,digest)
);
-- Decision contents and original deadlines are immutable. Lifecycle is audited by events.
CREATE TRIGGER disposition_record_immutable BEFORE UPDATE OF record,case_id,recorded_at,due_at ON dispositions
BEGIN SELECT RAISE(ABORT,'disposition records are immutable'); END;
