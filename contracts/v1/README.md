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

A command key is global to the database. Exact retries return the original response. Reusing it with another target/type/actor/reason/content conflicts. A title update needs the current case version and creates an audited event; it cannot change an in-flight task objective. M1 qualification commands are described below. Incident declaration, independent verification, and policy-editing commands remain unimplemented.

The current Task/ExecutionAdapter seam is internal Rust, not a remotely callable worker protocol. Do not treat this minimal fake contract as proof a real executor has isolation or cancellation capabilities. See [ADR 0004](../../implementation/adrs/0004-execution-protocol-spike.md).

## Qualification extension

The strict types in [domain/qualification.rs](../../src/domain/qualification.rs) are added to local v1. Snapshot adds `qualification: {dispositions, obligations, evidence}` with an empty default for reading old snapshots. Existing Case fields and old event bytes/receipts are preserved. New event kinds require an updated reader; schema 2 prevents old binaries from operating the upgraded database.

Examples: [record-evidence.json](record-evidence.json), [record-disposition.json](record-disposition.json), [reconsider-case.json](reconsider-case.json). These fixtures validate shape; replace zero digests, example disposition ID, and expected versions with your actual case snapshot/receipts before submitting.

| Command | Fields beyond schema_version/key/actor/reason/expected_version |
| --- | --- |
| RecordEvidence | scope, statement, collection_method, observed_at (RFC3339 or null), uncertainty |
| RecordDisposition | owner, scope, uncertainty, evidence_refs, reconsideration_conditions, reopen_after_occurrences, plan |
| ReconsiderCase | disposition_id, trigger, evidence_refs |

Command field `idempotency_key` is global to the database. RecordEvidence returns `{case, evidence}` including digest, attribution/provenance, summary, and recorded timestamp. Disposition/reconsideration return the original post-command Case receipt; query snapshot for current decision state.

`plan` is a strict tagged object: `expected_behavior` requires after_seconds; `apparently_transient` also requires recovery_indicators; `insufficient_evidence` also requires collection_task. Intervals must be 1..2592000 seconds and thresholds 1..1000 distinct occurrences. All decisions cite 1..20 unique, digest-valid evidence references belonging to this case. Expected/transient plans require at least one operator-reported reference; fake evidence alone only supports insufficient-evidence qualification. Reports are attribution, not independent checks.

Trigger values: operator_review, recurrence, contradictory_evidence, scope_changed. The current active disposition and version must match. Each recurrence command counts one occurrence and requires operator evidence recorded after the current decision, not already counted/cited by it. Other triggers reopen immediately. Conditions are explicit operator-readable scope/criteria; natural language is not parsed as automation. New source integration must normalize observations before matching them.

New events: evidence.recorded, disposition.recorded, obligation.scheduled, disposition.reconsidered, case.reopened, obligation.due. Decisions are active/reopened/superseded; obligations pending/due/superseded. Every active decision has one owned persisted obligation. Timer processing makes it due and reopens review once, preserving the original deadline. New decisions supersede prior records/obligations; they do not claim those tasks were completed. Execution status stays awaiting_review. No repair, recovery, notification, or new executor is implied.

## Bounded M2 team extension

The [team contract catalogue](team/README.md) describes strict message/run/block/handoff/escalation commands and versioned examples. Snapshot adds a defaulted team projection while preserving historical Case/Assignment shapes and v1 events/receipts. New event families include team.message_posted/acknowledged, run.heartbeat/progressed/late_result_recorded, assignment.blocked, handoff.proposed/accepted/expired, and escalation.opened/acknowledged. Each important transition remains in the case aggregate's durable timeline. No typed message can grant authority or resolve an independent challenge.
