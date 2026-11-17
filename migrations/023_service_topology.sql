CREATE TABLE service_topologies(revision INTEGER PRIMARY KEY,digest TEXT NOT NULL,record TEXT NOT NULL CHECK(json_valid(record)),operator TEXT NOT NULL,source TEXT NOT NULL,installed_at TEXT NOT NULL);
CREATE TRIGGER immutable_topology BEFORE UPDATE ON service_topologies BEGIN SELECT RAISE(ABORT,'service topology is immutable'); END;
CREATE TRIGGER retained_topology BEFORE DELETE ON service_topologies BEGIN SELECT RAISE(ABORT,'service topology is retained'); END;
