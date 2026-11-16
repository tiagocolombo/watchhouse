# Current status

Last updated: 2026-11-15.

## Main and current slice

PRs [#2](https://github.com/tiagocolombo/watchhouse/pull/2)–[#11](https://github.com/tiagocolombo/watchhouse/pull/11) are merged. PR #11 was checked at exact head `0541b97` with successful Linux/macOS/Windows [CI](https://github.com/tiagocolombo/watchhouse/actions/runs/43778213167), then merged without conflicts into main `823aa2c`.

Current branch: `dev/m4-adapter-lifecycle`. Continues M4 with a durable **synthetic adapter protocol trial**, not real harness dispatch or completion of M4. See [ADR 0017](adrs/0017-durable-adapter-trials.md) and [adapter trials contract](../contracts/v1/adapter-trials/README.md).

## Implemented in this slice

- Case-owner commands start a synthetic trial against an existing frozen repair request. Envelope digest binds source revision/manifest/base, objective/expected behavior/regression plan, evidence references, owner, case-pinned policy, attempt, fresh fence and absolute deadline. Source references are not fresh filesystem validation.
- Start, explicit fake advance/result, cancellation and reconciliation have durable state and ordered per-trial events. Same-key retries inspect current status, never start again or return a stale terminal view. Changed payloads conflict. Reads do not advance the simulator.
- Separate fake backend journal simulates lost start/cancel acknowledgment. Unknown effects block new starts; reconciliation uses the existing handle. Missing backend observations never prove absence. Reconciliation does not automatically resolve escalation or grant capability.
- Fake schema/fence-valid output is explicitly synthetic with unknown model/usage. Exit-success-shaped invalid output and structured fake quota failures preserve evidence and escalate. Foreign/stale/schema-invalid/non-synthetic results cannot pass. A completed trial never accepts a repair or regression.
- Case-pinned policy bounds wall time and total attempts per repair request. Polls cannot extend deadlines. Model-independent supervision persists timeout/cancellation uncertainty once and escalates. Unknown starts observed running after deadline are cancelled/timed out rather than resumed.
- Migration 014 atomically stores fake backend observations, coordinator state/events, case audit/evidence and command receipts. Rollback and restart are fixture-verified. Fake effects are local SQLite writes; this is not proof of a distributed transaction with an actual process or remote provider.
- CLI commands, authenticated status/event/command API, ordinary case timeline and visible web panel expose owner, deadline, unknown outcome, evidence and next action. Imported text stays inert; display caps do not affect attempt/concurrency gates or direct handle reads.
- All records use `deterministic-fake-v1`, scope `fake-adapter-protocol-v1`, `real_execution_admitted: false`. No arbitrary executable, repository task, credential or external effect can be supplied.

## Validation

Local Linux:

| Check | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed |
| `rustfmt --edition 2024 --check tools/isolation_probe.rs` | Passed |
| `cargo clippy --locked --all-targets -- -D warnings` | Passed |
| `cargo test --locked` | 124 tests passed; one static-payload compilation test is Linux-only |
| `npm run build --prefix web` | Strict TypeScript production build passed |
| `cargo build --locked` | Passed |
| `npm test --prefix web` | All six Chromium scenarios passed |
| Local Markdown links / `git diff --check` | Passed |

New coverage: restart/lost start, lost cancel acknowledgment, current-status retries, ordered cursor replay, structured quota/invalid output, fence/schema/synthetic rejection, offline deadline supervision, missing backend uncertainty (poll/cancel/deadline), late-start reconciliation, owner/version/cross-case/unknown-field rejection, concurrent starts, persistent attempt limits, and transaction rollback before/after fake work. Browser coverage extends repair readiness with start/reconcile/result, authenticated reads, evidence, duplicate command, unknown-outcome UI and reload; repairs remain blocked.

Remote CI for the new exact head must be checked separately. Ordinary tests make no model, Sentry, Slack or production request. They do not qualify physical isolation, real cancellation, authentication/subscription access, candidate regression, PR automation, deployment or recovery.

## Retained worker qualification blocker

PR #11 implemented immutable pinned worker policy, fixed Linux synthetic probes, durable evidence, interruption/late-output fencing, quarantine and evidence-backed reconciliation. Its product CLI native attempt failed with zero verified capabilities; an independent direct Bubblewrap test reported `Failed to create NETLINK_ROUTE socket: Operation not permitted`. The host restriction was not weakened. No successful live isolation or real harness admission is claimed. Quarantined resources still require explicit inspection/reconciliation.

## Overall done and pending

| Milestone | Implemented | Remaining |
| --- | --- | --- |
| M0 | Bootstrap, versioned contracts, migrations, deterministic fixtures, CI | Distribution/license decisions remain later work |
| M1 | Durable cases/events/evidence, qualification/reopening, CLI/API/web replay | Operational hardening belongs to M8 |
| M2 | Bounded fake collaboration, supervision, handoffs, independent challenges/gates, trusted policy, Slack fallback/retry/ack/resolution adapter | Actual human destinations/credentials and live delivery drill |
| M3 | Sentry intake/enrichment/reconciliation/freshness, incident operations/roles, investigations/drafts/review, scoped correlation/diagnostics | Actual sanitized service history, configured live activation, richer impact windows/disposition categories/cross-service correlation |
| M4 | Frozen repair inputs/runtime metadata, denied repair requests, controlled qualification/evidence/loss/quarantine/reconciliation; durable fake adapter lifecycle | Qualified actual worker/runtime profile and real adapter; candidate/regression/review/PR delivery lifecycle |
| M5 | Design/templates | Authorized mitigation, deployment/recovery evidence, postmortems/actions, accepted knowledge |
| M6 | Design/policy requirements | Eligible automatic merging and effect reconciliation |
| M7 | Adapter/portability design | Second harness/source, native worker routing, packaging/platform drills |
| M8 | Initial local visibility | Remote RBAC, backup/restore, retention/redaction, backpressure, full workspace and operating drills |

M3 still needs real service/release mappings, Sentry install/client secret/token/plan permissions, trusted ingress, and authorized Slack destinations/secrets/participants; the prepared [activation drill](16-m3-activation-drill.md) is not executed. Synthetic fixtures do not meet actual-history/live acceptance.

M4 still needs a properly provisioned worker for a real controlled drill, then stronger adversarial/FD/seccomp/cgroup/aggregate-budget/fatal-loss and host/tool identity validation, workspace sealing, scoped authentication/credentials and revocation, and a full start/events/cancel/result/reconcile harness adapter. Codex is the first candidate, not an admitted runtime. Only after those gates: independently frozen candidates, fail-on-base/pass-on-candidate regression, verification/review, and idempotent/reconciled PR delivery. No request/receipt/cleanup report/CLI flag grants any of these powers.

Current actor labels are single-operator attribution, not remote RBAC. A hostile host administrator/DB owner or malicious operator-installed tool remains outside this boundary. Repo merges performed by the coding assistant are not Watchhouse automatic merging. Source status, a probe pass, patch merge or unsent draft never proves deployment/recovery.

## Exact next step

Review the synthetic lifecycle slice. On a properly provisioned Linux worker, execute the controlled boundary drill without weakening containment. Define and verify the actual runtime/credential/worker profile and real external dispatch/reconciliation protocol; the transactional fake journal cannot authorize it. Then implement independently frozen candidate/base regression, verification/review and idempotent GitHub delivery. M3 service-history/live activation can proceed independently once operator mappings/secrets/authority exist.
