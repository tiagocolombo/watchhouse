# Scheduled evidence-store expiry v1

Migration 035 and ADR 0042 add immutable OS policy and model-independent scheduled removal through the same transaction as [named body removal](README.md). This is evidence-store body retention, not whole-record/privacy/export/journal erasure.

## Configuration

```json
{"schema_version":1,"revision":1,"paused":false,"interval_seconds":1,"retain_seconds":{"normalized_source":604800,"execution_evidence":2592000,"reported_evidence":null,"supervisor_checkpoint":null,"simulation":604800,"other":null}}
```

These values are illustrative policy choices, not recommended legal or organizational retention periods. Every class must appear explicitly. Null retains it indefinitely; configured ages are 1..315360000 seconds. Names/unknown fields and unsupported versions reject rather than silently changing scope. `interval_seconds` is 1..86400; the scheduler removes at most one body per persisted interval. It does not silently waive dependent proof, deadlines, cleanup or uncertainty to meet an expiry target.

```sh
watchhouse --db state.db retention-policy --file /private/retention.json --operator operator
watchhouse --db state.db retention-status
watchhouse --db state.db retention-tick
```

OS revisions advance consecutively, remain immutable and support exact retries without resetting schedule/fences. A new revision recomputes cadence from the last retained tick. No HTTP or harness endpoint installs deletion authority. The ordinary coordinator `serve`, fake `worker` loop and explicit `tick` run configured expiry independently of browser/model availability. `retention-tick` performs one scheduled attempt; a row must satisfy the configured age and current pacing. Factory pause does not imply retention pause; use an explicit paused retention revision. A restored copied policy is held permanently, including after factory resume. A fresh OS revision is required before draining a restored backlog.

## Classes and indexed eligibility

| Class | Existing trusted producer provenance |
| --- | --- |
| normalized_source | sentry-api-v1, sentry-webhook-v1, sentry-v1:unaccepted-source-read, datadog-monitor-api-v1 |
| execution_evidence | native-container-v1, native-text-client-v1, container-fixture-v1, text-client-fixture-v1 |
| reported_evidence | operator-reported-v1 |
| supervisor_checkpoint | supervisor-checkpoint-v1 |
| simulation | fake-* producer provenance |
| other | All other provenance; retain unless explicitly aged |

Classification is generated from stored producer provenance, not event tags, model text or an API's claimed class. Migration and insert/metadata/removal triggers maintain an indexed queue of nonremoved bodies; class/recorded-time filtering precedes selection limits. Old unrelated classes and already removed history cannot occupy its prefix. Byte totals refer to the actual evidence-store bodies, not formal records or the whole database. Native and fixture evidence share an expiry choice while their original verification provenance remains distinct.

A canonical recorded UTC millisecond timestamp from the coordinator is required for automatic ageing. Legacy unknown/noncanonical ages are counted, retained and assigned to the installed OS operator for review; they are not guessed through SQLite's wall-clock interpretation. Unknown ages in an expiring class degrade factory health. Clock rollback below the factory watermark holds deletion. Time/cutoffs/cadence share one sampled instant under the write reservation.

Policy/currency, age, body integrity, tombstone creation, proof invalidation, effect fences, original duties, schedule and audit commit together without network I/O. New facts require current proof after the invalidation epoch. Existing stopped/unknown resources are still the original worker's responsibility. No live cancellation or rollback is inferred from deleting evidence.

## Failure and explicit integrity reconciliation

An item failure rolls back all body/effect/proof changes. Retain bounded generic failure metadata with digest, policy revision, attempt count and a 60-second retry deferral. Skip it during that interval so other valid bodies continue; persisted cadence/deferrals survive process restart. Do not log raw body/private exception text. Deferred items and unavailable policy/schedule produce visible factory degradation. Fatal retention errors are reported once while independent core duties continue; explicit CLI commands report the error. Status identifies the OS policy owner and the required inspection action.

Automatic policy never removes a corrupted body by assuming its original identity. Inspect current metadata without printing the body:

```sh
watchhouse --db state.db evidence-inspect --digest sha256:ORIGINAL_ID
watchhouse --db state.db evidence-redact --digest sha256:ORIGINAL_ID --observed-digest sha256:ACTUAL_CURRENT_BODY --operator operator --reason 'Explicitly remove the inspected corrupt body'
```

The actual current digest must still match under the removal transaction. A stale/wrong pin rejects. An explicit corruption tombstone retains both the original identity and the inspected body digest; it does not certify that corrupt content matched the original proof. The removed body remains unusable. Successful targeted removal clears its deferred item; no reconstruction of the old private body is required. This OS-only reconciliation cannot be requested by a worker or remote API actor.

## Read surfaces and limits

Authenticated `GET /api/v1/retention`, factory status and CLI expose policy/owner, copied-authority hold, next/last tick, class body/byte counts, removed count, deferred count and unknown ages. They do not return evidence bodies. Service-owner UI credentials cannot read global retention configuration. Global workspace health displays configured/paused/restored-held/unavailable expiry and review counts. No policy changes occur through a browser.

One selected body can have several dependent cases; its invalidation is atomic and the key set streams through a temporary table. The interval bounds selected bodies, not the duration of arbitrarily large dependency histories. Audit/tombstone metadata grows separately. Derived case/source/job/document/receipt content, accepted artifacts, private runtime journals/files, prior backups/exports and full incident bundles remain independent unfinished retention scopes. Secure physical erasure, actual private-data removal and live native cleanup are not claimed by fixture verification.

Rust fixtures cover all six explicit classes, unselected-prefix fairness, restart cadence, immutable/pause configuration, corrupt body deferral and pinned OS removal, unknown legacy age, restore fences, authenticated body-free metadata and continued core processing under corrupt policy/schedule. A separate actual local coordinator/browser fixture verifies configured expiry before any page is opened and without an executor. These use invented records, not live integration accounts or real operational retention decisions.
