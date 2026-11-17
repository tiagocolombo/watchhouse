# Structured case assessment v1

This extends the existing qualification/disposition and incident artifact contracts without granting execution, source suppression or recovery authority. All existing plan shapes and command fingerprints remain readable. The local bearer session is still one operator; identity labels are attribution, not remote RBAC.

## Remaining disposition categories

Every disposition retains case version, scoped case-bound evidence, rationale, uncertainty, owner, recurrence/reopening conditions and a policy-bounded review deadline. Other than `insufficient_evidence`, plans require supporting operator-reported evidence. All retain an owned review/collection obligation and leave independent action/repair/recovery gates intact.

| Plan `kind` | Additional required fields besides `after_seconds` |
| --- | --- |
| `duplicate` | `target_case`, exact current accepted duplicate `relation_revision` |
| `already_addressed` | `change_reference`, `verification_task` |
| `external_dependency` | `dependency`, `dependency_owner`, `status`, `recovery_tracking`, `resilience_task` |
| `operational_configuration` | `workflow`, `permitted_action_reference` |
| `application_defect` | `basis`, `repair_assessment` |

`basis` is `{ "kind":"evidence_gap", "collection_task":"..." }` or `{ "kind":"accepted_hypothesis", "id":"...", "revision":2 }`. The accepted variant must reference the exact currently independently accepted supported hypothesis and cite its supporting evidence. A missing/invented/unsupported/stale artifact cannot supply the basis. A classification or reported change does not make an action policy, deployment or service recovery exist.

Duplicate classification requires an already accepted same-scope duplicate relation. A relation alone does not classify either case. The deterministic obligation supervisor reopens the classification if that exact relation is revoked/superseded or either frozen source digest changes, as well as at its deadline. Neither case's incident, impact, communications, escalation or corrective-action duties are transferred or closed.

The existing `case record-disposition` and `POST /api/v1/cases/{id}/dispositions` accept the extended plans. The web disposition form provides the required fields and retains literal plan inspection.

## Independent hypothesis acceptance

`case record-incident-artifact` / `POST /api/v1/cases/{id}/incident/artifacts` add:

```json
{"command":{"meta":{"schema_version":1,"idempotency_key":"accept-hypothesis-unique","actor":"independent-reviewer","reason":"Review the exact claim and discriminating experiment"},"expected_version":42},"action":{"type":"accept_hypothesis","id":"HYPOTHESIS","revision":2,"evidence_refs":["SUPPORTING_DIGEST"]}}
```

The reviewer must hold current commander/operations authority and must not have authored any revision of the hypothesis or its discriminating experiment. The hypothesis must already be supported by its exact completed experiment. Review validates source/incident context, evidence integrity and blocking challenges; acceptance is immutable and cannot approve another revision. Ordinary unrelated case events do not invalidate hypothesis acceptance. Changed source/incident context, new hypothesis revision, blocking challenge or corrupt supporting/counterevidence prevents current acceptance. Communication review retains its existing stricter case-version rule.

Acceptance reviews the retained evidence and its limitations. It does not turn reported/synthetic observations into independent native execution, established production causality, accepted reusable knowledge or recovery proof.

## Reported impact windows

`case assess-impact CASE --file FILE` and `POST /api/v1/cases/{id}/impact` share an expected-version/revision command:

```json
{"command":{"meta":{"schema_version":1,"idempotency_key":"impact-unique","actor":"operator","reason":"Record the attributed observation and remaining collection"},"expected_version":42},"expected_revision":0,"service":"checkout","environment":"test","scope":"Order submission","window":{"start":"2026-11-15T11:58:00Z","end":"2026-11-15T11:59:00Z","affected_requests":0,"total_requests":0,"affected_users":null,"total_users":null,"instrumentation":"Reported zero request traffic; ingestion completeness unknown"},"critical_workflow":"Submit order","impact_statement":"Actual affected users remain unknown","uncertainty":"Invented fixture report only","collection_owner":"impact-owner","collection_task":"Collect request/user denominators and coverage","review_seconds":60,"evidence_refs":["OPERATOR_REPORT_DIGEST"]}
```

The current case owner or incident commander/operations owner supplies supporting reported evidence. Source-backed cases must match their configured service/environment. A missing window/count/denominator remains null; it never becomes zero. Windows must be ordered, historical and at most 30 days. Counts are nonnegative, exactly representable JSON integers; affected counts cannot exceed a supplied denominator. Zero traffic is explicitly `no_request_traffic`, not recovered. No severity or recovery state is inferred from this record.

Snapshot `impact` lists the latest 100 immutable revisions, newest first, with provenance `operator-reported-impact-v1`, source-context currency and collection-review state. The latest revision's deadline or changed source digest creates an owned escalation without models. A new revision supersedes the collection assessment without claiming the previous task was completed or resolving its escalation. Old revisions/deadlines remain retained; original command receipts remain historical. Qualification Markdown includes escaped impact records.

The web provides reported-impact fields, optional window/counts, collection owner/deadline and literal retained observations. Real authenticated telemetry measurement remains a distinct integration path.

## Trusted cross-service mapping

Install through the OS operator plane only:

```sh
watchhouse --db /trusted/watchhouse.db service-topology --file /trusted/topology-v1.json --operator operator
watchhouse --db /trusted/watchhouse.db service-topology-status
```

```json
{"schema_version":1,"revision":1,"paused":false,"max_gap_seconds":300,"dependencies":[{"caller":{"service":"checkout","environment":"test","repository":"example/checkout"},"dependency":{"service":"payments","environment":"test","repository":"example/payments"},"rationale":"Operator-configured request dependency"}]}
```

Policies retain immutable revision/digest/installer/source/time; at most 64 distinct dependency pairs and a 1..3600-second suggestion gap are allowed. Models and HTTP clients cannot install them. Read-only suggestions prioritize existing exact same-scope diagnostic matches, then inspect a bounded prefix of 200 neighbor-service observations, returning at most 20 total hints. Both sides need current successful reads and independently mapped release/revision context. Different services' release strings/commits are not equated.

These are dependency/timing hints, not shared stack signatures, duplicates or causal evidence. A cross-service `related` link requires current evidence and expected case versions on both sides, common accountable operator ownership, no blocking challenges and `expected_topology_revision` matching the active unpaused mapping. Cross-service duplicates are denied. Accepted links retain topology revision/digest; changed/paused mapping makes their projected currency stale. Revocation remains available without accepting a new graph. No link combines incidents or waives their separate duties.

The current source adapter still has a single-project configuration path. Synthetic normalized multi-scope fixtures prove these domain gates; multi-project Sentry configuration/intake/reconciliation is the next independent adapter continuation. Actual sanitized operational history and live activation remain unverified.
