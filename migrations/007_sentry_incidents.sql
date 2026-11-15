CREATE TABLE source_configs (revision INTEGER PRIMARY KEY, record TEXT NOT NULL, installed_by TEXT NOT NULL, installed_at TEXT NOT NULL);
CREATE TRIGGER immutable_source_config BEFORE UPDATE ON source_configs BEGIN SELECT RAISE(ABORT,'source configuration is immutable'); END;
CREATE TABLE source_deliveries (id TEXT PRIMARY KEY, revision INTEGER NOT NULL REFERENCES source_configs(revision), record TEXT NOT NULL, state TEXT NOT NULL DEFAULT 'pending', next_at TEXT NOT NULL, received_at TEXT NOT NULL);
CREATE TABLE source_receipts (raw_digest TEXT NOT NULL, revision INTEGER NOT NULL REFERENCES source_configs(revision), delivery_id TEXT NOT NULL REFERENCES source_deliveries(id), received_at TEXT NOT NULL, PRIMARY KEY(raw_digest,revision));
CREATE TABLE source_cases (source_key TEXT PRIMARY KEY, case_id TEXT NOT NULL UNIQUE REFERENCES cases(id), record TEXT NOT NULL, digest TEXT NOT NULL REFERENCES evidence(digest));
CREATE TABLE source_sync (revision INTEGER PRIMARY KEY REFERENCES source_configs(revision), window_start TEXT NOT NULL, window_end TEXT NOT NULL, cursor TEXT, covered_until TEXT, next_at TEXT NOT NULL, error TEXT);
CREATE TABLE source_cursors (revision INTEGER NOT NULL, window_end TEXT NOT NULL, cursor TEXT NOT NULL, PRIMARY KEY(revision,window_end,cursor));
CREATE TABLE incidents (id TEXT PRIMARY KEY, case_id TEXT NOT NULL UNIQUE REFERENCES cases(id), record TEXT NOT NULL, next_update_at TEXT NOT NULL, update_escalated INTEGER NOT NULL DEFAULT 0);
CREATE TRIGGER immutable_incident BEFORE UPDATE OF record ON incidents BEGIN SELECT RAISE(ABORT,'incident declaration is immutable'); END;
