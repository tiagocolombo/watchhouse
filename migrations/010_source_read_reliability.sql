CREATE TABLE source_read_state (source_key TEXT PRIMARY KEY, generation INTEGER NOT NULL, configuration_revision INTEGER NOT NULL REFERENCES source_configs(revision), started_at TEXT NOT NULL, next_at TEXT NOT NULL, last_success_at TEXT, last_success_generation INTEGER, error TEXT);
CREATE INDEX source_read_due ON source_read_state(next_at);
