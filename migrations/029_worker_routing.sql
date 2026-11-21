CREATE TABLE worker_routes (
 revision INTEGER PRIMARY KEY CHECK(revision>0),
 record TEXT NOT NULL,
 digest TEXT NOT NULL,
 operator TEXT NOT NULL,
 source TEXT NOT NULL,
 installed_at TEXT NOT NULL
);
CREATE TRIGGER worker_routes_no_update BEFORE UPDATE ON worker_routes BEGIN SELECT RAISE(ABORT,'immutable worker routing policy'); END;
CREATE TRIGGER worker_routes_no_delete BEFORE DELETE ON worker_routes BEGIN SELECT RAISE(ABORT,'immutable worker routing policy'); END;
CREATE TABLE worker_bindings (
 kind TEXT NOT NULL CHECK(kind IN ('model','container')),
 job_id TEXT NOT NULL,
 worker_id TEXT NOT NULL,
 platform TEXT NOT NULL,
 architecture TEXT NOT NULL,
 route_revision INTEGER REFERENCES worker_routes(revision),
 recorded_at TEXT NOT NULL,
 PRIMARY KEY(kind,job_id)
);
CREATE TRIGGER worker_bindings_no_update BEFORE UPDATE ON worker_bindings BEGIN SELECT RAISE(ABORT,'immutable dispatch owner'); END;
CREATE TRIGGER worker_bindings_no_delete BEFORE DELETE ON worker_bindings BEGIN SELECT RAISE(ABORT,'immutable dispatch owner'); END;
