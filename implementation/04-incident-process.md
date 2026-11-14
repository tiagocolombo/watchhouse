# Incident operating process

## Preparation

Before autonomous response, register service ownership, critical workflows, repository/release mapping, observability access, approved runbooks, severity profiles, escalation recipients, and authority boundaries.

A response profile specifies mandatory artifacts, acknowledgment/update deadlines, mitigation limits, verification windows, postmortem criteria, and follow-up expectations. Timer values are configuration, not embedded assumptions about every company.

## 1. Intake and qualification

Authenticate, deduplicate, persist, and acknowledge source deliveries. Fetch bounded evidence: representative events, frequency, affected users/requests where available, environments, releases, recent changes, dependency state, and source context.

Classify using deterministic rules first and bounded investigation where necessary:

| Disposition | Required record |
| --- | --- |
| Expected behavior | Evidence, applicability scope, expiry/review, reopening triggers |
| Apparently transient | Observation window, recovery indicators, escalation/reopening conditions |
| Duplicate/already addressed | Existing case/change link and correlation rationale |
| External dependency | Dependency owner/status, impact, recovery tracking, resilience investigation if warranted |
| Operational/configuration problem | Correct response workflow and permitted actions |
| Application defect | Accepted hypothesis or evidence gap, repair eligibility |
| Insufficient evidence | Specific collection task or explicit escalation |

Absence of events is not proof of health. Distinguish no traffic, broken instrumentation, delayed ingestion, and recovery. Source classification updates are optional external effects with separate permissions.

## 2. Declare and assess

Declare an incident when coordination, impact, urgency, or policy warrants it. Severity follows measured/estimated impact, criticality, scope, and uncertainty; preserve rationale and revisions. Keep severity separate from repair priority and log level.

Assign a commander, operations lead, communications responsibility, and scribe responsibility. A small response may combine roles where independence is not compromised. Declare objectives and next update time. Unknown impact remains unknown with a collection owner.

A proposed starting profile set:

| Profile | Response |
| --- | --- |
| Maintenance case | Bounded asynchronous qualification/repair; concise disposition and evidence |
| Coordinated incident | Commander, live timeline, explicit recovery, communications cadence, postmortem criteria |
| Major incident | Expanded specialists, stricter escalation/update timers, mandatory postmortem and recovery review |
| Security/data concern | Preserve evidence and route to configured restricted handling; no ordinary autonomous suppression |

Organizations define numeric severities and thresholds. These profile names do not claim universal SEV conventions.

## 3. Investigate and mitigate

Investigators maintain a hypothesis register. Each experiment has a question, expected discriminating results, scope, cost/risk, execution evidence, and interpretation. Retain disproven hypotheses and failed attempts to prevent repeated work.

Prioritize restoring service over finishing root-cause analysis. Mitigation can proceed with an incomplete causal explanation when authorized and justified. Preserve evidence before disruptive actions when feasible.

The operations lead coordinates production mutations so parallel responders do not issue conflicting actions. Every mitigation needs preconditions, exact target scope, expected impact, authority, stop conditions, rollback/recovery plan, and independent outcome assessment.

Unapproved production actions remain proposals. The coordinator escalates authority requests without stopping read-only investigation and monitoring.

## 4. Repair and change control

Create a linked problem/known-error record if an underlying issue outlives incident recovery. Define the intended behavior and validation before implementation.

For reproducible bugs, demonstrate regression failure on the base and success on the candidate. Where this cannot be done, record the alternative validation method and obtain the policy-required exception.

Freeze the candidate for checks and independent review. Review references code, tests, scope, risks, and evidence. Any revision invalidates affected checks/review. Rebase approval carryover requires an explicit, tested rule; default to revalidation until implemented.

Submit PRs and merge only under [policy](10-policy-security.md). Track deployment separately. Emergency changes use a configured expedited path with compensating controls and retrospective review; they do not bypass authority silently.

## 5. Monitor and recover

Define recovery criteria before declaring success: affected workflows, error/latency/saturation or other relevant indicators, traffic sufficiency, observation duration, and remaining risks.

Mark recovery as verified, partial, ineffective, or inconclusive with evidence. The commander can resolve active response after criteria are met while permanent repairs and follow-ups remain open. A recurrence creates or links an episode and reopens the appropriate workflow.

## 6. Review, learn, and follow through

Generate a blameless postmortem when the response profile requires one, or when recurrence/learning value warrants it. Analyze contributing conditions and system/process gaps; do not force a single root cause or invent certainty.

Independent challenge checks the impact calculation, chronology, causal claims, effectiveness of mitigation, and action quality. Postmortem acceptance requires resolved blocking objections and assigned actions.

Actions include prevention, detection, mitigation, resilience, tests, runbooks, and process improvements. Every action has accountable ownership, priority, deadline, acceptance criteria, and verification. Waiving an action requires a recorded decision and residual risk.

Incident resolution, postmortem acceptance, and action completion remain separate states. Report overdue follow-ups after the incident is closed.

## Closure gates

A disposition requires rationale and reconsideration conditions. Incident resolution requires recovery evidence or an authorized risk acceptance. Postmortem acceptance requires reviewed analysis and owned actions. Action completion requires demonstrated acceptance criteria.

See [artifact templates](templates/README.md). The structured records drive document rendering; generating prose does not satisfy a state transition on its own.

## Implemented qualification boundary (M1)

Expected behavior, apparently transient, and insufficient evidence are now explicit operator decisions through shared CLI/web services. Every decision cites case-bound evidence, scope, rationale, uncertainty, owner, reconsideration conditions, a recurrence threshold, and a bounded deadline. Expected/transient decisions require operator-reported supporting observations; fake agent results alone cannot justify them. Operator reports remain distinguished from independently observed checks.

Review/monitoring/collection obligations survive restart. Reaching a deadline, observing a distinct recurrence threshold, reporting contradictory evidence or a scope change, or requesting operator review reopens the decision for accountable reassessment. A new decision supersedes the old record/obligation, retaining history. Reopening creates no repair/deployment authority and never asserts recovery. Other disposition categories and the coordinated incident process above remain planned. See [ADR 0005](adrs/0005-qualification-and-reconsideration.md).
