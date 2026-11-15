# Decisions and open choices

This is the initial lightweight decision register. Split substantial future ADRs into their own files when needed and link them here. Do not rewrite historical decisions without a supersession record.

## Confirmed product decisions

| ID | Decision | Basis |
| --- | --- | --- |
| D01 | Watchhouse is a separate product; Hallmark integration is optional. | Owner selected separate integration. |
| D02 | Watchhouse owns the brain: processes, policy, assignments, communication, evidence, knowledge. | Owner explicitly required this. |
| D03 | Harnesses/model providers are replaceable; supported harness subscription access can be reused. | Owner named Hermes, pi, Claude Code, Codex, and other backends. |
| D04 | Observability is pluggable, including Sentry and Datadog. | Owner explicitly broadened integrations. |
| D05 | Operate a complete maintenance/incident/SRE team with formal artifacts and collaboration. | Owner required company-style incident processes and mutual challenge. |
| D06 | Detect stuck agents, notify another responder, recover/reassign, or escalate. | Owner explicitly required this. |
| D07 | Include advanced web visibility and operational controls. | Owner required a fuller team interface than hall web. |
| D08 | Support policy-controlled automatic merging of eligible fixes. | Owner selected automatic eligible merging. |
| D09 | Preserve lessons by default; evaluate Hallmark compatibility. | Owner requested lessons learned and reuse. |
| D10 | Preserve the plan as README plus implementation Markdown for future sessions. | Owner requested repository-backed continuity. |
| D11 | Working product/repository name is Watchhouse. | Name proposed; owner created tiagocolombo/watchhouse. |

## Proposed implementation decisions

| ID | Proposal | Rationale / validation needed |
| --- | --- | --- |
| P01 | Rust coordinator and CLI, initially one process. | Similar distribution goals to Hallmark; validate developer ergonomics and packaging. |
| P02 | SQLite canonical state/events/outbox plus content-addressed artifacts. | Transactional local durability; prove restore and concurrent-command behavior. |
| P03 | Persisted role assignments and typed communication. | Durable teamwork across sessions/harnesses; prove bounded messaging and challenge handling. |
| P04 | Deterministic supervisor independent of model availability. | Escalation and authority enforcement survive provider failure. |
| P05 | Bundled TypeScript web application with versioned HTTP API. | Shared CLI/web application services; choose framework after minimal UI spike. |
| P06 | Built-in knowledge plus Hallmark-format compatibility first. | No required Hallmark install; investigate shared package only after boundaries stabilize. |
| P07 | Fake runtime first, one real adapter next, second adapter before portability claims. | Fast tests and early abstraction pressure. |
| P08 | Explicit incident profiles and independent lifecycle records. | Proportional process without conflating recovery, repair, and follow-up. |

These are implementation recommendations, not owner-approved technology commitments. A contributor may resolve routine choices with an ADR and evidence, preserving confirmed product decisions.

## Accepted implementation ADRs

| ADR | Resolution |
| --- | --- |
| [0001](adrs/0001-local-stack.md) | P01/P05: Rust 1.90 coordinator/CLI, bundled framework-free TypeScript UI |
| [0002](adrs/0002-transactional-local-store.md) | P02: SQLite state/events/jobs/receipts; bounded evidence inline by digest; aggregate versions and fencing |
| [0003](adrs/0003-local-api-and-timeline.md) | P05: loopback bearer session, shared application services, snapshot plus durable SSE replay |
| [0004](adrs/0004-execution-protocol-spike.md) | P07/M0 protocol spike: fake-only admission; Codex exec is the first real-adapter candidate, unverified until M4 |
| [0005](adrs/0005-qualification-and-reconsideration.md) | M1: separate qualification lifecycle, operator-reported evidence, atomic review obligations and deterministic reconsideration |
| [0006](adrs/0006-durable-fake-team-coordination.md) | First M2 slice: directed messages, deterministic supervision, owned blockers, acknowledged handoff, late evidence, local inbox; supersedes silent expired-fake retries |

These resolve implementation proposals within the owner's authorized scope. They do not change confirmed product decisions. P03/P04/P06/P08 remain wider design proposals; partial assignment/lease foundations do not complete them.

## Open decisions and when to resolve

| Question | Resolve by |
| --- | --- |
| Final stack, minimum versions, UI framework, package layout | Resolved: ADR 0001 |
| First real runtime, installed version, protocol, auth/capability coverage | M0 spike: ADR 0004; full installed-version admission and implementation M4 |
| Initial service/repository and sanitized historical fixtures | M3 |
| Sentry plan features and webhook/enrichment permissions | M3 |
| Numeric severity profiles and default timers | M3; configurable before real response |
| Human escalation channel and recipient chain | M2 before unattended use |
| Deployment mapping and recovery indicators | M5 |
| Sensitive paths, allowed risk classes, merge strategy and required approvals | M6 |
| Hallmark evidence, authorization, and knowledge compatibility | M5/M7 |
| Relay hosting/retention and remote-worker trust model | Before connected/remote deployment |
| Project license and distribution policy | Before public release; do not invent a license |
| Export/report formats beyond Markdown/JSON | After artifact contracts and user needs are established |

## Requirement traceability

| Requirements | Primary design | Initial acceptance |
| --- | --- | --- |
| R01–R02 | Integrations, incident process | V01–V06 |
| R03–R05 | Architecture, execution, policy | V09–V12, V24 |
| R06 | Architecture, execution | V30 and OS contract suite |
| R07–R08 | Incident process, team communication | V13, V15, V18, V21 |
| R09 | Team communication, operations | V07–V11, V25 |
| R10 | Templates, incident process | V18–V22 |
| R11 | Web/API | V26 plus browser acceptance |
| R12 | Knowledge | V02, V23 |
| R13 | Integrations | Hallmark contract suite, M7 |
| R14 | Policy, integrations | V16–V17 plus M6 gates |
| R15 | Guide, STATUS, AGENTS | Documentation link/state checks |

## Change record

2026-11-14: initial product and implementation baseline captured from the owner's requirements. No application capability is yet implemented.

2026-11-14: M0/M1 slice implementation accepted ADRs 0001–0004. Evidence and limitations are recorded in STATUS.md.

2026-11-14: remaining M1 qualification/reopening slice implemented under ADR 0005; no real-execution or production authority was added.

2026-11-14: ADR 0006 implements the first bounded M2 core slice; full challenges, external notification chains and real-worker authority remain open.

2026-11-14: [ADR 0007](adrs/0007-independent-challenges.md) implements independent fake review and version-bound assessment gates.

2026-11-14: [ADR 0008](adrs/0008-trusted-policy.md) replaces scattered development timer/budget constants with immutable case-pinned operator policy.

2026-11-14: [ADR 0009](adrs/0009-human-escalation.md) adds explicit human delivery/fallback/acknowledgment/resolution, with uncertainty reconciliation.

2026-11-15: [ADR 0010](adrs/0010-first-sentry-incident-slice.md) implements the first authenticated Sentry/enrichment/reconciliation/incident slice. Live activation and broader M3/M5 workflows remain separate.

2026-11-15: [ADR 0011](adrs/0011-versioned-incident-operations.md) implements versioned incident operations, accepted role handoffs and recurring update duties.

2026-11-15: [ADR 0012](adrs/0012-investigations-and-draft-review.md) implements versioned investigations and independent, context-bound communication draft review.

2026-11-15: [ADR 0013](adrs/0013-source-freshness-and-known-issue-recovery.md) adds fenced source reads, known-issue recovery beyond discovery windows and separate freshness tracking.
