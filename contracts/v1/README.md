# Implemented v1 contracts

The Rust types in [contracts.rs](../../src/contracts.rs) and [domain.rs](../../src/domain.rs) are the executable schema. Numeric schema_version is exactly 1; missing/unsupported versions, unknown command/event fields, unknown event kinds, and malformed results are rejected. These are initial local-development contracts, not a released public API. Version them explicitly when changing compatibility.

- [manual-case.json](manual-case.json) is the deterministic fake-source/manual intake fixture.
- [event.json](event.json) pins the event envelope and case.created payload. Tests deserialize fixtures, round-trip emitted events, and reject incompatible inputs.
- CreateCase: schema_version, idempotency_key, actor, reason, title, description.
- RenameCase: schema_version, idempotency_key, actor, reason, title, expected_version.
- Case: schema_version, id, title, description, owner, status, version, created_at. Status: queued, running, awaiting_review.
- Assignment: schema_version, id, case_id, role, owner, objective, state. Only investigator is admitted; state: queued, running, completed.
- ExecutionResult: schema_version, assignment_id, summary, conclusion, actual_model, usage. Fake-v1 requires insufficient_evidence, null model, and null usage; it carries no real recovery/check attestation.
- Snapshot: schema_version, case, assignment, events, cursor, results. Snapshot and cursor come from one SQLite read transaction.

Event envelope fields are in the fixture. Supported kinds: case.created, assignment.queued, run.started, run.lease_expired, assignment.completed, case.renamed. Each carries a reason; completion carries evidence_refs as sha256 digests. Child events share the case aggregate/version in this slice. recorded_at is coordinator time; occurred_at is null because source occurrence time is unknown. The cursor is a global SQLite sequence and may have gaps within one case; event identity is stable and events are append-only.

A command key is global to the database. Exact retries return the original response. Reusing it with another target/type/actor/reason/content conflicts. A title update needs the current case version and creates an audited event; it cannot change an in-flight task objective. No disposition, incident declaration, independent verification, or policy-editing command exists yet.

The current Task/ExecutionAdapter seam is internal Rust, not a remotely callable worker protocol. Do not treat this minimal fake contract as proof a real executor has isolation or cancellation capabilities. See [ADR 0004](../../implementation/adrs/0004-execution-protocol-spike.md).
