# Current status

Last updated: 2026-11-16.

## Completion direction and current native worker slice

Local acceptance testing remains deferred until remaining development is complete. Automated checks continue. The [completion gates](COMPLETION.md) enumerate outstanding exit criteria.

The interrupted M4 working tree was recovered intact on 2026-11-16. This branch continues above PR #16 (`dev/completion-gates`, `7cf5e6f`) and implements the [native repair contract](../contracts/v1/native-repair/README.md): immutable operator container policy, credential-free fixed Linux boundary, durable reserve-before-I/O/reconciliation/cancellation, offline deadlines and bounded cleanup, native admission evidence, frozen operator-provided text candidates, exact base/candidate regression and independent attributed review. CLI/API/web/timeline/report surfaces are wired. See [ADR 0021](adrs/0021-native-container-regression.md).

Local validation: 151 Rust tests passed (including ten new controller/protocol tests and a new HTTP boundary test); the explicit physical Docker integration test is separate and ignored by ordinary tests. Strict Clippy, formatting, web TypeScript compilation and native build passed. All seven Chromium scenarios passed, including the new queued-to-blocked native preflight/replay/inert-text/mobile scenario. The local host has no Docker daemon. First Linux native CI run 43925249166 created a container but returned unknown/quarantine during start/result; physical success was not claimed at that point. A second native run (43926191160) showed the container remained in created state, so the failure occurs during inspection before start. Bounded diagnostics identify the exact mismatched field/capability metadata; admission stays closed. macOS CI passed; Windows exposed Unix-only fixture control paths, now corrected to native absolute paths without changing the Linux-only execution boundary. The native container inspection rejected Docker's canonical `CAP_SETUID`/`CAP_SETGID` names; comparison now normalizes the optional `CAP_` prefix while rejecting all additional capabilities. Regression tests cover both equivalent spellings and privilege expansion. The separate exact-version Codex no-account/loopback CI drill reached the synthetic endpoint; its fixture SSE needed the required `total_tokens` usage field, now corrected. Linux native containment, descendant cancellation, failing-base/passing-candidate and attributed review subsequently **passed** at `2b2a65d` in CI run 43928311850. All three OS coordinator checks, browser scenarios and packaging passed at that head. The separate Codex machine-protocol fixture completed a native turn against the synthetic endpoint, but a strict tool-free profile is **denied** because `request_user_input` remains advertised. CI now explicitly asserts this denial and retains `real_execution_admitted:false`; it does not waive the gate or call a real model. A different explicitly scoped text-only runtime profile would need its own review and lifecycle tests before admission. Both the bundled and npm-installed 0.167.0-alpha.4 runtimes failed locally before making any request because the host denied the read-only filesystem helper; no model/account capability is claimed. No model, publication, merge or production integration was invoked.

The worker tests found and fixed expired unknown jobs failing to schedule cleanup. Unknown creation/start responses cannot repeat effects; all cancellation paths share cleanup attempts, late results cannot restore authority, and new/expired/corrupt qualification proof cannot authorize work. A failed read-only engine preflight records an owned block before container creation rather than claiming an uncertain resource exists.

This is a completed bounded test-execution slice, not the full M4 exit. First real harness authentication/lifecycle and reconciled GitHub delivery remain the next dependent work. M3 live acceptance and the M5–M8 gates remain open.

## Hallmark format compatibility in the current stacked slice

The actual upstream Hallmark JSON/source definitions were inspected at `15ee84d9de5385658b8d9dff1408728fb9e28fbd`. Migration 018 and the [Hallmark contract](../contracts/v1/hallmark/README.md) implement bounded canonical/CLI/bare-array import, immutable source namespace/path/content/record digests, byte-exact selected re-export, collision/revision handling, explicit native proposal mapping and conservative native-to-Hallmark proposed exports. External accepted frontmatter never changes local acceptance or governing briefings. Unknown/foreign/corrupt import provenance is rejected; pending replacements cannot displace the accepted export revision.

