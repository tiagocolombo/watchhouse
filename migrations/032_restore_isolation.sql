CREATE TABLE restore_runs (id TEXT PRIMARY KEY,record TEXT NOT NULL CHECK(json_valid(record)));
CREATE TRIGGER restore_run_no_update BEFORE UPDATE ON restore_runs BEGIN SELECT RAISE(ABORT,'restore audit retained'); END;
CREATE TRIGGER restore_run_no_delete BEFORE DELETE ON restore_runs BEGIN SELECT RAISE(ABORT,'restore audit retained'); END;
CREATE TABLE restore_fences (restore_id TEXT NOT NULL REFERENCES restore_runs(id),kind TEXT NOT NULL,identity TEXT NOT NULL,record TEXT NOT NULL CHECK(json_valid(record)),PRIMARY KEY(restore_id,kind,identity));
CREATE TRIGGER restore_fence_no_update BEFORE UPDATE ON restore_fences BEGIN SELECT RAISE(ABORT,'restore authority cannot be inherited'); END;
CREATE TRIGGER restore_fence_no_delete BEFORE DELETE ON restore_fences BEGIN SELECT RAISE(ABORT,'restore authority cannot be inherited'); END;
CREATE TABLE restore_artifacts (restore_id TEXT NOT NULL REFERENCES restore_runs(id),kind TEXT NOT NULL,identity TEXT NOT NULL,path TEXT NOT NULL,manifest_digest TEXT NOT NULL,PRIMARY KEY(restore_id,kind,identity));
CREATE TRIGGER restore_artifact_no_update BEFORE UPDATE ON restore_artifacts BEGIN SELECT RAISE(ABORT,'restore materialization immutable'); END;
CREATE TRIGGER restore_artifact_no_delete BEFORE DELETE ON restore_artifacts BEGIN SELECT RAISE(ABORT,'restore materialization immutable'); END;
CREATE TABLE notification_pacing (id INTEGER PRIMARY KEY CHECK(id=1),interval_seconds INTEGER NOT NULL CHECK(interval_seconds BETWEEN 0 AND 3600),next_at TEXT NOT NULL);
INSERT INTO notification_pacing VALUES(1,0,'');
DROP VIEW source_context_validity;
CREATE VIEW source_context_validity AS
SELECT c.case_id, CASE
 WHEN c.source_key LIKE 'sentry:%' THEN EXISTS(SELECT 1 FROM source_configs old JOIN source_projects p ON p.organization=json_extract(old.record,'$.organization') AND p.project_id=json_extract(old.record,'$.project_id') WHERE old.revision=json_extract(c.record,'$.configuration_revision') AND p.revision=old.revision AND NOT EXISTS(SELECT 1 FROM restore_fences f WHERE f.kind='source' AND f.identity=CAST(old.revision AS TEXT)))
 WHEN c.source_key LIKE 'datadog:%' THEN EXISTS(SELECT 1 FROM datadog_monitors d WHERE c.source_key='datadog:'||d.profile||'/'||d.monitor_id AND d.revision=json_extract(c.record,'$.configuration_revision') AND NOT EXISTS(SELECT 1 FROM restore_fences f WHERE f.kind='datadog' AND f.identity=CAST(d.revision AS TEXT)))
 ELSE 1 END AS current FROM source_cases c;
CREATE TABLE restore_read_permits (id TEXT PRIMARY KEY,restore_id TEXT NOT NULL REFERENCES restore_runs(id),kind TEXT NOT NULL,identity TEXT NOT NULL,operator TEXT NOT NULL,expires_at TEXT NOT NULL);
CREATE TRIGGER restore_read_permit_no_update BEFORE UPDATE ON restore_read_permits BEGIN SELECT RAISE(ABORT,'restore read permits immutable'); END;
CREATE TRIGGER restore_read_permit_no_delete BEFORE DELETE ON restore_read_permits BEGIN SELECT RAISE(ABORT,'restore read permits retained'); END;
