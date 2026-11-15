# ADR 0009: Durable Slack escalation with explicit uncertainty

Accepted 2026-11-14. Completes the bounded M2 notification/fallback/resolution slice for local fake execution.

Slack incoming webhooks provide a small concrete human channel. Configuration is an explicit versioned operator-only CLI operation, separate from the case's fake execution policy. Each escalation freezes its recipient chain and retry limits. Only secret environment references persist. Worker messages cannot install destinations or communication authority. Slack notices include stable IDs, never imported prose or telemetry.

Delivery claims commit before external I/O. Lease/fence completion prevents concurrent sender results from overwriting current state. Known rate-limit rejection can retry within budget; unknown/network/server outcomes or process loss require operator reconciliation with evidence. Slack acceptance is not human acknowledgment. Missing acknowledgment/response advances the bounded chain; exhausted chains remain unresolved in the local inbox. Explicit evidence-backed resolution is a separate durable record and cannot clear the underlying blocker or challenge.

The local operator boundary remains unchanged: recipient actor labels are attribution under trusted token/OS access, not Slack-user authentication. No interactive Slack command endpoint is claimed. Actual destinations and secrets are activation blockers; tests cover fake transport acceptance/rejection/retry/uncertainty, restart, fallback, acknowledgment versus resolution, process loss and post-effect persistence failure without models. No live notification was sent or verified.
