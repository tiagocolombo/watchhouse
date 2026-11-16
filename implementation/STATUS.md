# Current status

Last updated: 2026-11-16.

## Completion direction and current native worker slice

Local acceptance testing remains deferred until remaining development is complete. Automated checks continue. The [completion gates](COMPLETION.md) enumerate outstanding exit criteria.

The interrupted M4 working tree was recovered intact on 2026-11-16. This branch continues above PR #16 (`dev/completion-gates`, `7cf5e6f`) and implements the [native repair contract](../contracts/v1/native-repair/README.md): immutable operator container policy, credential-free fixed Linux boundary, durable reserve-before-I/O/reconciliation/cancellation, offline deadlines and bounded cleanup, native admission evidence, frozen operator-provided text candidates, exact base/candidate regression and independent attributed review. CLI/API/web/timeline/report surfaces are wired. See [ADR 0021](adrs/0021-native-container-regression.md).

Local validation: 151 Rust tests passed (including ten new controller/protocol tests and a new HTTP boundary test); the explicit physical Docker integration test is separate and ignored by ordinary tests. Strict Clippy, formatting, web TypeScript compilation and native build passed. All seven Chromium scenarios passed, including the new queued-to-blocked native preflight/replay/inert-text/mobile scenario. The local host has no Docker daemon. First Linux native CI run 43925249166 created a container but returned unknown/quarantine during start/result; physical success was not claimed at that point. A second native run (43926191160) showed the container remained in created state, so the failure occurs during inspection before start. Bounded diagnostics identify the exact mismatched field/capability metadata; admission stays closed. macOS CI passed; Windows exposed Unix-only fixture control paths, now corrected to native absolute paths without changing the Linux-only execution boundary. The native container inspection rejected Docker's canonical `CAP_SETUID`/`CAP_SETGID` names; comparison now normalizes the optional `CAP_` prefix while rejecting all additional capabilities. Regression tests cover both equivalent spellings and privilege expansion. The separate exact-version Codex no-account/loopback CI drill reached the synthetic endpoint; its fixture SSE needed the required `total_tokens` usage field, now corrected. Linux native containment, descendant cancellation, failing-base/passing-candidate and attributed review subsequently **passed** at `2b2a65d` in CI run 43928311850. All three OS coordinator checks, browser scenarios and packaging passed at that head. The separate Codex machine-protocol fixture completed a native turn against the synthetic endpoint, but a strict tool-free profile is **denied** because `request_user_input` remains advertised. CI now explicitly asserts this denial and retains `real_execution_admitted:false`; it does not waive the gate or call a real model. A different explicitly scoped text-only runtime profile would need its own review and lifecycle tests before admission. Both the bundled and npm-installed 0.167.0-alpha.4 runtimes failed locally before making any request because the host denied the read-only filesystem helper; no model/account capability is claimed. No model, publication, merge or production integration was invoked.

The worker tests found and fixed expired unknown jobs failing to schedule cleanup. Unknown creation/start responses cannot repeat effects; all cancellation paths share cleanup attempts, late results cannot restore authority, and new/expired/corrupt qualification proof cannot authorize work. A failed read-only engine preflight records an owned block before container creation rather than claiming an uncertain resource exists.

This is a completed bounded test-execution slice, not the full M4 exit. First real harness authentication/lifecycle and reconciled GitHub delivery remain the next dependent work. M3 live acceptance and the M5–M8 gates remain open.

## Baseline and current work

PRs #2–#11 are merged; main is `823aa2c`. [PR #12](https://github.com/tiagocolombo/watchhouse/pull/12) remains open at `e6588a6`; its Linux/macOS/Windows CI passed. [PR #13](https://github.com/tiagocolombo/watchhouse/pull/13) adds recovery and passed exact-head Linux/macOS/Windows CI at `adcfacb`. [PR #14](https://github.com/tiagocolombo/watchhouse/pull/14) adds learning at `a5363ef`. PR #15 (`dev/native-test-bundles`, `d0e4605`) builds on PR #14; its exact-head Linux/macOS/Windows CI passed. The documentation completion-gates branch is stacked on PR #15. No pending PR was merged in this batch.

M4 physical worker/runtime/credential admission remains blocked. This independent M5 slice completes operator-driven recovery and follow-through records without bypassing real-execution admission. See [ADR 0018](adrs/0018-recovery-and-follow-through.md) and [recovery contract](../contracts/v1/recovery/README.md).

## Implemented recovery and learning

- Prospective source/incident/policy-bound recovery criteria, bounded traffic/error/latency/coverage windows, deterministic met/partial/ineffective/inconclusive evaluation. Missing traffic, coverage, latency or excess source lag cannot pass.
- Independent attributed review with the reviewer's own new distinct evidence, commander resolution, stale-context/challenge invalidation and reopening. This is explicitly **operator-reported review**, not Watchhouse-authenticated telemetry. Declaration is immutable and response resolution stays separate from repair/deployment.
- Corrective actions with category, owner, distinct verifier, acceptance criteria, deadline, submission, independent proof, waiver/residual risk and persistent overdue escalation, including after resolution. Late transitions preserve missed duties.
- Structured mitigation proposals remain blocked for production authority and escalate; no execute endpoint or override exists. Scoped reported deployment records do not establish recovery.
- Migration 015, append-only records, atomic state/audit/receipts, restart/rollback, shared CLI/API, escaped Markdown, visible web surface, authenticated commands and browser replay. Direct acceptance checks do not use display caps.
- Example generator produces an inspectable synthetic recovery/follow-up scenario and recovery Markdown alongside incident/investigation/qualification records. No external service/model or production action is invoked.

