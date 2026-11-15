# Implementation roadmap

Every milestone delivers a usable vertical slice with evidence and a STATUS.md update. A checked document is not an implemented feature.

## Current delivery state (2026-11-14)

M0/M1 and bounded fake-only M2 mechanisms are implemented, including independent challenges, trusted policy and human escalation/fallback/resolution. The first M3 Sentry/enrichment/reconciliation/incident-declaration slice is fixture-verified. Live activation remains blocked on actual configuration, credentials, destinations and trusted ingress. Broader M3 workflows and M4–M8 remain unfinished; see [STATUS](STATUS.md) and ADRs 0007–0010.

## M0 — Bootstrap and contracts

Deliver repository scaffold, chosen language/runtime versions, formatting/CI, migrations framework, versioned contracts, fake clock/runtime/source, and ADRs for initial stack.

Exit: a deterministic test creates a case, assignment, event, and evidence reference; migrations and event validation run without model access. Resolve stack/protocol spikes without committing to unsupported harness claims.

## M1 — Durable case and visible workflow

Deliver transactional case/event store, artifact references, job/outbox processing, manual intake, disposition/reopening rules, application API, and a minimal web case/timeline view.

Exit: restart after each transactional boundary loses no accepted case or obligation. Browser reconnect replays events. Expected/transient/uncertain examples have evidence-backed dispositions. Conflicting commands fail explicitly.

## M2 — Team collaboration and stuck-work recovery

Deliver roles, assignments, message threads, challenges, supervisor, leases/fencing, checkpoints, handoffs, acknowledgment timers, escalation inbox, and one configured human notification channel.

Exit: a fake investigator stalls while alive; another responder receives a handoff; the old worker cannot advance state or retain mutable authority. Unacknowledged escalation advances the chain. All models can be offline while notifications/timers still work.

## M3 — Sentry and incident records

Deliver authenticated intake, API enrichment, source/release mappings, reconciliation, incident declaration/severity profiles, command roles, investigation register, communications drafts, and structured artifact rendering.

Exit: duplicate/out-of-order notifications create no duplicate incident. Missed events are discovered by reconciliation. A real sanitized fixture completes a documented qualification path. Source lag and missing evidence are visible.

## M4 — Complete repair-to-PR workflow

Deliver isolated workspace/test executor, first real harness adapter chosen by capability spike, engineer/verifier/reviewer roles, regression validation, bounded repair loops, GitHub delivery, and PR reconciliation.

Exit: a fixture defect produces a reviewed candidate and one PR. Its regression fails on base and passes on candidate. Stale review, quota failure, process loss, and uncertain PR creation are handled. Fake adapters cover standard CI.

## M5 — Incident recovery and learning

Deliver authorized runbook-action boundary, recovery criteria/observation, deployment mapping, problem/known-error links, postmortem generation/challenge, corrective actions, accepted knowledge retrieval, Hallmark-format compatibility.

Exit: an incident recovers before its permanent repair; follow-ups persist. Low traffic yields inconclusive recovery. A false causal claim is challenged. Proposed knowledge cannot change its own review. Overdue actions escalate.

## M6 — Eligible automatic merging

Deliver explicit merge policy, repository checks, final candidate/base validation, conflict handling, idempotent/reconciled merging, audit, pause controls, and sensitive-change handling.

Exit: one eligible fixture merges automatically; changed head/base, red CI, unmet repository approvals, blocking challenge, or denied paths prevent merging. A crash after merge is reconciled without duplicate action. Validate in a dedicated test repository.

## M7 — Portability and second adapters

Deliver a second real harness, Datadog adapter, worker capability routing, OS-specific process cancellation tests, packaged macOS/Windows/Linux coordinator, and an optional Hallmark repair spike.

Exit: the same domain scenarios pass with two execution adapters and two source adapters without provider fields leaking into domain policy. Platform-required builds route correctly. Reassignment can cross harnesses using shared records.

## M8 — Operational hardening and full workspace

Deliver richer team/incident/change/knowledge views, RBAC for configured remote access, migrations/backup/restore, retention/redaction, load/backpressure controls, replay evaluation, and incident drills.

Exit: restore reconstructs records and obligations; redaction invalidates evidence safely; event floods do not starve escalation; degraded factory operation is visible and recoverable.

## Dependency and scope rules

M0 -> M1 -> M2 establishes safety and continuity. M3/M4 follow those contracts. M5 completes operational learning. M6 requires validated repair/review and external-effect reconciliation. M7 proves portability; perform an early protocol spike for several harnesses during M0 to avoid late lock-in.

Do not defer all UI to M8. Every milestone has a visible operating surface. Do not defer escalation until after production-capable automation.

## First implementation task

Completed: M0 plus manual case creation -> persisted event -> queued fake assignment -> observed result -> web timeline. Completed: versioned disposition/reconsideration commands, case-bound evidence, configurable review/monitoring/collection deadlines, and deterministic reopening in CLI/web. Completed first M2 slice: durable directed messages, owned blockers/checkpoints, progress/heartbeat supervision, acknowledged fake-worker handoff, stale-result evidence, and a local escalation inbox. Next: independent challenge lifecycle and scoped validation gates; then complete trusted policy configuration and an explicitly configured human notification/fallback channel before real execution.

2026-11-14 phase 1: independent challenge lifecycle, deadline escalation and exact-version assessment gates implemented; see ADR 0007 and STATUS. Trusted policy and external escalation remain next.

2026-11-14: Phase 2 trusted configuration implemented; external escalation is the remaining M2 delivery dependency.

2026-11-14: Bounded M2 mechanisms implemented for fake workers and fixture notifications; live activation remains blocked on explicit destinations/secrets. M3 fixture slice follows.

2026-11-15: first M3 source/incident slice implemented; current coverage limits and exact remaining work are in STATUS and ADR 0010.

2026-11-15 next M3 batch: versioned incident operations/roles, investigation and communication records, then known-issue reconciliation/freshness. PRs #2–#5 are merged; see STATUS.

2026-11-15 M3 next batch: incident operations/roles, versioned investigation/draft review, and known-issue read reliability are implemented with fixture evidence. Actual service-specific qualification/correlation, live activation and M4+ remain open; current STATUS supersedes historical next-task text above.
