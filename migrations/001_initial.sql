CREATE TABLE cases (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    description TEXT NOT NULL,
    owner TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('queued','running','awaiting_review')),
    version INTEGER NOT NULL CHECK (version >= 0),
    created_at TEXT NOT NULL
);
CREATE TABLE assignments (
    id TEXT PRIMARY KEY,
    case_id TEXT NOT NULL UNIQUE REFERENCES cases(id),
    role TEXT NOT NULL CHECK (role = 'investigator'),
    owner TEXT NOT NULL,
    objective TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('queued','running','completed')),
    schema_version INTEGER NOT NULL CHECK (schema_version = 1)
);
CREATE TABLE jobs (
    id TEXT PRIMARY KEY,
    assignment_id TEXT NOT NULL UNIQUE REFERENCES assignments(id),
    state TEXT NOT NULL CHECK (state IN ('queued','running','completed')),
    attempt INTEGER NOT NULL DEFAULT 0,
    lease_until TEXT,
    fence TEXT
);
CREATE TABLE events (
    cursor INTEGER PRIMARY KEY AUTOINCREMENT,
    event_id TEXT NOT NULL UNIQUE,
    case_id TEXT NOT NULL REFERENCES cases(id),
    aggregate_version INTEGER NOT NULL,
    envelope TEXT NOT NULL CHECK (json_valid(envelope)),
    UNIQUE(case_id, aggregate_version)
);
CREATE TABLE evidence (
    digest TEXT PRIMARY KEY,
    media_type TEXT NOT NULL,
    content TEXT NOT NULL,
    provenance TEXT NOT NULL,
    recorded_at TEXT NOT NULL
);
CREATE TABLE runs (
    id TEXT PRIMARY KEY,
    assignment_id TEXT NOT NULL REFERENCES assignments(id),
    attempt INTEGER NOT NULL,
    adapter TEXT NOT NULL,
    started_at TEXT NOT NULL,
    finished_at TEXT,
    state TEXT NOT NULL CHECK (state IN ('running','completed','expired')),
    result TEXT CHECK (result IS NULL OR json_valid(result)),
    evidence_digest TEXT REFERENCES evidence(digest),
    UNIQUE(assignment_id, attempt)
);
CREATE TABLE commands (
    idempotency_key TEXT PRIMARY KEY,
    fingerprint TEXT NOT NULL,
    response TEXT NOT NULL CHECK (json_valid(response))
);
CREATE INDEX events_case_cursor ON events(case_id, cursor);
CREATE INDEX jobs_runnable ON jobs(state, lease_until);
CREATE TRIGGER events_no_update BEFORE UPDATE ON events BEGIN SELECT RAISE(ABORT, 'events are append-only'); END;
CREATE TRIGGER events_no_delete BEFORE DELETE ON events BEGIN SELECT RAISE(ABORT, 'events are append-only'); END;
