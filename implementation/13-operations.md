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

## Historical local baseline (M1)

Build/run/test commands are in the root README. Startup applies/checks migrations 1–3 before CLI or HTTP work. `serve` binds only to 127.0.0.1 and also polls fake work; `worker --once` can process a bounded batch of due qualification obligations and finish one queued or expired fake job without a browser/server. `tick` processes up to 100 due qualification obligations without an executor. A generated token changes on restart unless WATCHHOUSE_TOKEN is supplied. A claimed fake run waits at most its remaining 30-second lease before recovery; fake results have no external effects to reconcile.

The database contains state, events, command receipts, queue, runs, and bounded evidence. Retain the database rather than relying on process memory; do not copy a live WAL database casually. Consistent backup/restore tooling, clock-jump handling, scheduler health/backpressure, terminal retry budgets, notification/escalation, and automatic fault isolation are not implemented. A worker/storage error is surfaced to the process; restarting preserves committed work. No production-readiness claim is made by this local slice.

M1 deadlines are persisted rather than process-local timers. `serve` and continuous `worker` tick before each fake-work claim (250 ms idle polling). On restart, overdue active decisions are reopened once with an owned due obligation; original deadline and trigger time remain inspectable. Review/monitoring/collection durations are explicitly set in commands within 1 second–30 days. No automatic notification, acknowledgment chain, repair, or recovery action is caused by these deadlines. See ADR 0005 for lifecycle and clock-jump limits.

## First M2 core operation

The native binary now runs team coordination independently of HTTP: use `worker --block --once`, a versioned `team offer-handoff`, and `worker --identity fake-specialist --once` against the same database. Successor workers discover/accept offers from persisted state. `worker --stall` heartbeats without progress for supervisor drills. `serve --manual-workers` runs timers/API while separate CLI responders dispatch; default serve still runs the initial fake responder. `tick` processes qualification and team deadlines without any executor.

Lost/progress-expired attempts become owned blocks rather than automatic silent retries. Deadline obligations and local inbox records survive restarts. Worker version conflicts are recoverable contention; storage faults remain surfaced. No installable release/OS service, external notification, real-worker isolation, or remote authority has been added. Development fake limits and readiness exclusions are in ADR 0006.

## Worker qualification operations

The [controlled qualification CLI](../contracts/v1/worker-qualification/README.md) reserves attempts before I/O. The existing supervisor expires interrupted attempts without models; same-key retries inspect their status instead of launching processes. Quarantined resources require operator inspection and case-bound reported evidence before a fresh attempt. No arbitrary-PID termination or automatic path deletion is exposed. Native policy installation/invocation are CLI-only; status/evidence/Markdown/reconciliation are visible through the authenticated case API/web interface. Current scope is probe-only and never authorizes real model or repository execution.

## Consistent backup operation

The [backup contract](../contracts/v1/backups/README.md) implements bounded OS-owned SQLite WAL snapshots, complete declared frozen artifacts, manifest/evidence/migration/inventory verification and visible metadata audit. Use `backup` and `backup-inspect`; inspection opens no coordinator database. Failed/incomplete destinations are retained and never overwritten. Store exports in an OS-protected parent directory and retain the manifest digest independently. Authority-safe restore remains unavailable; running a copied snapshot as another coordinator is not a supported restore. Later factory controls/capacity and STATUS.md supersede the historical M1 operating limitations above.
