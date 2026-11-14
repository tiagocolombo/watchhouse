# Operational artifact templates

Templates describe required structure; they are not completed incident records.

| Template | Use |
| --- | --- |
| [Incident record](incident.md) | Declaration, living timeline, impact, response and recovery |
| [Investigation](investigation.md) | Hypotheses, experiments, evidence, challenges |
| [Escalation and handoff](escalation-handoff.md) | Blocked work, notifications, transfer, authority requests |
| [Change and mitigation](change-mitigation.md) | Operational action or code-change plan, validation, rollout/rollback |
| [Postmortem](postmortem.md) | Reviewed blameless analysis and corrective-action register |

## Generation rules

Persist structured records and stable IDs first; render documents from versioned snapshots. Updating a document creates a new version and retains the history. Reference exact evidence, candidate, policy, and knowledge versions.

The system populates actual timestamps, owners, actions, command results, and delivery status. Agents draft interpretation and explanation. Keep observation, hypothesis, decision, and uncertainty distinguishable.

Use unknown, not measured, not applicable with reason, or pending collection where appropriate. Never fill missing numbers or causes to make a document look complete. Redact sensitive evidence according to audience.

Response profiles determine mandatory sections and closure gates. A concise maintenance disposition may omit major-incident sections with explicit applicability; it must still retain rationale and reconsideration conditions.

Each generated artifact carries artifact ID/version, related record IDs, owner/reviewer, created/updated times, source snapshot/evidence manifest, sensitivity, and current review status. A summary cannot overwrite its source records.
