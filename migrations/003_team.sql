CREATE TABLE team_threads (id TEXT PRIMARY KEY, case_id TEXT NOT NULL UNIQUE REFERENCES cases(id));
INSERT INTO team_threads SELECT 'thread_'||id,id FROM cases;
CREATE TABLE assignment_control (
    assignment_id TEXT PRIMARY KEY REFERENCES assignments(id),
    executor TEXT NOT NULL,
    state TEXT NOT NULL CHECK(state IN ('queued','running','blocked','handoff_pending','completed')),
    overall_deadline TEXT,
    max_attempts INTEGER NOT NULL
);
INSERT INTO assignment_control
SELECT a.id,'fake-investigator',a.state,
CASE WHEN a.state='running' THEN strftime('%Y-%m-%dT%H:%M:%fZ',r.started_at,'+300 seconds') END,
MAX(3,j.attempt+1) FROM assignments a JOIN jobs j ON j.assignment_id=a.id
LEFT JOIN runs r ON r.assignment_id=a.id AND r.attempt=j.attempt;
CREATE TABLE run_control (
    run_id TEXT PRIMARY KEY REFERENCES runs(id),
    executor TEXT NOT NULL, fence TEXT NOT NULL,
    heartbeat_at TEXT NOT NULL, progress_at TEXT NOT NULL, progress_deadline TEXT NOT NULL,
    checkpoint_digest TEXT REFERENCES evidence(digest),
    briefing TEXT NOT NULL CHECK(json_valid(briefing))
);
-- Active v2 fake runs retain their current proof and gain deterministic supervision.
INSERT INTO run_control SELECT r.id,'fake-investigator',j.fence,r.started_at,r.started_at,
strftime('%Y-%m-%dT%H:%M:%fZ',r.started_at,'+60 seconds'),NULL,
json_object('schema_version',1,'objective',a.objective,'role','investigator','checkpoint_digest',NULL,'evidence_refs',json('[]'),'message_refs',json('[]'),'previous_run_id',NULL,'overall_deadline',c.overall_deadline,'remaining_attempts',MAX(0,c.max_attempts-r.attempt))
FROM runs r JOIN jobs j ON j.assignment_id=r.assignment_id AND j.attempt=r.attempt
JOIN assignments a ON a.id=r.assignment_id JOIN assignment_control c ON c.assignment_id=a.id
WHERE r.state='running';
CREATE TABLE team_messages (
    id TEXT PRIMARY KEY, thread_id TEXT NOT NULL REFERENCES team_threads(id),
    record TEXT NOT NULL CHECK(json_valid(record)), ack_due_at TEXT,
    escalated INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE message_ack (
    message_id TEXT NOT NULL REFERENCES team_messages(id), recipient TEXT NOT NULL,
    acknowledged_at TEXT, PRIMARY KEY(message_id,recipient)
);
CREATE TABLE blocks (
    id TEXT PRIMARY KEY, assignment_id TEXT NOT NULL REFERENCES assignments(id),
    run_id TEXT NOT NULL UNIQUE REFERENCES runs(id),
    record TEXT NOT NULL CHECK(json_valid(record)),
    state TEXT NOT NULL CHECK(state IN ('open','handoff_pending','transferred','escalated')),
    response_due_at TEXT NOT NULL
);
CREATE TABLE handoffs (
    id TEXT PRIMARY KEY, block_id TEXT NOT NULL REFERENCES blocks(id),
    record TEXT NOT NULL CHECK(json_valid(record)),
    state TEXT NOT NULL CHECK(state IN ('proposed','accepted','expired')),
    acknowledge_by TEXT NOT NULL
);
CREATE UNIQUE INDEX one_proposed_handoff ON handoffs(block_id) WHERE state='proposed';
CREATE TABLE escalations (
    id TEXT PRIMARY KEY, case_id TEXT NOT NULL REFERENCES cases(id),
    source_type TEXT NOT NULL, source_id TEXT NOT NULL,
    record TEXT NOT NULL CHECK(json_valid(record)),
    state TEXT NOT NULL CHECK(state IN ('open','acknowledged')),
    acknowledged_at TEXT, UNIQUE(source_type,source_id)
);
CREATE TABLE progress_evidence (
    assignment_id TEXT NOT NULL REFERENCES assignments(id),
    digest TEXT NOT NULL REFERENCES evidence(digest), PRIMARY KEY(assignment_id,digest)
);
CREATE TABLE late_results (
    run_id TEXT NOT NULL REFERENCES runs(id), digest TEXT NOT NULL REFERENCES evidence(digest),
    recorded_at TEXT NOT NULL, PRIMARY KEY(run_id,digest)
);
CREATE INDEX team_due_messages ON team_messages(escalated,ack_due_at);
CREATE INDEX team_due_blocks ON blocks(state,response_due_at);
CREATE INDEX team_due_handoffs ON handoffs(state,acknowledge_by);
CREATE TRIGGER team_message_immutable BEFORE UPDATE OF record ON team_messages BEGIN SELECT RAISE(ABORT,'message content is immutable'); END;
CREATE TRIGGER block_record_immutable BEFORE UPDATE OF record ON blocks BEGIN SELECT RAISE(ABORT,'block content is immutable'); END;
CREATE TRIGGER handoff_record_immutable BEFORE UPDATE OF record ON handoffs BEGIN SELECT RAISE(ABORT,'handoff content is immutable'); END;
CREATE TRIGGER run_briefing_immutable BEFORE UPDATE OF executor,fence,briefing ON run_control BEGIN SELECT RAISE(ABORT,'run authority and briefing are immutable'); END;
