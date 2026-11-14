# ADR 0002 — SQLite transactional state, queue, and evidence

Date: 2026-11-14. Status: accepted for M0/M1. Resolves P02 for the local slice.

## Decision

Use bundled SQLite through rusqlite, WAL, foreign keys, synchronous FULL, bounded busy timeout, and short BEGIN IMMEDIATE write transactions. The SQL migration and its SHA-256 checksum are applied in one transaction; reject edited migration history and newer schemas. Add future migrations rather than editing 001 after release.

Persist cases, assignments, runs, append-only events, jobs, idempotency receipts, and bounded synthetic evidence bytes in one database. Jobs are the local transactional outbox: intake atomically creates the case, two events, its investigator assignment, one job, and the command receipt. Claim commits before execution. Completion atomically writes the result, evidence, states, and result event. No external-message outbox/delivery implementation is implied.

The case is the M1 aggregate: child assignment/run events advance its version. Operator updates require expected_version. A global command key binds to a hash of command type, target, and typed payload. An identical retry returns the original response even if the case advanced; different payloads conflict. Failed commands have no receipt. A read transaction produces snapshot state, full case history, and the exact resume cursor.

Jobs are at-least-once. Each claim creates an attempt/run, unique fence, and 30-second persisted lease. A live fence excludes another claim. After expiry a new attempt supersedes the old one, retaining the expired run and event. Results must match run, assignment, case, current fence, and unexpired lease. Repeating an accepted identical result is harmless. Fake execution is deterministic and has no mutable workspace or external authority, so retries are safe. These leases are not the complete M2 supervisor/handoff system.

## Evidence placement exception

The architecture proposed external content-addressed files. For bounded JSON fake results, store bytes inline with a sha256 digest, provenance, media type, and timestamp. This avoids a second crash boundary and orphan cleanup in the first slice. The digest/reference contract remains content-addressed; larger artifacts need a completed-write/storage lifecycle ADR before introduction. No raw telemetry, transcript, or real check evidence is collected here.

## Validation and limits

Tests inject SQL failures during intake, claim, and completion to prove rollback. Child processes exit without destructors after intake, claim, fake execution, and completion; reopening and retrying preserves accepted work and exactly one accepted result. Concurrent duplicate intake, title updates, and claims have explicit outcomes; stale and misattributed results are rejected.

One local SQLite file is authoritative. Do not put it on a network filesystem or use it as remote-worker transport. Clock rollback, long-running heartbeats, retry budgets, terminal failure/escalation, restore, and external-effect reconciliation remain future work. The 30-second lease is a fake-run engineering default, not an incident-response deadline.
