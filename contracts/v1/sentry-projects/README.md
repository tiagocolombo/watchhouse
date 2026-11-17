# Independent Sentry project profiles v1

This extends the existing Sentry contract to independently configured projects. Each active project has one explicit service/environment/repository scope, release mappings, severity-update intervals and credential references. The fixed Sentry Cloud origin and environment-filtered reads remain the supported wire profile.

## Install and inspect

The first project remains the immutable default identity for the existing webhook route. Additional projects use a separate OS operator command:

```sh
watchhouse --db /trusted/watchhouse.db sentry --file /trusted/checkout-v1.json --operator operator
watchhouse --db /trusted/watchhouse.db sentry-project --file /trusted/payments-v2.json --operator operator
watchhouse --db /trusted/watchhouse.db sentry-projects
watchhouse --db /trusted/watchhouse.db source-status
```

`revision` advances the global configuration sequence; active heads are independent per organization/project. Updating the first project after adding revision 2 therefore uses revision 3. Reinstalling identical historical bytes is a receipt-like replay; it does not reactivate that version or replace provenance. Use a new version for a deliberate configuration change.

Example additional project:

```json
{"schema_version":1,"revision":2,"organization":"fixture-org","project_id":"43","service":"payments","environment":"test","repository":"example/payments-fixture","releases":{"fixture-release-1":"abcdef1234567890"},"incident_update_seconds":{"critical":900,"high":1800,"moderate":3600},"webhook_secret_env":"WATCHHOUSE_SENTRY_PAYMENTS_WEBHOOK_SECRET","api_token_env":"WATCHHOUSE_SENTRY_PAYMENTS_API_TOKEN","refresh_seconds":60,"read_policy":{"schema_version":1,"request_seconds":5,"lookback_seconds":86400,"overlap_seconds":300,"max_pages":10}}
```

These are invented mappings and secret references, not activation credentials. Install secrets only in the source worker's trusted environment. No HTTP or model configuration installer exists. At most 32 projects are active, and this adapter forbids aliasing one numeric project ID under different organizations. Existing default-project installation still refuses identity remapping.

New explicit project installation requires `read_policy`. Request time is 1..30 seconds, lookback is 1..2592000 seconds, overlap is smaller than lookback and at most 86400 seconds, and page budget is 1..100 per discovery window. Refresh remains bounded by the existing configuration contract. Older default profiles without this field retain frozen v1 compatibility values: 10-second requests, 86400-second lookback, 300-second overlap and 100 pages. The operator status names whether a profile uses explicit policy or frozen legacy compatibility; no missing field silently selects a new dynamic default.

Profiles retain installer/time and configuration-source provenance. The CLI records canonical input paths; migrated profiles truthfully state that their original path was not retained. Secret values are never in profiles, health output or evidence. Boolean credential presence is not live permission or delivery proof.

## Authenticated project intake

The original `POST /integrations/sentry` selects the default project's current profile. Independent installations use:

```text
POST /integrations/sentry/{organization}/{project_id}
sentry-hook-signature: HMAC-SHA256 over the exact raw body
```

The selected profile supplies its own secret reference. Unknown projects, wrong keys and payloads naming another project are rejected. The local Host/Origin and body-size boundaries still apply; trusted ingress must deliver the correct loopback authority. Acknowledgment follows durable receipt/queue commit. There is no shared-key or default-project fallback for another project.

Issue IDs are namespaced by organization/project. The same issue number in two projects produces separate cases and evidence, while retries/out-of-order hints within a project retain the original case. Queued receipts retain their authenticated configuration provenance; enrichment uses the currently installed profile for that same project. It cannot borrow another project's credentials, scope or source key.

## Fair discovery and recovery

Each source tick discovers at most one due project page, processes one pending delivery and refreshes one known issue. The discovery cursor rotates durably among due active projects, so a paginated or unavailable project cannot monopolize discovery. Each project's window, continuation cursor, page budget, coverage watermark, backoff/error and refresh ledger remain independent.

Long offline gaps advance from the retained watermark in bounded lookback-sized windows with overlap; they do not jump straight to the current time or silently omit the intervening interval. The configured page cap leaves incomplete coverage visible rather than pretending the window completed. Retrying/restarting cannot reset the same window's consumed continuation pages. Configuration changes establish a new profile/window and retain the preceding profile's coverage history; coverage is not transferred across changed scopes.

Known-issue refresh selects the issue's own current project profile, even outside discovery windows or while another project is unavailable. Read generations and active profile checks fence late results. Configuration changes during discovery prevent the old page from queueing signals; changes during enrichment retain the late result as unaccepted evidence. Current mapped facts cannot be restored by a late receipt or a historical configuration replay.

Freshness explicitly reports `configuration_changed` / `configuration_unavailable` and both read/active revisions where known. Candidate/model authority, accepted hypotheses and reported-impact currency also reject obsolete project configuration. Impact collection supervision can escalate that change before a replacement source read. These are context gates, not evidence that the new configuration is correct or that a service recovered.

The existing authenticated source-health view now includes all project profiles/provenance and per-revision reconciliation/issue freshness. Source status, completed polling, zero new events and a merged patch never prove recovery.

## Verification and activation

Contract fixtures exercise independent projects, duplicate IDs, out-of-order/missed deliveries, restart-safe discovery rotation, per-project outage, bounded backlog windows/page budgets, configuration changes during reads, stale impact context, immutable provenance and real HMAC routing with distinct synthetic keys. They perform no live Sentry/model/notification call.

Live acceptance still requires authorized project/service/release mappings, Sentry installation secrets/tokens/plan permissions, trusted ingress and actual sanitized operational history. Fixture success is not that live activation. See ADR 0028 and COMPLETION.md.
