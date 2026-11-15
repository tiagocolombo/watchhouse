# Current status

Last updated: 2026-11-15.

## Merged baseline and current slice

PRs [#2](https://github.com/tiagocolombo/watchhouse/pull/2)–[#8](https://github.com/tiagocolombo/watchhouse/pull/8) are merged into main. The latest merged baseline is `33da6d9` (PR #8). It contains M0/M1, bounded fake-only M2, and M3 source intake/reconciliation/freshness, incident operations/roles, structured investigations and independently reviewed unsent communications drafts.

Current branch `dev/m3-service-qualification` starts at that main. This reviewable M3 slice adds bounded diagnostic context, conservative case correlation and synthetic service-specific qualification acceptance. It is not merged. [ADR 0014](adrs/0014-scoped-case-correlation.md), the [executable contract](../contracts/v1/correlations/README.md) and the [prepared activation drill](16-m3-activation-drill.md) define its boundaries.

M3 remains partially accepted: actual sanitized service history and configured live activation are missing. M4 real execution/repair-to-PR and M5–M8 remain unimplemented. Repository PR merges performed by the operator's coding assistant do not constitute Watchhouse merge automation.

## Implemented in this slice

- Sentry representative-event evidence adds an optional diagnostic record: validated event ID, at most 20 module/function symbol pairs and category-only breadcrumbs. Traversal/strings are bounded. Paths, source code, variables, messages, request/user data and breadcrumb data are excluded. Symbols remain untrusted; the allowlist does not guarantee arbitrary identifiers contain no sensitive text. Missing/wrong-environment events have no diagnostic record. Legacy observations retain their serialized shape when the optional field is absent.
- Read-only suggestions require matching configured service/environment/repository, exact mapped release/revision, exception/frame signature, occurrences within one hour and current successful source reads. An indexed scan reads at most the newest 200 same-scope source records and emits at most 20 candidates. This deliberately incomplete heuristic grants no authority and changes no case or incident.
- A trusted operator owning both cases can record related/duplicate catalogue links against both case versions and current source evidence. Duplicate judgments additionally require operator-reported evidence on each case; status codes, matching diagnostics and fake results alone are insufficient. Blocking challenges prevent acceptance. Cross-service links are rejected.
- Migration 011 stores immutable unordered-pair revisions. Both case events, the relation revision and command receipt commit atomically. Exact retries return the historical receipt. Revocation works during challenges/outages; changed source facts make an active link's context stale, while unchanged rereads preserve it. No link suppresses intake, transfers ownership, merges cases or closes an incident/assessment/escalation/obligation.
- CLI `case correlate` / API `.../correlations` / web review commands share the application service. The web provides comparison/link status and explicit refresh/navigation. `case qualification-markdown` / API `.../qualification/markdown` render escaped diagnostics, freshness, dispositions/obligations and correlations. Target-only changes become visible on refresh without fabricated source-case events.
- A synthetic checkout-service contract distinguishes intentional unauthenticated rejection from a submit failure and a duplicate observation. Expected behavior requires operator-reported contract evidence and remains subject to deterministic reassessment. This is invented fixture history, not a claim of customer-history or live acceptance.

## Verification actually run

Linux, 2026-11-15:

| Check | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed |
| `cargo clippy --locked --all-targets -- -D warnings` | Passed |
| `cargo test --locked` | 83 Rust tests passed |
| `npm run build --prefix web` | Strict TypeScript passed |
| `cargo build --locked` | Bundled native binary passed |
| `npm test --prefix web` | All five Chromium scenarios passed |
| `git diff --check` and local Markdown link validation | Passed |

Nine new Rust tests cover bounded/sanitized diagnostic evidence and old observation serialization, matching/misleading/stale scopes, no automatic declarations, link caps, explicit duplicate evidence, source drift, revocation/restart/history immutability, independent duties, foreign evidence, blocking challenges, competing commands, transaction rollback, shared CLI/HTTP receipts, authorization and literal reports. Existing crash/recovery and offline timer/escalation tests remain green. The browser source scenario now compares two offline source cases, rejects unsupported links without API evidence, renders qualification Markdown and replays the interface on reload/mobile.

Ordinary tests use only synthetic fixtures/fake execution/injected source and notification transports. No live Sentry, Slack destination, subscription model or production service was exercised. Remote CI results belong to the exact PR head and must be checked separately.

## Remaining work and blockers

- **M3 acceptance:** the operator must provide one real, sanitized service-history corpus and explicit scope/mappings. Additional disposition categories (external dependency, operational/configuration problem, application defect), richer frequency/user/request windows and cross-service correlation remain planned. The synthetic contract test does not satisfy actual-history acceptance.
- **Live activation:** verified Sentry install/plan permissions, client secret/read-only API token, trusted reachable ingress, actual service/release mappings, and authorized Slack destinations/secrets/fallback participants are absent. Follow the prepared drill only after these inputs/authority exist. No live delivery, intake or read coverage has been verified.
- **M4 next independent work:** isolated workspace/test executor and installed-version capability admission for the first real harness, followed by bounded engineer/verifier/reviewer repair and reconciled PR delivery. Real-worker authentication, scoped credentials, revocation and containment are prerequisites; local actor labels are not remote authentication.
- **M5–M8:** mitigation authority, deployment/recovery observation, postmortems, accepted knowledge, automatic merging, second source/harness, packaging, remote RBAC, backup/restore, retention/redaction and backpressure remain ahead. Recovery is never inferred from Sentry status, an accepted hypothesis, a link or an approved unsent draft.
- Current source coverage is bounded Sentry Cloud issue snapshots plus one representative event, not complete historical per-error/lifecycle replay. Snapshot caps, fleet hardening, clock jumps, installers/services and license remain open. A hostile process with operator database access remains outside the current single-operator boundary.

## Exact next step

Review this M3 slice against main. Supply sanitized actual service history and activation inputs to complete the documented acceptance/drill; meanwhile M4 isolation and execution admission can proceed independently without live notification or observability credentials. Preserve fake-only CI and separate proposal/execution, merge/deployment and recovery boundaries.
