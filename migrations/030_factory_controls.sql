CREATE TABLE factory_state (id INTEGER PRIMARY KEY CHECK(id=1),version INTEGER NOT NULL DEFAULT 0,last_tick TEXT,high_at TEXT);
INSERT INTO factory_state(id) VALUES(1);
CREATE TABLE factory_pauses (
 id TEXT PRIMARY KEY, scope TEXT NOT NULL, target TEXT, effect TEXT NOT NULL,
 owner TEXT NOT NULL, reason TEXT NOT NULL, cancel_running INTEGER NOT NULL,
 state TEXT NOT NULL, created_at TEXT NOT NULL, resumed_at TEXT
);
CREATE TABLE factory_events (sequence INTEGER PRIMARY KEY AUTOINCREMENT,record TEXT NOT NULL CHECK(json_valid(record)));
CREATE TRIGGER factory_events_no_update BEFORE UPDATE ON factory_events BEGIN SELECT RAISE(ABORT,'factory audit is append-only'); END;
CREATE TRIGGER factory_events_no_delete BEFORE DELETE ON factory_events BEGIN SELECT RAISE(ABORT,'factory audit is append-only'); END;
CREATE VIEW factory_holds AS
 SELECT c.id AS case_id,p.id AS pause_id,p.effect,p.cancel_running
 FROM cases c JOIN factory_pauses p ON p.state='active' AND (
 p.scope='global' OR (p.scope='case' AND p.target=c.id) OR
 (p.scope='service' AND EXISTS(SELECT 1 FROM source_cases s WHERE s.case_id=c.id AND json_extract(s.record,'$.service')=p.target)) OR
 (p.scope='repository' AND (EXISTS(SELECT 1 FROM source_cases s WHERE s.case_id=c.id AND json_extract(s.record,'$.repository')=p.target) OR EXISTS(SELECT 1 FROM repair_requests r WHERE r.case_id=c.id AND json_extract(r.record,'$.repository')=p.target))) OR
 (p.scope='incident' AND EXISTS(SELECT 1 FROM incidents i WHERE i.case_id=c.id AND i.id=p.target))
 );
