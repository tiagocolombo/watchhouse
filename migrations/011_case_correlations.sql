CREATE TABLE case_correlations (
    left_case TEXT NOT NULL REFERENCES cases(id),
    right_case TEXT NOT NULL REFERENCES cases(id),
    revision INTEGER NOT NULL CHECK(revision>0),
    record TEXT NOT NULL CHECK(json_valid(record)),
    PRIMARY KEY(left_case,right_case,revision),
    CHECK(left_case<right_case)
);
CREATE INDEX correlation_right ON case_correlations(right_case,left_case,revision);
CREATE TRIGGER immutable_correlation_update BEFORE UPDATE ON case_correlations BEGIN SELECT RAISE(ABORT,'correlation revisions are immutable'); END;
CREATE TRIGGER immutable_correlation_delete BEFORE DELETE ON case_correlations BEGIN SELECT RAISE(ABORT,'correlation revisions are immutable'); END;
CREATE INDEX source_case_scope ON source_cases(json_extract(record,'$.service'),json_extract(record,'$.environment'),json_extract(record,'$.repository'));
