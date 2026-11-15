# M3 investigation and draft review slice

Branch `dev/m3-investigation-records`, stacked on [PR #6](https://github.com/tiagocolombo/watchhouse/pull/6). Implements immutable hypothesis/experiment/draft versions, exact-claim experiment binding, independent draft review with context invalidation, owned evidence/review deadlines, deterministic escalation and assessment closure gates. CLI/API/web and Markdown expose the same records. [ADR 0012](adrs/0012-investigations-and-draft-review.md) records the boundary. Validation passed on Linux: formatting, Clippy with warnings denied, 69 Rust tests, strict TypeScript, native build, five Chromium scenarios and whitespace checks. Next: strengthen source reconciliation/freshness for known issues. No live sending, production execution or verified service recovery is implied.

# Next M3 batch

Current branch `dev/m3-incident-operations`, based on main `1cab931` after PRs #2–#5 merged in order. This batch continues with three stacked slices: incident operations/roles, investigations/communication drafts, then known-issue source reliability.

Slice 1 implements versioned incident briefs, evidence-backed severity updates, accepted role transfers, recurring update deadlines and durable overdue duty escalation. Original declarations remain immutable; reports show current coordination separately. See [ADR 0011](adrs/0011-versioned-incident-operations.md) and the [operations contract](../contracts/v1/incidents/README.md). Validation passed on Linux: formatting, Clippy with warnings denied, 64 Rust tests, strict TypeScript, bundled native build, all five Chromium scenarios and whitespace checks. No live integration or production authority was added.

# Current status

Last updated: 2026-11-15.

## Repository reality and stack

M0/M1 and bounded fake-only M2 mechanisms are implemented. The first M3 Sentry/incident vertical slice is implemented and fixture-verified. Live activation remains incomplete. No real harness, production mutation, repair-to-PR workflow, automatic merge, verified service recovery or accepted-knowledge workflow exists.

The previous four-phase stack started at main `a744d35`. PRs #2–#5 were merged in order on 2026-11-15; current batch starts at main `1cab931`.

| Phase | Branch | Base / dependency | Delivered |
| --- | --- | --- | --- |
| 1 | `dev/m2-independent-challenges` | `main` | Independent challenges, evidence-backed answer/arbitration, validation/closure gates |
| 2 | `dev/m2-trusted-policy` | Phase 1 / [#2](https://github.com/tiagocolombo/watchhouse/pull/2) | Validated immutable operator policy, pinned limits/provenance |
| 3 | `dev/m2-human-escalation` | Phase 2 / [#3](https://github.com/tiagocolombo/watchhouse/pull/3) | Slack adapter, human fallback, delivery/retry/uncertainty, acknowledgment/resolution |
| 4 | `dev/m3-sentry-incidents` | Phase 3 / [#4](https://github.com/tiagocolombo/watchhouse/pull/4) | Authenticated Sentry intake, enrichment/reconciliation, structured incident declaration/report |

Each phase was validated and committed before the next began. Final phase 1 review corrections were propagated through later branches: overdue obligations continue blocking closure, browser reconnect waits for persisted connection state, reserved challenger/verifier roles cannot execute investigator work, and historical investigator attribution disqualifies self-validation. The stack was rebased and revalidated. Read PR heads for exact commits.

## Implemented boundaries

- Existing transactional state/events/jobs/evidence/receipts, strict CLI/API/web contracts, qualification/reopening, fake supervision/checkpoints, acknowledged handoff and late-result fencing remain. Migrations 001–003 and historical event bytes were not rewritten.
- Migration 004 adds challenges tied to exact immutable event/version claims, separate fake challenger/respondent/verifier responsibilities, acknowledgment/investigation/evidence-backed answers, independent arbitration and deterministic deadline escalation. New verifier observations are required; agreement or an answer cannot resolve itself. Dissent/uncertainty stays in history. Blocking challenges fence result acceptance, disposition and assessment gates. Later case events invalidate prior validation/closure. Overdue obligations remain blockers. Assessment closure does not assert service recovery.
- Migration 005 replaces scattered development constants with strict immutable operator policy. Cases pin governing revision before their first event. Events/snapshots retain name/revision/digest; installer/source/time persist. Existing work retains its original proof, deadline and shared budget. No worker/HTTP policy-edit route exists. Fake role labels remain attribution under a trusted operator/OS boundary, not remote identity or containment.
- Migration 006 adds explicit immutable human notification chains, secret references, frozen limits, leased/fenced delivery claims, Slack response classification, known-rejection retries, fallback/exhaustion visibility, uncertain-send reconciliation and evidence-backed explicit resolution. Acceptance, acknowledgment and resolution are separate facts. Resolution does not clear an underlying assignment block or challenge. No destination is inferred from an owner label.
- Migration 007 adds trusted Sentry mappings/profiles, exact-byte HMAC before durable HTTP 202, separate signed receipts/event deduplication, fixed-host read-only enrichment, bounded allowlisted observations, retryable missing enrichment, stable source case identity and non-regressing UTC occurrence projections. Raw private user/request/message/variable payloads are discarded; imported text grants no authority.
- Reconciliation retains windows/cursors across restart, overlaps completed windows, queues each page atomically and only advances coverage after the final page. Cyclic pagination fails closed. Health shows credential presence, pending age and incomplete coverage. Coverage is issue snapshots discoverable in a requested window, not complete historical per-error/lifecycle recovery.
- Explicit operator incident declaration freezes exact source/configuration/policy provenance, severity/rationale, impact/uncertainty, roles, investigation evidence, unknown timing/recovery and an unsent communication draft. Trusted profiles set update obligations; overdue updates escalate without models. JSON/escaped Markdown reports cover brief, roles, impact/timing, response, recovery assessment, evidence and timeline. Duplicate source intake/declaration cannot create duplicate cases/incidents for the same identity.
- CLI/API/web share services and show persisted review, policy, delivery, source and incident state. Slow source/notification pumps run independently of deterministic supervision. Closing the browser cannot stop coordination.

## Verification actually run

| Checks | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed |
| `cargo clippy --locked --all-targets -- -D warnings` | Passed |
| `cargo test --locked` | Phase 1: 46; phase 2: 48; phase 3: 53; phase 4: 59 tests passed |
| `npm run build --prefix web` then `cargo build --locked` | Strict TypeScript and bundled native binary passed |
| `npm test --prefix web` | Five Chromium scenarios passed in the final stack; phases 1–3 have four scenarios |
| `git diff --check` | Passed |

Coverage includes independence and self-validation denial, stale/concurrent commands, rollback, overdue closure gates, policy pinning/revision denial, model-offline handoff/recovery/escalation, concurrent delivery, retries/fallback, sender loss and post-effect persistence failure, cryptographic vectors/raw bytes, source scope, receipt/event deduplication, out-of-order/timezone handling, allowlist exclusion, missed-issue discovery across pages/restart/outage, missing enrichment retries, case-creation rollback, incident uniqueness/update deadlines, report rendering/escaping, and signed HTTP → browser incident → report/reload/mobile layout.

Normal tests use synthetic sanitized fixtures and injected transports, never live Sentry, Slack or model services. No production recovery is established by a fake test.

Remote CI passed at current phase 1 `c039519` ([run](https://github.com/tiagocolombo/watchhouse/actions/runs/43643855883)), phase 2 `3ded3f1` ([run](https://github.com/tiagocolombo/watchhouse/actions/runs/43721690475)) and phase 3 `199fbfa` ([run](https://github.com/tiagocolombo/watchhouse/actions/runs/43721807948)): Linux/macOS/Windows native checks and Linux Chromium. Phase 4 remote CI must be read at its published head; no result is inferred from local checks.

## Activation blockers and remaining work

- Slack: verified human destinations, injected webhook secrets and explicit operator configuration are missing. No live delivery was verified. Uncertain webhook delivery requires operator inspection/evidence because there is no provider receipt lookup.
- Sentry: actual organization/project/service/release mappings, app installation/client secret, API token/plan permissions and reachable trusted ingress are missing. The server remains loopback/Host/Origin restricted. No live intake, enrichment or coverage was verified. Only Sentry Cloud's fixed `sentry.io` API is admitted.
- Real-worker authentication, OS isolation, scoped credentials/revocation and external-effect containment remain prerequisites for M4. A hostile process with operator database access is outside the current fake-only trust boundary.
- This is the first M3 slice. Incident revision, broader role execution, rich stack/breadcrumb access, mitigation/recovery, postmortems and corrective actions remain M3/M5 work. Source 'resolved', absent telemetry or counts without denominators cannot prove recovery.
- Snapshot pagination, retention/redaction, backup/restore, clock-jump hardening, fleet overload controls, installers/services and project license remain open. Older binaries reject newer migrations rather than operating unknown records.

## Decisions and exact next step

[ADR 0007](adrs/0007-independent-challenges.md), [0008](adrs/0008-trusted-policy.md), [0009](adrs/0009-human-escalation.md) and [0010](adrs/0010-first-sentry-incident-slice.md) document the implemented proposals and limits. Executable contracts are under `contracts/v1/challenges`, `policy`, `contracts/v1/notifications` and `contracts/v1/sentry`.

The previous stack is merged. Next independent development: strengthen M3 incident revision/investigation/communications artifacts and source coverage using these fixtures. Prepare a concrete live activation drill when actual service ownership, credentials and ingress are available. These PRs grant no production authority.
