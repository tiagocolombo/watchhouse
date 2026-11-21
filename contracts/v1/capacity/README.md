# Factory capacity v1

```sh
watchhouse factory-capacity
watchhouse factory-limits --file limits.json --operator operator
```

Capacity is also included in authenticated `GET /api/v1/factory` and the web health summary/details. Configuration is OS-only. Revision 1 is a retained built-in default; the first explicit override uses revision 2. Revisions are consecutive/immutable; an exact retry preserves the original provenance.

```json
{"schema_version":1,"revision":2,"max_queued_cases":1000,"max_queued_effects":1000,"max_pending_deliveries":1000,"max_evidence_bytes":268435456,"supervisor_evidence_bytes":16777216,"max_native_resources":2,"max_native_per_repository":2,"max_native_per_service":2}
```

New queued case, execution-effect and signed source-delivery records are rejected transactionally at capacity, before an acknowledgment or command receipt is committed. API callers receive 429 / `factory_capacity` with `Retry-After: 1`; this is retry guidance, not a promise capacity will exist in one second. Exact case/model receipts and duplicate signed deliveries still work while full. Existing state transitions, resumed/reconciled records, cleanup and overdue obligations are not new queue admission and can drain/reproject existing work even when the new-item limit is reached.

Source polling treats typed capacity pressure as transient and continues the independent coordinator and other source. A rejected signed delivery remains unacknowledged/retriable; it has no partially committed receipt. Current-state Datadog polling retains its documented offline-transition limits. Expected storage pressure from fake completion does not terminate the coordinator; the old run retains its original finite lease/deadline and supervision.

Evidence quotas count UTF-8 bytes in SQLite content, across all evidence writers. Digest deduplication does not spend capacity again. Insert/update/delete accounting is transactional; shrink is permitted even after a policy reduction leaves retained usage above the new limit. Ordinary evidence cannot consume the separate bounded `supervisor-checkpoint-v1` reserve; that provenance is selected by the deterministic supervisor, not an input field. This keeps lease-expiry checkpoints and owned escalation available under ordinary payload pressure. The reserve is finite and visible. Exhausted reserve/storage faults still require operator capacity management; no data is silently discarded.

Native resource reservations have global and mapped repository/service limits. A first reservation pins its original scope atomically with the worker/effect reservation. Later source remapping cannot free occupied old-scope capacity. Unknown/quarantined resources continue counting until positively cleaned or explicitly reconciled. Queue selection filters capacity before limiting rows, so an inadmissible queued job cannot starve running result observation, created-resource progress or cleanup. Waiting never extends the job's original deadline or attempt budget. Lowering limits holds new reservations and does not silently kill already admitted work.

A corrupted limit policy cannot authorize a new resource binding; already-owned cleanup stays available. Evidence capacity failure never accepts model/container results or creates native admission. Model resources are marked for bounded cleanup. Container results whose trusted wrapper already positively acknowledged removal retain only cancelled/cleanup metadata; other containers retain uncertainty and request scoped cleanup. No unknown write or model call is replayed to regenerate missing evidence.

Status shows actual queue counts, ordinary/reserved evidence use, native reservation counts, pinned per-scope use and database/WAL/SHM bytes. Counts include simulated fixture reservations and are not proof of physical execution. Scoped display caps each dimension at 100; admission counts all reservations. Limits allow 1–100000 queued items, 1–64 native reservations, 1KiB–16GiB ordinary evidence, and 64KiB–256MiB supervisor reserve; per-scope native limits cannot exceed the global limit.

These limits do not cap audit history, command/receipt metadata, candidate files, every filesystem allocation or total database size. Retention/redaction, backups/free-space recovery and operating drills remain separate M8 work. Source/notification credentials, destinations, native platform workers and live acceptance are not supplied by capacity configuration.
