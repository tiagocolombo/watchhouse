CREATE TABLE factory_limits (revision INTEGER PRIMARY KEY,record TEXT NOT NULL CHECK(json_valid(record)),digest TEXT NOT NULL,operator TEXT NOT NULL,source TEXT NOT NULL,installed_at TEXT NOT NULL);
CREATE TRIGGER factory_limits_no_update BEFORE UPDATE ON factory_limits BEGIN SELECT RAISE(ABORT,'factory limits are immutable'); END;
CREATE TRIGGER factory_limits_no_delete BEFORE DELETE ON factory_limits BEGIN SELECT RAISE(ABORT,'factory limits are retained'); END;
CREATE VIEW factory_capacity_limits AS SELECT
 json_extract(record,'$.max_queued_cases') AS queued_cases,
 json_extract(record,'$.max_queued_effects') AS queued_effects,
 json_extract(record,'$.max_pending_deliveries') AS pending_deliveries,
 json_extract(record,'$.max_evidence_bytes') AS evidence_bytes,
 json_extract(record,'$.supervisor_evidence_bytes') AS supervisor_bytes,
 json_extract(record,'$.max_native_resources') AS native_total,
 json_extract(record,'$.max_native_per_repository') AS native_repository,
 json_extract(record,'$.max_native_per_service') AS native_service
 FROM factory_limits WHERE revision=(SELECT MAX(revision) FROM factory_limits);
CREATE TABLE factory_evidence_usage (id INTEGER PRIMARY KEY CHECK(id=1),ordinary_bytes INTEGER NOT NULL CHECK(ordinary_bytes>=0),supervisor_bytes INTEGER NOT NULL CHECK(supervisor_bytes>=0));
INSERT INTO factory_evidence_usage SELECT 1,COALESCE(SUM(CASE WHEN provenance!='supervisor-checkpoint-v1' THEN length(CAST(content AS BLOB)) ELSE 0 END),0),COALESCE(SUM(CASE WHEN provenance='supervisor-checkpoint-v1' THEN length(CAST(content AS BLOB)) ELSE 0 END),0) FROM evidence;
CREATE TRIGGER evidence_capacity_insert BEFORE INSERT ON evidence WHEN NOT EXISTS(SELECT 1 FROM evidence WHERE digest=NEW.digest) BEGIN
 SELECT CASE WHEN NEW.provenance!='supervisor-checkpoint-v1' AND (SELECT ordinary_bytes FROM factory_evidence_usage)+length(CAST(NEW.content AS BLOB))>(SELECT evidence_bytes FROM factory_capacity_limits) THEN RAISE(ABORT,'factory_capacity_evidence') END;
 SELECT CASE WHEN NEW.provenance='supervisor-checkpoint-v1' AND (SELECT supervisor_bytes FROM factory_evidence_usage)+length(CAST(NEW.content AS BLOB))>(SELECT supervisor_bytes FROM factory_capacity_limits) THEN RAISE(ABORT,'factory_capacity_supervisor_evidence') END;
END;
CREATE TRIGGER evidence_capacity_update BEFORE UPDATE ON evidence BEGIN
 SELECT CASE WHEN NEW.provenance!='supervisor-checkpoint-v1' AND length(CAST(NEW.content AS BLOB))>CASE WHEN OLD.provenance!='supervisor-checkpoint-v1' THEN length(CAST(OLD.content AS BLOB)) ELSE 0 END AND (SELECT ordinary_bytes FROM factory_evidence_usage)+length(CAST(NEW.content AS BLOB))-CASE WHEN OLD.provenance!='supervisor-checkpoint-v1' THEN length(CAST(OLD.content AS BLOB)) ELSE 0 END>(SELECT evidence_bytes FROM factory_capacity_limits) THEN RAISE(ABORT,'factory_capacity_evidence') END;
 SELECT CASE WHEN NEW.provenance='supervisor-checkpoint-v1' AND length(CAST(NEW.content AS BLOB))>CASE WHEN OLD.provenance='supervisor-checkpoint-v1' THEN length(CAST(OLD.content AS BLOB)) ELSE 0 END AND (SELECT supervisor_bytes FROM factory_evidence_usage)+length(CAST(NEW.content AS BLOB))-CASE WHEN OLD.provenance='supervisor-checkpoint-v1' THEN length(CAST(OLD.content AS BLOB)) ELSE 0 END>(SELECT supervisor_bytes FROM factory_capacity_limits) THEN RAISE(ABORT,'factory_capacity_supervisor_evidence') END;