CLI/API/timeline/snapshot/web inspection are wired. Raw Markdown stays inert. Local validation: **157 Rust tests passed**, all **eight Chromium scenarios passed**, strict Clippy, Rust formatting, TypeScript compilation and native build passed. New tests cover exact wire shapes/bytes, idempotency, append-only records, rollback/restart, scope/auth boundaries, independent promotion, pending revisions and corrupted lineage. This proves format compatibility using invented fixtures, not a real Hallmark executable/import destination or historical operational acceptance. See [ADR 0022](adrs/0022-hallmark-knowledge-exchange.md).

## Baseline and current work

PRs #2–#11 are merged; main is `823aa2c`. [PR #12](https://github.com/tiagocolombo/watchhouse/pull/12) remains open at `e6588a6`; its Linux/macOS/Windows CI passed. [PR #13](https://github.com/tiagocolombo/watchhouse/pull/13) adds recovery and passed exact-head Linux/macOS/Windows CI at `adcfacb`. [PR #14](https://github.com/tiagocolombo/watchhouse/pull/14) adds learning at `a5363ef`. PR #15 (`dev/native-test-bundles`, `d0e4605`) builds on PR #14; its exact-head Linux/macOS/Windows CI passed. The documentation completion-gates branch is stacked on PR #15. No pending PR was merged in this batch.

At the earlier recovery milestone, M4 physical admission was blocked; PR #17 subsequently verified the Docker boundary in native CI. Real model authentication and full M4 delivery remain open. The M5 recovery slice completes operator-driven recovery and follow-through records without bypassing real-execution admission. See [ADR 0018](adrs/0018-recovery-and-follow-through.md) and [recovery contract](../contracts/v1/recovery/README.md).

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

The prior native Bubblewrap drill failed closed (`Failed to create NETLINK_ROUTE socket: Operation not permitted`), with zero verified capabilities and quarantine. That historical failure does not negate PR #17's later native Docker verification. Live model authentication remains unverified. Real Sentry/notification destinations, credentials, mappings and actual-history acceptance are still unavailable. Operator actor labels remain single-operator attribution, not remote RBAC.

## Overall done and pending

| Milestone | Implemented | Remaining |
| --- | --- | --- |
| M0 | Bootstrap, versioned contracts, migrations, deterministic fixtures, CI | Distribution/license decisions remain later work |
| M1 | Durable cases/events/evidence, qualification/reopening, CLI/API/web replay | Operational hardening belongs to M8 |
| M2 | Bounded fake collaboration, supervision, handoffs, independent challenges/gates, trusted policy, Slack fallback/retry/ack/resolution adapter | Actual human destinations/credentials and live delivery drill |
| M3 | Sentry intake/enrichment/reconciliation/freshness, incident operations/roles, investigations/drafts/review, scoped correlation/diagnostics | Actual sanitized service history, configured live activation, richer impact windows/disposition categories/cross-service correlation |
| M4 | Native-verified Docker isolation, cancellation/watchdog and regression; frozen candidates/operator review; bounded Codex text-role controller and explicit native test path | Native text-client CI and live authentication; candidate/model-review adoption and reconciled GitHub PR delivery |
| M5 | Operator recovery criteria/windows/review/resolution, deployment reports, blocked mitigation proposals, independently verified corrective actions, Hallmark knowledge exchange | Actual mitigation/deployment/telemetry integrations, problem/known-error linkage and actual-history evaluation |
| M6 | Design/policy requirements | Eligible automatic merging and effect reconciliation |
| M7 | Adapter/portability design; native coordinator test-bundle workflow | Second harness/source, native worker routing, packaging/platform drills |
| M8 | Initial local visibility and documented synthetic operator test path | Remote RBAC, backup/restore, retention/redaction, backpressure, full workspace and operating drills |

M3 still needs real service/release mappings, Sentry install/client secret/token/plan permissions, trusted ingress, and authorized Slack destinations/secrets/participants; the prepared [activation drill](16-m3-activation-drill.md) is not executed. Synthetic fixtures do not meet actual-history/live acceptance.

