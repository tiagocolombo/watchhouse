# First Sentry / incident slice v1

This implements one explicitly mapped Sentry Cloud organization/project, Integration Platform issue webhooks and read-only API enrichment/reconciliation. It does not require per-error webhooks. No live account, relay, token or destination is configured by this repository.

## Operator configuration

Copy [config.json](config.json), replace the invented mappings, trusted severity update intervals and secret references, then install through the OS-level operator control plane:

```sh
watchhouse sentry --file /trusted/sentry-config.json --operator operator
watchhouse source-status
watchhouse source-sync
```

Configuration revisions are immutable. Organization/project identity cannot be remapped. Explicit service/environment/repository/release mappings do not grant repair, publication or production authority. `incident_update_seconds` defines bounded critical/high/moderate update obligations. Imported source text cannot edit these settings.

Arrange trusted ingress for `POST /integrations/sentry`. The current coordinator remains loopback/Host/Origin restricted; no public relay or TLS deployment is included. Inject the configured Integration Platform client secret and read-only API token. Actual Sentry install/plan permissions, including `event:read`, must be verified at activation. The adapter only reads fixed `https://sentry.io/api/0/organizations/...` endpoints; self-hosted/regional hosts are not supported yet. Payload URLs are never fetched, redirects are disabled, and credentials are not forwarded to imported links.

## Durable intake and source processing

`Sentry-Hook-Signature` is HMAC-SHA256 over exact raw request bytes. Constant-time verification, a 32KiB limit, configured-project validation and supported issue actions precede durable HTTP 202 acknowledgment. Unsigned request IDs/timestamps/resource headers are not ordering or authority. Signed-body digests identify receipts separately from normalized event deduplication. Raw private webhook/API bodies are discarded after bounded allowlist normalization.

The source pump runs independently of team supervision and notification delivery. It fetches issue details and a representative latest event, retains bounded issue/count/release/exception-type fields, and excludes user/request headers, private messages and variables. Source data remains untrusted, including in Markdown reports. API snapshots are distinct from webhook hints and verified service recovery. Occurrence timestamps normalize to UTC; older occurrence snapshots cannot regress the projection. Missing enrichment remains visible and retries after 60 seconds without creating another case. Stable organization/project/issue identity enforces one case; observations do not automatically declare incidents.

Reconciliation fetches one bounded issue page per tick using explicit project/environment, an empty status query and a persisted time window. The first window is 24 hours; subsequent completed windows overlap by five minutes. Page queueing and cursor progress commit together. Coverage advances only after the final page. Restart/outage preserves incomplete windows; repeated/cyclic cursors fail closed. Health exposes credential presence, pending age, cursor/error state and last covered window. Coverage means issue snapshots discoverable in that requested API window, not a complete historical per-error/lifecycle feed or proof of old status-only changes outside the window. Retention and live plan coverage still need an activation drill.

`serve` and continuous `worker` run source processing. `source-sync` advances one bounded page and one intake/enrichment item; `source-status` reads health. `tick` handles deterministic obligations and human delivery, including incident updates. Models or sources being offline does not disable these duties.

## Incident declaration and records

`case declare-incident CASE_ID --file FILE` / `POST /api/v1/cases/CASE_ID/incident` / the web versioned command form share this strict command:

```json
{
 "command":{"meta":{"schema_version":1,"idempotency_key":"incident-1","actor":"operator","reason":"Coordinate response to observed impact"},"expected_version":5},
 "severity":"high",
 "rationale":"Critical workflow impact warrants coordination",
 "impact":"Source events observed; users and traffic denominator unknown",
 "uncertainty":"Cause and recovery have not been established",
 "objective":"Reproduce and assess the failure",
 "roles":{"commander":"operator","operations":"operator","communications":"operator","scribe":"operator"},
 "evidence_refs":["REPLACE_WITH_CURRENT_SOURCE_SNAPSHOT_DIGEST"],
 "known_start":null
}
```

Declaration requires the accountable trusted operator, exact case version and current source evidence. It freezes source/configuration/policy provenance, severity/rationale, impact/uncertainty, roles, investigation evidence, an unsent communication draft, unknown timing/recovery and the next update deadline. Duplicate commands return the original declaration; another incident for the same case conflicts. Missed updates escalate without models. Source 'resolved' status never closes an incident.

JSON appears in snapshots. `case incident-markdown CASE_ID` prints plain Markdown; `GET /api/v1/cases/CASE_ID/incident/markdown` returns the report. It includes current brief, roles, impact/timing, response, recovery assessment, investigation register and live timeline, with imported text escaped. The web shows source health, observations and incident records. Further incident revision, mitigation/recovery, postmortem and corrective-action lifecycles remain future M3/M5 work.

## Verification and activation blockers

[Sanitized fixtures](../../../tests/fixtures/sentry/README.md) are synthetic examples based on public schemas, not customer history. Tests cover raw-byte signatures/cryptographic vectors, scope, receipt/event deduplication, out-of-order timestamps, allowlist exclusion, missed-issue recovery, pagination/outage/restart, incomplete coverage, enrichment retry, rollback, declaration uniqueness/deadlines, report escaping and HTTP/browser replay. Normal tests invoke no live Sentry, Slack or model service.

Live activation needs actual service/release mappings, Sentry app installation/client secret, API token/plan permissions, reachable trusted ingress and an operator-approved drill. No live intake, enrichment, reconciliation coverage or Slack delivery has been verified.

References: [signature contract](https://docs.sentry.io/integrations/integration-platform/webhooks/), [issue payload](https://docs.sentry.io/integrations/integration-platform/webhooks/issues/), [issue API](https://docs.sentry.io/api/events/retrieve-an-issue/), [organization issues](https://docs.sentry.io/api/events/list-an-organizations-issues/), [pagination](https://docs.sentry.io/api/pagination/).

## Known-issue refresh and read freshness

Source configuration accepts optional `refresh_seconds` (1–86400; v1 default 60). The default is omitted when serializing configuration so previously installed canonical configuration bytes and signature-path comparisons remain compatible. Installing a different interval requires a new immutable operator configuration revision. This bounds discovery retries, intake retries and known-issue refresh scheduling.

Each source tick processes at most one discovery page, one due intake item and one due known-issue refresh. Known issues use the fixed issue/enrichment API even when their last occurrence lies outside the current discovery window. This discovers missed status-only changes without another case or incident. Last successful read and requested-window coverage remain different facts: neither proves complete historical lifecycle/per-error coverage or service recovery.

Before read I/O, a durable per-source generation and retry deadline are claimed. Only the current generation can replace the source projection or mark read success. Overlapping late results remain unaccepted case evidence; they cannot overwrite a newer read. Older API data cannot establish freshness over retained newer occurrence evidence. An API snapshot older than the accepted webhook hint leaves enrichment pending. Process loss leaves a visible refreshing/interrupted read and a persisted retry deadline.

Unchanged accepted facts refresh the read ledger without creating evidence/timeline spam or invalidating communication review. The source snapshot's observation time is the time of its retained fact version; `last_success_at` is the latest successful reread. Changes to actual normalized facts still append evidence and invalidate context-bound review. Repeated unchanged failure hints are deduplicated; errors retain the last usable snapshot and expose incomplete freshness.

Case snapshots expose `source_freshness`; source health exposes known-issue read generations, configuration revision, next check, last success, age and error (newest 200 tracked sources). States distinguish current snapshot, refresh due, unavailable, refreshing/interrupted and untracked legacy records. The web polls read-only health independently of case events; this observation poll does not schedule core work. Fair due-time ordering prevents one failing old source from permanently occupying the intake/refresh head.
