CREATE TABLE publisher_policies(revision INTEGER PRIMARY KEY,digest TEXT NOT NULL,record TEXT NOT NULL CHECK(json_valid(record)),operator TEXT NOT NULL,source TEXT NOT NULL,installed_at TEXT NOT NULL);
CREATE TRIGGER publisher_policy_immutable BEFORE UPDATE ON publisher_policies BEGIN SELECT RAISE(ABORT,'publisher policy is immutable'); END;
CREATE TRIGGER publisher_policy_retained BEFORE DELETE ON publisher_policies BEGIN SELECT RAISE(ABORT,'publisher policy is retained'); END;