END;
CREATE TRIGGER evidence_usage_insert AFTER INSERT ON evidence BEGIN
 UPDATE factory_evidence_usage SET ordinary_bytes=ordinary_bytes+CASE WHEN NEW.provenance!='supervisor-checkpoint-v1' THEN length(CAST(NEW.content AS BLOB)) ELSE 0 END,supervisor_bytes=supervisor_bytes+CASE WHEN NEW.provenance='supervisor-checkpoint-v1' THEN length(CAST(NEW.content AS BLOB)) ELSE 0 END WHERE id=1;
END;
CREATE TRIGGER evidence_usage_update AFTER UPDATE ON evidence BEGIN
 UPDATE factory_evidence_usage SET ordinary_bytes=ordinary_bytes+CASE WHEN NEW.provenance!='supervisor-checkpoint-v1' THEN length(CAST(NEW.content AS BLOB)) ELSE 0 END-CASE WHEN OLD.provenance!='supervisor-checkpoint-v1' THEN length(CAST(OLD.content AS BLOB)) ELSE 0 END,supervisor_bytes=supervisor_bytes+CASE WHEN NEW.provenance='supervisor-checkpoint-v1' THEN length(CAST(NEW.content AS BLOB)) ELSE 0 END-CASE WHEN OLD.provenance='supervisor-checkpoint-v1' THEN length(CAST(OLD.content AS BLOB)) ELSE 0 END WHERE id=1;
END;
CREATE TRIGGER evidence_usage_delete AFTER DELETE ON evidence BEGIN
 UPDATE factory_evidence_usage SET ordinary_bytes=ordinary_bytes-CASE WHEN OLD.provenance!='supervisor-checkpoint-v1' THEN length(CAST(OLD.content AS BLOB)) ELSE 0 END,supervisor_bytes=supervisor_bytes-CASE WHEN OLD.provenance='supervisor-checkpoint-v1' THEN length(CAST(OLD.content AS BLOB)) ELSE 0 END WHERE id=1;
END;
CREATE VIEW factory_queued_effects AS
 SELECT 'model' AS kind,id FROM model_jobs WHERE state='queued' UNION ALL
 SELECT 'container',id FROM container_jobs WHERE state='queued' UNION ALL
 SELECT 'publication',id FROM publication_jobs WHERE state='ready' UNION ALL
 SELECT 'merge',id FROM merge_jobs WHERE state='ready' UNION ALL
 SELECT 'operation',id FROM operation_jobs WHERE state IN ('proposed','approved');
