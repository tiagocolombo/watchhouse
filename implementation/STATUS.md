# Current status

Last updated: 2026-11-14.

## Repository reality

**M0 and the bounded M1 manual/fake workflow are implemented.**

Manual case → atomic events/job → fake investigation → persisted evidence → operator qualification → owned review/monitoring/collection obligation → deadline/recurrence/reconsideration → reopened review, visible in CLI and web.

Current development branch: `dev/m1-dispositions`, based on `3af6c21` from `dev/m0-m1-durable-slice`. This branch's implementation commit includes the updated contracts and handoff. M1 exit scenarios are covered for local sanitized/manual/fake cases; automated sources, real harnesses, independent verification, production actions, and M2–M8 are not implemented.

## Implemented

- Existing Rust 1.90 coordinator/CLI, bundled strict TypeScript UI and Watchhouse branding, shared Application services, fake execution/source, injected clock, SQLite WAL/FULL, durable queue, event replay, command receipts, optimistic versions, attempts/leases/fences, and synthetic-result evidence remain intact.
- Migration 002 adds immutable disposition contents with separate lifecycle, owned obligations, case/evidence links, and recurrence deduplication. It backfills v1 fake evidence links without changing migration 001, old event bytes, or command receipts. Migration history is validated before applying pending SQL; LF checksums and exact legacy CRLF checksums are supported without rewriting history. Future/gapped/modified histories fail explicitly.
- Versioned RecordEvidence records attributed scope/statement/collection method/optional occurrence time/uncertainty. Operator reports remain distinguished from independent check evidence and synthetic results. Digests and case ownership of references are checked before decisions.
- Versioned RecordDisposition supports expected_behavior (expiry review), apparently_transient (monitoring duration and recovery indicators), and insufficient_evidence (concrete collection task/deadline). All require actor/reason, key/version, owner, scope, uncertainty, evidence, reconsideration conditions, and recurrence threshold. Synthetic evidence alone cannot justify expected/transient behavior.
- Decisions create their next obligation in the same transaction as state, events, ownership, and receipt. New decisions supersede prior decisions/obligations, retaining the full history and original contents; supersession is not proof that the old collection task was completed.
- Versioned ReconsiderCase binds an explicit trigger to the current active disposition/version. New recurrence evidence must be operator-reported after the decision and not previously counted/cited. A threshold reopens; operator review, contradictory evidence, or scope change reopen immediately. Same-key retries return the original response; recounting evidence under a new key conflicts.
- Persisted deadlines (explicitly configured per command, 1 second–30 days) reopen review and make the owned obligation due once. Original deadline/trigger time remain visible. `serve`/`worker` tick before claiming fake work; standalone `tick` processes up to 100 due obligations without an executor or model.
- CLI and authenticated HTTP expose the same commands. Snapshot adds a consistent qualification projection with current/prior decisions, obligations, evidence/provenance, and its timeline cursor. Old snapshots deserialize with an empty projection; old Case shapes and receipts remain unchanged. New event kinds require an updated reader; old binaries refuse schema 2.
- Web evidence entry, decision/reconsideration forms, uncertainty/scope/history/deadlines, pending/due ownership, and timeline replay work. Drafts retain their original version while edited; live updates cannot silently make a stale draft current. Uncertain network retries retain the same key/version. Switching cases clears unrelated drafts.

Qualification is separate from execution and recovery. Case execution status remains awaiting_review after the fake run. A reopened decision invokes no new agent, repair, production action, notification, or recovery declaration. Accountable owner strings are trusted-operator attribution, not remote identities/RBAC or a completed M2 handoff protocol.

## Decisions

- [ADR 0005](adrs/0005-qualification-and-reconsideration.md): qualification lifecycle, operator evidence, bounded deterministic reconsideration, atomic obligations, and compatibility.
- [ADR 0001](adrs/0001-local-stack.md): Rust/TypeScript local package.
- [ADR 0002](adrs/0002-transactional-local-store.md): SQLite transactions, queue/evidence/receipts, leases/fencing.
- [ADR 0003](adrs/0003-local-api-and-timeline.md): loopback bearer session and snapshot/SSE replay.
- [ADR 0004](adrs/0004-execution-protocol-spike.md): fake-only admission; real harness compatibility is unverified.

## Validation actually run

Linux, 2026-11-14:

| Command/check | Result |
| --- | --- |
| `npm run build --prefix web` | Strict TypeScript passed |
| `cargo fmt --all -- --check` | Passed |
| `cargo clippy --locked --all-targets -- -D warnings` | Passed |
| `cargo test --locked` | 25 tests passed: 2 CLI, 3 HTTP, 11 qualification, 9 original workflow (including two crash-child helpers); doc tests passed |
| `cargo build --locked` | Binary/bundled assets built |
| `npm test --prefix web` | 2 Chromium scenarios passed: original workflow and full qualification/reopening/offline-deadline/stale-draft flow |
| Screenshot/layout inspection | Branded qualification view reviewed; 390px overflow check passed |
| `git diff --check` and local Markdown checks | Whitespace, final newlines, fences and relative references passed |

