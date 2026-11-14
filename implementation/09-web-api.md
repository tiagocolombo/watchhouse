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
