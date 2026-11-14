# Domain and event contracts

These are proposed logical contracts. Implement versioned schemas and compatibility fixtures before treating examples as a public API.

## Records

| Entity | Required identity and key fields |
| --- | --- |
| Service | ID, owner, repositories, environments, dependencies, critical workflows, runbook references |
| Signal | ID, source/installation/project, external identity, observed/ingested timestamps, type, source severity, evidence references |
| Case | ID, affected services, disposition, impact/uncertainty, owner, source links, version |
| Episode | ID, case, environment/release scope, start/end observations, recurrence links |
| Incident | ID, linked cases, severity/rationale, commander, response profile, objectives, status, recovery criteria |
| Hypothesis | ID, incident/case, claim, state, supporting/contradicting evidence, next discriminating experiment |
| Assignment | ID, role/assignee, objective, owner, state, deadline, dependencies, capabilities, budget |
| Run | ID, assignment/attempt, runtime/profile/version, requested and actual model, capability snapshot, lease/fence, result |
| Message | ID, thread, sender role/run, recipients, type, reply links, evidence/artifact versions, acknowledgment |
| Challenge | ID, contested claim/artifact version, reason, requested evidence, owner, deadline, disposition |
| Escalation | ID, trigger, urgency, recipient chain, acknowledgment deadline, response deadline, resolution |
| Handoff | ID, previous/new owner, checkpoint, mutable resources, lease transfer, acceptance |
| Evidence | ID/digest, provenance, collection method, observed time, scope, sensitivity, integrity/retention |
| Change candidate | ID, work item, repository, base/head SHA, tree, diff digest, checks, review, authorization |
| Operational action | ID, plan/version, target scope, authority, preconditions, executor, rollback, actual result |
| Deployment | ID, service/environment, artifact/release, included changes, observed status, verification |
| Postmortem | ID/version, incident, owner, reviewed analysis, timeline/evidence, action links |
| Action item | ID, incident/problem, accountable owner, priority, deadline, acceptance and verification evidence |
| Knowledge entry | ID/namespace/version, kind, scope, provenance, status, validity, contradictory evidence |

Use globally unique stable IDs internally. Human-friendly INC-..., CASE-..., and ACT-... identifiers are display handles. External IDs are namespaced by source and installation.

## Relationships

Signals can link to several cases when evidence is ambiguous. Cases can link to several source issues. Correlation changes retain history; merge/split decisions must be reversible.

Episodes distinguish recurrence from ongoing activity. Link known errors and underlying problem records across incidents without erasing each incident's impact and timeline.

A change may address several cases; a case may need several changes. Track candidate supersession explicitly. A deployment can contain several changes and have an inconclusive recovery result.

## Event envelope

~~~json
{
  "event_id": "evt_example",
  "schema_version": 1,
  "aggregate_type": "incident",
  "aggregate_id": "inc_example",
  "aggregate_version": 8,
  "type": "assignment.blocked",
  "recorded_at": "2026-11-14T17:00:00Z",
  "occurred_at": null,
  "actor": {"kind": "supervisor", "id": "coordinator"},
  "correlation_id": "inc_example",
  "causation_id": "evt_previous",
  "policy_version": "policy_digest",
  "payload": {
    "assignment_id": "asn_example",
    "reason": "evidence_unavailable",
    "evidence_refs": []
  }
}
~~~

Envelope fields: event identity; schema and aggregate versions; actor; timestamps; correlation/causation; applicable policy; typed payload. Null occurrence time means unknown, not recording time.

Event families include signal.received, case.classified, incident.declared, assignment.created/accepted/progressed/blocked, hypothesis.proposed/refuted, challenge.opened/resolved, escalation.opened/acknowledged, handoff.accepted, run.started/failed, candidate.frozen, check.completed, review.recorded, merge.observed, deployment.observed, recovery.assessed, postmortem.accepted, action.verified, knowledge.accepted.

## Evidence and decisions

A claim contains a statement, confidence/uncertainty, evidence references, counterevidence, and scope. Confidence is not permission. Record observations separately from interpretations.

Decision records include alternatives considered, chosen action, rationale, authority, inputs and their versions, dissent, and reconsideration conditions. Replaying a decision uses the same snapshots, not today's mutable source state.

Check evidence includes exact command or runner identity, environment, candidate/base identifiers, start/end time, exit/result, and output digest. Distinguish model-reported checks from checks the execution system actually observed.

## Retention and redaction

Immutable means an artifact's content does not silently change. Authorized deletion/redaction creates an audit tombstone and invalidates dependent evidence where necessary; it does not require retaining secret or personal bytes forever.

Role briefings are immutable manifests of the evidence, messages, knowledge versions, task instructions, and policy scope supplied to a run. Preserve reproducibility without storing hidden model reasoning.
