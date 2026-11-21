# Factory operation controls v1

```sh
watchhouse factory-status
watchhouse factory-control --file pause.json
```

`GET /api/v1/factory` shares the status projection; authenticated `POST /api/v1/factory` shares the CLI command service. The web workspace has explicit scope/effect/stop/reason fields and individual resume controls. Current HTTP authentication is the existing single trusted local operator, not remote RBAC.

```json
{"meta":{"schema_version":1,"idempotency_key":"pause-checkout-001","actor":"operator","reason":"Inspect an owned worker outage"},"expected_version":0,"action":{"type":"pause","scope":"service","target":"checkout","effect":"all","cancel_running":false}}
```

Scopes are `global` (null target), `service`, `repository`, `incident` and `case`. Effects are `all`, `fake`, `model`, `container`, `publication`, `merge` and `operation`. Service mapping is explicit source service identity; repository matches source or repair-request mapping, including manually created repair cases. Scope matching remains live for future cases; no wildcard strings or arbitrary SQL are supported. Case/incident targets must exist.

Each pause has an operator owner and reason. Factory controls have their own expected version, append-only audit and global command idempotency; they do not change every case version just to record a hold. Exact receipt retries work after later controls. Conflicting keys/versions and failed audit writes cannot partially apply controls.

With `cancel_running:false`, new fake starts, native create/start, publication writes, merge writes and runbook dispatch wait. Already-running model observation, source intake/reads, messages, timers, escalation delivery, retained-effect reconciliation and original-worker cleanup remain active. Removing one pause does not remove overlapping pauses. Work keeps original deadlines and budgets while waiting.

With `cancel_running:true`, the control also expires active fake leases/progress authority, marks nonterminal native jobs cancelled or unknown/quarantined for bounded original-worker cleanup, and replaces active publication/merge/runbook reservation fences. In-flight effects may already have happened. Late acknowledgments cannot advance a superseded reservation; explicit read-only reconciliation can retain the actual result without replay. A stop is not target rollback, physical cleanup acknowledgment, credential revocation, deployment or recovery. Gateway/credential operations require their own configured authority.

Resume a specific pause using the returned ID and current factory version:

```json
{"meta":{"schema_version":1,"idempotency_key":"resume-checkout-001","actor":"operator","reason":"Inspection completed; retain all existing gates"},"expected_version":1,"action":{"type":"resume","pause_id":"REPLACE_WITH_RETURNED_PAUSE_ID"}}
```

Resume removes only that admission hold. It cannot resurrect an expired fake fence, replay a cancelled/unknown resource, restore native qualification, approve a candidate, reset attempts/deadlines or grant a credential.

Factory health is separate from service health. It exposes the model-independent supervisor tick/lag, monotonic observed-clock high watermark, active owned holds, per-kind state counts/earliest deadlines, uncertain resources, open escalation count and retained source read failures. Unknown/stale ticking and backward clock are visible. Backward clock holds new effects until time catches up while preserving inspection, supervision and cleanup. Read projections cap pause/audit display at 200/100; admission checks every active pause. This slice does not yet implement capacity limits, backup/restore, retention/redaction, remote RBAC or replay evaluation; those remain M8 development.
