CREATE TABLE datadog_configs (revision INTEGER PRIMARY KEY, profile TEXT NOT NULL, digest TEXT NOT NULL, record TEXT NOT NULL, operator TEXT NOT NULL, source TEXT NOT NULL, recorded_at TEXT NOT NULL);
CREATE TRIGGER datadog_config_no_update BEFORE UPDATE ON datadog_configs BEGIN SELECT RAISE(ABORT,'Datadog profiles immutable'); END;
CREATE TRIGGER datadog_config_no_delete BEFORE DELETE ON datadog_configs BEGIN SELECT RAISE(ABORT,'Datadog profiles immutable'); END;
CREATE TABLE datadog_monitors (profile TEXT NOT NULL, monitor_id INTEGER NOT NULL, revision INTEGER NOT NULL REFERENCES datadog_configs(revision), generation INTEGER NOT NULL, next_at TEXT NOT NULL, PRIMARY KEY(profile,monitor_id));
CREATE INDEX datadog_due ON datadog_monitors(next_at);
CREATE VIEW source_context_validity AS
SELECT c.case_id, CASE
 WHEN c.source_key LIKE 'sentry:%' THEN EXISTS(SELECT 1 FROM source_configs old JOIN source_projects p ON p.organization=json_extract(old.record,'$.organization') AND p.project_id=json_extract(old.record,'$.project_id') WHERE old.revision=json_extract(c.record,'$.configuration_revision') AND p.revision=old.revision)
 WHEN c.source_key LIKE 'datadog:%' THEN EXISTS(SELECT 1 FROM datadog_monitors d WHERE c.source_key='datadog:'||d.profile||'/'||d.monitor_id AND d.revision=json_extract(c.record,'$.configuration_revision'))
 ELSE 1 END AS current FROM source_cases c;
CREATE TABLE datadog_reads (source_key TEXT PRIMARY KEY, generation INTEGER NOT NULL, configuration_revision INTEGER NOT NULL REFERENCES datadog_configs(revision), started_at TEXT NOT NULL, next_at TEXT NOT NULL, last_success_at TEXT, last_success_generation INTEGER, error TEXT);
CREATE INDEX datadog_read_due ON datadog_reads(next_at);
CREATE VIEW source_reads AS SELECT * FROM source_read_state UNION ALL SELECT * FROM datadog_reads;
