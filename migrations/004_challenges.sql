CREATE TABLE challenges (
 id TEXT PRIMARY KEY, case_id TEXT NOT NULL REFERENCES cases(id),
 state TEXT NOT NULL, acknowledge_by TEXT NOT NULL, respond_by TEXT NOT NULL,
 record TEXT NOT NULL CHECK(json_valid(record))
);
CREATE TABLE case_gates (
 case_id TEXT PRIMARY KEY REFERENCES cases(id), validated_version INTEGER NOT NULL,
 verifier TEXT NOT NULL, evidence_refs TEXT NOT NULL, closed_version INTEGER
);
CREATE INDEX challenge_deadlines ON challenges(state,acknowledge_by,respond_by);
