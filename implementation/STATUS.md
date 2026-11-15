# Current status

Last updated: 2026-11-15.

## Main and next batch

PRs [#2](https://github.com/tiagocolombo/watchhouse/pull/2), [#3](https://github.com/tiagocolombo/watchhouse/pull/3), [#4](https://github.com/tiagocolombo/watchhouse/pull/4) and [#5](https://github.com/tiagocolombo/watchhouse/pull/5) were checked and merged in order using merge commits. Main at `1cab931` contains the complete original stack; no feature was discarded by squashing or retargeting.

The next M3 batch starts at that main and has three reviewable slices. Each slice was validated/committed before the next began; these new PRs have not been merged.

| Slice | Branch | Dependency | Delivered |
| --- | --- | --- | --- |
| Incident operations | `dev/m3-incident-operations` | `main` | Versioned briefs, accepted role handoffs, repeating update duties |
| Investigations and drafts | `dev/m3-investigation-records` | [#6](https://github.com/tiagocolombo/watchhouse/pull/6) | Immutable hypotheses/experiments/drafts, independent review and owned obligations |
| Source reliability | `dev/m3-source-reliability` | [#7](https://github.com/tiagocolombo/watchhouse/pull/7) | Known-issue recovery, fenced reads, separate freshness and unchanged-fact deduplication |

M0/M1 and bounded fake-only M2 mechanisms remain implemented. M3 now has source intake/reconciliation plus operational incident/investigation/communication records. Live operational activation and remaining M3 acceptance on actual sanitized service history are still incomplete. Real harnesses, production mutation, repair-to-PR automation, automatic merging, verified recovery and accepted knowledge are not implemented.

## Actual implementation

- Migrations 001–007 and historical event bytes remain unchanged. Existing challenge independence, policy provenance/pinned budgets, fake supervision/handoff, notification delivery/fallback/reconciliation and incident declaration contracts remain intact.
- Migration 008 adds current incident operations separately from immutable declaration. Commander updates bind evidence and versions; the original source configuration governs severity/update intervals. Role offers require addressed acceptance and retain prior accountability until then. Expiry escalates. A handoff cannot reset update deadlines. Every overdue update cycle has its own escalation identity, including an update arriving after deadline but before a tick. New updates never resolve old escalations. Due-row filtering prevents resolved earlier duties from starving later incidents.
- Migration 009 adds immutable hypothesis/experiment/draft versions. Supported/refuted interpretations require a completed discriminating experiment bound to the exact unchanged claim/scope revision and its evidence. Results retain failed and inconclusive attempts. Plans record methods/risk/scope but execute nothing and grant no authority. Owned evidence/review deadlines survive restart, escalate without models and block assessment closure while pending/due. Late completion does not resolve past escalation.
- Communication drafts are unsent records. A distinct current commander/operations reviewer must approve the exact draft under unchanged incident/source context and clear blocking challenges. Approval is conservatively invalidated by later case events, and never authorizes sending. Role handoffs permit distinct authors/reviewers without inventing remote identity authentication.
- Migration 010 claims source read generations and retry deadlines before I/O. Only current generations can advance source projection/freshness; late output is unaccepted evidence. Known-issue sweeps recover status-only changes beyond discovery windows. Older API data cannot supersede retained newer occurrence evidence or prematurely complete a newer webhook's collection duty. Failures/interrupted reads retain old snapshots with visible uncertainty.
- Unchanged facts update freshness without case-event/evidence spam or review invalidation. Actual source changes retain normal case history. `observed_at` describes the retained fact snapshot, while read state shows latest successful reread. Configuration revisions accept a bounded refresh interval while preserving legacy default serialization. Due-time ordering and one page/item/known-source per tick bound work. Source/model unavailability cannot disable deterministic supervision and human notification duties.
- CLI/API/web share all services. Snapshots show current operations/offers/artifacts/approval currency/read freshness; Markdown renders current brief, roles, hypothesis register, experiments/drafts, original declaration and timeline. Imported text is inert. Web health polling observes freshness independently of case-event scheduling.

## Verification actually run

Linux, 2026-11-15:

| Check | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed |
| `cargo clippy --locked --all-targets -- -D warnings` | Passed |
| `cargo test --locked` | Slice 1: 64; slice 2: 69; slice 3: 74 tests passed |
| `npm run build --prefix web` and `cargo build --locked` | Strict TypeScript and bundled native binary passed |
| `npm test --prefix web` | All five Chromium scenarios passed per slice |
| `git diff --check` | Passed |

New coverage includes evidence/version/role admission, accepted/wrong/late handoff and restart, repeating deadlines, late update materialization, concurrent commands and event rollback; exact-claim experiment binding, failed hypotheses, self/stale/delayed review, source-context review invalidation, pending-duty closure gates, immutable history, case-bound evidence and inert artifact replay; known status changes outside discovery windows, unchanged reads preserving review, outage/restart, interrupted reads, stale overlapping response rejection, canonical configuration compatibility and atomic source projection retry. Browser acceptance extends signed intake → incident → current update → hypothesis → reload/mobile layout and visible missing read freshness.

Standard tests use synthetic sanitized fixtures and injected read transports. No live Sentry, Slack destination, model subscription or production service was exercised. Each PR records its own current-head remote CI result; do not infer that from local checks or an older commit.

## Activation blockers and limits

- Slack still needs verified human destinations, webhook secrets and explicit operator configuration. Sentry still needs actual service/release mappings, app/client secret, API token/permissions/plan coverage and trusted reachable ingress. No live integration was verified.
- Actor labels remain attribution under one trusted operator-token/OS boundary. A hostile process with operator database access is outside this fake-only boundary. Real-worker authentication, isolation, scoped credentials/revocation and external-effect containment remain prerequisites for M4.
- Recovery, mitigation, deployment, repair, postmortem acceptance and follow-up completion remain separate future lifecycles. Source 'resolved', an approved unsent draft, a supported hypothesis or absent telemetry establishes none of them.
- Source coverage is bounded discovery plus direct known-issue snapshots, not complete historical per-error/lifecycle replay. Only Sentry Cloud's fixed API host is supported. Real sanitized workload/retention coverage and live activation drills remain unverified.
- Snapshots include the newest 200 artifact heads/known read states; full event history remains authoritative and unpaginated. Retention/redaction, backup/restore, fleet backpressure, clock-jump hardening, installers/services and license remain open. Old binaries reject unknown newer schemas.

## Decisions and exact next step

[ADR 0011](adrs/0011-versioned-incident-operations.md), [0012](adrs/0012-investigations-and-draft-review.md) and [0013](adrs/0013-source-freshness-and-known-issue-recovery.md) capture this batch. Executable contracts: [incident operations/artifacts](../contracts/v1/incidents/README.md), [Sentry/read reliability](../contracts/v1/sentry/README.md). Earlier ADRs 0001–0010 remain historical decisions.

Review the three new PRs in stack order. Next independent M3 work: strengthen qualification/correlation and richer bounded evidence using sanitized service-specific fixtures, then prepare the concrete live activation drill. M4 real execution must satisfy the documented adapter/isolation admission boundary; this batch adds no production authority.
