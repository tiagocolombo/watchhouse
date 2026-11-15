CREATE TABLE repair_sources (revision INTEGER PRIMARY KEY, digest TEXT NOT NULL, record TEXT NOT NULL CHECK(json_valid(record)));
CREATE TRIGGER immutable_repair_source_update BEFORE UPDATE ON repair_sources BEGIN SELECT RAISE(ABORT,'repair source revisions are immutable'); END;
CREATE TRIGGER immutable_repair_source_delete BEFORE DELETE ON repair_sources BEGIN SELECT RAISE(ABORT,'repair source revisions are immutable'); END;
CREATE TABLE repair_requests (id TEXT PRIMARY KEY, case_id TEXT NOT NULL REFERENCES cases(id), record TEXT NOT NULL CHECK(json_valid(record)));
CREATE INDEX repair_request_case ON repair_requests(case_id);
CREATE TRIGGER immutable_repair_request_update BEFORE UPDATE ON repair_requests BEGIN SELECT RAISE(ABORT,'repair requests are immutable'); END;
CREATE TRIGGER immutable_repair_request_delete BEFORE DELETE ON repair_requests BEGIN SELECT RAISE(ABORT,'repair requests are immutable'); END;
