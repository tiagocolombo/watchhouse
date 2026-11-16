# Incident recovery and follow-through v1

CLI `watchhouse case recovery CASE_ID --file command.json`, authenticated POST `/api/v1/cases/{id}/recovery`, and the web's versioned command form share one service. `case recovery-markdown` and GET `/api/v1/cases/{id}/recovery/markdown` render the actual records. Start from [plan.json](plan.json); replace expected version/key for each new command.

All commands use `{ "command": { "meta": { "schema_version": 1, "idempotency_key": "...", "actor": "operator", "reason": "..." }, "expected_version": 12 }, "action": {...} }`. Keys replay historical receipts; changed payloads conflict. Actors are local single-operator attribution, not independently authenticated remote users. Real production execution is not admitted.

## Recovery

| Action | Fields / rule |
| --- | --- |
| `plan` | `criteria`, `review_seconds`; current commander only |
| `observe` | `plan_id`, `window`; operations/commander; active unexpired exact-context plan |
| `review` | `observation_id`, `evidence_refs`; distinct from plan/observation authors; current latest met window |
| `resolve` | `review_id`, `evidence_refs`; current commander; current independently reviewed met window, no blocking challenges |
| `reopen` | `evidence_refs`; commander; retain history and require new plan |

Criteria: workflow, positive minimum traffic/window, maximum error parts per million, p95 latency and source lag. Deadline is bounded by case-pinned policy. Observation window has `started_at`, `ended_at`, requests/errors, nullable p95, complete-coverage flag, source lag, collection method, uncertainty and case-bound evidence references. Its interval begins after the plan and ends no later than now. Counts/intervals are bounded and errors cannot exceed requests.

Evaluation is deterministic: inadequate traffic/window, absent latency, incomplete coverage or excess lag is **inconclusive**. With sufficient coverage, both targets passing is **met**, both failing **ineffective**, otherwise **partial**. Integer arithmetic avoids floating point rates/overflow. A later window replaces the active observation and invalidates its review. New plans bind the current source digest and incident operating revision; source/incident drift makes existing review unusable. Drift or a blocking challenge after resolution reopens assessment under supervision. Plan review deadlines escalate without models.

Observations and review require operator-reported evidence. Review requires the reviewer's own new, distinct report after submission, checked by logical case version as well as timestamp; equal clocks cannot make an older report count as new. These records represent **operator-reported recovery with independent attributed review**, not telemetry independently collected/authenticated by Watchhouse. Numeric inputs remain claims. No source silence, merge, deployment, successful fake run or prose claim establishes recovery automatically.

Resolution is independent of repair, postmortem and corrective actions. Current resolved response suppresses the recurring active-response update duty; pending role offers and all other obligations continue. Existing update escalations are not automatically resolved. Declaration records remain immutable; reports distinguish declaration context and current recovery.

## Corrective actions

`open_action`: title, category (`prevent`, `detect`, `mitigate`, `resilience`, `test`, `runbook`, `process`), owner, distinct verifier, acceptance criteria, bounded `due_seconds`. Commander opens it.

`submit_action`: id, expected `revision`, evidence_refs; only owner; state becomes submitted. `verify_action`: same identity/revision plus new evidence; only assigned verifier, distinct from owner, with their own report collected after submission and disjoint from submission evidence. Blocking challenges prevent verification. `waive_action`: id/revision, residual_risk, evidence_refs; commander only, clear challenges; waiver is not verification. Terminal actions cannot be rewritten.

Overdue open/submitted actions escalate once, including after incident resolution. A late submission/verification first preserves the missed duty. Action transitions and their immutable records commit with case events and command receipts. Display caps never control acceptance checks.

## Mitigation and deployment

`propose_mitigation`: runbook, target, steps, preconditions, stop_conditions, rollback, risk, evidence_refs. It records **blocked_authority**, `executed:false`, and opens an owned escalation. It runs nothing and exposes no execute/override switch. A scoped production executor remains a separate integration.

`record_deployment`: repository, revision (full commit ID), environment, observed_at, evidence_refs. Operations/commander reports it against the explicitly mapped incident repository/environment. A report neither changes the source mapping nor marks response recovered. Merge and deployment remain separate facts. Deployment-provider verification is not implemented.

Migration 015 preserves append-only recovery records and separate current state/actions. Restart, rollback, deadlines, stale context, evidence provenance/independence and UI replay are verified by fixtures. See [ADR 0018](../../../implementation/adrs/0018-recovery-and-follow-through.md).
