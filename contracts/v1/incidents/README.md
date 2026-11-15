# Incident operations v1

`case operate-incident CASE_ID --file FILE` and `POST /api/v1/cases/CASE_ID/incident/operations` use `{"command":{"meta":{"schema_version":1,"idempotency_key":"unique","actor":"operator","reason":"Evidence-backed coordination update"},"expected_version":CURRENT},"action":{...}}`.

| Action type | Fields | Authority |
| --- | --- | --- |
| `update` | `severity`, `impact`, `uncertainty`, `objective`, `evidence_refs` | Current commander |
| `offer_role` | `role`, `successor`, `acknowledge_seconds` | Current commander |
| `accept_role` | `offer_id` | Addressed successor before deadline |

Roles: commander, operations, communications, scribe. Update severity uses the incident's frozen source-configuration profiles. New deadlines are scheduled only by an accepted evidence-backed update; handoffs do not reset them. An overdue update is first materialized as an escalation even if an operator updates before a scheduler tick. A new cycle never resolves an old escalation.

Offers keep the prior owner accountable until explicit acceptance. Missing acceptance expires and escalates without changing ownership. Only trusted local human attribution is admitted; fake executor role labels cannot take these responsibilities. Actor labels remain attribution under the existing single operator-token/OS boundary, not remote user authentication.

Snapshots expose current operations/revision and offers alongside the immutable incident declaration and event history. CLI/API/web share the service. Markdown uses the current brief and roles while retaining the original declaration as a distinct record. Source status and coordination updates do not assert mitigation, deployment or recovery.
