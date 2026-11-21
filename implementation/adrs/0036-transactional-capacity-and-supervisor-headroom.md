# ADR 0036: transactional admission capacity and separate supervisor evidence

Implementation decision, 2026-11-20. M8 requires bounded intake/storage/concurrency without starving observations, cleanup or model-independent duties.

Migration 031 installs immutable OS limit revisions, evidence byte counters/triggers, new-item queue guards, native reservation projections and immutable first-reservation scope bindings. Existing receipts/deduplication are checked or excluded before new-admission quota errors. SQLite constraint markers map to typed backpressure and API 429. Source/worker loops treat expected pressure as a wait, preserving independent core operation.

A separate finite reserve is available only to deterministic supervisor checkpoints. It is not exposed as a caller-supplied provenance or unlimited safety exemption. Ordinary evidence, including worker output, must fit its own quota. Evidence loss cannot grant result/native admission; trusted positive cleanup can be retained separately without promoting the missing result. Unknown resources remain counted until cleanup/reconciliation.

Global and original repository/service resource scopes are reserved under the same write transaction as effect/worker identity. Created/running/cleanup work progresses independently of new-creation capacity, and original deadlines remain governing. Scope remapping and policy corruption cannot manufacture a free resource slot.

Limits are not a full-disk or whole-history quota. Audit/receipt/candidate retention, backup/restore, redaction, remote RBAC and replay/operating drills remain required work. Live credentials/authority and native macOS/Windows workers remain separate gates. Fixture and actual local-process liveness tests do not imply live source/model or production verification.
