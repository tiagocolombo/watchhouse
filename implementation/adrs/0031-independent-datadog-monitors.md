# ADR 0031: Independent Datadog current-monitor intake

Implement an independent, read-only monitor polling adapter with explicit immutable OS mappings and budgets. Separate its configuration/read foreign-key namespaces from Sentry. Share normalized source records, source validity/freshness and incident timers while retaining legacy serialized field names; Datadog monitor status is not an error issue, event count, metric denominator or recovery proof.

Reserve before reads, schedule fairly, fence every projection by exact configuration/generation and retain stable source/case identity. Failed reads remain visible factory health. Exclude queries/messages/tags/user fields. Unchanged facts do not spam case events. Removed/remapped sources invalidate governing context before another read.

Polling discovers current state after restart; it cannot reconstruct transient historical transitions. Webhook intake, arbitrary discovery, event/metric/deployment APIs are not advertised capabilities of this bounded adapter. They are optional future source capabilities rather than substituted/fabricated fields. Native/live acceptance remains separate from fixture verification. M7's second harness, capability routing/platform cancellation and optional Hallmark repair spike remain required work.

Primary protocol sources inspected:
- https://docs.datadoghq.com/api/latest/monitors/
- https://github.com/DataDog/datadog-api-client-rust/blob/master/src/datadogV1/model/model_monitor.rs
- https://github.com/DataDog/datadog-api-client-rust/blob/master/src/datadogV1/model/model_monitor_overall_states.rs

No production authority is added by this decision. Live activation requires actual mappings, read credentials/network and sanitized history supplied by the accountable operator.
