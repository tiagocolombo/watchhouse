# Web interface and application API

## Product surface

The web app is the maintenance team's operating workspace. It must show actual persisted state, evidence, ownership, and next obligations. An editorial summary is optional and never the sole operational record.

CLI and browser use the same application commands and query contracts. Closing the browser cannot stop processing. Render projections from persisted records and stream changes with replayable cursors.

## Views

| View | Required information and actions |
| --- | --- |
| Operations overview | Service health, source lag, active incidents, priority, assigned owners, overdue obligations, factory health |
| Case queue | Signals, correlation, classification rationale, impact/uncertainty, monitoring/reopening rules |
| Incident workspace | Brief, command structure, objectives, timeline, investigation threads, decisions, mitigations, recovery |
| Team | Responsibilities, assignments, actual execution status, blockers, dependencies, handoffs |
| Investigation | Hypotheses, supporting/counterevidence, experiments, next discriminating questions, challenges |
| Change delivery | Base/head, diff, reproduction, checks, review, PR/CI state, eligibility reasons, deployment |
| Escalation inbox | Trigger, urgency, accountable owner, delivered/acknowledged state, deadlines, fallback, requested decision |
| Review and follow-up | Postmortem versions, review objections, actions, owners, deadlines, completion evidence |
| Knowledge | Runbooks, lessons, known errors, source evidence, proposals, acceptance, contradictions |
| Administration | Policies, adapters/profiles, secrets references, worker capabilities, budgets, audit, pause/stop |

For a stuck agent, show what stopped it, the last useful progress, work attempted, who was notified, who owns the response, acknowledgment deadline, and automatic next action.

Show reported, observed, inferred, and unknown facts distinctly. Distinguish agent-generated prose from actual check output. Avoid model token streaming as the only indication of progress.

## Operator actions

Declare/reclassify incident; assign/handoff; ask a question; open/resolve a challenge; approve/deny a scoped action; request evidence; retry/cancel; propose/accept a disposition; pause/resume; review a postmortem or lesson.

Each mutation supplies actor, expected aggregate version, reason, and idempotency key where appropriate. Return structured conflict/errors and valid next actions. A stale browser cannot overwrite newer decisions.

## Proposed API shape

Versioned HTTP JSON endpoints for cases, incidents, assignments, threads/messages, challenges, escalations, changes, artifacts, actions, knowledge, workers, runs, policies, and health.

Commands express domain intentions rather than unrestricted row updates. Example intentions: accept-assignment, report-blocked, accept-handoff, assess-recovery, evaluate-merge, accept-postmortem.

Use cursor-based timeline/event streaming, with snapshot plus resume cursor. Recover dropped connections without losing or duplicating rendered events. Expose pagination and filter boundaries.

Artifact downloads check authorization and sensitivity. Never expose secrets or raw credentials through configuration endpoints. Audit access to sensitive incident material.

## Access model

Start with loopback-only binding and an authenticated local session; protect state-changing requests against CSRF and unsafe origins. Remote access requires explicit configuration, authentication, transport security, and roles.

Operator, observer, service-owner, and worker capabilities are separate. A worker cannot impersonate an operator or read unrelated incidents. Do not treat loopback network location as sufficient authorization.

## Proposed UI stack

A TypeScript browser application is a candidate; framework and packaging remain open. Prefer bundled static assets served by the coordinator for the first release. The application must remain useful with no model available.

Acceptance requires browser-level checks for the incident timeline, stale-action conflicts, stuck-worker takeover, and provenance navigation. Every status should link to evidence, policy reason, and the next responsible action.

## Implemented local interface (first M1 slice)

The current branded UI provides token entry, manual intake, newest 200 cases, case ownership/next obligation, assignment status, synthetic result, evidence inspection, and a live/replayable activity timeline. Title updates demonstrate audited expected-version conflicts. M1 adds evidence/qualification/reconsideration controls below; other operating controls remain planned.

| Method | Implemented endpoint | Meaning |
| --- | --- | --- |
| GET / POST | /api/v1/cases | List / create via CreateCase |
| GET | /api/v1/cases/{id} | Consistent snapshot, assignment, results, complete history and resume cursor |
| POST | /api/v1/cases/{id}/rename | RenameCase with actor/reason/key/expected_version |
| POST | /api/v1/cases/{id}/evidence | RecordEvidence, attributed operator observation |
| POST | /api/v1/cases/{id}/dispositions | RecordDisposition, owned decision plus review obligation |
| POST | /api/v1/cases/{id}/reconsider | ReconsiderCase, current decision plus explicit trigger/evidence |
| GET | /api/v1/cases/{id}/events | SSE replay after cursor; Last-Event-ID overrides after query |
| GET | /api/v1/evidence/{digest} | Authenticated persisted fake-result JSON |