Local Linux validation: **140 Rust tests passed**, all **six Chromium scenarios passed**, strict TypeScript and native build passed, rustfmt/Clippy passed. The expanded example generator includes postmortem, accepted knowledge and a successor fake briefing. Markdown links and diff checks passed. New exact-head remote CI must be checked separately; these local observations do not prove native containment or live integrations.

## Learning added in the current slice

- Immutable postmortem revisions, bounded/unknown impact, supported-hypothesis linkage or explicit unknown cause, owned follow-ups, independent attributed acceptance and source/incident/recovery context validation.
- Scoped knowledge proposals, immutable promotion/retirement, expiry/review escalation, literal lexical retrieval and accepted version/digest/body snapshots in new fake briefings. Pending/imported drafts cannot change governing accepted versions. Stale/expired/challenged/corrupt/retired records are excluded.
- Review evidence now uses logical case ordering alongside timestamps, preventing evidence collected before a submission from being reused at equal clock time. Legacy unversioned unreviewed submissions need fresh versioned resubmission.
- Shared CLI/API/web commands, formal Markdown sections/action tables, script-inert browser rendering, review/source drift/restart/rollback tests and the expanded synthetic gallery. See [ADR 0019](adrs/0019-reviewed-operational-learning.md) and [learning contract](../contracts/v1/learning/README.md).

## Native test packaging

- Native debug coordinator/UI, synthetic demo database, readable reports, TESTING.md and platform/source/hash manifests packaged by a stdlib-only build tool. Fixed archive paths/digests are checked before extracting and executing a trusted CI-built binary.
- Local Linux archive extraction/`--version`/case list/snapshot checks passed, including reported resolved recovery, continuing open action, accepted postmortem/lesson and no live model identity. Overwrite/corruption checks passed. Existing 140 Rust tests and six browser scenarios remain the validated core baseline; packaging changes no domain code.
- CI now generates and validates test artifacts on Linux/macOS/Windows after ordinary checks. Actual new exact-head remote results/artifact IDs must be observed separately. Builds are unsigned/debug/native-runner test artifacts; installers, signing, wider compatibility and license/release decisions remain open. See [ADR 0020](adrs/0020-native-operator-test-bundles.md) and [testing guide](../TESTING.md).

## Retained activation blockers

The prior native Bubblewrap drill failed closed (`Failed to create NETLINK_ROUTE socket: Operation not permitted`), with zero verified capabilities and quarantine. No successful physical isolation or real harness admission is claimed. Real Sentry/notification destinations, credentials, mappings and actual-history acceptance are still unavailable. Operator actor labels remain single-operator attribution, not remote RBAC.

## Overall done and pending

| Milestone | Implemented | Remaining |
| --- | --- | --- |
| M0 | Bootstrap, versioned contracts, migrations, deterministic fixtures, CI | Distribution/license decisions remain later work |
| M1 | Durable cases/events/evidence, qualification/reopening, CLI/API/web replay | Operational hardening belongs to M8 |
| M2 | Bounded fake collaboration, supervision, handoffs, independent challenges/gates, trusted policy, Slack fallback/retry/ack/resolution adapter | Actual human destinations/credentials and live delivery drill |
| M3 | Sentry intake/enrichment/reconciliation/freshness, incident operations/roles, investigations/drafts/review, scoped correlation/diagnostics | Actual sanitized service history, configured live activation, richer impact windows/disposition categories/cross-service correlation |
| M4 | Frozen repair inputs/runtime metadata, denied repair requests, controlled qualification/evidence/loss/quarantine/reconciliation; durable fake adapter lifecycle | Qualified actual worker/runtime profile and real adapter; candidate/regression/review/PR delivery lifecycle |
| M5 | Operator recovery criteria/windows/review/resolution, deployment reports, blocked mitigation proposals, independently verified corrective actions | Actual mitigation/deployment/telemetry integrations; Hallmark format compatibility and actual-history evaluation |
| M6 | Design/policy requirements | Eligible automatic merging and effect reconciliation |
| M7 | Adapter/portability design; native coordinator test-bundle workflow | Second harness/source, native worker routing, packaging/platform drills |
| M8 | Initial local visibility and documented synthetic operator test path | Remote RBAC, backup/restore, retention/redaction, backpressure, full workspace and operating drills |

M3 still needs real service/release mappings, Sentry install/client secret/token/plan permissions, trusted ingress, and authorized Slack destinations/secrets/participants; the prepared [activation drill](16-m3-activation-drill.md) is not executed. Synthetic fixtures do not meet actual-history/live acceptance.

M4 still needs a properly provisioned worker for a real controlled drill, then stronger adversarial/FD/seccomp/cgroup/aggregate-budget/fatal-loss and host/tool identity validation, workspace sealing, scoped authentication/credentials and revocation, and a full start/events/cancel/result/reconcile harness adapter. Codex is the first candidate, not an admitted runtime. Only after those gates: independently frozen candidates, fail-on-base/pass-on-candidate regression, verification/review, and idempotent/reconciled PR delivery. No request/receipt/cleanup report/CLI flag grants any of these powers.

Current actor labels are single-operator attribution, not remote RBAC. A hostile host administrator/DB owner or malicious operator-installed tool remains outside this boundary. Repo merges performed by the coding assistant are not Watchhouse automatic merging. Source status, a probe pass, patch merge or unsent draft never proves deployment/recovery.

## Exact next step

Observe the native Linux container CI drill, then finish real harness authentication/lifecycle and reconciled PR delivery. Complete the remaining M3/M5–M8 gates in COMPLETION.md. Continue independent fixture/adapter work while named activation inputs are missing. Do not hand off local acceptance testing or mark a phase complete without its required evidence.
