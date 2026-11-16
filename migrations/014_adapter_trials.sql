CREATE TABLE adapter_trials (id TEXT PRIMARY KEY, case_id TEXT NOT NULL REFERENCES cases(id), repair_id TEXT NOT NULL REFERENCES repair_requests(id), state TEXT NOT NULL, deadline TEXT NOT NULL, record TEXT NOT NULL CHECK(json_valid(record)));
CREATE INDEX adapter_trials_due ON adapter_trials(state,deadline);
CREATE TABLE adapter_trial_events (trial_id TEXT NOT NULL REFERENCES adapter_trials(id), sequence INTEGER NOT NULL, record TEXT NOT NULL CHECK(json_valid(record)), PRIMARY KEY(trial_id,sequence));
-- Fake provider journal: simulates an independently observed effect, never a real process.
CREATE TABLE fake_adapter_sessions (id TEXT PRIMARY KEY REFERENCES adapter_trials(id), state TEXT NOT NULL, result TEXT CHECK(result IS NULL OR json_valid(result)));
