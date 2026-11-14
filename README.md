# Watchhouse

**An agent-operated maintenance, incident response, and SRE team.**

Watchhouse turns operational signals into coordinated investigations, evidence-backed decisions, verified fixes, and lasting improvements.

Connect an observability tool such as Sentry or Datadog. Watchhouse assesses what happened and whether action is needed. When a response is warranted, it assigns agents to investigate, challenge findings, mitigate within authorized limits, implement repairs, test, review, and submit changes. Eligible changes can merge automatically under configured policy.

The team keeps the records a real incident organization needs: incident details, investigation logs, escalation and handoff records, change plans, recovery assessments, postmortems, corrective actions, and runbooks.

**Status: design and implementation planning.** This repository currently contains the specification; the capabilities below describe the intended product. There is no released application or install command yet.

## A typical response

1. Sentry reports errors after a release.
2. Triage correlates the signal with existing cases and checks the affected service, release, and users.
3. Expected behavior is catalogued. Uncertain or apparently transient issues receive a monitoring decision with explicit reopening conditions.
4. For an actionable incident, a commander coordinates investigators and assigns work.
5. Investigators exchange evidence and challenge hypotheses. A verifier reproduces the failure.
6. An engineer prepares a fix. Independent verification and review assess the exact candidate.
7. The release workflow opens a PR and may merge it if all configured gates and repository requirements pass.
8. Deployment and service behavior are checked separately. The incident receives a recovery assessment, a postmortem when required, and tracked follow-up actions.
9. Accepted lessons inform future responses.

If an agent is stuck, Watchhouse records the blocker, notifies another responder, transfers work safely, or escalates to a person. The task always has an accountable owner.

## The brain lives here

Watchhouse owns the incident process, shared state, assignments, communication, evidence, policy, and learning.

Claude Code, Codex, Hermes, pi, and other harnesses are replaceable execution backends. They provide agent execution and model access using their supported authentication and configuration. Watchhouse does not need to copy their subscription credentials.

A role can use a different harness or model without changing the incident workflow. Runtime capabilities and subscription availability must be checked; support is not assumed from a product name.

## A team you can see

The planned web interface shows:

- Service health, incoming cases, active incidents, and priorities.
- Agent assignments, conversations, challenges, decisions, and handoffs.
- Reproductions, diffs, checks, reviews, PRs, deployment, and recovery evidence.
- Blockers, escalation deadlines, and requests for human judgment.
- Postmortems, corrective actions, runbooks, and accepted lessons.
- Policies, budgets, runtime health, and pause controls.

Every status should explain why it is true, who owns the next action, and what happens next.

## Built on explicit boundaries

- Observability providers are integrations; their grouping is not the factory's entire data model.
- Classification and authorization are separate decisions.
- Incident recovery, permanent repair, deployment, and follow-up have separate lifecycles.
- Agents collaborate through durable shared records, with independent challenge and verification.
- Agents cannot approve their own changes or expand their own authority.
- Timers, notifications, leases, budgets, and escalation work without an available model.
- Formal operational artifacts are authoritative. An editorial summary is optional.

## Relationship to Hallmark

[Hallmark](https://github.com/tiagocolombo/hallmark) is a related local development and review system. Watchhouse is a separate product.

Hallmark may serve as an optional repair backend. Its lessons format is a candidate for knowledge compatibility. Watchhouse must remain usable without Hallmark installed.

## Continue the work

Start with the [implementation guide](implementation/README.md), then the [current status](implementation/STATUS.md) and [roadmap](implementation/11-roadmap.md).

The [agent instructions](AGENTS.md) explain how to continue across sessions. The implementation documents preserve requirements, proposed architecture, interfaces, acceptance scenarios, and unresolved decisions without relying on chat history.
