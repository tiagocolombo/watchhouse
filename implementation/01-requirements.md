# Requirements and boundaries

## Product goal

Operate a complete software maintenance, incident response, and SRE team staffed by agents, with explicit processes, evidence, communication, escalation, and operational artifacts.

The target outcome is correctly handled operational problems and improved reliability. PR volume and agent activity are not success measures.

## Confirmed requirements

| ID | Requirement |
| --- | --- |
| R01 | Support multiple observability integrations, beginning with Sentry and extensible to Datadog and others. |
| R02 | Understand, triage, correlate, and catalogue observations; expected, transient, duplicate, unauthorized-request, or uncertain cases need not generate code changes. |
| R03 | Own the brain in Watchhouse: process, assignments, policy, shared state, decisions, and learning. |
| R04 | Execute through interchangeable harnesses including Claude Code, Codex, Hermes, pi, and future backends. Model selection is configurable per role. |
| R05 | Reuse supported harness authentication and subscription configuration without making direct API access mandatory. |
| R06 | Operate across macOS, Windows, and Linux, with platform-specific workers where builds require them. |
| R07 | Provide investigators, maintenance engineers, testing, review, release, incident coordination, escalation, and SRE responsibilities. |
| R08 | Agents communicate, challenge, validate, and hand work to each other through durable shared state. |
| R09 | Detect stuck agents; notify, recover, reassign, or escalate with explicit ownership. |
| R10 | Generate and maintain professional incident records, investigation evidence, mitigation/change plans, postmortems, action items, and runbooks. |
| R11 | Make team activity and operational records visible and actionable in an advanced web interface. |
| R12 | Preserve lessons and use accepted knowledge in future work; evaluate Hallmark compatibility. |
| R13 | Remain a separate product with optional Hallmark integration. |
| R14 | Support automatic merging of eligible fixes under configured policy. |
| R15 | Preserve the implementation plan and continuity in repository Markdown. |

## Behavioral invariants

- A signal is not automatically an incident, and an incident does not automatically require a code patch.
- Expected HTTP 401/403 responses and unauthorized factory actions are different concepts. Status codes alone do not establish expected behavior.
- A claimed transient issue has a monitoring decision and reconsideration conditions.
- A hypothesis is not an established cause. Unknowns remain explicit.
- An agent recommendation cannot expand authority.
- Each active incident, assignment, challenge, escalation, and follow-up action has an accountable owner.
- Independent verification and review reference the exact candidate and relevant environment.
- Recovery, merge, deployment, postmortem acceptance, and corrective-action closure are distinct facts.
- No model is required to run timers, detect missed acknowledgments, notify, enforce permissions, or evaluate deterministic gates.
- External effects are idempotent or reconciled after uncertainty.
- Every important state transition has a reason and supporting evidence or explicit uncertainty.

## Autonomy boundary

Automatic submission and eligible merging are in scope. Automatic deployment, arbitrary production mutation, customer messaging, data deletion, and security incident handling require separate policies and integrations. Merge permission does not imply these permissions.

The system may prepare an operational plan before the required authority exists. It must then block the execution action and escalate while preserving the incident's monitoring and communication duties.

## Success measures

Track time to acknowledge, qualify, mitigate, and recover using explicit timestamps. Track recurrence, ineffective fixes, reopened cases, missed actionable cases, unnecessary repair attempts, escalation acknowledgment, overdue actions, and cost/time per correctly handled case.

Report denominators and observation windows. Low traffic or absent telemetry cannot prove recovery.

## Scope profiles

Support compact maintenance cases and coordinated incidents. Severity and impact choose the response profile and mandatory artifacts; log severity alone does not. A low-volume issue affecting a critical workflow may need a stronger response than a noisy harmless exception.

The first release targets a single trusted operator and one coordinator with optional workers. Multi-tenant SaaS, billing, unrestricted self-modifying policies, model training, and general-purpose agent social chat are not initial goals.