CREATE TRIGGER cases_capacity BEFORE INSERT ON cases WHEN NOT EXISTS(SELECT 1 FROM cases WHERE id=NEW.id) AND NEW.status='queued' AND (SELECT COUNT(*) FROM cases WHERE status='queued')>=(SELECT queued_cases FROM factory_capacity_limits) BEGIN SELECT RAISE(ABORT,'factory_capacity_cases'); END;
CREATE TRIGGER source_capacity BEFORE INSERT ON source_deliveries WHEN NOT EXISTS(SELECT 1 FROM source_deliveries WHERE id=NEW.id) AND NEW.state='pending' AND (SELECT COUNT(*) FROM source_deliveries WHERE state='pending')>=(SELECT pending_deliveries FROM factory_capacity_limits) BEGIN SELECT RAISE(ABORT,'factory_capacity_deliveries'); END;
CREATE TRIGGER model_jobs_capacity BEFORE INSERT ON model_jobs WHEN NOT EXISTS(SELECT 1 FROM model_jobs WHERE id=NEW.id) AND NEW.state IN ('queued') AND (SELECT COUNT(*) FROM factory_queued_effects)>=(SELECT queued_effects FROM factory_capacity_limits) BEGIN SELECT RAISE(ABORT,'factory_capacity_effects'); END;
CREATE TRIGGER container_jobs_capacity BEFORE INSERT ON container_jobs WHEN NOT EXISTS(SELECT 1 FROM container_jobs WHERE id=NEW.id) AND NEW.state IN ('queued') AND (SELECT COUNT(*) FROM factory_queued_effects)>=(SELECT queued_effects FROM factory_capacity_limits) BEGIN SELECT RAISE(ABORT,'factory_capacity_effects'); END;
CREATE TRIGGER publication_jobs_capacity BEFORE INSERT ON publication_jobs WHEN NOT EXISTS(SELECT 1 FROM publication_jobs WHERE id=NEW.id) AND NEW.state IN ('ready') AND (SELECT COUNT(*) FROM factory_queued_effects)>=(SELECT queued_effects FROM factory_capacity_limits) BEGIN SELECT RAISE(ABORT,'factory_capacity_effects'); END;
CREATE TRIGGER merge_jobs_capacity BEFORE INSERT ON merge_jobs WHEN NOT EXISTS(SELECT 1 FROM merge_jobs WHERE id=NEW.id) AND NEW.state IN ('ready') AND (SELECT COUNT(*) FROM factory_queued_effects)>=(SELECT queued_effects FROM factory_capacity_limits) BEGIN SELECT RAISE(ABORT,'factory_capacity_effects'); END;
CREATE TRIGGER operation_jobs_capacity BEFORE INSERT ON operation_jobs WHEN NOT EXISTS(SELECT 1 FROM operation_jobs WHERE id=NEW.id) AND NEW.state IN ('proposed','approved') AND (SELECT COUNT(*) FROM factory_queued_effects)>=(SELECT queued_effects FROM factory_capacity_limits) BEGIN SELECT RAISE(ABORT,'factory_capacity_effects'); END;
CREATE VIEW factory_native_resources AS
 SELECT 'model' AS kind,id,case_id FROM model_jobs WHERE state IN ('creating','created','starting','running','polling','finishing','recovering','cancelling') OR (state='unknown' AND (json_extract(record,'$.create_dispatched')=1 OR json_extract(record,'$.quarantined')=1)) UNION ALL
 SELECT 'container',id,case_id FROM container_jobs WHERE state IN ('creating','created','starting','running','cancelling') OR (state='unknown' AND (json_extract(record,'$.create_dispatched')=1 OR json_extract(record,'$.quarantined')=1));
CREATE VIEW factory_case_scopes AS SELECT c.id AS case_id,
 COALESCE((SELECT json_extract(s.record,'$.repository') FROM source_cases s WHERE s.case_id=c.id),(SELECT json_extract(r.record,'$.repository') FROM repair_requests r WHERE r.case_id=c.id ORDER BY r.rowid DESC LIMIT 1)) AS repository,
 (SELECT json_extract(s.record,'$.service') FROM source_cases s WHERE s.case_id=c.id) AS service FROM cases c;
CREATE TABLE factory_resource_scopes (kind TEXT NOT NULL,job_id TEXT NOT NULL,repository TEXT,service TEXT,recorded_at TEXT NOT NULL,PRIMARY KEY(kind,job_id));
INSERT INTO factory_resource_scopes SELECT r.kind,r.id,s.repository,s.service,'migration-031' FROM factory_native_resources r JOIN factory_case_scopes s ON s.case_id=r.case_id;
CREATE TRIGGER factory_resource_scopes_no_update BEFORE UPDATE ON factory_resource_scopes BEGIN SELECT RAISE(ABORT,'reserved resource scope is immutable'); END;
CREATE TRIGGER factory_resource_scopes_no_delete BEFORE DELETE ON factory_resource_scopes BEGIN SELECT RAISE(ABORT,'reserved resource scope is retained'); END;
CREATE VIEW factory_scoped_resources AS SELECT r.kind,r.id,r.case_id,CASE WHEN b.job_id IS NOT NULL THEN b.repository ELSE s.repository END AS repository,CASE WHEN b.job_id IS NOT NULL THEN b.service ELSE s.service END AS service FROM factory_native_resources r JOIN factory_case_scopes s ON s.case_id=r.case_id LEFT JOIN factory_resource_scopes b ON b.kind=r.kind AND b.job_id=r.id;
CREATE VIEW factory_case_capacity AS SELECT s.case_id,
 (SELECT COUNT(*) FROM factory_native_resources)<l.native_total AND
 (s.repository IS NULL OR (SELECT COUNT(*) FROM factory_scoped_resources r WHERE r.repository=s.repository)<l.native_repository) AND
 (s.service IS NULL OR (SELECT COUNT(*) FROM factory_scoped_resources r WHERE r.service=s.service)<l.native_service) AS available
 FROM factory_case_scopes s CROSS JOIN factory_capacity_limits l;
