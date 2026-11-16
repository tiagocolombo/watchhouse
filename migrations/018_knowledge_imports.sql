CREATE TABLE knowledge_imports(id TEXT PRIMARY KEY,case_id TEXT NOT NULL REFERENCES cases(id),source_namespace TEXT NOT NULL,normalized_path TEXT NOT NULL,content_digest TEXT NOT NULL,record_digest TEXT NOT NULL,record TEXT NOT NULL CHECK(json_valid(record)),UNIQUE(case_id,source_namespace,normalized_path,content_digest));
CREATE INDEX knowledge_import_case ON knowledge_imports(case_id);
CREATE TRIGGER knowledge_import_immutable BEFORE UPDATE ON knowledge_imports BEGIN SELECT RAISE(ABORT,'imported provenance is immutable'); END;
CREATE TRIGGER knowledge_import_retained BEFORE DELETE ON knowledge_imports BEGIN SELECT RAISE(ABORT,'imported provenance is retained'); END;