M4 Docker containment/cancellation/regression has native CI proof. The bounded Codex text-client adapter has its own required native cgroup/protocol drill and cannot inherit Docker admission. Live account authentication, model-result adoption/independent-review lineage and reconciled GitHub delivery remain open. No request, receipt, reported cleanup or CLI flag grants these powers.

Current actor labels are single-operator attribution, not remote RBAC. A hostile host administrator/DB owner or malicious operator-installed tool remains outside this boundary. Repo merges performed by the coding assistant are not Watchhouse automatic merging. Source status, a probe pass, patch merge or unsent draft never proves deployment/recovery.

## Exact next step

Complete the new native Linux model-client CI drill, model-result adoption/independent-review lineage and reconciled PR delivery; retain the already-passing Docker CI evidence. Complete the remaining M3/M5–M8 gates in COMPLETION.md. Continue independent fixture/adapter work while named activation inputs are missing. Do not hand off local acceptance testing or mark a phase complete without its required evidence.

## Coordinator-loss watchdog follow-up

The container initializer now starts its own hard-wall watchdog before input preparation. This closes the coordinator-loss gap: PID 1 exit terminates namespace descendants even without a host cancellation request. The native CI drill additionally starts a two-second sleeping workload, waits without cancelling it, verifies watchdog exit 124, then performs scoped cleanup. Qualification uses an explicit 30-second native test profile; production policy defaults were not changed. Ten controller/protocol tests, formatting and strict Clippy passed locally. The new physical watchdog case still needs its exact-head CI result; prior native proof covers the preceding implementation only.

## Bounded model-client work (current stacked slice)

Migration 019 adds immutable model profiles, durable jobs and expiring native transport admission. The Codex text-role adapter, sealed detached initializer, cgroup-v2 identities/resources/cancellation, coordinator-independent watchdog, delayed-start cancellation fence, no-replay reservation/reconciliation, bounded shared budgets and owner/context/knowledge checks are implemented. API/CLI/snapshot/timeline and inert web inspection are wired. `serve` does not start a model worker; explicit OS configuration and owned enqueueing are required.

The strict tool-free spike remains denied. This is a distinct text-only adapter that permits only the inert question-function advertisement and rejects actual tool/interaction/plan events. Candidate execution remains credential-free in the Docker worker. Transport qualification uses actual Codex with synthetic loopback SSE and an empty auth home, not a live account/model. See [ADR 0023](adrs/0023-bounded-text-model-client.md) and [model runtime contract](../contracts/v1/model-runtime/README.md).

Local validation: 166 Rust tests passed (including nine new model controller/protocol tests); strict Clippy/fmt, TypeScript and native build passed. Targeted model and HTTP tests also passed after boundary changes. All eight Chromium scenarios passed, including blocked model preflight, idempotency, reload and inert output inspection. The new native cgroup/actual-Codex CI drill is pending; no model-client native proof is claimed. This workspace has no mounted cgroup hierarchy, so it cannot run that physical drill. Live account authentication/model access, candidate/model-review adoption and reconciled GitHub delivery remain open M4 gates. M3/M5–M8 remain governed by COMPLETION.md. No PR was merged and no paid model/live integration was exercised.

Native text-client CI at `e6da2ef` correctly failed closed on a missing terminal result. The adapter flushed but retained its child stdin pipe while awaiting output, leaving a CLI that reads its prompt to EOF waiting. It now drops the owned pipe before waiting, with a fixed benign process/EOF regression test. The native cgroup/Codex drill must pass on the follow-up commit; no admission is inferred from this diagnosis.

## Model proposal and independent-review adoption (current slice)

Completed model results can now be explicitly adopted into existing frozen candidates and native regression/review gates through CLI/authenticated API/web commands. Adoption derives the exact proposal/decision/findings from the retained job; model authorship and reviewer session/job lineage remain visible. Synthetic/cancelled/unaccepted/corrupt output, modified material, wrong candidate/checks, stale context and a reused author session cannot grant approval. Existing operator command fingerprints remain unchanged when no model job is provided.

