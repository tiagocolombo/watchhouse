# Evidence-store removal and invalidation v1

Migration 034 implements an OS-owned **named evidence-store body** removal slice. [Class-specific scheduled body expiry](policy.md) is implemented in migration 035. Derived/formal-record removal, native journals/files, backup/export expiry and complete incident-bundle redaction remain subsequent development. This contract does not claim privacy erasure of every copy.

## Commands and retained scope

```sh
watchhouse --db state.db evidence-redact --digest sha256:ACTUAL_HEX --operator operator --reason 'Remove this named evidence-store body'
watchhouse --db state.db evidence-redactions --case case_ACTUAL_ID
```

There is no HTTP mutation or agent self-erasure capability. The OS caller supplies an exact existing valid digest, human operator identity and bounded reason. Ordinary removal rejects corrupt/missing original content; [metadata inspection and a pinned current-body digest](policy.md#failure-and-explicit-integrity-reconciliation) provide the separate explicit OS corruption-review path. Repeat removal returns the original immutable tombstone, never a new reason, timestamp or authority. A worker/model identity cannot remove evidence.

A single immediate transaction replaces `evidence.content` with the empty body and records a hash-verified immutable tombstone containing original digest, byte length, provenance, actor, reason and removal time. The empty body is not presented as having the original digest. The digest row, foreign-key memberships, command keys/receipts and historical records are retained. Updates/deletes cannot rehydrate a tombstoned body or remove its audit identity. Evidence byte accounting shrinks through existing triggers. `secure_delete` is requested for replaced SQLite cells, but logical removal is the supported guarantee: old WAL readers/pages, filesystem snapshots, exports and prior backups are not securely erased by this command.

This scope removes the one named evidence-store body. Case prose/events, source projections, model/job records, accepted document text, native journals, candidate/source files, receipts and exported copies may independently contain derived or copied data; they are explicitly outside this slice's content-removal scope. Full retention/redaction remains open until those remaining content scopes are implemented. A tombstone/invalidation cannot be advertised as having removed those copies.

## Invalidation and operating duties

For every direct case/source membership, plus duplicate-disposition dependents, append a case proof invalidation epoch. Immutable proof-registration metadata binds existing candidate/model/container, operation/observation/problem, impact, incident-artifact, learning and recovery-plan identities to their original epoch. Migration backfills existing records at epoch zero. New records bind transactionally; an old identity cannot be rebound after invalidation, even at equal clock time. Trigger inserts explicitly avoid duplicates rather than relying on OR IGNORE being preserved under an outer UPSERT conflict policy.

Current acceptance gates compare their proof binding with the latest case epoch. Native qualification, candidate/model adoption, runbook admission, hypothesis/message approval, impact and problem currency, recovery and accepted knowledge cannot reuse affected old context. Existing domain ownership, independent review, versions, deadlines, source validity and separate effect policy remain required. Cross-case accepted knowledge is rechecked through its original accepted-document lineage. Evidence-reference checks explicitly reject tombstones, including an originally empty blob whose hash would otherwise still match. Source context is not current when its source evidence was removed.

The transaction pauses/stops affected case effects through the same factory-stop primitive: it expires fake proof and fences native/publication/merge/runbook dispatch or mutation results. Cleanup and uncertain-effect reconciliation remain the original worker/controller's duty; no external cancel, rollback, credential lookup or live call occurs inside removal. Already dispatched effects are not undone. Explicit resume does not restore old proof. Fresh artifacts/plans and native qualification must establish their own current authority.

Reopen dispositions without extending original obligations, make old impact reviews due, invalidate reported recovery resolution/review and reopen submitted/fulfilled/waived corrective-action proof while retaining immutable history. Create an owned local reassessment escalation. Factory audit and pause history remain visible. An injected failure in audit/events rolls back body removal, counters, invalidation, pauses, effects and duties together.

## Reading, export and restore

`GET /api/v1/evidence/{digest}` returns `{redacted:true,tombstone:{...}}` for removed bodies. Scoped access still requires evidence membership; a known digest does not grant access. Case snapshots expose `evidence_redactions`, evidence choices identify removed proof, and the UI renders the reason/digest/removal scope inertly with owned reassessment and pause state.

Consistent backup inspection accepts an intentional empty body only with a known schema-34 tombstone whose retained metadata hash/identity verifies. Other content mismatches still reject. Restores preserve tombstones, epochs and missing bodies, alongside their existing external-authority fences; restoring cannot recreate removed content. Older backups are separate retained copies and need their own expiry/removal policy. Backups whose frozen artifacts are required remain complete under the existing backup contract; this slice does not prune those files.

## Verification

Synthetic tests cover removal/byte accounting, original receipt retry, rejected evidence reuse, immutable absence, original deadlines and owned duties, atomic failure rollback, equal-clock proof epochs, fresh prospective recovery planning and backup/restore preservation. A browser fixture runs the OS command against invented evidence and verifies inert tombstone/reassessment/pause visibility after reload. Ordinary tests use no live account/model/source. Existing physical native drills remain separate CI evidence; a logical removal fixture is not a live erasure or native cleanup claim.