See [v1 contracts](../contracts/v1/README.md) for payloads. Command validation errors return 400 invalid_command; command-key/version/fence conflicts return 409 conflict, missing records 404 not_found. Unknown internal/persistence errors return 500 internal_error without exposing database detail. Framework rejection of malformed query/body limits may use its native errors.

Binding remains loopback-only. Explicit bearer auth, Host/Origin checks, CSP, and inert text rendering establish the local session boundary. Optional configured remote roles now require the explicit trusted HTTPS proxy contract below; there are no cookies. The token remains in browser tab sessionStorage and is supplied as an Authorization header, including fetch-based SSE. Snapshots/stream frames are deduplicated by event ID. [ADR 0003](adrs/0003-local-api-and-timeline.md) describes reconnection and current pagination limits. Closing the browser cannot stop the polling fake worker.

The M1 qualification UI displays current/prior decisions, scope, reason, uncertainty, evidence choices/provenance, recurrence count/threshold, original deadline, trigger time, and accountable pending/due obligations. A disposition is distinct from the unchanged case execution state. Form retries retain the command key and expected version after uncertain network responses; server rejections require a fresh user submission. Bodies remain bounded and all case text is rendered inertly. The qualification snapshot is read within the same transaction as the timeline/cursor. Browser tests cover offline deadline processing and replay without duplicate history.

## Implemented first M2 operating surface

The browser shows coordination state, current executor/owner, heartbeat/progress/deadlines, block help/checkpoint/uncertainty/escalation path, pending/accepted/expired handoffs with frozen briefing limits, local inbox acknowledgment, directed messages/acks, and unaccepted late evidence. Operator message/handoff forms use the shared application services and retain draft version checks. Team clocks, worker discovery, and transfer logic remain in Rust.

| Method | Endpoint | Purpose |
| --- | --- | --- |
| POST | /api/v1/cases/{id}/team/messages | Typed directed operator message |
| POST | /api/v1/cases/{id}/team/message-ack | Listed recipient acknowledgment |
| POST | /api/v1/cases/{id}/team/handoffs | Offer a bounded handoff |
| POST | /api/v1/cases/{id}/team/accept-handoff | Addressed fake successor accepts |
| POST | /api/v1/cases/{id}/team/escalation-ack | Local operator acknowledges inbox entry |
| POST | /api/v1/team/claim | Idempotent local fake claim packet |
| POST | /api/v1/team/heartbeat | Current proof heartbeat |
| POST | /api/v1/team/progress | Distinct reported checkpoint |
| POST | /api/v1/team/block | Current proof reports missing evidence/capability |
| POST | /api/v1/team/finish | Fake result from stored task/proof |
| GET | /api/v1/team/escalations | Newest 200 local escalation records |

All endpoints retain loopback bearer/Host/Origin boundaries. These local operator simulation APIs do not authenticate independent remote workers. The ordinary snapshot omits fences. See [team contracts](../contracts/v1/team/README.md) for nested command metadata, idempotency, role limits and attribution.

## Implemented configured human access

[Access v1](../contracts/v1/access/README.md) implements global observer/operator/admin identities, `/api/v1/session`, OS-only configuration, strict proxy transport assertions, actor binding, revocation, bounded audit and role-aware controls. It does not authenticate the legacy fake-worker simulator remotely. Scoped service-owner data visibility is implemented below; independent remote worker access remains required subsequent development. The self-signed localhost TLS browser fixture is not production TLS/account activation.

## Implemented scoped service-owner workspace

ADR 0040 extends [Access v1](../contracts/v1/access/README.md) with exact OS service/environment and case grants, a filtered queue, case/evidence/target authorization, scope-aware SSE and shared filtered snapshot/qualification exports. Service owners retain granted case commands but do not request global configuration/health or create unclassified manual intake. Authorized case content includes its retained references/prose; references do not authorize unrelated incident reads. Independent remote workers remain separate development.

## Implemented missing-evidence visibility

Case snapshots expose `evidence_redactions`; evidence reads return an explicit tombstone instead of an apparently valid body. The browser shows inert removal reasons, retained-history scope, owned reassessment and the resulting case pause. Removal itself is OS-only. See [Retention v1](../contracts/v1/retention/README.md); other copied/derived content is a separate unfinished scope.
