CREATE TABLE notification_configs (revision INTEGER PRIMARY KEY, record TEXT NOT NULL, installed_by TEXT NOT NULL, installed_at TEXT NOT NULL);
CREATE TRIGGER immutable_notification_config BEFORE UPDATE ON notification_configs BEGIN SELECT RAISE(ABORT,'notification configuration is immutable'); END;
CREATE TABLE escalation_routes (
 escalation_id TEXT PRIMARY KEY REFERENCES escalations(id), config TEXT NOT NULL,
 level INTEGER NOT NULL, acknowledge_by TEXT NOT NULL, response_by TEXT NOT NULL,
 exhausted INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE notification_deliveries (
 id TEXT PRIMARY KEY, escalation_id TEXT NOT NULL REFERENCES escalations(id),
 level INTEGER NOT NULL, recipient TEXT NOT NULL, state TEXT NOT NULL,
 attempts INTEGER NOT NULL DEFAULT 0, next_at TEXT NOT NULL,
 lease_until TEXT, fence TEXT, detail TEXT NOT NULL, UNIQUE(escalation_id,level)
);
CREATE TABLE escalation_resolutions (escalation_id TEXT PRIMARY KEY REFERENCES escalations(id), record TEXT NOT NULL);