New tests include schema-1 upgrade preservation/backfill; exact legacy CRLF checksum preservation; future-schema rejection; strict version/plan fixtures; old snapshot reads; operator evidence requirements; corrupt/foreign references; expected expiry; distinct transient recurrence thresholds; authenticated-401 counterevidence; stale/duplicate/concurrent decisions and ticks; SQL rollback of evidence/decisions/ownership/obligations/timer events; and a real child exit after accepting a deadline. The reopened process catches up once using the fake clock. CLI commands run in separate processes. The browser goes offline while the coordinator reaches a deadline, then reconnects/reloads with complete duplicate-free history. A draft edited before a competing update is explicitly rejected.

The first browser pass exposed an inaccessible select label; explicit label associations fixed it, and the full browser suite passed afterward. Standard tests never invoke live models, subscriptions, production systems, or external message delivery.

Prior baseline [CI 43625938174](https://github.com/tiagocolombo/watchhouse/actions/runs/43625938174) passed Linux/macOS/Windows at `c9553a2`. New branch CI is pending publication/verification; do not infer its cross-platform result from that older run. CI keeps Ubuntu 24.04 for pinned Playwright compatibility and checks all three platforms independently.

## How to run

```sh
npm ci --prefix web
npm run build --prefix web
cargo run --locked -- serve
```

Open http://127.0.0.1:8787 and enter the printed local token. Create a case, wait for the fake result, add operator evidence, and record a scoped disposition/deadline. Expected/transient decisions need operator-reported support; insufficient evidence needs an explicit collection task. Use the reconsideration form for review/recurrence/counterevidence/scope change.

Equivalent CLI commands, using actual case ID, current expected versions and evidence references in the versioned JSON files:

```sh
cargo run --locked -- case record-evidence CASE_ID --file evidence.json
cargo run --locked -- case record-disposition CASE_ID --file disposition.json
cargo run --locked -- case reconsider CASE_ID --file reconsideration.json
cargo run --locked -- tick
cargo run --locked -- case show CASE_ID
```

[Command examples and semantics](../contracts/v1/README.md). `--db PATH` selects the database; `migrate` validates/applies schema 2. `serve`/continuous `worker` process deadlines even with no browser. `worker --once` ticks then processes at most one fake job. WATCHHOUSE_TOKEN retains a local token across restarts; otherwise reconnect using the new generated token.

## Limits and unresolved work

- Local manual qualification only. Conditions/scope are operator-readable criteria, not a natural-language rule engine. Recurrences are explicit operator reports; automatic source normalization/correlation, other disposition categories, incident declaration, recovery proof, and real regression reproduction remain later work.
- M2 durable team messages, roles/acknowledgments, independent challenges, checkpoint/handoff, meaningful progress/heartbeat supervision, retry budgets, escalation inbox/notification chains, and human escalation delivery are absent. Simple qualification timers are not that full supervisor.
- One completed fake investigator assignment per case; qualification reopening deliberately does not create another executor. Real adapter protocol/cancel/reconcile, process isolation, scoped authority, and external-effect reconciliation are still prerequisites to operational agents.
- No formal incident artifact renderer, knowledge acceptance, production/repository effects, general policy editing, pause/stop, or remote-worker/RBAC surface. No project license was invented.
- Loopback bearer session and OS-protected CLI database only. New event kinds require current clients; no public released API compatibility is claimed. Initial snapshots include full history, queue lists newest 200 cases, evidence is bounded inline JSON. Pagination, backpressure, retention/redaction, backup/restore and clock-jump handling remain open.
- Due obligations stay visibly due until explicitly superseded by a new operator decision; no external notification or escalation is sent. An unavailable coordinator processes overdue obligations after restart, not while stopped. Worker/storage failures are surfaced, not yet handled by a full supervisor.

## Exact next task

**Begin the smallest M2 durable communication and acknowledged handoff slice, still using fake workers.**

1. Add versioned typed team messages/threads, assignment acknowledgment and owned block/handoff records using a new migration; preserve all case/qualification history and receipts.
2. Allow one fake investigator to report a block with last useful progress, checkpoint/evidence, required help, accountable owner, response deadline, and escalation path. Record block/message/obligation atomically; show them in CLI and web.
3. Assign a successor with a fresh attempt/fence and require acceptance before transferring accountability. Assemble a bounded durable briefing; retain independence of author/challenger/verifier responsibilities. Reject late results from the old worker and retain their attributed evidence without advancing state.
4. Use the fake clock for missed acknowledgment/progress deadlines and an owned local escalation inbox; test crash/restart, duplicate/concurrent handoff, stalled-but-alive work, late results, and browser replay.
5. Complete the rest of M2 with an explicitly configured human notification channel/recipient fallback chain and verified delivery/acknowledgment timers before unattended real-agent execution. Do not infer that authority or those destinations from local owner labels.

No missing owner decision blocks the next local/fake slice. Human recipients, production permissions, deployment/recovery mappings, and installed real-runtime compatibility remain explicit configuration/admission work at their documented milestone.