The new controller scenario passes exact proposal/tamper/idempotency checks, native-shaped base-fail/candidate-pass gating, different-session review, same-session denial and downstream invalidation after author evidence corruption. These observations are injected fixtures, not actual model/kernel proof. Validation: 169 Rust tests and all eight Chromium scenarios passed, plus strict Clippy/fmt, TypeScript/native build and diff checks. See [ADR 0024](adrs/0024-model-result-adoption.md). GitHub delivery/reconciliation is the next M4 implementation dependency; real authentication/live model and broader milestone gates remain open.
The EOF follow-up still timed out in native CI (run 43944221929); it was a genuine pipe bug, not established as the sole timeout cause. The native fixture now refreshes its manual clock after expensive setup, logs bounded stage/journal-presence diagnostics and uses optimized hash primitives in debug builds. Exact-byte identity checks, resource limits, cancellation requirements and admission gates remain unchanged. Native model-client acceptance is still open.

## Hosted continuation: initializer diagnosis

The execution workspace is unavailable. Source changes are being made through GitHub and validated by CI; no local validation is claimed for this follow-up. Exact heads 7d3524a and 3aecb1b passed macOS/Windows and the independent protocol check but failed Linux native model-client qualification. Stage diagnostics show no started journal. Qualification now captures only the trusted initializer's stderr under its clean, nonce-only synthetic scope and bounds test inspection to 4096 bytes. Production roles retain discarded initializer stderr; private client stderr/reasoning is not published. Admission remains closed until the actual native drill passes. Broader completion gates remain open.

Native diagnostics in CI run 43959221842 identified initializer rejection of a non-private per-job directory. The coordinator now sets Unix job-directory permissions to 0700 and validates the private directory before persisting the request. A fake-backend regression checks this before dispatch, so ordinary tests cover the boundary without running a model. The initializer permission check is unchanged. Native qualification and all-PR CI still require fresh successful results.

The private-directory fix advanced native startup to cgroup attachment. CI run 43960374158 then correctly rejected migration from outside the delegated subtree. The native drill now bootstraps only its uniquely owned coordinator child into that subtree and drops to the unprivileged runner before Cargo execution, with a clean environment. No permissions on the host cgroup root are changed. Native root validation also rejects a worker outside its delegated subtree before dispatch. Whole-resource proof and all limits remain required; fresh native CI must verify the correction.

## Recovery browser submission synchronization

The recovery browser scenario now waits for its own plan POST response, checks that response and consumes its committed plan before collecting the observation window. The previous `not assessed` text existed before submission and did not establish that a plan had been saved, producing intermittent null-plan failures. This changes test synchronization, not recovery thresholds, authority or evidence requirements. Fresh exact-head CI is required for this follow-up.

## Delivery preparation continuation

The execution workspace was restored from an older checkpoint. Its uncommitted work was preserved; a fresh worktree at the green PR #20 head (`635676b`) continues on `dev/m4-github-delivery`. Rust 1.90, rustfmt and Clippy were restored.

Reviewed delivery preparation now computes exact Git tree identities from verified canonical candidate files and exposes the artifact through CLI, authenticated API and inert review-card inspection. The current candidate/review/context and manifest digest remain governing. Binary base files and Git directory sorting are included; parent symlinks, collisions, byte changes and changed payloads are rejected. It grants no publication authority. See ADR 0025 and the delivery contract.

Validation: 173 Rust tests passed; all eight Chromium scenarios passed after installing the missing browser in the restored environment. Strict Clippy, formatting, TypeScript and native build passed. Selected object/controller tests are rerun after bounded-read hardening. Durable authorized GitHub creation/update/reconciliation remains the next M4 implementation requirement. No live model, publisher integration, merge, deployment or recovery was exercised by this continuation. Other COMPLETION.md gates remain open.

Native Windows CI exposed a coordinator entry-point stack overflow after publication preparation enlarged the debug dispatch future (exit 0xC00000FD). CLI fixture diagnostics now retain bounded stderr/status. The CLI constructs and polls its dispatcher on an explicitly bounded 8 MiB coordinator thread, matching the usual Linux main-stack scale; child worker/cgroup/container limits are unchanged. Fresh Windows CI must validate this fix.
