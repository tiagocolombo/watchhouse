CREATE TABLE publication_slots(repair_id TEXT PRIMARY KEY,branch TEXT NOT NULL UNIQUE,head TEXT,pr INTEGER,attempts INTEGER NOT NULL DEFAULT 0);
CREATE TABLE publication_jobs(id TEXT PRIMARY KEY,case_id TEXT NOT NULL REFERENCES cases(id),repair_id TEXT NOT NULL REFERENCES publication_slots(repair_id),state TEXT NOT NULL,record TEXT NOT NULL CHECK(json_valid(record)));
CREATE INDEX publication_dispatch ON publication_jobs(state);
