# Current status

Last updated: 2026-11-14.

## Repository reality

M0 foundations and the requested smallest M1 vertical slice are implemented:

**Manual case → committed state/events/job → claimed fake investigation → persisted result/evidence → visible replayable web timeline.**

Development branch: `dev/m0-m1-durable-slice`, based on `19fdcf2`. The implementation commit on this branch includes this handoff. No real harness, production integration, incident automation, or merge workflow exists. M1 as a whole is not complete; M2–M8 remain planned.

## Implemented

- Rust 1.90.0 coordinator/CLI, shared Application services, separate domain/time/contracts, SQLite infrastructure, deterministic fake source/execution adapter, and controllable clock.
- Bundled strict TypeScript UI using the existing Watchhouse logo/icon. Manual intake, queue, case/assignment ownership/status/next obligation, synthetic results, evidence inspection, timeline, and audited title changes work.
- Checksummed transactional migration 001; version-1 strict command/event/case/assignment/result contracts and compatibility fixtures. Dependency lockfiles and CI for Linux/macOS/Windows Rust checks; Chromium acceptance on Linux.
- SQLite WAL/FULL/foreign keys; atomic state/event/job/command-receipt writes. The queue is a local transactional outbox, with no external message delivery implementation.
- Stable global command keys, original-response replay, mismatched-key conflicts, optimistic aggregate versions, serialized concurrent commands, and exclusive work claims.
- Persisted attempts, 30-second fake leases, fresh fences on expired claims, rejection of late/misattributed results, idempotent result completion, append-only case history, and inline content-addressed evidence with synthetic provenance.
- Consistent snapshot plus cursor and durable SSE replay, explicit bearer auth on API/evidence, loopback binding, Host/Origin checks, CSP, bounded bodies, and inert rendering of untrusted text.
- Per-case operator remains accountable; completed fake investigation leaves the case **awaiting_review**, with uncertainty explicit. It never claims real checks, independent verification, a disposition, deployment, or recovery.
- Bounded M0 protocol spike: local Codex version/help plus primary Claude Code/pi/Hermes documentation. No live model/account/subscription check. Codex exec is the first M4 candidate, not a supported adapter.

## Accepted decisions

- [ADR 0001](adrs/0001-local-stack.md): P01/P05, Rust plus bundled framework-free TypeScript.
- [ADR 0002](adrs/0002-transactional-local-store.md): P02, transactional SQLite state/events/jobs/receipts; inline bounded JSON evidence by digest; aggregate version and lease/fence rules.
- [ADR 0003](adrs/0003-local-api-and-timeline.md): local bearer session, shared services, durable snapshot/SSE replay.
- [ADR 0004](adrs/0004-execution-protocol-spike.md): fake-only admission and first real-adapter candidate; production protocol remains gated.

## Validation actually run

On Linux, 2026-11-14:

| Command / check | Result |
| --- | --- |
| `npm install --prefix web` | Dependency lockfile created; no runtime web dependencies |
| `npm run build --prefix web` | Strict TypeScript compilation passed |
| `cargo fmt --all -- --check` | Passed |
| `cargo clippy --locked --all-targets -- -D warnings` | Passed |
| `cargo test --locked` | 12 tests passed: 1 CLI, 2 HTTP, 9 workflow (including crash-child helper); doc tests passed |
| `cargo build --locked` | Binary and bundled assets compiled |
| `npx --prefix web playwright install chromium` | Installed Chromium; primary CDN download failed, fallback succeeded |
| `npm test --prefix web` | Chromium end-to-end scenario passed, including offline replay/reload, evidence inspection, inert script-like input, and stale browser update rejection |
| Desktop/mobile screenshot inspection | Reviewed 1280px and 390px layouts; no horizontal overflow |
| `git diff --check` and local Markdown checks | Passed for whitespace, final newlines, balanced fences, and relative references |

Recovery tests exit a child process without destructors after intake commit, claim commit, adapter execution, and completion commit. A new application opens the same SQLite file, recovers pending/expired work, retains one accepted result, and never loses accepted intake. SQL-fault tests separately prove rollback during intake, claim, and completion (including evidence). Concurrent tests exercise eight identical intake commands, two competing versioned edits, and eight work claimers. Expired/superseded and incorrectly attributed results cannot advance state. HTTP tests replay from Last-Event-ID after a snapshot/connection gap. The CLI test runs distinct processes against the same database. Standard tests use no live models, subscriptions, or production systems.

The CI workflow is configured; local evidence above is Linux only. Remote CI execution and macOS/Windows behavior must be checked separately, and no distribution/portability claim is made yet.

## How to run

From repository root:

```sh
npm ci --prefix web
npm run build --prefix web
cargo run --locked -- serve
```

Open http://127.0.0.1:8787 and enter the terminal's local operator token. `serve` processes fake jobs without a browser. Alternatively use `case create --title "Example" --key example-001`, `worker --once`, `case list`, and `case show CASE_ID` through `cargo run --locked --`. `--db PATH` selects another database. See the root README for check commands and token configuration.

## Unresolved issues and limits

- Full M1 dispositions and reopening are unimplemented. A synthetic insufficient-evidence result is not an accepted disposition.
- Only one queued investigator assignment per case is currently supported. Durable team messages, challenge/independence gates, handoff/checkpoints, progress/heartbeat supervision, retry budgets, escalation/notification chains, and formal incident artifact rendering are M2+ work. Lease/fencing tests are foundations, not proof of that whole system.
- Fake execution is in-process, immediate, and side-effect-free. Its auto-retry path must never execute real mutable work; real harness probe/cancel/reconcile, isolation, authority, and external-effect reconciliation remain required.
- General policy evaluation, pause/stop, production/repository effects, accepted knowledge, and remote-worker protocols are absent. Fixed fake-only policy cannot be expanded by any endpoint.
- Local bearer session only; no remote access/RBAC. Actor strings are operator attribution. Generated tokens change on restart; configuring WATCHHOUSE_TOKEN retains the local session credential. CLI access trusts OS database permissions.
- Queue shows newest 200 cases; initial snapshots include all history. History pagination, backpressure, retention/redaction, backup/restore, and clock-jump handling remain open. A worker/storage error is surfaced rather than handled by a full supervisor.
- Evidence is bounded synthetic JSON inside SQLite. External/large artifact storage needs its own completed-write and recovery lifecycle. No real telemetry or private transcripts are retained.
- No project license was invented. Resolve distribution/license policy before public release.

## Exact next task

**Complete the remaining M1 disposition/reopening slice before starting M2.**

1. Add a new migration and versioned `record-disposition` / `reconsider-case` commands through Application, preserving existing v1 history and command receipts.
2. Require operator actor/reason, expected version/key, evidence digests, applicability scope, explicit uncertainty, accountable owner, and reconsideration conditions. Expected behavior needs review/expiry; apparently transient needs a monitoring window/recovery indicators; insufficient evidence needs a concrete collection obligation.
3. Define bounded, configurable review/monitoring timers and deterministic reopening with the fake clock/source. Record decisions and resulting obligations/events atomically; reopening must not imply repair or recovery.
4. Expose those records/actions in both CLI and web and verify expected/transient/uncertain examples, stale/duplicate commands, timer restart recovery, and timeline replay. Update the contracts, roadmap, and this status with actual evidence.
5. Then begin M2 durable messages/challenges, real stuck-work supervision/handoff, and configured human escalation before any operational real-agent execution.

No missing owner decision blocks this next local implementation task. Human escalation destinations, authority grants, production configuration, and real-runtime compatibility remain decisions for their documented milestone, not permissions inferred from this slice.
