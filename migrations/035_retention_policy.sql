CREATE TABLE retention_policies(revision INTEGER PRIMARY KEY,record TEXT NOT NULL CHECK(json_valid(record)),digest TEXT NOT NULL,operator TEXT NOT NULL,source TEXT NOT NULL,installed_at TEXT NOT NULL);
CREATE TRIGGER retention_policy_no_update BEFORE UPDATE ON retention_policies BEGIN SELECT RAISE(ABORT,'retention policy immutable'); END;
CREATE TRIGGER retention_policy_no_delete BEFORE DELETE ON retention_policies BEGIN SELECT RAISE(ABORT,'retention policy retained'); END;
CREATE TABLE retention_schedule(id INTEGER PRIMARY KEY CHECK(id=1),next_at TEXT NOT NULL,last_at TEXT,last_digest TEXT);
INSERT INTO retention_schedule VALUES(1,'',NULL,NULL);
CREATE TABLE retention_failures(digest TEXT PRIMARY KEY REFERENCES evidence(digest),policy_revision INTEGER NOT NULL,failed_at TEXT NOT NULL,next_at TEXT NOT NULL,attempts INTEGER NOT NULL,code TEXT NOT NULL);
CREATE TABLE retention_pending(digest TEXT PRIMARY KEY REFERENCES evidence(digest) ON DELETE CASCADE,class TEXT NOT NULL,recorded_at TEXT NOT NULL);
INSERT INTO retention_pending SELECT e.digest,CASE WHEN e.provenance IN ('sentry-api-v1','sentry-webhook-v1','sentry-v1:unaccepted-source-read','datadog-monitor-api-v1') THEN 'normalized_source'
WHEN e.provenance IN ('native-container-v1','native-text-client-v1','text-client-fixture-v1','container-fixture-v1') THEN 'execution_evidence'
WHEN e.provenance='operator-reported-v1' THEN 'reported_evidence'
WHEN e.provenance='supervisor-checkpoint-v1' THEN 'supervisor_checkpoint'
WHEN e.provenance LIKE 'fake-%' THEN 'simulation'
ELSE 'other' END,e.recorded_at FROM evidence e WHERE NOT EXISTS(SELECT 1 FROM evidence_redactions r WHERE r.digest=e.digest);
CREATE INDEX retention_pending_class_age ON retention_pending(class,recorded_at,digest);
CREATE TRIGGER retention_pending_insert AFTER INSERT ON evidence BEGIN INSERT INTO retention_pending SELECT NEW.digest,CASE WHEN NEW.provenance IN ('sentry-api-v1','sentry-webhook-v1','sentry-v1:unaccepted-source-read','datadog-monitor-api-v1') THEN 'normalized_source'
WHEN NEW.provenance IN ('native-container-v1','native-text-client-v1','text-client-fixture-v1','container-fixture-v1') THEN 'execution_evidence'
WHEN NEW.provenance='operator-reported-v1' THEN 'reported_evidence'
WHEN NEW.provenance='supervisor-checkpoint-v1' THEN 'supervisor_checkpoint'
WHEN NEW.provenance LIKE 'fake-%' THEN 'simulation'
ELSE 'other' END,NEW.recorded_at WHERE NOT EXISTS(SELECT 1 FROM evidence_redactions WHERE digest=NEW.digest); END;
CREATE TRIGGER retention_pending_metadata AFTER UPDATE OF provenance,recorded_at ON evidence BEGIN UPDATE retention_pending SET class=CASE WHEN NEW.provenance IN ('sentry-api-v1','sentry-webhook-v1','sentry-v1:unaccepted-source-read','datadog-monitor-api-v1') THEN 'normalized_source'
WHEN NEW.provenance IN ('native-container-v1','native-text-client-v1','text-client-fixture-v1','container-fixture-v1') THEN 'execution_evidence'
WHEN NEW.provenance='operator-reported-v1' THEN 'reported_evidence'
WHEN NEW.provenance='supervisor-checkpoint-v1' THEN 'supervisor_checkpoint'
WHEN NEW.provenance LIKE 'fake-%' THEN 'simulation'
ELSE 'other' END,recorded_at=NEW.recorded_at WHERE digest=NEW.digest; END;
CREATE TRIGGER retention_pending_remove AFTER INSERT ON evidence_redactions BEGIN DELETE FROM retention_pending WHERE digest=NEW.digest; END;
