# Implementation roadmap

Every milestone delivers a usable vertical slice with evidence and a STATUS.md update. A checked document is not an implemented feature.

## Completion gate (2026-11-16)

Complete the remaining milestone development and required acceptance evidence before requesting local operator acceptance testing. Continue automated validation during implementation. [COMPLETION.md](COMPLETION.md) expands the outstanding exit gates; [WORK_CHECKPOINT.md](WORK_CHECKPOINT.md) records interrupted, uncommitted M4 work. Neither a native demo bundle nor a blocked activation is full milestone completion.

## Prior delivery state (2026-11-15)

M0/M1 and bounded fake-only M2 mechanisms are implemented. PRs #2–#11 are merged. M3 now includes source intake/reconciliation/freshness, incident operations/roles, investigations/draft review and scoped correlation/diagnostic evidence. Synthetic service-specific qualification is tested; actual sanitized service-history acceptance and live activation remain blocked on operator inputs. M3 is not fully accepted. M4 preparation and controlled worker qualification are merged; durable synthetic adapter lifecycle is the current slice under review; real execution and M5–M8 remain unfinished; see [STATUS](STATUS.md).

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

2026-11-15: M3 scoped case correlation and bounded diagnostic evidence implemented with synthetic checkout-contract acceptance; see ADR 0014 and the prepared, unexecuted [activation drill](16-m3-activation-drill.md). Actual sanitized service history and configured live drill remain open; M4 isolation/execution is the next independent milestone.

2026-11-15: PR #9 merged into main `b15361d`. First M4 preparation slice adds frozen source/manifest/runtime metadata and durable denied-admission records with escalation. All real execution remains blocked pending qualified containment; see ADR 0015. Next independent work is the isolated-worker boundary and real adapter protocol, not an override of admission.

2026-11-15: PR #10 merged into main `55bce1b`. M4 controlled worker qualification adds measured synthetic probes, immutable policy/provenance, durable loss/late-output handling and explicit quarantine reconciliation. The local native attempt failed closed; full worker/runtime/credential admission and repair-to-PR remain pending. See ADR 0016.

2026-11-15: PR #11 merged into main `823aa2c`. Durable fake adapter trials implement frozen envelopes, start/event/result/cancel/reconcile commands, response-loss recovery and offline deadline escalation. Real adapter dispatch, successful native qualification and repair-to-PR exit criteria remain unfulfilled; see ADR 0017.

2026-11-15: Independent M5 recovery/follow-through records implemented atop open PR #12 while physical M4 execution remains blocked. Operator reports are explicitly distinct from authenticated telemetry. Postmortem/accepted knowledge and actual production integrations remain open; see ADR 0018 and STATUS.

2026-11-15: M5 operator postmortem/knowledge records, independent attributed acceptance, scoped retrieval and frozen fake-worker briefings implemented atop PR #13. Actual mitigation/deployment/telemetry and Hallmark compatibility remain unverified; physical M4 worker and real harness execution remain blocked. See ADR 0019.

2026-11-15: Native coordinator test bundles add an independent portion of M7 distribution/M8 operator testing while real M4 execution remains blocked. Existing CI generates, packages and smoke-checks the demo on each native OS; broader compatibility/signing/installers and operational hardening remain open. See ADR 0020.

2026-11-16: Hallmark JSON format compatibility implements bounded immutable imports, byte-preserving re-export and independently mapped native knowledge export/provenance (ADR 0022). This closes the M5 format-adapter item; production integrations, known-error/problem linkage and historical acceptance remain open.

2026-11-17: M5 controlled runbook gateway dispatch/reconciliation, deployment/telemetry collection and problem/known-error review records implemented (ADR 0029). Existing recovery/learning gates remain governing. Physical gateway target/stop/rollback qualification, actual history and live activation remain blocked; M6 merge authority is the next independent development slice.

2026-11-17: M6 separate policy, sensitive-change denial, exact publication/candidate/head/base/tree gates, final checks/approvals/protection, bounded retry and no-replay merge reconciliation implemented (ADR 0030). Native transport is present; dedicated protected-repository live acceptance remains blocked. Development PRs remain unmerged.

2026-11-20: M7 independent Datadog current-monitor polling/reconciliation, configuration/read namespaces, source validity and common incident timers implemented (ADR 0031). Broader Datadog capabilities are explicitly unadvertised. Live account/history activation remains blocked; second harness, capability routing/platform cancellation and M8 remain development work.

2026-11-20: A second fixed text harness adapter (Claude Code 2.1.329), independent named profile currency, common role schemas and cross-harness adoption/review fixtures implemented (ADR 0032). Actual local no-account CLI compatibility passed; independent native cgroup CI and live account/model acceptance remain distinct gates. Native worker capability routing/platform drills, Hallmark repair spike and M8 remain open.

2026-11-20: Local native worker routing, immutable dispatch ownership, profile/platform filtering and original-worker cleanup implemented (ADR 0033). Linux physical CI drills use named routes; native macOS/Windows workers and remote transport remain unfinished and are not replaced by coordinator package checks.

2026-11-20: Optional Hallmark repair spike completed at actual source/CLI 0.3.18/15ee84d (ADR 0034). Keep knowledge exchange; the full repair loop is not selected. No live/repair execution or native admission was inferred. Native platform workers and M8 remain required development.

2026-11-20: M8 scoped factory pause/stop, original-worker cleanup integration, mutation reservation fencing, health/tick/clock visibility and CLI/API/web controls implemented (ADR 0035). Capacity/backpressure, backup/restore, retention/redaction, remote RBAC, full workspace and replay/operating drills remain development work. M7 native macOS/Windows workers and remote transport remain open.

2026-11-20: M8 transactional capacity, evidence byte accounting, finite supervisor reserve, pinned global/repository/service resource concurrency and API backpressure implemented (ADR 0036). Backup/restore, retention/redaction, remote RBAC, fuller workspace/replay evaluation and operating drills remain development work; native macOS/Windows workers and remote transport remain open.

2026-11-21: M8 consistent private backup creation and read-only verification implemented (ADR 0037). The database snapshot includes committed WAL records and complete declared frozen source/candidate files, with bounded manifests and visible metadata audit. Authority-safe restore, retention/redaction, remote RBAC, fuller workspace/replay and operating drills remain open; backup verification is not restored authority or live integration proof.

2026-11-21: M8 isolated backup restore/migration recovery implemented (ADR 0038), including old authority/handle fences, no-replay reconciliation, fresh reminder review, notification pacing and repeated immutable artifact relocation. Retention/redaction, remote RBAC, fuller workspace/replay evaluation and broader operating drills remain development work; native macOS/Windows/remote workers remain open. Native same-host restore isolation requires final-head CI independently of controller fixtures.
