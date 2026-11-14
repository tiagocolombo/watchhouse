# Operating Watchhouse

## Factory health

Monitor coordinator heartbeat, scheduler lag, oldest queued job, stalled leases, outbox age, source ingestion/reconciliation lag, notification failures, worker capacity, artifact/database storage, and adapter/auth/quota health.

Separate service health from factory health. A quiet source with stale reconciliation is unknown, not healthy. Expose health without requiring a model.

## Pause and stop

Support global, service, repository, incident, and effect-class pause. Define separately:

- Stop admitting new work.
- Stop new external mutations while continuing observation and notification.
- Cancel/quarantine active runs and revoke effect credentials.

Show which actions remain active. Cancellation does not prove rollback or external failure. Reconcile uncertain effects before resuming. An operator can always inspect and export records while paused.

## Backups and restore

Back up the transactional database, referenced complete artifacts, policy/adapter configuration versions, and encryption/secret references through an appropriate secure mechanism. Do not assume repository source alone preserves incident state.

Use consistent SQLite backup/checkpoint mechanisms; do not copy a live database file casually. Verify artifact digests and restore to an isolated environment.

Restore marks old leases invalid, pauses mutations, rebuilds projections, restores deadlines, and reconciles PRs/merges/deployments before enabling effects. Test that overdue notifications do not flood recipients on restart.

## Upgrades

Version database migrations, event schemas, adapter protocols, artifact renderers, policies, and knowledge formats. Define supported upgrade paths and recovery for failed migration. Preserve historical facts; do not rerun agents to reconstruct missing audit history.

Pin runtime adapter compatibility ranges. Probe installed versions at startup and after changes. Show degraded compatibility instead of assuming a runtime upgrade is harmless.

## Retention and exports

Configure separate retention/access for raw telemetry, transcripts, formal artifacts, customer-safe summaries, and audit metadata. Redaction records preserve tombstones and dependent-evidence invalidation.

Export an incident bundle with structured records, document versions, evidence manifests, and permitted artifacts. Declare missing/redacted content and export time. A summary alone is not a forensic backup.

## Reliability and capacity

Prioritize active incidents and escalation over background knowledge distillation. Apply intake limits, deduplication, bounded enrichment, queue backpressure, and per-service/repository concurrency.

Control retry storms. Authentication/quota outages trigger circuit breakers and notices; they must not produce thousands of duplicate assignments. A held incident must not block unrelated work.

## Readiness and drills

Before enabling automatic merges, run the relevant verification scenarios in a dedicated repository. Before production runbook actions, demonstrate isolation, target scoping, stop/rollback behavior, and independent outcome checks.

Practice commander loss, worker loss, notification-channel failure, source outage, injected evidence, and restore. Use postmortems on factory failures too; accepted lessons cannot weaken factory authority.

## Current local development operations

Build/run/test commands are in the root README. Startup applies/checks migration 1 before CLI or HTTP work. `serve` binds only to 127.0.0.1 and also polls fake work; `worker --once` can finish one queued or expired job without a browser/server. A generated token changes on restart unless WATCHHOUSE_TOKEN is supplied. A claimed fake run waits at most its remaining 30-second lease before recovery; fake results have no external effects to reconcile.

The database contains state, events, command receipts, queue, runs, and bounded evidence. Retain the database rather than relying on process memory; do not copy a live WAL database casually. Consistent backup/restore tooling, clock-jump handling, scheduler health/backpressure, terminal retry budgets, notification/escalation, and automatic fault isolation are not implemented. A worker/storage error is surfaced to the process; restarting preserves committed work. No production-readiness claim is made by this local slice.
