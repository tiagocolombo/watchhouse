-- Case command lineage is metadata, not renewed execution authority.
CREATE TABLE case_command_refs(idempotency_key TEXT NOT NULL REFERENCES commands(idempotency_key),case_id TEXT NOT NULL REFERENCES cases(id),PRIMARY KEY(idempotency_key,case_id));
INSERT OR IGNORE INTO case_command_refs SELECT c.idempotency_key,e.case_id FROM commands c JOIN events e ON json_extract(e.envelope,'$.causation_id')=c.idempotency_key;
INSERT OR IGNORE INTO case_command_refs SELECT k.idempotency_key,c.id FROM commands k JOIN cases c ON json_valid(k.response) AND (json_extract(k.response,'$.id')=c.id OR json_extract(k.response,'$.case_id')=c.id OR json_extract(k.response,'$.case.id')=c.id);
CREATE TRIGGER case_command_lineage AFTER INSERT ON commands BEGIN
 INSERT OR IGNORE INTO case_command_refs SELECT NEW.idempotency_key,e.case_id FROM events e WHERE json_extract(e.envelope,'$.causation_id')=NEW.idempotency_key;
 INSERT OR IGNORE INTO case_command_refs SELECT NEW.idempotency_key,c.id FROM cases c WHERE json_valid(NEW.response) AND (json_extract(NEW.response,'$.id')=c.id OR json_extract(NEW.response,'$.case_id')=c.id OR json_extract(NEW.response,'$.case.id')=c.id);
END;
CREATE TRIGGER case_command_ref_no_update BEFORE UPDATE ON case_command_refs BEGIN SELECT RAISE(ABORT,'command lineage immutable'); END;
CREATE TRIGGER case_command_ref_no_delete BEFORE DELETE ON case_command_refs BEGIN SELECT RAISE(ABORT,'command lineage retained'); END;
CREATE TABLE case_exports(id TEXT PRIMARY KEY,case_id TEXT NOT NULL REFERENCES cases(id),state TEXT NOT NULL,record TEXT NOT NULL CHECK(json_valid(record)));
CREATE INDEX case_exports_by_case ON case_exports(case_id,state);
CREATE TABLE case_export_locations(export_id TEXT PRIMARY KEY REFERENCES case_exports(id),root TEXT NOT NULL,recorded_at TEXT NOT NULL);
CREATE TRIGGER export_location_no_update BEFORE UPDATE ON case_export_locations BEGIN SELECT RAISE(ABORT,'export location immutable'); END;
CREATE TRIGGER export_location_no_delete BEFORE DELETE ON case_export_locations BEGIN SELECT RAISE(ABORT,'export location retained'); END;
