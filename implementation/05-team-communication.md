# Team, communication, and escalation

## Responsibilities

| Role | Accountable output | Boundary |
| --- | --- | --- |
| Incident commander | Response objectives, assignments, priority/severity decisions, recovery decision | Cannot expand configured authority |
| Operations/SRE lead | Coordinated mitigation plan and observed service outcome | Only scoped authorized operational actions |
| Investigator/specialist | Evidence-backed hypotheses and experiments | No unsupported causal assertions |
| Maintenance engineer | Candidate patch and implementation rationale | No self-approval or publishing credentials |
| Verification engineer | Independently observed reproduction/check results | Cannot merely repeat engineer claims |
| Reviewer/challenger | Findings on the exact candidate/claim and resolved objections | No permission to weaken gates |
| Communications lead | Audience-specific updates, next update time, delivery evidence | Drafting and sending are separate permissions |
| Scribe | Actual timeline, decisions, assignments, artifact links | Cannot turn speculation into fact |
| Release operator | Eligible PR/merge operations, deployment tracking | Authority determined by policy |
| Knowledge steward | Reviewed postmortem, lessons, runbook/action proposals | Cannot silently promote operational policy |

Roles are persistent responsibilities. Sessions are disposable executions. One person or agent can hold several compatible responsibilities; an author cannot independently verify or approve its own candidate. Fresh context and distinct authority are required; different models are optional.

## Shared communication

Each incident has an authoritative workspace and timeline, with focused threads for investigations and changes. Messages are durable records, not transient chat.

Message types: observation, hypothesis, question, assignment, challenge, evidence, decision, handoff, and status update. Include sender role/run, incident/case, recipients, referenced versions, reply links, acknowledgment requirement, and deadline.

Target recipients and subscriptions. Do not broadcast every token to every agent. Only new relevant information, outstanding obligations, and bounded context enter briefings. Rate-limit automatic replies and deduplicate notifications.

Important decisions from side threads must link back to the main incident record. Chat cannot grant capabilities or mutate authoritative state directly; commands validate authority and preconditions.

## Challenge protocol

A challenge identifies the contested claim/artifact version, counterevidence or uncertainty, requested validation, accountable recipient, and deadline.

States: open, acknowledged, investigating, answered, resolved, escalated. A proposed answer is not automatic resolution. The designated independent reviewer or policy-defined arbiter records the disposition.

Resolve disagreement through discriminating evidence, bounded expert arbitration, or human judgment. Agreement count is not a correctness signal. Track dissent and residual risk. Time-box challenge cycles; a higher-token model is not an automatic solution.

## Assignment lifecycle

Proposed -> accepted -> running -> completed, with explicit blocked, cancelled, superseded, and failed outcomes. The dispatcher can assign accountability before an executor accepts; the response owner remains responsible until handoff succeeds.

Track deadline, last heartbeat, last meaningful progress, attempts, dependencies, and budgets. Useful progress means new evidence, an experiment result, an artifact revision, or a completed action, not activity volume.

## Detecting stuck work

Use both explicit reports and deterministic supervision:

- Process exit, lost worker, missed heartbeat, execution deadline.
- Authentication/quota outage or interactive approval request.
- Missing dependency, access, evidence, or tool.
- Repeated equivalent failures without new evidence.
- Progress deadline exceeded despite healthy heartbeats.
- Unresolved challenge or ownership acknowledgment overdue.

Capture a block record containing reason code, work attempted, evidence, checkpoint, required assistance, responsible owner, and deadline. Timers and notifications must not depend on an LLM.

## Recovery and safe handoff

1. Record the block and notify the commander and relevant responder.
2. Evaluate an allowed bounded retry, revised approach, specialist assignment, runtime fallback, or authority escalation.
3. Stop or quarantine the old execution and revoke effect credentials. Confirm mutable work is not concurrently writable.
4. Preserve the patch/checkpoint; assign a new isolated attempt with a newer fence.
5. Deliver objective, evidence, failed hypotheses, open challenges, workspace/commit references, blocker, limits, and remaining budget.
6. Require acceptance. Keep the prior accountable owner until transfer is accepted.
7. Reject late authoritative results from the superseded attempt; retain them as attributed evidence.

If an external mutation may have happened, reconcile before retrying or transferring that mutation. Never assume process termination rolled it back.

## Escalation

| Path | Trigger | Destination |
| --- | --- | --- |
| Functional | Specialist knowledge/tooling required | Configured specialist role or service owner |
| Coordination | Conflicting work, priorities, or stalled response | Commander/deputy |
| Severity | Impact increases or recovery stalls | Stronger response profile and owner |
| Authority | Action exceeds capabilities | Authorized human or preconfigured approval role |
| Dependency | External system/vendor prevents progress | Dependency owner and tracked external request |
| Human judgment | Contradictory evidence, exhausted options, risk decision | Named human escalation chain |

Each escalation has acknowledgment and response deadlines, fallback recipients, urgency, delivery status, and a required resolution. Acknowledgment is not resolution. Reassignment cannot reset the overall incident deadline or budget without a recorded authorization.

If nobody acknowledges, advance the configured chain and keep the incident visibly blocked. Send external messages only through explicitly configured notification authority.

## Fairness and overload

Prioritize active impact and overdue obligations. Bound concurrency, reply depth, and attempts. A flood of source events must not starve recovery, escalation, or existing incidents. Support deputy takeover when the commander itself fails.
