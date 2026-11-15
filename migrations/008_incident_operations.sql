CREATE TABLE incident_operations (case_id TEXT PRIMARY KEY REFERENCES cases(id), record TEXT NOT NULL CHECK(json_valid(record)));
CREATE TABLE incident_role_offers (id TEXT PRIMARY KEY, case_id TEXT NOT NULL REFERENCES cases(id), record TEXT NOT NULL CHECK(json_valid(record)), state TEXT NOT NULL, acknowledge_by TEXT NOT NULL);
CREATE UNIQUE INDEX one_pending_incident_role ON incident_role_offers(case_id,json_extract(record,'$.role')) WHERE state='pending';
