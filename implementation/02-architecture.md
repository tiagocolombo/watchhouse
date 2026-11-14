# Architecture

## Proposed shape

A modular application with a Rust coordinator/CLI, SQLite persistence, an artifact store, a web API, and a browser application. These technology choices are proposals; domain contracts must not depend on them.

Start as one deployable coordinator process with internal modules. Use worker processes for execution. Avoid introducing a distributed service per role.

~~~mermaid
flowchart TD
    Source["Observability and other sources"] --> Intake["Authenticated durable intake"]
    Intake --> Cases["Cases, incidents, evidence"]
    Cases --> Workflow["Workflow and policy"]
    Workflow --> Team["Assignments and communication"]
    Team --> Runtime["Execution adapters and workers"]
    Runtime --> Cases
    Workflow --> Delivery["Repository and delivery adapters"]
    Delivery --> Outcomes["Deployment and recovery checks"]
    Outcomes --> Cases
    Cases --> Knowledge["Reviewed operational knowledge"]
    Cases --> Web["Web interface and CLI"]
~~~

## Ownership

| Module | Owns |
| --- | --- |
| Intake | Delivery authentication, deduplication, normalization, reconciliation cursors |
| Case service | Correlation proposals, dispositions, episodes, links |
| Incident service | Severity, command, objectives, timeline, recovery |
| Team service | Role assignments, messages, challenges, handoffs, escalation |
| Workflow engine | Runnable jobs, dependencies, transitions, timers, retries |
| Policy engine | Versioned authorization and eligibility decisions |
| Evidence service | Immutable artifacts, provenance, access scope, retention |
| Execution service | Adapter capabilities, runs, budgets, leases, worker routing |
| Delivery service | PR/CI/merge state and uncertain-effect reconciliation |
| Knowledge service | Accepted lessons, runbooks, known errors, provenance |
| API and UI | Authenticated queries and commands through application services |

Only application commands mutate domain state. Adapters return observations and results; they do not directly edit authoritative tables.

## Persistence

Use one canonical transactional store for domain records and append-only domain events. Write a state transition, its event, and any outgoing job/message into the same transaction. Avoid independently writable JSON journals and SQL state.

Use an outbox for effects and an inbox/delivery table for source deduplication. Persist artifacts by content digest outside the database; reference only completed artifacts. Orphan cleanup must be safe after crashes.

Suggested aggregate concurrency: version check on command, bounded transaction, incremented aggregate version. Events contain schema versions. Domain event time, source-observed time, and ingestion time are separate.

SQLite is a single-coordinator starting point, not a shared network database. Remote workers communicate through the API. Distributed coordinators require a later storage/lease ADR.

## Jobs and effects

Jobs are at-least-once. Use leases, heartbeats, attempt IDs, fencing tokens, and idempotency keys. A worker result cannot advance state after its lease or assignment has been superseded.

Exactly-once execution is not assumed. Before retrying a PR creation, merge, or operational action with an uncertain result, query its authoritative external state. If reconciliation cannot determine the outcome, hold the action and escalate.

Fencing at the database does not prevent an old subprocess from editing files or making external calls. Terminate/quarantine it, revoke effect credentials, and isolate attempts before transferring mutable work.

## Deployments

- Local mode: coordinator, database, artifacts, and web UI on one machine.
- Connected mode: durable webhook intake/relay accepts deliveries while workers are offline; coordinator consumes them through an authenticated channel.
- Remote-worker mode: coordinator dispatches bounded work to registered workers through authenticated outbound worker connections.
- Polling mode: source adapters reconcile where webhooks are unavailable or incomplete.

A relay must durably retain deliveries before acknowledging them and make replay/deduplication explicit. It must not become a second incident brain.

## Platforms

Keep process spawning, cancellation, path handling, service installation, and secret-store access behind platform interfaces. No dependence on a particular shell.

Advertise worker labels such as macOS, Xcode, Android SDK, Linux container, repository access, and isolation profile. Route Apple builds to qualified macOS workers. A cross-platform coordinator does not make all builds portable.

## Failure containment

A failed adapter, unavailable model, or held incident must not stop unrelated incidents. Per-repository work limits prevent conflicting repair attempts. Protect shared testing ports, caches, and fixture services with explicit resource leases.

The deterministic supervisor owns deadlines and escalation even when all execution backends are unavailable. See [team communication](05-team-communication.md) and [operations](13-operations.md).
