# Current status

Last updated: 2026-11-14.

## Phase 1: independent challenges

Branch `dev/m2-independent-challenges`, based on main `a744d35`; first branch of the four-phase stack. [ADR 0007](adrs/0007-independent-challenges.md) records the boundary.

Implemented strict challenge commands, exact contested event/version references, separate fake challenger/respondent/verifier responsibilities, acknowledgment/investigation/evidence-backed answer/independent resolution, durable dissent and uncertainty, deadline escalation, and deterministic blocking gates. New independent evidence is mandatory for arbitration. Result acceptance and qualification reject unresolved blocking challenges. Validation and assessment closure bind to the exact case version, exclude pending review obligations, and do not assert service recovery. CLI/API/web expose the same commands and persisted records.

Verification on Linux: `cargo fmt --all -- --check`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo test --locked` (46 passed), `cargo build --locked`, `npm run build --prefix web`, `npm test --prefix web` (4 Chromium scenarios), and `git diff --check` passed. New checks cover independent evidence, deadline/restart deduplication, competing commands, injected rollback, stale closure and browser command/reload. No live models, notifications, or production system was used.

Next: trusted versioned timer and execution policy, then external escalation delivery, then Sentry intake and incident records. Real worker identity/isolation and production execution remain M4+ work. Local fake role labels are attribution under one trusted operator, not remote authentication.

Final phase 1 review: overdue (due) review obligations continue blocking assessment closure; regression test added. Reserved challenger/verifier identities cannot take investigator work, and any historical investigator attribution disqualifies self-validation. The browser test now waits for persisted connection state before reload, addressing the CI race. All 46 Rust tests and four Chromium scenarios passed locally with the correction.

## Baseline evidence and limits


Last updated: 2026-11-14.

## Repository reality

**M0/M1 and the first bounded M2 core coordination slice are implemented. Full M2 is unfinished.**

The native Rust coordinator owns state, supervision, communication and transfer. A fake investigator can claim work, report an owned blocker/checkpoint, receive a directed reply, and be replaced by an addressed successor that accepts the frozen briefing before ownership changes. CLI workers execute this path without HTTP. The web app observes the same persisted state and submits validated operator commands.

Development branch: `dev/m2-team-handoff`, based on `4fcfc16` from `dev/m1-dispositions`. Implementation commit: `0adf9d8`; the final documentation commit records its verified CI handoff. No real harness, production system, external notification, independent challenge/verification gate, or automatic merge exists.

## Implemented

- Existing M0/M1 case creation, strict contracts, SQLite state/events/jobs/receipts, case-bound evidence, scoped dispositions, deterministic reopening, HTTP/CLI services, bearer boundary and replayable branded UI are retained.
- Migration 003 adds case coordination threads, typed directed messages and recipient acknowledgments, assignment coordination state, immutable run proof/briefing, checkpoints, owned blocks, handoffs, progress deduplication, a local escalation inbox, and unaccepted late-result references. Migrations 001/002, old event bytes, and command receipts remain unchanged. Active schema-2 runs upgrade with their current fence and original start-based budget.
- Provider-independent domain/team records and Rust team application services own the new behavior; team/store maps projections. Snapshot adds a defaulted team projection. Existing Case/Assignment execution fields stay compatible; control exposes queued/running/blocked/handoff_pending/completed coordination state.
- Typed observation/question/status messages have sender, local recipients, case/thread/reply links, evidence, referenced version, time, and optional acknowledgment deadline. Body text cannot grant capabilities. Block/handoff notices commit only through their corresponding commands. Reply depth, text size, recipient count and briefing message count are bounded.
- Workers use current run/fence/executor plus lease/progress/absolute deadlines. Heartbeats do not count as useful progress. New checkpoint digests are required for reported progress; reusing one cannot reset the deadline. Semantic proof of useful progress is still future work.
- Missed heartbeat/progress/absolute deadlines and explicit missing-evidence/capability reports create a blocker, immutable checkpoint and directed message in one transaction, retaining the current accountable owner and a response deadline/local escalation path. The old proof loses all authoritative mutation rights.
- Handoff offers preserve objective, investigator role, checkpoint/evidence, latest relevant message IDs, original absolute deadline and shared remaining attempts. The previous owner remains accountable until the addressed successor explicitly accepts; acceptance/new run/fence/owner transfer/receipt commit together. Fake worker processes discover their offers without browser orchestration.
- Known late output is retained once per run/digest as wrapped unaccepted evidence and an audit event; it cannot complete or change the current workflow. Unknown/forged proof is rejected without invented attribution. Accepted and rejected artifact provenance remain distinct.
- Deterministic team supervision detects stalled-but-alive fake work, overdue block response, missed handoff acceptance, and missing message acknowledgment. It writes deduplicated owned records into the local operator inbox, without models or external messaging. Inbox acknowledgment is not resolution, recipient delivery, or ownership transfer.
- Headless CLI adds `team` commands, `worker --identity`, deterministic `--block` and `--stall` modes. `serve --manual-workers` keeps supervision/API alive while separate workers dispatch; `tick` evaluates both qualification and team deadlines. Worker version contention is retryable; infrastructure errors remain surfaced.
- Web shows executor/owner/control state, heartbeat/progress/deadlines, blockers/checkpoints/uncertainty, frozen handoff limits, local inbox/acks, directed messages, and unaccepted late evidence. Operator message/handoff drafts retain their original version; all text is rendered inertly. The browser never owns scheduling or run authority.

## Material decision

[ADR 0006](adrs/0006-durable-fake-team-coordination.md) partially implements P03/P04. It **supersedes the M0 automatic expired-fake retry path** from ADR 0002: lost attempts now become owned blocks and require accepted transfer. Recovery tests were strengthened to cover that change while preserving every accepted case/obligation and rejecting stale results. Historical run.lease_expired events remain readable.

Development-only fake limits: 30-second lease, 60-second progress deadline, 120-second block response window, 300-second assignment wall time, and three total attempts; migrated histories receive at least one remaining attempt. Heartbeats/progress/handoffs cannot extend the original deadline or reset attempts. These are test-profile defaults, not organization severity policy. General trusted policy configuration is still open.

[Earlier ADRs](14-decisions.md) cover the stack, transactional persistence, replay/session boundary, fake-only adapter admission, and M1 qualification. No real runtime authority was added.

## Validation actually run

Linux, 2026-11-14:

| Command/check | Result |
| --- | --- |
| `npm run build --prefix web` | Strict TypeScript passed |
| `cargo fmt --all -- --check` | Passed |
| `cargo clippy --locked --all-targets -- -D warnings` | Passed |
| `cargo test --locked` | 41 tests passed: 3 CLI, 4 HTTP, 11 qualification, 14 team, 9 original workflow (including three crash-child helpers); doc tests passed |
| `cargo build --locked` | Native binary and bundled assets built |
| `npm test --prefix web` | 3 Chromium scenarios passed across normal and manual-worker coordinators |
| Screenshot/layout inspection | Team/qualification operating view reviewed; 390px overflow checks passed |
| `git diff --check` and local Markdown checks | Whitespace, final newlines, fenced blocks and relative references passed |

New evidence includes a multi-process CLI-only block → offer → successor discovery/acceptance → result flow, healthy heartbeats without useful progress, repeated checkpoints, shared time/attempt budgets, competing handoff/accept/tick commands, wrong/late proof, late artifact deduplication, message acknowledgment vs resolution, schema-2 active-run preservation, and injected rollback of checkpoint/message/block, ownership/acceptance and escalation. Children exit after block, offer, and acceptance; reopening the database preserves the pending obligation or current proof. Browser acceptance disconnects while the core accepts/completes/rejects old output, then replays full history and observes/acknowledges a local overdue-message escalation. Untrusted message text stays inert.

Standard checks use no live models, subscriptions, production systems, external notification destinations, or real mutable workspaces. Previous M1 [CI 43629885681](https://github.com/tiagocolombo/watchhouse/actions/runs/43629885681) passed at `f400e09`; **current [CI 43634045680](https://github.com/tiagocolombo/watchhouse/actions/runs/43634045680) passed at `0adf9d8` on Linux (Ubuntu 24.04), macOS and Windows: strict TypeScript, formatting, Clippy, all 41 Rust tests and native builds. All 3 Chromium scenarios passed on Linux.**

## How to run

```sh
npm ci --prefix web
npm run build --prefix web
cargo run --locked -- serve
```

Default serve runs the core scheduler, initial fake responder, and HTTP interface at http://127.0.0.1:8787. Enter the printed local token; closing the browser cannot stop core work. Use `--db PATH` to select a database and `migrate` to validate/apply schema 3.

Headless teamwork, without starting a web server:

```sh
cargo run --locked -- case create --title "Investigate timeout" --key team-001
cargo run --locked -- worker --block --once
cargo run --locked -- case list
cargo run --locked -- case show CASE_ID
cargo run --locked -- team offer-handoff CASE_ID --file handoff.json
cargo run --locked -- worker --identity fake-specialist --once
cargo run --locked -- team inbox
```

Use [the handoff example](../contracts/v1/team/handoff.json) with the actual block ID/current case version. The successor worker discovers and accepts the persisted offer itself. `worker --stall` simulates healthy heartbeats with no progress; a deterministic block follows. `serve --manual-workers` exposes observation/operator commands and keeps supervision active while separate CLI responders dispatch. `tick` processes due qualification/team work without an executor. [All team contracts and examples](../contracts/v1/team/README.md).

## Limits and unresolved work

- This is a **partial M2** fake-team implementation. Independent challenge/verification state/gates, broader persistent roles, escalation resolution and an externally configured human notification/fallback chain are absent. Local inbox rows never claim external delivery. Acknowledgment is not resolution.
- All local executor identities are fake-* investigators with no external effects or mutable workspace. Database fencing therefore suffices only here. Real subprocess cancellation/quarantine, resource isolation, worker authentication, credential scoping/revocation, and uncertain-effect reconciliation remain mandatory before a real adapter.
- One investigator responsibility per case is retained across attempts; no parallel builder/verifier/reviewer execution or automatic repair is available. New checkpoint bytes are necessary but do not establish semantic useful progress; hard shared limits bound simulated repetition.
- Timer/budget defaults are development-only. Trusted versioned policy configuration, semantic repeat detection, more overload/backpressure, notification retries/fallback and clock-jump handling remain open. A stopped coordinator catches up only on restart; storage faults are surfaced.
- Actor/recipient labels are local attribution under the single trusted operator token/OS database permissions, not remote identities or delivery destinations. Ordinary snapshots omit fences; explicit fake claim/accept commands return proofs. There is no real-worker registration path.
- No real observability source, formal incident artifact renderer, accepted knowledge workflow, production/repository effects, policy editing, pause/stop controls, packaged installer, OS service integration, or new project license has been added.
- History snapshots remain unpaginated, inbox/queue show newest 200 entries, artifacts are bounded inline JSON, and backup/restore/retention/redaction are unimplemented. New event kinds require current clients; old binaries reject schema 3 rather than operating unknown records.

## Exact next task

**Continue M2 with independent challenges and explicit validation/closure gates, still fake-only.**

1. Add versioned challenge commands/records tied to the exact contested claim/event/artifact version, requested evidence, accountable recipient, acknowledgment/response deadlines, and designated independent arbiter.
2. Implement open → acknowledged/investigating → answered → independently resolved or escalated; a proposed answer or author agreement must never resolve its own blocking challenge. Keep dissent and remaining uncertainty durable.
3. Build separate briefings and role admission for fake investigator/challenger/verifier responsibilities, excluding private transcripts and self-approval. Prevent conflicting completion/acceptance while a required blocking challenge is unresolved; policy remains outside agent-controlled candidate content.
4. Expose challenge evidence/state/next owner through CLI, API and web. Verify changed claim versions, duplicate/concurrent/stale commands, restart/deadline escalation, independent resolution, and timeline replay without live models.
5. Then finish M2 with trusted versioned timer/budget configuration and one explicitly configured human notification channel/recipient fallback chain, verified delivery/acknowledgment/retry and resolution before unattended real-agent execution. Prepare adapters/configuration reviewably before asking for a missing destination/authority; do not infer those from local owner labels.

No missing owner decision blocks the next local fake challenge slice. M3 sources/incident artifacts and M4 real execution still follow the documented dependency/authority boundaries.
