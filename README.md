# Watchhouse

<img src="assets/logo.png" alt="Watchhouse logo" width="640">

**An agent-operated maintenance, incident response, and SRE team.**

Watchhouse turns operational signals into coordinated investigations, evidence-backed decisions, verified fixes, and lasting improvements.

Connect an observability tool such as Sentry or Datadog. Watchhouse assesses what happened and whether action is needed. When a response is warranted, it assigns agents to investigate, challenge findings, mitigate within authorized limits, implement repairs, test, review, and submit changes. Eligible changes can merge automatically under configured policy.

The team keeps the records a real incident organization needs: incident details, investigation logs, escalation and handoff records, change plans, recovery assessments, postmortems, corrective actions, and runbooks.

**Status: M0 foundations and the first M1 slice run locally.** Manual case creation → durable events → queued fake investigation → persisted evidence/result → live web timeline works. The broader team described below remains the intended product; no real harness, production integration, or merge automation is implemented.

## Run the local slice

Prerequisites: Rust via rustup (the repository pins 1.90.0), Node.js 22+, and npm. No model account or subscription is needed.

```sh
npm ci --prefix web
npm run build --prefix web
cargo run --locked -- serve
```

Open `http://127.0.0.1:8787` and enter the operator token printed in your terminal. Create a case; the coordinator processes its fake assignment even if you close the browser. Reopen the case to inspect the persisted result and timeline. Results are explicitly synthetic, and the case remains awaiting operator review.

CLI uses the same services and database (default `.watchhouse/watchhouse.db`):

```sh
cargo run --locked -- case create --title "Checkout timeout" --key intake-001
cargo run --locked -- worker --once
cargo run --locked -- case list
cargo run --locked -- case show CASE_ID
```

The key makes command retries safe. `worker --once` is useful without `serve`; `serve` already runs the worker. Use `--db PATH` to select another database, `migrate` to validate/apply migrations, and `serve --port PORT` for another loopback port. Set `WATCHHOUSE_TOKEN` (at least 32 bytes) for a stable local operator token; otherwise each server start generates one. Local CLI access relies on operating-system access to the database.

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --locked
cd web
npx playwright install chromium
npm test
```

Browser tests use only a temporary local database and fake execution. Standard checks never invoke a live harness or production system. Linux was validated locally; CI also configures Rust checks on macOS and Windows. Full M1 dispositions/reopening, M2 communication/supervision/escalation, and real execution are still ahead. See [current status](implementation/STATUS.md) for evidence and the exact next task.

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

[Icon and logo assets](assets/README.md).
