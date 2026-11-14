# Verification and acceptance

## Approach

Test domain invariants and failure boundaries with fake clocks, sources, adapters, workers, and external-effect simulators. Standard tests must not require subscriptions, paid models, live production, or network access.

Contract fixtures pin schema/runtime versions and make unsupported behavior explicit. Live compatibility runs are opt-in and report actual runtime versions and permissions.

## Acceptance scenarios

| ID | Scenario | Required evidence |
| --- | --- | --- |
| V01 | Correctly rejected authentication request | Catalogue rationale; no repair; scoped reconsideration |
| V02 | Authentication refresh regression also returns 401 | Reproduction admits defect; not suppressed by V01 lesson |
| V03 | Short dependency timeout | Monitored disposition; threshold/recurrence reopens |
| V04 | Sentry and Datadog report one fault | Linked signals and reversible correlation, no duplicate repair |
| V05 | One provider issue contains two causes | Separate hypotheses/cases with provenance |
| V06 | Duplicate/out-of-order webhook and missed delivery | Idempotent intake, historical timeline, reconciliation coverage |
| V07 | Worker crashes or stops heartbeating | Checkpoint recovery and fenced reassignment |
| V08 | Worker alive but repeating failed experiments | Progress deadline triggers block and revised approach |
| V09 | All model providers unavailable | Supervisor, human notifications, escalation timers remain active |
| V10 | Old worker returns after reassignment | Result attributed but cannot mutate authoritative workflow |
| V11 | Quota/auth failure mid-fix | Typed failure, bounded fallback, preserved budget and workspace |
| V12 | Unsupported isolation or usage guarantee | Admission denied or routed to qualified worker |
| V13 | Investigator and challenger disagree | Targeted experiment or explicit arbitration; no vote-based approval |
| V14 | Missing authority for production rollback | Proposal retained; escalation delivered; no execution |
| V15 | Patch without adequate regression evidence | Validation/review blocks advancement |
| V16 | Candidate or relevant base changes after review | Evidence invalidated/revalidated before merge |
| V17 | Crash after PR creation or merge | External state reconciled; no duplicate effect |
| V18 | Incident mitigated before permanent repair | Response resolves separately; problem/actions remain owned |
| V19 | Merge without observed deployment | No false deployed/recovered status |
| V20 | No events because traffic stopped | Recovery inconclusive, not successful |
| V21 | Postmortem invents cause or impact number | Challenge blocks acceptance until corrected/marked unknown |
| V22 | Corrective action marked done without proof | Acceptance criteria and independent verification enforced |
| V23 | Unaccepted/imported lesson weakens suppression | Proposal excluded; trusted version remains |
| V24 | Malicious telemetry or candidate instructions | No authority expansion, secret access, or uncontrolled fetch |
| V25 | Escalation unacknowledged | Fallback chain, delivery evidence, accountable ownership |
| V26 | Operator closes/reopens browser | No processing interruption or missing/duplicated timeline state |
| V27 | Backup restored on a new coordinator | Obligations preserved; stale leases revoked; effects reconciled |
| V28 | Event flood during an active incident | Escalation/recovery jobs remain schedulable |
| V29 | Sensitive evidence is redacted | Tombstone/provenance preserved; affected decisions flagged |
| V30 | Apple build assigned to Linux worker | Rejected/rerouted by capability requirements |

## Test layers

- Pure domain/state-machine tests: transitions, policy, clocks, correlation, closure gates.
- Persistence tests: transactions, concurrency, migrations, outbox, recovery, artifact completeness.
- Worker tests: leases, process-tree termination, stale results, workspace isolation, fence revocation.
- Adapter contracts: protocol framing, errors, capability claims, output validation, cancellation.
- Workflow simulations: full response, challenges, mitigation, repair, review, recovery, actions.
- Browser acceptance: operating views, evidence navigation, stale commands, escalation and controls.
- Opt-in integration: sanitized source fixtures, dedicated GitHub test repository, installed harness/version.
- Operational drills: degraded models, broken source, unreachable notification, storage pressure, restore.

## Evaluation

Replay historical cases with frozen evidence/policy/knowledge snapshots. Separate training/reference cases from evaluation cases. Score dispositions, unnecessary repairs, missed actionable defects, causal accuracy, fix effectiveness, recurrence, escalation quality, and resource use.

Human review is required for evaluation labels where operational truth is uncertain. Retain ambiguous labels instead of optimizing against invented certainty.

## Definition of done per feature

Implementation plus meaningful tests, visible state/evidence, typed failure/recovery behavior, documented authority, updated contracts, and STATUS.md entries with actual commands/results. Avoid tests that merely mirror field assignments.
