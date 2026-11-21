# ADR 0041: Retain missing-proof identities without retaining valid authority

Status: accepted implementation choice, 2026-11-21.

Implement named evidence-store body removal first, with an immutable tombstone and conservative case proof invalidation. Keep original content identity, foreign-key memberships and historical command receipts, but never present the replacement body as original proof. Invalidate current context/acceptance and fence in-flight effects atomically with removal and owned reassessment. Use the existing factory-stop primitive and preserve original deadlines/cleanup/reconciliation duties.

Bind immutable proof identities to a case invalidation epoch in separate metadata, preserving old serialized record bytes and legacy identities. Backfill existing proof at zero; register new identities transactionally. Monotonic sequence identity distinguishes changes at equal clock time. Original bindings never update, including mutable job/recovery-state UPSERTs. New current facts require fresh identities/revisions and existing domain checks. Redacted references always deny proof, even if the removed body was originally empty.

Use OS-only removal; no agent/HTTP self-erasure. A durable error rolls back body/counter/epoch/effect/duty/audit state together. Return the original tombstone on repeat and expose missing proof clearly through CLI/API/snapshot/browser. Verified backups preserve intentional absence and immutable audit; restore never rehydrates it.

This slice removes only the named evidence-store body. Other case/source/job/document/receipt/journal/filesystem/export copies remain separate retention scopes. Do not claim semantic privacy erasure, whole-disk secure deletion or complete M8 retention. Scheduled class retention, derived/formal content handling, complete incident bundles and export/backup/journal expiry remain required development. See the [retention contract](../../contracts/v1/retention/README.md).
