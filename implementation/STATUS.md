# Current status

Last updated: 2026-11-15.

## Main and current slice

PRs [#2](https://github.com/tiagocolombo/watchhouse/pull/2)–[#10](https://github.com/tiagocolombo/watchhouse/pull/10) are merged. PR #10 was checked at its exact head with green Linux/macOS/Windows CI and merged without conflicts. Merged main baseline: `55bce1b`.

Current branch `dev/m4-worker-qualification` continues M4 with controlled worker qualification, durable evidence and quarantine/reconciliation. [ADR 0016](adrs/0016-controlled-worker-qualification.md) and the [executable contract](../contracts/v1/worker-qualification/README.md) define scope. This is **not** completion of M4 or admission of a real harness.

## Implemented in this slice

- Immutable active worker policy with pinned tool identities, explicit resource/time bounds and installation identity/time. Only the OS operator can install it; agents/HTTP cannot. New attempts require the active revision. Already-reserved attempts and audit history retain their exact policy/source/case provenance.
- Fixed Linux adapter plus a statically compiled trusted payload. It measures synthetic input/policy read-only behavior, hidden host canary, clean environment, allowed scratch writes, loopback exclusion, private network/PID namespace identity, selected per-process rlimits, and known descendant termination. Fixed argv arrays allow no shared-network/host-root/arbitrary-task override. No candidate code, model, credential or publication action is supplied.
- Before native I/O, one transaction reserves the attempt/nonce/deadline/owner and receipt. Retries inspect current state rather than repeating the probe. Results must match strict version/nonce schemas and complete measurements. Tool identity is checked before and after observations. Failed/malformed/partial outcomes cannot pass. Fixture-labelled outcomes remain simulated without verified capabilities.
- Case-bound content-addressed evidence retains normalized observations/host confirmations/origin/outcome/output digest and binds attempt, nonce, policy and source manifest. Evidence is retrievable through the existing authenticated endpoint. Private synthetic controller tests exercise success/currency branches but are not physical isolation verification.
- The model-independent supervisor expires lost/overdue attempts into interrupted/quarantined state with owned escalation. Late output is retained once as unaccepted evidence and cannot restore proof. Metadata failure cannot trigger a blind repeat. No canonical writer lock is held during the external probe.
- Quarantine blocks new attempts until the accountable operator records explicit case-bound reported inspection evidence and reconciliation. Reconciliation clears only the resource hold, preserves failed/interrupted history, grants no capabilities, deletes/kills nothing and resolves no escalation/repair automatically. Its update, audit event and receipt are atomic. Old-key retries remain inspection only.
- CLI policy/status/qualification/reconciliation/reporting, authenticated read/reconciliation API and a visible web panel show scope, owner, evidence, time currency and next action. Policy installation and native invocation remain CLI-only. Markdown and imported text remain inert. Snapshots show the newest 100 attempts; history and authority checks do not depend on that display cap.
- All records have scope `hermetic-probe-v1` and `real_execution_admitted: false`, including a measured probe pass. Time currency is not authority, current tool/host validation, supported account access or verified arbitrary-code containment.

## Actual validation

Linux, 2026-11-15:

| Check | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed |
| `rustfmt --edition 2024 --check tools/isolation_probe.rs` | Passed; added to CI |
| `cargo clippy --locked --all-targets -- -D warnings` | Passed |
| `cargo test --locked` | 113 Rust tests passed on Linux; static-payload compile check is Linux-only |
| `npm run build --prefix web` | Strict TypeScript passed |
| `cargo build --locked` | Bundled binary passed |
| `npm test --prefix web` | Six Chromium scenarios passed |
| `git diff --check` and local Markdown links | Passed |

Coverage includes policy bounds/immutability/provenance, active-policy enforcement, fixed boundary arguments, static payload compatibility, schema/nonce checks, complete-versus-missing measurements, fixture/native distinction, currency/expiry, same-key inspection, competing versions, rollback before I/O and during finalization, restart/supervision without models, late-output fencing, evidence retrieval, quarantine holds, reported reconciliation/authentication and inert browser/Markdown replay. The browser extends repair readiness with a failed controlled fixture, evidence/status/reporting and absence of an HTTP execution endpoint. Existing fake/source/notification/challenge/incident tests remain green.

### Native observation — failed, not qualified

A product CLI attempt used pinned installed Bubblewrap/prlimit and a statically linked payload in a separate synthetic database/scratch area. It returned `failed`, scope `hermetic-probe-v1`, zero verified capabilities, `real_execution_admitted: false`, retrievable failure evidence, quarantine and an owned escalation. An independent direct Bubblewrap test reported `Failed to create NETLINK_ROUTE socket: Operation not permitted`. The host restriction was not weakened or bypassed. No successful native isolation pass is claimed.

Ordinary tests use synthetic local repositories, fake execution, harmless runtime metadata and private controller fixtures. Linux CI statically compiles the trusted payload but does not claim an actual namespace drill. No live Sentry/Slack destination, account/subscription/model invocation, arbitrary candidate test, repair, PR delivery automation, deployment or service recovery was verified. Exact-head remote CI must be checked separately from local validation.

## Overall done and pending

| Milestone | Implemented | Remaining |
| --- | --- | --- |
| M0 | Bootstrap, versioned contracts, migrations, deterministic fixtures, CI | Distribution/license decisions remain later work |
| M1 | Durable cases/events/evidence, qualification/reopening, CLI/API/web replay | Operational hardening belongs to M8 |
| M2 | Bounded fake collaboration, supervision, handoffs, independent challenges/gates, trusted policy, Slack fallback/retry/ack/resolution adapter | Actual human destinations/credentials and live delivery drill |
| M3 | Sentry intake/enrichment/reconciliation/freshness, incident operations/roles, investigations/drafts/review, scoped correlation/diagnostics | Actual sanitized service history, configured live activation, richer impact windows/disposition categories/cross-service correlation |
| M4 | Frozen repair inputs/runtime metadata, denied repair requests, controlled qualification/evidence/loss/quarantine/reconciliation | Qualified actual worker/runtime profile and real adapter; candidate/regression/review/PR delivery lifecycle |
| M5 | Design/templates | Authorized mitigation, deployment/recovery evidence, postmortems/actions, accepted knowledge |
| M6 | Design/policy requirements | Eligible automatic merging and effect reconciliation |
| M7 | Adapter/portability design | Second harness/source, native worker routing, packaging/platform drills |
| M8 | Initial local visibility | Remote RBAC, backup/restore, retention/redaction, backpressure, full workspace and operating drills |

M3 still needs real service/release mappings, Sentry install/client secret/token/plan permissions, trusted ingress, and authorized Slack destinations/secrets/participants; the prepared [activation drill](16-m3-activation-drill.md) is not executed. Synthetic fixtures do not meet actual-history/live acceptance.

M4 still needs a properly provisioned worker for a real controlled drill, then stronger adversarial/FD/seccomp/cgroup/aggregate-budget/fatal-loss and host/tool identity validation, workspace sealing, scoped authentication/credentials and revocation, and a full start/events/cancel/result/reconcile harness adapter. Codex is the first candidate, not an admitted runtime. Only after those gates: independently frozen candidates, fail-on-base/pass-on-candidate regression, verification/review, and idempotent/reconciled PR delivery. No request/receipt/cleanup report/CLI flag grants any of these powers.

Current actor labels are single-operator attribution, not remote RBAC. A hostile host administrator/DB owner or malicious operator-installed tool remains outside this boundary. Repo merges performed by the coding assistant are not Watchhouse automatic merging. Source status, a probe pass, patch merge or unsent draft never proves deployment/recovery.

## Exact next step

Review this slice against main. Run the controlled probe on an appropriately provisioned Linux worker without weakening the host boundary. Keep failed/interrupted resources held until explicit reconciliation. Complete actual worker/runtime/credential admission and the real adapter protocol before candidate execution; M3 service-history/live activation can proceed independently when operator inputs exist. M4 exit criteria remain unmet.
