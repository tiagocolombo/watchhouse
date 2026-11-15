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

## Investigation and communication records

`case record-incident-artifact CASE_ID --file FILE` / `POST .../incident/artifacts` use the same `command` envelope with an `action` tagged by `type`:

- `hypothesis`: `id`/`expected_revision` (null for new), `claim`, `scope`, `state`, `supporting`, `counterevidence`, `uncertainty`, `next_experiment`, `experiment_id`. Proposed/testing/inconclusive interpretations preserve unknowns. Supported/refuted requires a completed discriminating experiment on the exact unchanged claim/scope revision and its evidence. Changing a claim requires a new experiment.
- `plan_experiment`: `hypothesis_id`, `hypothesis_revision`, `question`, `method`, `environment`, `expected_outcomes`, `authorization_scope`, `risk`, `due_seconds`. This records a read-only proposal and an owned evidence duty; it runs nothing and grants no authority.
- `finish_experiment`: `id`, `expected_revision`, `outcome` (supported/refuted/inconclusive), `evidence_refs`, `interpretation`, `uncertainty`. Failed attempts and prior versions remain durable; results cannot be silently overwritten.
- `draft_communication`: `id`/`expected_revision`, `audience`, `body`, `evidence_refs`, `review_seconds`. Only the current communications owner can draft. Each revision creates a bounded review obligation and clears approval for the new revision.
- `approve_communication`: `id`, `revision`, `evidence_refs`. Current commander/operations owner, distinct from author, must review the exact current draft under unchanged incident/source context with no blocking challenge. Approval is bound conservatively to the case frame and becomes stale on subsequent events. It is not permission or a receipt for sending.

Actor labels remain local trusted attribution. Role handoffs provide a distinct author/reviewer when one operator initially owns every role. Hypotheses/experiments require commander or operations responsibility; an experiment's accountable owner can report its result. All evidence must belong to the case. History rows are immutable, revisions and case versions serialize writes, and receipts make retries safe. Pending/overdue experiment or review duties prevent assessment closure. Late completion/approval materializes any missed deadline but never resolves its escalation automatically.

Snapshots and the web show latest artifacts and approval currency; full revision events remain in the timeline. `case investigation-markdown CASE_ID` and the incident Markdown report render the hypothesis register, experiments and unsent drafts. Imported Markdown/HTML remains inert. At most the newest 200 artifact heads are included in a snapshot; pagination remains future work.
