# Current status

Last updated: 2026-11-15.

## Main and current phase

PRs [#2](https://github.com/tiagocolombo/watchhouse/pull/2)–[#9](https://github.com/tiagocolombo/watchhouse/pull/9) are merged. PR #9 was the only open PR when this phase started; its current-head Linux/macOS/Windows CI was green and it merged without conflicts. Main baseline: `b15361d`.

Current branch `dev/m4-execution-admission` starts at that main. This is the first **M4 preparation slice**, not the complete repair-to-PR workflow. [ADR 0015](adrs/0015-repair-inputs-and-denied-admission.md) and the [repair contract](../contracts/v1/repair/README.md) define executable scope. M3 actual sanitized service-history acceptance and live activation remain incomplete; they do not prevent this independent M4 preparation work.

## Completed through M3

M0/M1 and bounded fake-only M2 mechanisms remain implemented: durable cases/events/evidence, qualification/reconsideration, trusted pinned policy, fake-worker messages/supervision/checkpoints/accepted handoffs, independent challenges and validation gates, human notification fallback/retry/acknowledgment/explicit resolution. M3 adds authenticated Sentry intake/enrichment/reconciliation/known-issue freshness, incident declaration/versioned updates/roles, hypothesis/experiment/draft records and independent draft review, bounded diagnostic evidence and reversible scoped case correlations. These retain prior tests and historical migrations/events. Actual sources, destinations and real customer history are unverified.

## First M4 slice implemented

- Immutable OS-operator-only repair-source configuration with explicit repository/full base commit, separate absolute source/snapshot roots, pinned runtime binary digest and bounded file/byte/probe/preparation limits. There is no agent/HTTP installation endpoint or execution override. New versions do not rewrite old inputs or requests.
- Object-only repository preparation reads committed Git objects with clean caller environment, disabled lazy fetch/replacement refs and fixed bounded commands. No checkout/filter/hook/build/test executes. Regular blobs are checked independently against their SHA-1/SHA-256 Git identity and a SHA-256 manifest; paths/modes/sizes/base/tree are frozen. Symlinks/gitlinks, metadata/traversal/device aliases, case collisions and oversized input are rejected. No `.git`, untracked file or publishing credential is copied.
- The private base copy has read-only files as an integrity convention, **not** a security sandbox. Integrity verification detects missing/extra/altered files and occurs outside the canonical writer lock. Crashes can leave unregistered temporary directories, which have no admission authority; robust quarantine/cleanup and sealing must precede worker execution.
- Bounded version/help-only inspection from fresh home/config with no inherited account secrets records binary/output digests, runtime identity, advertised flags and errors. Pin mismatch is not invoked. Verified capability list remains empty. Authentication, subscriptions, model/usage, process-tree cancellation and actual filesystem/network isolation are not verified by help output.
- Evidence-backed operator repair requests freeze the case policy, configuration/manifest/base/tree and expected behavior/regression proposal. Source cases require exact explicit repository/deployed-revision mapping. Blocking challenges, missing/foreign evidence, forged actor/version and stale commands fail closed. Every currently accepted request is **blocked**, owned and escalated atomically with its event/receipt. It never enters the fake-worker queue. Fake completion and escalation acknowledgment cannot complete a repair.
- CLI/API/web expose repair readiness/denials/owner/next action, exact command retries and escaped preparation Markdown. Snapshot shows the newest 100 requests; full event history remains authoritative.
- `examples/generate_gallery.rs` produces actual incident/investigation/qualification Markdown, structured JSON and a consistent separate demo database from sanitized synthetic fixtures. Operator review screenshots and sample exports were generated from the working interface. No production payloads or generated databases are committed.

## Verification actually run

Linux, 2026-11-15:

| Check | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed |
| `cargo clippy --locked --all-targets -- -D warnings` | Passed |
| `cargo test --locked` | 98 Rust tests passed |
| `npm run build --prefix web` | Strict TypeScript passed |
| `cargo build --locked` | Bundled binary passed |
| `npm test --prefix web` | Six Chromium scenarios passed |
| `git diff --check` / local Markdown links | Passed |

New coverage includes exact committed bytes despite dirty working trees/filters, no Git metadata/untracked secrets, object corruption, portable paths/unsafe modes/size limits, caller-environment clearing, probe output bounds/deadline, immutable/sequential config, denied runtime pins, source/base mismatch, snapshot drift, independent challenges, retries/restart, concurrent requests, rollback across event/escalation/receipt, shared HTTP/CLI and inert Markdown/browser reload/mobile. Standard tests use synthetic local repositories and harmless Git version output as an unrecognized runtime; no model service or live notification/source is invoked.

Additional local metadata-only observation: the pinned installed `codex-cli 0.167.0-alpha.4` returned version/help from a clean environment. Advertised flags included `--json`, `--output-schema`, `--ignore-user-config`, `--ignore-rules`, `--ephemeral`; verified capabilities stayed empty. Preparation froze 135 files, 2,093,570 bytes at tree `872d11a8abb1513037bfb8fce9d49240c05d9b31` from merged main. No account/authentication/subscription or model run was probed. A Bubblewrap qualification attempt failed at NETLINK_ROUTE setup with Operation not permitted, so this host has not demonstrated qualified containment. This is not successful real execution.

Remote CI must be checked for the exact PR head separately; local checks do not imply remote success.

## Remaining work and activation blockers

- **M4 next:** qualify a real OS/container/worker boundary for filesystem/network restrictions, trusted instructions outside candidate control, scoped credentials/revocation, bounded resources, process-tree cancellation and lost-worker reconciliation. Then implement the real Codex start/events/cancel/result/reconcile adapter, candidate freezing, base/candidate regression evidence, independent verification/review and idempotent/reconciled PR delivery. No real worker is admitted by this slice on any platform, even if a CLI advertises sandbox flags. M4 exit criteria are not met.
- **M3:** operator-supplied actual sanitized service history, exact service/release mappings, Sentry app/token/plan permissions, trusted ingress and authorized Slack destinations/secrets/participants are still absent. Additional disposition categories, richer impact windows and cross-service correlation remain planned. Follow the prepared [activation drill](16-m3-activation-drill.md) only with actual configuration/authority; synthetic reports do not establish live coverage/delivery.
- **M5–M8:** authorized mitigation, deployment/recovery verification, postmortems, accepted knowledge/actions, automatic merging, second source/harness, packaging, remote RBAC, backup/restore, retention/redaction and backpressure remain ahead. Repository PR merges by the coding assistant are not Watchhouse automatic merging. Source status, copy success, an accepted hypothesis, a link or an unsent draft never establish recovery.
- Current local actor labels remain trusted single-operator attribution. A hostile process with canonical database/OS access is outside this boundary. Quarantine cleanup, filesystem sealing, host hardening/clock jumps, installers/services, retention and license remain open. No production effect, real repair or verified service recovery is claimed.

## Exact next step

Review this M4 preparation slice against main. Qualify the isolated-worker boundary before adding any real execution path; preserve deterministic fake-only CI and the distinct proposal/execution, candidate/review, merge/deployment and recovery lifecycles. M3 history/activation can proceed independently when the required operator inputs exist.
