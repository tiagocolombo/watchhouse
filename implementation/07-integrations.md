# Integration boundaries

## Source adapter

~~~text
capabilities()
verify(delivery)
normalize(delivery) -> signals and external links
fetch_evidence(reference, bounded_query)
reconcile(cursor, window) -> changes, next cursor, coverage
optional_writeback(action, authority)
~~~

Keep provider-native fields in a versioned extension alongside normalized fields. Do not flatten away dimensions needed for diagnosis. Supported signal types include error/log evidence, issue/alert lifecycle, metrics, incidents, deployment changes, and manual reports.

Separate delivery deduplication from event deduplication and semantic correlation. A retry is not a new incident. Different providers may report one failure; one grouped issue may contain several causes.

Correlation uses service/release/environment, stack signatures, dependencies, timing, and accepted evidence. Agent-proposed merges/splits require recorded rationale and reversible links.

## Sentry first

Verify signatures against exact raw bytes and acknowledge only after durable intake. Keep response work out of the webhook handler. Implement replay protection appropriate to actual provider guarantees; do not assume all headers are cryptographically covered.

Use issue/alert notifications plus API enrichment and periodic reconciliation as supported by the configured plan. Per-error webhook availability differs by plan; do not make it a product prerequisite.

Capture stack information, breadcrumbs, representative events, environment/release, user/request counts where available, and source status. Redact and bound the evidence before agent access.

Resolve project -> service -> repository -> deployed revision using explicit mappings. A matching repository name alone is insufficient authorization to repair.

## Datadog and other sources

The second source adapter is an abstraction test. Map monitors/incidents/events/metrics and evidence links without pretending they are Sentry issues. Preserve provider-specific queries, dimensions, timestamps, and coverage.

Capability differences are visible in the UI. If evidence cannot be fetched, state the limitation and route an evidence request instead of manufacturing equivalent fields.

## Repository and delivery adapter

Responsibilities: inspect repository/ref; prepare isolated checkout; read CI requirements/results; create/update PR; query reviews; merge when permitted; reconcile external effects.

Keep publishing credentials outside builders. Use minimal scoped tokens and a stable operation identity/branch convention to reconcile creation after a crash.

Never bypass repository protection. Inspect the exact PR head and target base before a merge decision. Stale reviews/checks must not authorize a different candidate. Freeze or revalidate the final candidate according to repository merge strategy.

Deployment adapters return observed release/artifact/environment and included changes. A PR merge event is not a deployment event. Without a deployment mapping, outcome is pending or inconclusive.

## Notifications and stakeholder communication

Separate internal agent messages from external notifications. Delivery channels implement destination validation, configured authority, urgency, retries, delivery receipts, deduplication, and fallback. Send no customer message merely because an agent generated a draft.

The initial operator inbox and web notification stream can work without third-party messaging. Human escalation requires a reachable configured destination and must expose delivery failures.

## Hallmark repair backend

Proposed contract:

~~~text
submit(work_spec, authorization_scope, idempotency_key) -> handle
status(handle) -> progress, candidate, evidence, review, delivery
cancel(handle)
reconcile(handle)
~~~

Watchhouse owns cases, incidents, team coordination, and outcome verification. Hallmark can own an admitted repair job within its delegated authority.

Choose one authoritative publisher/merger per work item. If Hallmark publishes/merges, Watchhouse supplies bounded authorization and validates returned evidence. Do not run two independent merge controllers.

Current Hallmark integration is a proposal. Its Claude-specific runner and evidence/permission semantics need explicit compatibility verification. A facade cannot grant capabilities the backend lacks.

## Local intake and offline workers

Provide a durable relay or polling/reconciliation option for machines that are not always online. Relay delivery identity and retention must survive coordinator downtime. Duplicate delivery is expected. Show the oldest unprocessed event, reconciliation coverage, and source lag.

A provider outage must be distinguishable from healthy silence.
